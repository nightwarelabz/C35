//! Wall-running — bought with speed, spent against gravity.
//!
//! Honest physics, never a scripted rail: entry is priced by
//! the gait band, the run decays against gravity, wall angle
//! and surface wetness price the budget, and no wall-run may
//! gain energy — exit speed ≤ entry speed minus the priced
//! decay, every tick. The budget is spent, not scripted: when
//! it empties, the arc falls to the capsule's normal gravity.

use crate::traversal::events::{EventLog, StimulusKind};
use crate::traversal::physics::{Body, StaticWorld, CAPSULE_RADIUS};
use crate::traversal::surface::SurfaceRecord;
use glam::Vec3;

/// Minimum entry speed (m/s): the run band starts at 3.4, so
/// 3.0 gates the run to run/sprint entries. Below it, no entry
/// at all — a walk at a wall is a walk at a wall.
pub const MIN_ENTRY_SPEED: f32 = 3.0;
/// Budget seconds per m/s of entry speed, at grip 1.0: a 5.8
/// sprint buys ~3.2 s of dry-brick run.
pub const BUDGET_PER_SPEED: f32 = 0.55;
/// Wall lean past this (degrees from vertical) admits no run:
/// a surface leaning away spends faster, and past the limit
/// there is no run at all.
pub const MAX_WALL_LEAN_DEG: f32 = 25.0;
/// Reach from the capsule surface to a runnable wall (m).
pub const WALL_REACH: f32 = 0.6;
/// Sink rate at full budget (m/s) growing to this at empty.
pub const SINK_FULL: f32 = 0.2;
pub const SINK_EMPTY: f32 = 2.5;

/// Which side the wall is on, from the runner's facing.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WallSide {
    Left,
    Right,
}

/// A wall-run in progress. `normal` points FROM the wall TO the
/// runner; `dir` is the run direction along the wall.
#[derive(Copy, Clone, Debug)]
pub struct WallRun {
    pub normal: Vec3,
    pub dir: Vec3,
    pub side: WallSide,
    pub entry_speed: f32,
    pub budget: f32,
    pub budget_max: f32,
    pub surface: SurfaceRecord,
    /// Footfall phase (seconds since the last emitted step).
    pub step_clock: f32,
}

impl WallRun {
    /// Try to enter from a ground/air body: speed gate, wall
    /// within reach to either side, movement roughly parallel
    /// to the wall (|v·n| ≤ half the speed), wall lean within
    /// the limit. `facing` is the horizontal unit facing.
    /// Returns None when any price is not met — no partial runs.
    pub fn try_enter(body: &Body, facing: Vec3, world: &StaticWorld) -> Option<WallRun> {
        let speed = body.horizontal_speed();
        if speed < MIN_ENTRY_SPEED {
            return None;
        }
        let vel_h = Vec3::new(body.vel.x, 0.0, body.vel.z);
        // Side directions, house convention (locomotion's
        // facing()): left = (fz, -fx) for facing (fx, fz) —
        // up × facing, so the left hand is the left hand.
        let left = Vec3::new(facing.z, 0.0, -facing.x);
        let probe_at = body.feet + Vec3::new(0.0, 1.0, 0.0);
        for (side, dir) in [(WallSide::Left, left), (WallSide::Right, -left)] {
            let reach = CAPSULE_RADIUS + WALL_REACH;
            if let Some(hit) = world.raycast(probe_at, dir, reach) {
                let n = hit.normal;
                // Wall normal must be roughly horizontal: lean
                // is the normal's tilt above horizontal.
                let lean_deg = n.y.abs().asin().to_degrees();
                if lean_deg > MAX_WALL_LEAN_DEG {
                    continue;
                }
                // Normal must face the runner.
                if n.dot(dir) >= 0.0 {
                    continue;
                }
                // Roughly parallel: the velocity's wall-normal
                // component must be less than half the speed.
                if vel_h.dot(n).abs() > 0.5 * speed {
                    continue;
                }
                // Run direction: velocity with its normal
                // component removed, along the wall.
                let along = (vel_h - n * vel_h.dot(n)).normalize();
                let grip = hit.surface.wall_grip();
                if grip <= 0.0 {
                    continue;
                }
                let budget_max = speed * BUDGET_PER_SPEED * grip;
                return Some(WallRun {
                    normal: n,
                    dir: along,
                    side,
                    entry_speed: speed,
                    budget: budget_max,
                    budget_max,
                    surface: hit.surface,
                    step_clock: 0.0,
                });
            }
        }
        None
    }

    /// Remaining budget fraction, 1 at entry, 0 at exhaustion.
    pub fn budget_frac(&self) -> f32 {
        if self.budget_max > 0.0 {
            (self.budget / self.budget_max).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Current run speed: decays with the budget, so exit
    /// speed is always below entry speed — the no-energy-gain
    /// law, enforced by construction, not by audit.
    pub fn speed(&self) -> f32 {
        self.entry_speed * (0.55 + 0.45 * self.budget_frac())
    }

    /// Sink rate: gentle while the budget holds, falling as it
    /// empties — the player feels the fall coming before it
    /// arrives, and the exit is chosen, not suffered.
    pub fn sink(&self) -> f32 {
        SINK_EMPTY + (SINK_FULL - SINK_EMPTY) * self.budget_frac()
    }

    /// One fixed-step tick. Moves the body along the wall at
    /// the decayed speed, sinks it, emits footfalls at gait
    /// cadence, and reports the outcome: true while the run
    /// holds, false when the budget emptied (caller drops the
    /// body into normal gravity with velocity kept).
    pub fn update(
        &mut self,
        dt: f32,
        body: &mut Body,
        world: &StaticWorld,
        events: &mut EventLog,
        tick: u64,
    ) -> bool {
        self.budget -= dt;
        if self.budget <= 0.0 {
            return false;
        }
        let speed = self.speed();
        body.vel = self.dir * speed + Vec3::new(0.0, -self.sink(), 0.0);
        body.feet += body.vel * dt;
        body.feet = world.resolve_capsule(body.feet);
        // Keep the capsule glued to the wall face: re-seat the
        // lateral offset each tick (the wall is the rail's
        // reference, not a suggestion).
        let probe = body.feet + Vec3::new(0.0, 1.0, 0.0);
        match world.raycast(probe, -self.normal, CAPSULE_RADIUS + WALL_REACH) {
            Some(hit) => {
                let want = hit.point + self.normal * (CAPSULE_RADIUS + 0.05);
                body.feet.x = want.x;
                body.feet.z = want.z;
            }
            // The wall ended. Budget left is not a rail in the
            // air — the run stops and the caller drops the body.
            None => return false,
        }
        // Footfalls at the run's cadence: step length 0.86 m
        // at run speed (the gait table's run anchor).
        let step_len = 0.86f32;
        self.step_clock += dt;
        let period = step_len / speed.max(0.5);
        if self.step_clock >= period {
            self.step_clock -= period;
            events.push(
                tick,
                StimulusKind::Footstep,
                body.feet,
                self.surface,
                self.surface.footstep_gain(speed),
            );
        }
        true
    }

    /// Jump exit: away from the wall, momentum kept, plus the
    /// jump impulse. Returns the exit velocity.
    pub fn jump_exit(&self) -> Vec3 {
        self.normal * 3.0 + self.dir * self.speed() + Vec3::new(0.0, 4.6, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traversal::events::EventLog;
    use crate::traversal::physics::{Body, BoxCollider, StaticWorld, FIXED_DT};
    use crate::traversal::surface::{SurfaceKind, SurfaceRecord};

    #[test]
    fn run_ends_when_the_wall_ends() {
        let mut world = StaticWorld::new();
        world.add(BoxCollider::new(
            Vec3::new(-0.3, 0.0, -1.0),
            Vec3::new(0.0, 3.0, 1.0),
            SurfaceRecord::dry(SurfaceKind::Brick),
        ));
        let mut body = Body::new(Vec3::new(0.5, 0.0, 0.0));
        body.vel = Vec3::new(0.0, 0.0, 5.0);
        let facing = Vec3::new(0.0, 0.0, 1.0);
        let mut run = WallRun::try_enter(&body, facing, &world).expect("wall in reach");
        let mut events = EventLog::new();
        let mut held = true;
        for t in 0..800 {
            held = run.update(FIXED_DT, &mut body, &world, &mut events, t);
            if !held {
                break;
            }
        }
        assert!(!held, "still running at z={}", body.feet.z);
        assert!(
            body.feet.z < 3.0,
            "skated to z={} after the wall ended at 1",
            body.feet.z
        );
    }
}
