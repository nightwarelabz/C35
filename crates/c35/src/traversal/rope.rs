//! The grapple — a verlet rope with the Nexuiz tarzan character.
//!
//! Grounded in the house's own records (thisissparta2 spec §4 and
//! the Nexuiz 2.5.2 `g_hook.qc` behavior), re-derived in C35 units
//! — the numbers below are the traversal build's own, not copied
//! constants: the house note itself says the shipped balance
//! values were NOT verified in the house tree, so nothing here
//! claims stock fidelity, only the stock CHARACTER.
//!
//! Character, fixed:
//! - E press (edge) fires, with a waiting-for-release guard:
//!   holding E through a detach does not instantly refire.
//! - E hold keeps the rope attached (pull-and-swing); E release
//!   detaches instantly, momentum kept, never zeroed.
//! - Crouch stops the reel, not the swing.
//! - Sky (and any unbiteable surface) never latches: the claw
//!   falls, visibly, and the strike's noise still happened.
//! - Movers latch and are followed: the far end rides the mover.
//! - A strike beyond line length does not connect.
//! - ONE deliberate deviation from Nexuiz, kept: Nexuiz's hook
//!   is offhand (shoot while hooked); DeBrief's launcher is
//!   two-handed, so while committed the weapon layer is CLOSED
//!   as graph state — no firing, no aim pose — until release.
//! - The rope is a verlet chain: the far end pinned at the
//!   strike, the near end coupled to the capsule, interior
//!   points pushed out of geometry — so a line over a parapet
//!   drapes at the edge and its free length shortens there,
//!   the pendulum pivot MOVING to the contact. A rope through
//!   a building is a failed milestone, however good the swing
//!   feels.

use crate::traversal::events::{EventLog, StimulusKind};
use crate::traversal::physics::{Body, StaticWorld, CAPSULE_RADIUS};
use crate::traversal::surface::{SurfaceKind, SurfaceRecord};
use glam::Vec3;

/// Claw flight speed (m/s) along the aim line.
pub const CLAW_SPEED: f32 = 40.0;
/// Maximum line length (m): a strike beyond this does not connect.
pub const MAX_LINE: f32 = 40.0;
/// Reel-in speed while held and not crouched (m/s).
pub const REEL_SPEED: f32 = 6.0;
/// Shortest the rope reels to (m).
pub const MIN_ROPE: f32 = 2.0;
/// Rubber-band acceleration per metre of stretch (1/s²).
pub const RUBBER_K: f32 = 18.0;
/// Extra acceleration per metre past the stretch allowance.
pub const OVERSTRETCH_K: f32 = 40.0;
/// Stretch the rope tolerates before the overstretch force and
/// the length clamp engage (m).
pub const STRETCH_ALLOW: f32 = 3.0;
/// Rope air damping rate (1/s): the swing loses energy slowly,
/// never gains it.
pub const ROPE_DAMP: f32 = 0.2;
/// Verlet points in the chain (anchor + interior + body end).
pub const ROPE_POINTS: usize = 12;
/// Distance-constraint iterations per substep.
pub const ROPE_ITERS: usize = 8;
/// Chest attach height above the feet.
pub const ATTACH_HEIGHT: f32 = 1.4;

/// Where the far end sits. Movers are followed: the caller
/// updates `pos` every tick from the mover, and the rope's far
/// end rides it. If the mover dies, the caller severs.
#[derive(Copy, Clone, Debug)]
pub enum Anchor {
    Static(Vec3),
    Mover { pos: Vec3 },
}

impl Anchor {
    pub fn pos(&self) -> Vec3 {
        match self {
            Anchor::Static(p) => *p,
            Anchor::Mover { pos } => *pos,
        }
    }
}

/// Input for one tick. `press` is the press EDGE (already
/// edge-detected by the caller); `held` is the current hold.
#[derive(Copy, Clone, Debug, Default)]
pub struct HookInput {
    pub press: bool,
    pub held: bool,
    pub crouch: bool,
}

#[derive(Debug)]
pub enum GrappleState {
    Idle,
    Flying {
        pos: Vec3,
        vel: Vec3,
        travelled: f32,
    },
    Latched {
        anchor: Anchor,
        /// Verlet points, anchor first, body end last.
        rope: Vec<Vec3>,
        /// Previous positions, same order (verlet memory).
        prev: Vec<Vec3>,
        /// Current free length (m); reeling shortens it.
        len: f32,
        /// Last drape contact (a rope point resting on geometry),
        /// or None: the pendulum pivot moves to it when set.
        contact: Option<Vec3>,
    },
}

/// The grapple system. Owns no rendering: the host draws the
/// braided line through `rope_points()` when latched, and the
/// claw at `claw_pos()` when flying.
#[derive(Debug)]
pub struct Grapple {
    pub state: GrappleState,
    /// Waiting-for-release guard: set on detach, cleared when
    /// the button is observed released. Holding E through a
    /// detach never refires.
    pub waiting_release: bool,
}

impl Grapple {
    pub fn new() -> Grapple {
        Grapple {
            state: GrappleState::Idle,
            waiting_release: false,
        }
    }

    /// Both-hands law, as state: committed from the strike until
    /// release (flying counts — the launcher is raised and both
    /// hands are on it from the press). The pose graph closes
    /// the weapon layer on this, never on an input suggestion.
    pub fn hands_committed(&self) -> bool {
        !matches!(self.state, GrappleState::Idle)
    }

    pub fn claw_pos(&self) -> Option<Vec3> {
        match &self.state {
            GrappleState::Flying { pos, .. } => Some(*pos),
            _ => None,
        }
    }

    pub fn rope_points(&self) -> Option<&[Vec3]> {
        match &self.state {
            GrappleState::Latched { rope, .. } => Some(rope),
            _ => None,
        }
    }

    pub fn is_latched(&self) -> bool {
        matches!(self.state, GrappleState::Latched { .. })
    }

    /// Detach instantly, momentum kept. Called on E release and
    /// on sever; sets the waiting-for-release guard.
    pub fn detach(&mut self, events: &mut EventLog, tick: u64, sever: bool, at: Vec3) {
        if self.is_latched() || matches!(self.state, GrappleState::Flying { .. }) {
            if sever {
                events.push(
                    tick,
                    StimulusKind::Sever,
                    at,
                    SurfaceRecord::dry(SurfaceKind::Metal),
                    1.0,
                );
            }
            self.state = GrappleState::Idle;
            self.waiting_release = true;
        }
    }

    /// Sever the line by damage (rare, priced — not routine).
    pub fn sever(&mut self, events: &mut EventLog, tick: u64, at: Vec3) {
        self.detach(events, tick, true, at);
    }

    /// Retarget a latched anchor (mover follow): the host moves
    /// the platform mesh and calls this every tick with the
    /// mover's current point, so the rope's far end rides it.
    /// Converts a static latch into a followed mover — the
    /// latch point is where the mover was, and it moves on.
    pub fn retarget_anchor(&mut self, pos: Vec3) {
        if let GrappleState::Latched { anchor, .. } = &mut self.state {
            *anchor = Anchor::Mover { pos };
        }
    }

    /// One fixed-step tick. `origin` is the launcher muzzle,
    /// `aim` the unit aim direction, `body` the capsule (moved
    /// by the caller except for the rope's pull, applied here).
    /// State transitions defer through `next`: the match borrows
    /// the owned state, so a transition is staged and applied
    /// after the match, never mid-borrow.
    pub fn update(
        &mut self,
        dt: f32,
        input: HookInput,
        origin: Vec3,
        aim: Vec3,
        body: &mut Body,
        world: &StaticWorld,
        events: &mut EventLog,
        tick: u64,
    ) {
        if !input.held {
            self.waiting_release = false;
        }
        let mut state = std::mem::replace(&mut self.state, GrappleState::Idle);
        let mut next: Option<GrappleState> = None;
        match &mut state {
            GrappleState::Idle => {
                if input.press && !self.waiting_release {
                    next = Some(GrappleState::Flying {
                        pos: origin,
                        vel: aim * CLAW_SPEED,
                        travelled: 0.0,
                    });
                }
            }
            GrappleState::Flying {
                pos,
                vel,
                travelled,
            } => {
                if !input.held {
                    next = Some(GrappleState::Idle);
                    self.waiting_release = true;
                } else {
                    let step = *vel * dt;
                    let to = *pos + step;
                    let step_len = step.length();
                    // A hit along this step is judged by where the
                    // claw meets the surface, not by where the step
                    // would have ended. The old order added the full
                    // step first and then refused the strike whenever
                    // that endpoint crossed MAX_LINE, so a wall a few
                    // centimetres inside the line was a miss.
                    if let Some(hit) = world.segment_hit(*pos, to) {
                        let arrive = *travelled + hit.dist;
                        if arrive > MAX_LINE {
                            let used = (MAX_LINE - *travelled).max(0.0);
                            let at = *pos + (to - *pos) * (used / step_len.max(1e-6));
                            events.push(
                                tick,
                                StimulusKind::StrikeFail,
                                at,
                                SurfaceRecord::dry(SurfaceKind::Metal),
                                0.5,
                            );
                            next = Some(GrappleState::Idle);
                            self.waiting_release = true;
                        } else if hit.surface.biteable() {
                            *travelled = arrive;
                            // Latch: stop dead, impact sound at
                            // the strike point, no slide, no bounce.
                            events.push(tick, StimulusKind::Strike, hit.point, hit.surface, 1.0);
                            let len = (hit.point - origin).length().max(MIN_ROPE);
                            let mut rope = Vec::with_capacity(ROPE_POINTS);
                            let mut prev = Vec::with_capacity(ROPE_POINTS);
                            let attach = body.feet + Vec3::new(0.0, ATTACH_HEIGHT, 0.0);
                            for i in 0..ROPE_POINTS {
                                let t = i as f32 / (ROPE_POINTS - 1) as f32;
                                let p = hit.point.lerp(attach, t);
                                rope.push(p);
                                prev.push(p);
                            }
                            next = Some(GrappleState::Latched {
                                anchor: Anchor::Static(hit.point),
                                rope,
                                prev,
                                len,
                                contact: None,
                            });
                        } else {
                            // Sky / unbiteable: removed, not latched.
                            events.push(
                                tick,
                                StimulusKind::StrikeFail,
                                hit.point,
                                hit.surface,
                                0.5,
                            );
                            next = Some(GrappleState::Idle);
                            self.waiting_release = true;
                        }
                    } else {
                        *travelled += step_len;
                        if *travelled >= MAX_LINE {
                            events.push(
                                tick,
                                StimulusKind::StrikeFail,
                                *pos,
                                SurfaceRecord::dry(SurfaceKind::Metal),
                                0.5,
                            );
                            next = Some(GrappleState::Idle);
                            self.waiting_release = true;
                        } else {
                            *pos = to;
                        }
                    }
                }
            }
            GrappleState::Latched {
                anchor,
                rope,
                prev,
                len,
                contact,
            } => {
                if !input.held {
                    next = Some(GrappleState::Idle);
                    self.waiting_release = true;
                } else {
                    // Reel: hold shortens, crouch stops the reel
                    // (the swing continues regardless).
                    if !input.crouch {
                        *len = (*len - REEL_SPEED * dt).max(MIN_ROPE);
                    }
                    let anchor_pos = anchor.pos();
                    let attach = body.feet + Vec3::new(0.0, ATTACH_HEIGHT, 0.0);
                    // Verlet integrate interior points.
                    for i in 1..ROPE_POINTS - 1 {
                        let p = rope[i];
                        let pp = prev[i];
                        let mut np = p + (p - pp) * (1.0 - ROPE_DAMP * dt);
                        np.y -= 9.81 * dt * dt;
                        prev[i] = p;
                        rope[i] = np;
                    }
                    // Pin the ends.
                    rope[0] = anchor_pos;
                    prev[0] = anchor_pos;
                    rope[ROPE_POINTS - 1] = attach;
                    prev[ROPE_POINTS - 1] = attach;
                    // Distance constraints, then geometry push-out
                    // (the drape: points rest on edges instead of
                    // passing through them).
                    let rest = *len / (ROPE_POINTS - 1) as f32;
                    let mut new_contact: Option<Vec3> = None;
                    for _ in 0..ROPE_ITERS {
                        for i in 0..ROPE_POINTS - 1 {
                            let a = rope[i];
                            let b = rope[i + 1];
                            let d = b - a;
                            let dist = d.length();
                            if dist < 1e-6 {
                                continue;
                            }
                            let diff = (dist - rest) / dist;
                            // A pinned end cannot move, so the free
                            // end has to take the whole correction.
                            // Splitting 0.5/0.5 against a pin leaves
                            // half the error in place every iteration.
                            let pin0 = i == 0;
                            let pin1 = i + 1 == ROPE_POINTS - 1;
                            let w0 = if pin0 {
                                0.0
                            } else if pin1 {
                                1.0
                            } else {
                                0.5
                            };
                            let w1 = if pin1 {
                                0.0
                            } else if pin0 {
                                1.0
                            } else {
                                0.5
                            };
                            rope[i] += d * diff * w0;
                            rope[i + 1] -= d * diff * w1;
                        }
                        for i in 1..ROPE_POINTS - 1 {
                            let p = rope[i];
                            let mut pushed = p;
                            for b in &world.boxes {
                                let cp = b.closest(p);
                                let delta = p - cp;
                                let d = delta.length();
                                if d < CAPSULE_RADIUS * 0.5 {
                                    if d > 1e-6 {
                                        pushed = cp + delta * ((CAPSULE_RADIUS * 0.5) / d);
                                    } else {
                                        pushed.y = b.max.y + CAPSULE_RADIUS * 0.5;
                                    }
                                }
                            }
                            if pushed != p {
                                new_contact = Some(pushed);
                            }
                            rope[i] = pushed;
                        }
                        rope[0] = anchor_pos;
                        rope[ROPE_POINTS - 1] = attach;
                    }
                    *contact = new_contact;
                    // Rubber-band pull on the capsule, from the
                    // CURRENT pivot (drape contact when set, else
                    // the anchor) TOWARD the pivot: the rope
                    // pulls the body in, never pushes it away.
                    // Overstretch clamps the length and applies
                    // the stronger force.
                    let pivot = contact.unwrap_or(anchor_pos);
                    let to_body = attach - pivot;
                    let dist = to_body.length();
                    if dist > *len {
                        let pull = dist - *len;
                        let pull_dir = (pivot - attach) / dist;
                        let mut acc = RUBBER_K * pull;
                        if pull > STRETCH_ALLOW {
                            acc += OVERSTRETCH_K * (pull - STRETCH_ALLOW);
                            *len = dist - STRETCH_ALLOW;
                        }
                        body.vel += pull_dir * acc * dt;
                        body.vel *= 1.0 - ROPE_DAMP * dt;
                    }
                }
            }
        }
        if let Some(n) = next {
            state = n;
        }
        self.state = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traversal::events::EventLog;
    use crate::traversal::physics::{Body, BoxCollider, StaticWorld, FIXED_DT};
    use crate::traversal::surface::{SurfaceKind, SurfaceRecord};

    fn hold() -> HookInput {
        HookInput {
            press: false,
            held: true,
            crouch: false,
        }
    }

    #[test]
    fn strike_inside_max_line_latches_when_the_step_would_overshoot() {
        let mut world = StaticWorld::new();
        // Just inside the 40 m line. One claw step is 40/240 m,
        // so the step that reaches this wall ends past MAX_LINE.
        world.add(BoxCollider::new(
            Vec3::new(-1.0, 0.0, 39.90),
            Vec3::new(1.0, 4.0, 40.20),
            SurfaceRecord::dry(SurfaceKind::Brick),
        ));
        let mut g = Grapple::new();
        let mut body = Body::new(Vec3::ZERO);
        let mut events = EventLog::new();
        let origin = Vec3::new(0.0, 1.4, 0.0);
        g.update(
            FIXED_DT,
            HookInput {
                press: true,
                held: true,
                crouch: false,
            },
            origin,
            Vec3::Z,
            &mut body,
            &world,
            &mut events,
            1,
        );
        for t in 2..600 {
            g.update(
                FIXED_DT,
                hold(),
                origin,
                Vec3::Z,
                &mut body,
                &world,
                &mut events,
                t,
            );
            if g.is_latched() {
                break;
            }
        }
        assert!(
            g.is_latched(),
            "a wall inside MAX_LINE must latch, not fail because the step endpoint is past it"
        );
    }

    #[test]
    fn latched_segments_stay_near_rest_length() {
        let mut world = StaticWorld::new();
        world.add(BoxCollider::new(
            Vec3::new(-1.0, 0.0, 6.0),
            Vec3::new(1.0, 4.0, 6.4),
            SurfaceRecord::dry(SurfaceKind::Brick),
        ));
        let mut g = Grapple::new();
        let mut body = Body::new(Vec3::ZERO);
        let mut events = EventLog::new();
        let origin = Vec3::new(0.0, 1.4, 0.0);
        g.update(
            FIXED_DT,
            HookInput {
                press: true,
                held: true,
                crouch: false,
            },
            origin,
            Vec3::Z,
            &mut body,
            &world,
            &mut events,
            1,
        );
        for t in 2..200 {
            g.update(
                FIXED_DT,
                hold(),
                origin,
                Vec3::Z,
                &mut body,
                &world,
                &mut events,
                t,
            );
            if g.is_latched() {
                break;
            }
        }
        assert!(g.is_latched());
        // Hold the body where it latched and let gravity sag the
        // chain. The pins are a satisfiable distance apart, so
        // every segment can sit on the rest length. Pulling the
        // body past `len` cannot: the line is shorter than the
        // span on purpose, and the segments must stretch.
        for t in 0..12 {
            let feet = body.feet;
            g.update(
                FIXED_DT,
                hold(),
                origin,
                Vec3::Z,
                &mut body,
                &world,
                &mut events,
                t,
            );
            body.feet = feet;
            body.vel = Vec3::ZERO;
        }
        let GrappleState::Latched { rope, len, .. } = &g.state else {
            panic!("still latched");
        };
        let rest = len / (ROPE_POINTS - 1) as f32;
        let mut worst = 0.0f32;
        for w in rope.windows(2) {
            let err = (w[1] - w[0]).length() - rest;
            worst = worst.max(err.abs());
        }
        assert!(
            worst < rest * 0.08,
            "segment error {worst} against rest {rest}"
        );
    }
}
