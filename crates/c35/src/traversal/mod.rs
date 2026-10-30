//! Traversal — physics, grapple, wall-run, mantle, navigation.
//!
//! The traversal milestone lands as ONE piece or not at all
//! (the brief's own words): the capsule exists to serve the
//! rope, the rope exists to serve the wall-run chain, and the
//! navmesh exists to route all three. So one module, with the
//! physics slice inside it — not a general solver wearing a
//! milestone's name. (Name defended: `traversal`, with
//! `physics.rs` the capsule/gravity/seam slice beside
//! `locomotion.rs` in spirit — a standalone `physics` module
//! would promise a general engine this milestone is not.)
//!
//! The orchestrator, [`TravSim`], is the single entry point
//! the game host drives: one fixed-step tick, one mode
//! machine. Mode priority: Mantle > Grapple > WallRun >
//! Ground/Air. The pose family reads the mode; the weapon
//! layer reads the both-hands state. Nothing here renders,
//! nothing here perceives — physics prices INTO the fields,
//! and perception judges in its own milestone.

pub mod events;
pub mod nav;
pub mod physics;
pub mod poses;
pub mod rope;
pub mod surface;
pub mod wallrun;

use crate::operator::PoseFrame;
use crate::traversal::events::{EventLog, StimulusKind};
use crate::traversal::physics::{
    move_body, Body, Mantle, StaticWorld, FIXED_DT, GRAVITY,
};
use crate::traversal::poses::{traversal_pose, TravPose};
use crate::traversal::rope::{Grapple, HookInput};
use crate::traversal::wallrun::{WallRun, WallSide};
use glam::Vec3;

/// One tick of player intent. `hook_press` is the press EDGE
/// (the caller edge-detects E); `hook_held` is the hold.
/// `aim` is the unit aim direction for the claw. `jump` is
/// the jump intent this tick (mantle pre-empts it at a ledge).
#[derive(Copy, Clone, Debug, Default)]
pub struct TravInput {
    /// Horizontal wish direction (unit, or zero).
    pub wish: Vec3,
    /// Horizontal unit facing.
    pub facing: Vec3,
    /// Target ground speed (m/s) — the gait band's number.
    pub speed: f32,
    pub jump: bool,
    pub hook_press: bool,
    pub hook_held: bool,
    pub crouch: bool,
    pub aim: Vec3,
}

/// The body's mode. The pose family and the host read this;
/// the sim owns it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BodyMode {
    Ground,
    Air,
    WallRun,
    Grapple,
    Mantle,
}

/// The traversal sim: body, grapple, wall-run, mantle, events.
/// Ticks at FIXED_DT; the caller passes dt (clamped to the
/// fixed step — the sim never reads a wall clock).
pub struct TravSim {
    pub body: Body,
    pub grapple: Grapple,
    pub wallrun: Option<WallRun>,
    pub mantle: Option<Mantle>,
    pub mode: BodyMode,
    pub events: EventLog,
    pub tick: u64,
    /// Blend weight toward the traversal pose (eased by the
    /// caller over frames; the sim reports the target).
    pub pose_blend: f32,
    /// Landing-dip timer (ticks remaining): set when a Landing
    /// event lands this tick, so the Land pose shows briefly
    /// instead of forever after one hard landing.
    pub land_ticks: u32,
}

impl TravSim {
    pub fn new(start: Vec3) -> TravSim {
        TravSim {
            body: Body::new(start),
            grapple: Grapple::new(),
            wallrun: None,
            mantle: None,
            mode: BodyMode::Ground,
            events: EventLog::new(),
            tick: 0,
            pose_blend: 0.0,
            land_ticks: 0,
        }
    }

    /// Current traversal pose for the pose family, plus whether
    /// the weapon layer is open. The gait frame comes from the
    /// caller (the animator owns gaits); traversal overrides
    /// it by mode.
    pub fn pose(&self, gait_frame: &PoseFrame) -> (PoseFrame, bool) {
        let (pose, committed) = match self.mode {
            BodyMode::WallRun => match self.wallrun {
                Some(w) => match w.side {
                    WallSide::Left => (TravPose::WallRunLeft, false),
                    WallSide::Right => (TravPose::WallRunRight, false),
                },
                None => (TravPose::Gait, self.grapple.hands_committed()),
            },
            BodyMode::Grapple => {
                if self.grapple.is_latched() {
                    // Swing vs hang by horizontal speed: a
                    // swinging body tucks, a hanging one hangs.
                    if self.body.horizontal_speed() > 1.5 {
                        (TravPose::GrappleSwing, true)
                    } else {
                        (TravPose::GrappleHang, true)
                    }
                } else {
                    (TravPose::GrappleRaise, true)
                }
            }
            BodyMode::Mantle => (TravPose::Mantle, true),
            BodyMode::Air => {
                if self.body.vel.y < -1.0 {
                    (TravPose::Fall, self.grapple.hands_committed())
                } else {
                    (TravPose::Gait, self.grapple.hands_committed())
                }
            }
            BodyMode::Ground => {
                if self.land_ticks > 0 {
                    (TravPose::Land, self.grapple.hands_committed())
                } else {
                    (TravPose::Gait, self.grapple.hands_committed())
                }
            }
        };
        traversal_pose(gait_frame, pose, self.pose_blend, committed)
    }

    /// Landing-dip bookkeeping: a Landing event this tick sets
    /// the timer; otherwise it counts down. Called once per
    /// tick on every path that can land.
    fn settle_land_tick(&mut self) {
        let landed = self
            .events
            .events
            .last()
            .map_or(false, |e| e.tick == self.tick && e.kind == StimulusKind::Landing);
        if landed {
            self.land_ticks = 30;
        } else if self.land_ticks > 0 {
            self.land_ticks -= 1;
        }
    }

    /// One fixed-step tick. The mode machine, in priority
    /// order. Grapple firing pre-empts a wall-run (both hands
    /// commit, the run ends, momentum carries into the swing);
    /// mantling pre-empts everything except an active grapple
    /// (hands on the ledge cannot fire).
    pub fn tick(&mut self, input: TravInput, world: &StaticWorld) {
        let dt = FIXED_DT;
        self.tick += 1;
        let tick = self.tick;

        // Mantle in progress: kinematic arc, then plant.
        if let Some(m) = self.mantle.as_mut() {
            match m.step(dt) {
                Some(pos) => {
                    self.body.feet = pos;
                    self.body.vel = Vec3::ZERO;
                    self.mode = BodyMode::Mantle;
                    self.pose_blend = 1.0;
                    return;
                }
                None => {
                    let to = m.to;
                    self.body.feet = to;
                    self.body.vel = Vec3::ZERO;
                    self.body.grounded = true;
                    self.mantle = None;
                    self.mode = BodyMode::Ground;
                    if let Some(g) = world.ground_at(to.x, to.z) {
                        self.events.push(
                            tick,
                            StimulusKind::Mantle,
                            to,
                            g.surface,
                            0.8,
                        );
                    }
                    // Fall through to normal movement below.
                }
            }
        }

        // Mantle trigger: jump + forward intent + ledge within
        // reach — but never while grappled (hands committed).
        if input.jump
            && !self.grapple.hands_committed()
            && self.mantle.is_none()
            && (self.mode == BodyMode::Ground || self.mode == BodyMode::Air)
        {
            if let Some(m) = Mantle::try_start(self.body.feet, input.facing, world) {
                self.mantle = Some(m);
                self.mode = BodyMode::Mantle;
                self.pose_blend = 1.0;
                return;
            }
        }

        // Grapple update always runs (it owns its state
        // machine, including the waiting-for-release guard).
        let origin = self.body.feet + Vec3::new(0.0, 1.4, 0.0);
        self.grapple.update(
            dt,
            HookInput {
                press: input.hook_press,
                held: input.hook_held,
                crouch: input.crouch,
            },
            origin,
            input.aim,
            &mut self.body,
            world,
            &mut self.events,
            tick,
        );

        if self.grapple.is_latched() || self.grapple.hands_committed() {
            // Grappled (or claw flying): the rope owns the
            // body. Air steering applies on top of the pull.
            if self.grapple.is_latched() {
                self.wallrun = None;
                self.mode = BodyMode::Grapple;
                self.pose_blend = 1.0;
                // The pendulum: gravity pulls, the rope catches.
                // Without gravity here a slack rope would hang
                // the body motionless in mid-air.
                self.body.vel.y -= GRAVITY * dt;
                // Steering: small air accel toward the wish.
                let wish = Vec3::new(input.wish.x, 0.0, input.wish.z);
                self.body.vel += wish * 2.5 * dt;
                self.body.feet += self.body.vel * dt;
                self.body.feet = world.resolve_capsule(self.body.feet);
                // Ground settle while latched (standing on the
                // ground with a slack rope is still standing).
                if self.body.vel.y <= 0.0 {
                    if let Some(g) = world.ground_at(self.body.feet.x, self.body.feet.z)
                    {
                        if (self.body.feet.y - g.height).abs() <= 0.05 {
                            self.body.feet.y = g.height;
                            self.body.vel.y = 0.0;
                            self.body.grounded = true;
                        } else {
                            self.body.grounded = false;
                        }
                    } else {
                        self.body.grounded = false;
                    }
                }
                self.settle_land_tick();
                return;
            }
            // Claw flying: body keeps its normal physics.
            self.mode = if self.body.grounded {
                BodyMode::Ground
            } else {
                BodyMode::Air
            };
            self.pose_blend = 1.0;
            move_body(
                &mut self.body,
                Vec3::new(input.wish.x, 0.0, input.wish.z),
                input.speed,
                false,
                dt,
                world,
                &mut self.events,
                tick,
            );
            self.settle_land_tick();
            return;
        }

        // Wall-run in progress: it owns the body until its
        // budget empties, a jump exits, or a grapple fires
        // (handled above — firing ends the run).
        if let Some(w) = self.wallrun.as_mut() {
            if input.jump {
                let v = w.jump_exit();
                self.body.vel = v;
                self.body.grounded = false;
                self.wallrun = None;
                self.mode = BodyMode::Air;
                self.pose_blend = 0.0;
            } else {
                let held = w.update(
                    dt,
                    &mut self.body,
                    world,
                    &mut self.events,
                    tick,
                );
                if held {
                    self.mode = BodyMode::WallRun;
                    self.pose_blend = 1.0;
                    return;
                }
                // Budget emptied: drop with velocity kept.
                self.wallrun = None;
                self.mode = BodyMode::Air;
                self.pose_blend = 0.0;
            }
            // Fall through to normal movement after a jump or
            // a budget exit (the tick's remaining physics).
            move_body(
                &mut self.body,
                Vec3::new(input.wish.x, 0.0, input.wish.z),
                input.speed,
                false,
                dt,
                world,
                &mut self.events,
                tick,
            );
            self.settle_land_tick();
            return;
        }

        // Ground / air: normal movement, then wall-run entry.
        move_body(
            &mut self.body,
            Vec3::new(input.wish.x, 0.0, input.wish.z),
            input.speed,
            input.jump,
            dt,
            world,
            &mut self.events,
            tick,
        );
        self.mode = if self.body.grounded {
            BodyMode::Ground
        } else {
            BodyMode::Air
        };
        self.pose_blend = 0.0;
        self.settle_land_tick();
        if self.mode == BodyMode::Ground || self.mode == BodyMode::Air {
            if let Some(w) = WallRun::try_enter(&self.body, input.facing, world) {
                self.wallrun = Some(w);
                self.mode = BodyMode::WallRun;
                self.pose_blend = 1.0;
            }
        }
    }
}
