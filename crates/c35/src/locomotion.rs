//! Procedural locomotion and the movement model (M7).
//!
//! DeBrief's doctrine (DESIGN §4) is fast stealth: a professional at
//! full tempo — weight in the acceleration and the turns, none in
//! the top speed. This module is that doctrine as code, in two
//! layers that meet at the skeleton's pose interface
//! ([`crate::operator::PoseFrame`]) and nowhere else:
//!
//! * [`Animator`] turns a movement state (position, facing, speed,
//!   crouch / aim / slide blends) into a pose frame. The leg cycles
//!   are generated, not authored: each foot's trajectory is a
//!   function of speed, and the stance foot is *planted* — its world
//!   anchor is fixed at contact and the leg solves to it, so the
//!   foot's ground speed in stance is zero by construction, not by
//!   tuning. Legs are two-bone IK over the skeleton's own segment
//!   lengths; arms either swing counter-phase (uncarried) or hold
//!   the USP ready position with the aim pose blended over it.
//! * [`Controller`] is the movement model proper: a waypoint script
//!   driven through gait targets with acceleration, braking and
//!   turn-rate limits per gait. The headless host and the movement
//!   probe drive the same controller, so the numbers in the report
//!   are the numbers in the frames.
//!
//! Nothing here knows the operator mesh exists. Joint transforms
//! in, pose frames out — the mesh is a consumer, not a dependency.
//!
//! ## Pose-graph vocabulary (design-adopted, ENGINE-RESEARCH §10)
//!
//! The composition below is a small **pose graph** in Fyrox's
//! `fyrox-animation` sense, and is named that way on purpose, while
//! it still has only a handful of poses to grow from:
//!
//! * **Nodes** are pose sources: the staged poses (stand-ready,
//!   crouch, aim — clip-equivalents), and the locomotion node,
//!   which is a *procedural* pose source (the cycle + IK) rather
//!   than a clip.
//! * **Blends** are weighted pose mixtures (`BlendAnimations`):
//!   the gait blend by `gait_mix`, the rest-crouch blend, the
//!   slide blend — each a node whose weight is a rule parameter
//!   (speed, crouch input, the slide state).
//! * **Layers with masks**: the aim blend is an upper-body layer
//!   evaluated over [`AIM_MASK`] alone, so the legs keep whatever
//!   the locomotion node says while the torso, head and arms go to
//!   the weapon — aim-while-running is a layer weight, not a new
//!   pose. Future layers (a grapple layer over locomotion, a
//!   low-ceiling crouch cap) enter as new masked layers, not as
//!   rewrites.
//! * **States and transitions**: stand / locomote / crouch / slide
//!   are states; the blend chase rates in [`Animator::update`] are
//!   their transition durations, and the slide's entry/exit speeds
//!   are transition rules.
//! * **Signals**: the exact substep a foot plants is a graph event.
//!   The animator already detects it (the plant is where the
//!   anchor is set); when the perception model lands, footstep
//!   signals are emitted from there, so the operator is heard by
//!   the same rules he hears with.
//!
//! Design-adopt only: no Fyrox code or dependency enters the tree
//! (the zero-dependency law); the vocabulary and the layering
//! discipline are what is taken.

use glam::{Vec2, Vec3};

use crate::operator::{Bone, OperatorPose, PoseFrame};
use crate::scene::height_at;

// ---------------------------------------------------------------------
// Gait parameters
// ---------------------------------------------------------------------

/// Reference speeds (m/s) for the driven gaits — the doctrine's
/// numbers: a walk you can think at, a run that crosses ground, a
/// sprint that spends the man.
pub const WALK_SPEED: f32 = 1.5;
pub const RUN_SPEED: f32 = 3.4;
pub const SPRINT_SPEED: f32 = 5.8;
pub const CROUCH_SPEED: f32 = 1.1;

/// The skeleton's leg geometry, from the bind pivots: thigh
/// (hip 0.88 → knee 0.53), shin (knee 0.53 → ankle 0.15), and the
/// ankle's height over the sole plane.
pub const THIGH_LEN: f32 = 0.35;
pub const SHIN_LEN: f32 = 0.38;
pub const ANKLE_HEIGHT: f32 = 0.15;
/// Half the hip width: thigh pivots sit at x = ±0.11.
pub const HIP_HALF: f32 = 0.11;

/// Gait anchors: (speed, step length, duty factor, swing clearance,
/// ride drop, bob rise). Step length is one footfall to the next
/// (opposite feet); a full cycle is two steps. Duty is the fraction
/// of the cycle a foot spends planted; clearance is the swing
/// foot's apex over the ground line.
///
/// The step lengths are sized by the SKELETON, not by human tables:
/// the leg's reach is 0.73 m (thigh 0.35 + shin 0.38) and the bind
/// pose stands at full extension, so a plant at reach R ahead of
/// the hip is only reachable once the pelvis has ridden down by
/// 0.73 − √(0.73² − R²). The ride drop column is exactly that
/// geometry (plus a knee-bend reserve), and the bob rise is its
/// complement — the root rides back up toward mid-stance, where
/// the planted leg is vertical and can afford the height. The
/// probe's first run convicted the human-scaled table on this:
/// plants were placed beyond the leg, and the IK clamp spent the
/// difference as foot skate.
const GAIT_ANCHORS: [(f32, f32, f32, f32, f32, f32); 5] = [
    (0.0, 0.26, 0.66, 0.030, 0.030, 0.000),
    (WALK_SPEED, 0.60, 0.60, 0.050, 0.105, 0.040),
    (RUN_SPEED, 0.86, 0.44, 0.110, 0.115, 0.038),
    (SPRINT_SPEED, 1.24, 0.37, 0.170, 0.175, 0.050),
    (7.5, 1.38, 0.35, 0.200, 0.190, 0.055),
];

/// (step length, duty factor, clearance, ride drop, bob rise) for a
/// ground speed, with the crouch blend (0..1) folding the gait
/// down: shorter steps, longer plants, lower swings.
pub fn gait_params(speed: f32, crouch: f32) -> (f32, f32, f32, f32, f32) {
    let v = speed.max(0.0);
    let mut out = GAIT_ANCHORS[GAIT_ANCHORS.len() - 1];
    for w in GAIT_ANCHORS.windows(2) {
        let (a, b) = (w[0], w[1]);
        if v <= b.0 {
            let t = if b.0 > a.0 {
                (v - a.0) / (b.0 - a.0)
            } else {
                0.0
            };
            out = (
                a.0,
                a.1 + (b.1 - a.1) * t,
                a.2 + (b.2 - a.2) * t,
                a.3 + (b.3 - a.3) * t,
                a.4 + (b.4 - a.4) * t,
                a.5 + (b.5 - a.5) * t,
            );
            break;
        }
    }
    let (_, step, duty, clearance, drop, bob) = out;
    let c = crouch.clamp(0.0, 1.0);
    (
        step * (1.0 - 0.40 * c),
        duty.max(duty + (0.64 - duty) * c),
        clearance * (1.0 - 0.45 * c),
        drop,
        bob,
    )
}

/// Cadence (steps per second) a speed and step length imply — the
/// identity the no-skate proof hangs on: the animator advances the
/// cycle phase at exactly this rate, so stride × cadence == speed
/// holds instantaneously, and the planted foot makes it physical.
pub fn cadence(speed: f32, step: f32) -> f32 {
    if step > 0.01 {
        speed / step
    } else {
        0.0
    }
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp01((x - a) / (b - a));
    t * t * (3.0 - 2.0 * t)
}

fn wrap_pi(a: f32) -> f32 {
    let mut a = a;
    while a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    }
    while a < -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a
}

/// Facing basis for a yaw (radians, the root matrix's convention):
/// forward = (sin ψ, cos ψ) in world (x, z); his-left = (cos ψ, −sin ψ).
pub fn facing(yaw: f32) -> (Vec2, Vec2) {
    let (s, c) = yaw.sin_cos();
    (Vec2::new(s, c), Vec2::new(c, -s))
}

// ---------------------------------------------------------------------
// The animator
// ---------------------------------------------------------------------

/// One frame of movement intent, fed to [`Animator::update`].
#[derive(Copy, Clone, Debug)]
pub struct AnimInput {
    /// Root position (x, z used; the ground is sampled, y ignored).
    pub pos: Vec3,
    /// Facing, radians (the root matrix's yaw convention).
    pub yaw: f32,
    /// Current ground speed (m/s, >= 0).
    pub speed: f32,
    /// Crouch blend target, 0..1.
    pub crouch: f32,
    /// Aim blend target, 0..1 (upper body toward the Aim pose).
    pub aim: f32,
    /// Slide blend target, 0..1.
    pub slide: f32,
    /// The USP is in hand: arms hold the ready carry instead of
    /// swinging.
    pub carry: bool,
}

/// Per-leg animation state: the planted anchor while in stance.
#[derive(Copy, Clone)]
struct LegAnim {
    plant: Vec3,
    /// Raw terrain height under the plant, recorded at plant time:
    /// while this leg supports, the root's ground reference is
    /// drawn up toward it — the body rises onto its step.
    plant_ground: f32,
    contact: bool,
    init: bool,
}

impl LegAnim {
    fn new() -> LegAnim {
        LegAnim {
            plant: Vec3::ZERO,
            plant_ground: 0.0,
            contact: true,
            init: false,
        }
    }
}

/// The procedural animator. Stateful only in the ways feet are
/// stateful: the cycle phase, where each foot is planted, and the
/// ground reference the figure stands on (smoothed — see
/// [`Animator::update`]).
pub struct Animator {
    phase: f32,
    step_events: [Option<Footstep>; 2],
    legs: [LegAnim; 2],
    aim: f32,
    crouch: f32,
    slide: f32,
    targets: [Vec3; 2],
    /// Tracked leg-plane yaws (radians, pelvis space) — state, not
    /// a per-frame derivation; see the leg loop for the reason.
    leg_yaw: [f32; 2],
    ground_y: f32,
    ground_init: bool,
    /// The ground the figure walks on. None (the default) is the
    /// vista's island heightfield — the behaviour every proof to
    /// date measured. A host walking a figure somewhere the island
    /// is not (DeBrief's flat street) installs its own ground
    /// function through `set_ground_fn`; the solver is unchanged.
    ground_fn: Option<fn(f32, f32) -> f32>,
}

/// A foot entering stance, emitted where the foot is planted. Surface
/// classification and playback belong to the host's shared world query.
#[derive(Copy, Clone, Debug)]
pub struct Footstep {
    pub left: bool,
    pub position: Vec3,
    pub speed: f32,
    pub crouch: f32,
}

impl Default for Animator {
    fn default() -> Self {
        Self::new()
    }
}

impl Animator {
    pub fn new() -> Animator {
        Animator {
            phase: 0.0,
            step_events: [None; 2],
            legs: [LegAnim::new(), LegAnim::new()],
            aim: 0.0,
            crouch: 0.0,
            slide: 0.0,
            targets: [Vec3::ZERO; 2],
            leg_yaw: [0.0, 0.0],
            ground_y: 0.0,
            ground_init: false,
            ground_fn: None,
        }
    }

    /// Install a ground function (see the field's note). Passing
    /// None restores the island heightfield.
    pub fn set_ground_fn(&mut self, f: Option<fn(f32, f32) -> f32>) {
        self.ground_fn = f;
    }

    /// The animator's smoothed ground reference — the height the
    /// figure's root stands on. Callers placing the mesh
    /// (`build_operator_frame`) must use this, not a raw terrain
    /// sample, or the drawn figure and the solved figure disagree.
    pub fn ground_y(&self) -> f32 {
        self.ground_y
    }

    /// The cycle phase (radians; 0 = left contact, π = right).
    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Contact events emitted by the last update only. Consume after
    /// each simulation tick; no queue, allocation, or duplicate stance events.
    pub fn footsteps(&self) -> impl Iterator<Item = &Footstep> {
        self.step_events.iter().flatten()
    }

    /// Whether a leg is in its stance interval this frame.
    pub fn contact(&self, left: bool) -> bool {
        self.legs[if left { 0 } else { 1 }].contact
    }

    /// The ankle targets (world space) the last update solved to —
    /// the measurement seam for the foot-skate proof.
    pub fn ankle_targets(&self) -> [Vec3; 2] {
        self.targets
    }

    fn leg_phase(&self, left: bool) -> f32 {
        if left {
            self.phase
        } else {
            (self.phase + std::f32::consts::PI) % std::f32::consts::TAU
        }
    }

    /// Advance the animation by `dt` and produce the pose frame.
    pub fn update(&mut self, dt: f32, input: &AnimInput) -> PoseFrame {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        self.step_events = [None; 2];
        // The ground sample, dispatched once into a local: the leg
        // loop below holds &mut borrows into self, so sampling must
        // not borrow self (the island's height_at never did).
        let ground_fn = self.ground_fn;
        let ground = |x: f32, z: f32| -> f32 {
            match ground_fn {
                Some(f) => f(x, z),
                None => height_at(x, z),
            }
        };
        // Blend states chase their targets at professional rates:
        // aim snaps in a sixth of a second, the slide commits faster
        // than it releases.
        let chase = |cur: f32, tgt: f32, rate: f32| -> f32 {
            let d = (tgt.clamp(0.0, 1.0) - cur).clamp(-rate * dt, rate * dt);
            cur + d
        };
        self.aim = chase(self.aim, input.aim, 6.0);
        self.crouch = chase(self.crouch, input.crouch, 7.0);
        self.slide = chase(
            self.slide,
            input.slide,
            if input.slide > self.slide { 10.0 } else { 5.0 },
        );
        let crouch = self.crouch;
        let slide = self.slide;

        let speed = input.speed.max(0.0);
        let (step, duty, clearance, ride_drop, bob_amp) = gait_params(speed, crouch);

        // Cycle phase advances at the cadence the speed implies —
        // the identity stride × cadence == speed, integrated. The
        // slide freezes the cycle: a sliding man is not stepping.
        if slide < 0.5 {
            self.phase =
                (self.phase + std::f32::consts::PI * cadence(speed, step) * dt)
                    % std::f32::consts::TAU;
        }
        let phase = self.phase;
        let gait_mix = smoothstep(0.08, 0.65, speed);
        // The ground reference SLEWS toward the terrain sample at a
        // capped vertical rate. The vista's height function carries
        // a step ring (the M1 shore shelf rescales heights inside
        // |h| < 2.2 discontinuously — a ~0.84 m ledge in the
        // function, bridged by the terrain mesh as a steep ramp);
        // a root that samples it raw teleports vertically on the
        // crossing frame. Slewing turns the crossing into a step-up
        // the legs absorb. Foot plants always use the RAW sample at
        // the plant point (clamped to the leg's reach, below) —
        // feet stand on the ground as it is; only the body's ride
        // height is smoothed.
        // While a leg supports on higher ground, the reference is
        // drawn toward THAT plant's ground: the body rises onto its
        // step during the stance that made it, instead of folding
        // double under a plant left above the hip (the probe's
        // ring-crossing fold, shin at +170°).
        let mut ground_target = ground(input.pos.x, input.pos.z);
        for leg in &self.legs {
            if leg.contact && leg.plant_ground > ground_target {
                ground_target = leg.plant_ground;
            }
        }
        if !self.ground_init {
            self.ground_y = ground_target;
            self.ground_init = true;
        } else {
            let max_step = 2.2 * dt;
            self.ground_y += (ground_target - self.ground_y).clamp(-max_step, max_step);
        }
        let ground_y = self.ground_y;
        let (fwd, left_dir) = facing(input.yaw);

        // ---- root height: the pelvis rides DOWN by the gait's
        // ride drop (the reach geometry in the anchor table) and
        // rises back through each mid-stance — sin²(phase) peaks
        // between the two contacts of a cycle, which is where a
        // vertical leg can carry the height. Crouch and slide drop
        // him further on top. ----
        let bob = bob_amp * (1.0 - 0.5 * crouch) * phase.sin().powi(2) * gait_mix;
        let root_y = -0.01 - ride_drop * gait_mix + bob - 0.42 * crouch - 0.14 * slide;
        let origin = Vec3::new(input.pos.x, ground_y + root_y, input.pos.z);

        // ---- ankle targets (world) ----
        // Reach clamp: an ankle target is clamped in DISTANCE from
        // the hip — pulled along the hip→target line to at most
        // 0.70 m (the leg reaches 0.73; the margin is the knee's
        // reserve). A height-only clamp was the probe's third
        // conviction: it let a plant sit 0.70 BELOW the hip and
        // 0.36 ahead of it — 0.79 away, unreachable — and the
        // planted ankle hovered, converging only as the root
        // advanced. On smooth ground the clamp never engages;
        // across the height function's step ring it is the
        // difference between a foot that plants stiffly at the
        // ledge and a solver inverting its knee chasing an
        // impossible point.
        let hip_world = Vec3::new(input.pos.x, origin.y + 0.88, input.pos.z);
        let clamp_reach = |p: Vec3| -> Vec3 {
            let rel = p - hip_world;
            let d = rel.length();
            if d > 0.70 {
                hip_world + rel * (0.70 / d)
            } else {
                p
            }
        };
        let mut targets = [Vec3::ZERO; 2];
        for (i, left) in [true, false].into_iter().enumerate() {
            let lp = self.leg_phase(left);
            let stance_end = std::f32::consts::TAU * duty;
            let in_contact = lp < stance_end && speed > 0.02 && slide < 0.5;
            let hip_lat = if left { HIP_HALF } else { -HIP_HALF };
            let leg = &mut self.legs[i];
            if !leg.init {
                // First sight of this leg: plant it under the hip.
                let p = input.pos + Vec3::new(left_dir.x * hip_lat, 0.0, left_dir.y * hip_lat);
                leg.plant_ground = ground(p.x, p.z);
                leg.plant = clamp_reach(Vec3::new(p.x, leg.plant_ground + ANKLE_HEIGHT, p.z));
                leg.init = true;
            }
            if in_contact && !leg.contact {
                // Contact begins: plant ahead of the root by the
                // stance reach — from here the anchor is FIXED until
                // liftoff, which is the whole no-skate mechanism.
                let reach = step * duty;
                let p = input.pos
                    + Vec3::new(
                        fwd.x * reach + left_dir.x * hip_lat,
                        0.0,
                        fwd.y * reach + left_dir.y * hip_lat,
                    );
                leg.plant_ground = ground(p.x, p.z);
                leg.plant = clamp_reach(Vec3::new(p.x, leg.plant_ground + ANKLE_HEIGHT, p.z));
                if dt > 0.0 {
                    self.step_events[i] = Some(Footstep {
                        left, position: Vec3::new(p.x, leg.plant_ground, p.z),
                        speed, crouch: self.crouch,
                    });
                }
            }
            leg.contact = in_contact;
            let target = if in_contact || slide >= 0.5 {
                leg.plant
            } else {
                // Swing: the ankle rides a phase-relative trajectory
                // in the root's frame — from the stance reach behind
                // (−step·duty) to the reach ahead (+step·duty) — on
                // an ease tuned so mid-swing the foot passes close
                // under the hip (ease(½) = 0.40), folded, the way a
                // real leg carries it; a ground-track lerp would
                // drag it straight and out of the leg's reach. The
                // target is recomputed from the CURRENT root every
                // frame, so at contact it coincides exactly with the
                // anchor the stance is about to fix: no step, no pop.
                let u = if stance_end < std::f32::consts::TAU {
                    clamp01((lp - stance_end) / (std::f32::consts::TAU - stance_end))
                } else {
                    0.0
                };
                let ease = -1.2 * u * u * u + 2.2 * u * u;
                let rel = step * duty * (2.0 * ease - 1.0);
                let p = input.pos
                    + Vec3::new(
                        fwd.x * rel + left_dir.x * hip_lat,
                        0.0,
                        fwd.y * rel + left_dir.y * hip_lat,
                    );
                clamp_reach(Vec3::new(
                    p.x,
                    ground(p.x, p.z)
                        + ANKLE_HEIGHT
                        + clearance * (std::f32::consts::PI * u).sin(),
                    p.z,
                ))
            };
            targets[i] = target;
        }
        // The slide overrides both targets with its shape: lead leg
        // extended and planted ahead, trail leg folded under.
        if slide > 0.001 {
            let mk = |fwd_d: f32, lat: f32, lift: f32| -> Vec3 {
                let p = input.pos
                    + Vec3::new(fwd.x * fwd_d + left_dir.x * lat, 0.0, fwd.y * fwd_d + left_dir.y * lat);
                clamp_reach(Vec3::new(p.x, ground(p.x, p.z) + ANKLE_HEIGHT + lift, p.z))
            };
            let lead_t = mk(0.66, HIP_HALF, 0.0);
            let trail_t = mk(-0.36, -HIP_HALF, 0.10);
            targets[0] = targets[0].lerp(lead_t, slide);
            targets[1] = targets[1].lerp(trail_t, slide);
        }
        self.targets = targets;

        // ---- compose the frame ----
        let stand = PoseFrame::from_pose(OperatorPose::StandReady);
        let mut frame = stand;

        // Pelvis / torso / head: lean with speed, sway and counter-
        // rotation off the cycle, gaze held level against the lean.
        let lean = (1.2 + 1.35 * speed).min(9.0) * gait_mix + 5.0 * crouch - 13.0 * slide;
        let sway = 2.2 * gait_mix * (1.0 - 0.6 * crouch);
        let counter = (3.0 + 0.6 * speed).min(6.5) * gait_mix;
        frame.add(Bone::Pelvis, Vec3::new(lean * 0.55, counter * phase.sin(), -sway * phase.cos()));
        frame.add(
            Bone::Torso,
            Vec3::new(lean * 0.45 + 10.0 * crouch - 8.0 * slide, -counter * 0.8 * phase.sin(), sway * 0.35 * phase.cos()),
        );
        frame.add(Bone::Head, Vec3::new(-lean * 0.62, 0.0, 0.0));

        // Arms. Carried: the ready position from the staged pose,
        // riding the bob, muzzle steady. Uncarried: counter-phase
        // swing against the opposite leg.
        if input.carry {
            let ride = 1.6 * gait_mix * (2.0 * phase).sin() * (1.0 - self.aim);
            frame.add(Bone::UpperArmR, Vec3::new(ride, 0.0, 0.0));
            frame.add(Bone::UpperArmL, Vec3::new(ride * 0.85, 0.0, 0.0));
            frame.add(Bone::ForearmR, Vec3::new(ride * 0.4, 0.0, 0.0));
            frame.add(Bone::ForearmL, Vec3::new(ride * 0.4, 0.0, 0.0));
        } else {
            let amp = (10.0 + 2.4 * speed).min(24.0) * gait_mix;
            // Left arm answers the right leg (phase + π), and back.
            let swing_l = -amp * phase.cos();
            let swing_r = amp * phase.cos();
            frame.add(Bone::UpperArmL, Vec3::new(swing_l - 8.0, 0.0, 4.0));
            frame.add(Bone::UpperArmR, Vec3::new(swing_r - 8.0, 0.0, -4.0));
            frame.add(Bone::ForearmL, Vec3::new(-6.0 - 7.0 * (1.0 + phase.cos()) * 0.5 * gait_mix, 0.0, 0.0));
            frame.add(Bone::ForearmR, Vec3::new(-6.0 - 7.0 * (1.0 - phase.cos()) * 0.5 * gait_mix, 0.0, 0.0));
            frame.add(Bone::HandL, Vec3::new(4.0, 0.0, 0.0));
            frame.add(Bone::HandR, Vec3::new(4.0, 0.0, 0.0));
        }

        // The aim LAYER: an upper-body masked blend toward the
        // staged aim pose (see AIM_MASK).
        if self.aim > 0.001 {
            let aim_frame = PoseFrame::from_pose(OperatorPose::Aim);
            for b in AIM_MASK {
                let cur = frame.get(b);
                frame.set(b, cur.lerp(aim_frame.get(b), self.aim));
            }
        }

        // At rest in a crouch, settle into the staged crouch shape;
        // the IK below takes over as the gait mixes in.
        let rest_crouch = crouch * (1.0 - gait_mix) * (1.0 - slide);
        if rest_crouch > 0.001 {
            let crouch_frame = PoseFrame::from_pose(OperatorPose::Crouch);
            frame = frame.lerp(&crouch_frame, rest_crouch * 0.92);
        }
        frame.root_y = root_y + stand.root_y * (1.0 - gait_mix.max(slide));

        // ---- legs by IK, weighted over the composed base ----
        let leg_weight = gait_mix.max(slide).max(crouch * gait_mix);
        if leg_weight > 0.001 {
            // The solve runs in PELVIS space: a leg bone's angles
            // live in its parent's frame, so the ankle target is
            // carried through the pelvis's own matrix (lean, sway
            // and counter-rotation included) and solved against the
            // bind hip. Solving in figure space instead leaves the
            // pelvis's rotation unaccounted and the ankle lands a
            // pelvis-rotation away from its target — the probe
            // measured a constant ~5 cm (the 4.8° pelvis pitch over
            // a 0.63 m leg) before this was stated in the right
            // space.
            let mats = crate::operator::bone_matrices(&frame);
            let pelvis_inv = mats[Bone::Pelvis.idx()].inverse();
            let (sy, cy) = input.yaw.sin_cos();
            for (i, left) in [true, false].into_iter().enumerate() {
                let (thigh, shin, foot) = if left {
                    (Bone::ThighL, Bone::ShinL, Bone::FootL)
                } else {
                    (Bone::ThighR, Bone::ShinR, Bone::FootR)
                };
                let hip = thigh.pivot();
                // World ankle target → figure space (inverse root:
                // translate, then rotate by −yaw about Y), then into
                // pelvis space through the pelvis matrix inverse.
                let d = targets[i] - origin;
                let fig = Vec3::new(d.x * cy - d.z * sy, d.y, d.x * sy + d.z * cy);
                // The leg-plane yaw is a tracked STATE, not a
                // per-frame atan2 of the target: as the ankle passes
                // under the hip the target's azimuth is undefined
                // (forward distance → 0) and a pointwise derivation
                // flips ±90° in one substep, spinning the boot around
                // its ankle — the probe measured that flip as the
                // dominant remaining sole speed. The desired yaw is
                // therefore computed in pelvis space (so the pelvis's
                // own roll/yaw mixing is part of the direction, not
                // an error term) from a forward distance clamped
                // away from zero, and the state chases it fast;
                // through the degenerate instant it simply holds
                // its line, which is what a planted foot does.
                let local = pelvis_inv.transform_point3(fig);
                let rel_p = local - hip;
                let pf = -rel_p.z;
                let pf_c = pf.signum() * pf.abs().max(0.18);
                let mut desired = -rel_p.x.atan2(pf_c);
                if desired > std::f32::consts::FRAC_PI_2 {
                    desired -= std::f32::consts::PI;
                } else if desired < -std::f32::consts::FRAC_PI_2 {
                    desired += std::f32::consts::PI;
                }
                let cur = self.leg_yaw[i];
                let delta = (desired - cur + std::f32::consts::PI * 3.0)
                    % std::f32::consts::TAU
                    - std::f32::consts::PI;
                self.leg_yaw[i] = cur + delta.clamp(-12.0 * dt, 12.0 * dt);
                let solved = solve_leg(hip, local, self.leg_yaw[i]);
                let w = leg_weight;
                for (b, ang) in [(thigh, solved.0), (shin, solved.1), (foot, solved.2)] {
                    let cur = frame.get(b);
                    frame.set(b, cur.lerp(ang, w));
                }
                let _ = left;
            }
        }
        frame
    }
}

/// The aim layer's mask (pose-graph vocabulary, §10 of the engine
/// research): the bones the aim blend may touch. The legs are
/// never in it — locomotion owns them — which is what makes aim a
/// layer over the gait instead of a competing pose.
pub const AIM_MASK: [Bone; 8] = [
    Bone::Torso,
    Bone::Head,
    Bone::UpperArmL,
    Bone::ForearmL,
    Bone::HandL,
    Bone::UpperArmR,
    Bone::ForearmR,
    Bone::HandR,
];

/// Two-bone IK for one leg, in PELVIS space (the thigh's parent
/// frame — the space its angles actually live in), in the leg
/// plane the caller supplies: pitch thigh and shin by the law of
/// cosines with the knee forward, and level the foot. Returns
/// (thigh, shin, foot) Euler angles in degrees, in the skeleton's
/// convention.
///
/// The plane yaw is the caller's tracked state (see the leg loop):
/// deriving it per-frame from the target's azimuth is degenerate
/// exactly where the ankle passes under the hip. The plane
/// distance is the target's SIGNED projection onto the plane — a
/// foot behind the hip is a negative plane distance in the
/// figure's own sagittal plane, never a 180° yaw.
fn solve_leg(hip: Vec3, ankle: Vec3, leg_yaw: f32) -> (Vec3, Vec3, Vec3) {
    let rel = ankle - hip;
    let (syw, cyw) = leg_yaw.sin_cos();
    let plane = rel.x * syw + rel.z * cyw;
    // `down` is SIGNED: a plant above the hip (a ledge step-up
    // while the root's smoothed ground is still slewing up to it)
    // is a legitimate target, solved as a lunge, not clamped into
    // nonsense.
    let down = -rel.y;
    let d = (plane * plane + down * down).sqrt().clamp(0.05, (THIGH_LEN + SHIN_LEN) * 0.999);
    let ang = plane.atan2(down);
    let cos_beta = ((THIGH_LEN * THIGH_LEN + d * d - SHIN_LEN * SHIN_LEN)
        / (2.0 * THIGH_LEN * d))
        .clamp(-1.0, 1.0);
    let beta = cos_beta.acos();
    // Two circle intersections solve the same ankle; the knee is
    // the difference. Choose by the doctrine's rule — the knee
    // leads, forward-most in the leg plane — evaluated, not
    // assumed: below the hip that is ang + beta, above it (the
    // step-up lunge) ang − beta, and a fixed sign picks the
    // doubled-back fold in one of the two regimes (the probe's
    // shin at +170°: a leg folded against itself, reaching the
    // target with the wrong body).
    let tilt_a = ang + beta;
    let tilt_b = ang - beta;
    let thigh_tilt = if (THIGH_LEN * tilt_a.sin()) >= (THIGH_LEN * tilt_b.sin()) {
        tilt_a
    } else {
        tilt_b
    };
    // Knee position in the leg plane, then the shin's own tilt from
    // the knee→ankle vector (no sign is trusted that a vector can
    // state directly).
    let knee_fwd = THIGH_LEN * thigh_tilt.sin();
    let knee_down = THIGH_LEN * thigh_tilt.cos();
    let shin_tilt = (plane - knee_fwd).atan2(down - knee_down);
    let thigh = Vec3::new(-thigh_tilt.to_degrees(), leg_yaw.to_degrees(), 0.0);
    let shin = Vec3::new((thigh_tilt - shin_tilt).to_degrees(), 0.0, 0.0);
    // Foot: level sole (its x cancels the chain's), yawed back to
    // the figure's forward so a yawed leg doesn't toe the boot out.
    let foot = Vec3::new(
        -(thigh.x + shin.x),
        -leg_yaw.to_degrees(),
        0.0,
    );
    (thigh, shin, foot)
}

// ---------------------------------------------------------------------
// The movement model: a scripted controller with weighted gaits
// ---------------------------------------------------------------------

/// A driven gait. The controller's vocabulary — the animator only
/// ever sees speed and blends.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Gait {
    Stand,
    Walk,
    Run,
    Sprint,
    CrouchWalk,
}

impl Gait {
    pub fn name(self) -> &'static str {
        match self {
            Gait::Stand => "stand",
            Gait::Walk => "walk",
            Gait::Run => "run",
            Gait::Sprint => "sprint",
            Gait::CrouchWalk => "crouch-walk",
        }
    }
    pub fn target_speed(self) -> f32 {
        match self {
            Gait::Stand => 0.0,
            Gait::Walk => WALK_SPEED,
            Gait::Run => RUN_SPEED,
            Gait::Sprint => SPRINT_SPEED,
            Gait::CrouchWalk => CROUCH_SPEED,
        }
    }
    /// Acceleration toward a higher speed (m/s²): the professional
    /// surge — brisk, not explosive.
    fn accel(self) -> f32 {
        match self {
            Gait::Stand => 3.0,
            Gait::Walk => 3.2,
            Gait::Run => 3.8,
            Gait::Sprint => 3.4,
            Gait::CrouchWalk => 2.6,
        }
    }
    /// Turn rate (rad/s) the gait carries: speed bleeds agility.
    fn turn_rate(self) -> f32 {
        match self {
            Gait::Stand => 2.6,
            Gait::Walk => 1.75,
            Gait::Run => 1.40,
            Gait::Sprint => 0.96,
            Gait::CrouchWalk => 1.60,
        }
    }
}

/// Braking deceleration (m/s²) — a trained stop, hard on the legs.
pub const BRAKE_DECEL: f32 = 5.0;
/// Slide friction (m/s²): ground spent stopping a sliding man.
pub const SLIDE_FRICTION: f32 = 4.4;
/// Speed at which a slide ends and the crouch-walk takes over.
pub const SLIDE_EXIT_SPEED: f32 = 1.25;

/// One leg of a movement script: go to the waypoint, at this gait.
#[derive(Copy, Clone, Debug)]
pub struct ScriptLeg {
    pub target: Vec2,
    pub gait: Gait,
    /// Aim blend held over this leg (0..1).
    pub aim: f32,
    /// Entering this leg fast triggers the slide.
    pub slide: bool,
}

impl ScriptLeg {
    pub fn to(x: f32, z: f32, gait: Gait) -> ScriptLeg {
        ScriptLeg {
            target: Vec2::new(x, z),
            gait,
            aim: 0.0,
            slide: false,
        }
    }
    pub fn aiming(mut self) -> ScriptLeg {
        self.aim = 1.0;
        self
    }
    pub fn sliding(mut self) -> ScriptLeg {
        self.slide = true;
        self
    }
}

/// The driven agent's observable state (the probe's raw material,
/// the host's log columns).
#[derive(Copy, Clone, Debug)]
pub struct Agent {
    pub pos: Vec2,
    pub yaw: f32,
    pub speed: f32,
    pub leg: usize,
    pub gait: Gait,
    pub sliding: bool,
    pub done: bool,
    pub distance: f32,
}

/// The scripted controller. Deterministic: fixed-step updates of
/// the same script produce the same run on any host.
pub struct Controller {
    legs: Vec<ScriptLeg>,
    pub agent: Agent,
    slide_latched: bool,
}

impl Controller {
    pub fn new(start: Vec2, legs: Vec<ScriptLeg>) -> Controller {
        let yaw = match legs.first() {
            Some(l) => {
                let d = l.target - start;
                d.x.atan2(d.y)
            }
            None => crate::operator::OPERATOR_YAW_DEG.to_radians(),
        };
        Controller {
            agent: Agent {
                pos: start,
                yaw,
                speed: 0.0,
                leg: 0,
                gait: legs.first().map(|l| l.gait).unwrap_or(Gait::Stand),
                sliding: false,
                done: legs.is_empty(),
                distance: 0.0,
            },
            legs,
            slide_latched: false,
        }
    }

    pub fn agent(&self) -> &Agent {
        &self.agent
    }

    /// Advance one fixed step; returns the animator input this
    /// state implies.
    pub fn update(&mut self, dt: f32) -> AnimInput {
        let a = &mut self.agent;
        if a.done {
            a.speed = (a.speed - BRAKE_DECEL * dt).max(0.0);
            return self.input();
        }
        let leg = self.legs[a.leg];
        let to = leg.target - a.pos;
        let dist = to.length();
        let last = a.leg + 1 >= self.legs.len();

        // Slide latch: entering a slide leg fast commits the slide
        // until friction spends the speed.
        if leg.slide && !self.slide_latched && a.speed > CROUCH_SPEED + 0.35 {
            self.slide_latched = true;
        }
        if self.slide_latched && a.speed <= SLIDE_EXIT_SPEED {
            self.slide_latched = false;
        }
        a.sliding = self.slide_latched;
        a.gait = leg.gait;

        // Steering: face the waypoint, at the gait's turn rate.
        if dist > 0.05 {
            let want = to.x.atan2(to.y);
            let err = wrap_pi(want - a.yaw);
            let max_turn = leg.gait.turn_rate() * dt;
            a.yaw += err.clamp(-max_turn, max_turn);
        }

        // Speed: gait target, braked for whatever comes next, or
        // friction while sliding.
        let mut target = leg.gait.target_speed();
        if a.sliding {
            a.speed = (a.speed - SLIDE_FRICTION * dt).max(SLIDE_EXIT_SPEED * 0.9);
        } else {
            if last {
                let allow = (2.0 * BRAKE_DECEL * (dist - 0.15).max(0.0)).sqrt();
                target = target.min(allow);
            } else if let Some(next) = self.legs.get(a.leg + 1) {
                // A slide leg is entered AT SPEED — its friction is
                // the brake, so nothing is shed on the approach.
                if !next.slide {
                    let nv = next.gait.target_speed();
                    if nv < target {
                        let allow =
                            (nv * nv + 2.0 * BRAKE_DECEL * (dist - 0.6).max(0.0)).sqrt();
                        target = target.min(allow);
                    }
                }
            }
            if target > a.speed {
                a.speed = (a.speed + leg.gait.accel() * dt).min(target);
            } else {
                a.speed = (a.speed - BRAKE_DECEL * dt).max(target);
            }
        }

        // Advance along the facing — the man goes where he faces,
        // which is what makes his turns arcs instead of pivots.
        let (fwd, _) = facing(a.yaw);
        let step = fwd * (a.speed * dt);
        a.pos += step;
        a.distance += step.length();

        // Arrival.
        let arrived = if last {
            dist < 0.35 && a.speed < 0.5
        } else {
            dist < 0.9
        };
        if arrived {
            a.leg += 1;
            self.slide_latched = false;
            if a.leg >= self.legs.len() {
                a.done = true;
                a.gait = Gait::Stand;
            }
        }
        self.input()
    }

    fn input(&self) -> AnimInput {
        let a = &self.agent;
        let leg = self.legs.get(a.leg);
        AnimInput {
            pos: Vec3::new(a.pos.x, 0.0, a.pos.y),
            yaw: a.yaw,
            speed: a.speed,
            crouch: if a.gait == Gait::CrouchWalk || a.sliding {
                1.0
            } else {
                0.0
            },
            aim: if a.done { 0.0 } else { leg.map(|l| l.aim).unwrap_or(0.0) },
            slide: if a.sliding { 1.0 } else { 0.0 },
            carry: true,
        }
    }
}

/// The proof scripts, shared by the headless host and the movement
/// probe so the driven run and the measured run are the same run.
/// The route is surveyed ground (the probe's shore survey): the
/// beach at x ≈ −13 runs south, dry and gentle (2.7 → 0.2 over
/// z 64 → 90); the shelf at x ≈ −8 runs north, gentle to z ≈ 38
/// (0.7 → 3.1) and a wall beyond it (15.5 by z 20 — the survey's
/// second lesson: the route stays at z ≥ 34 on the shelf). The
/// island's south tail and east shore are water.
pub fn proof_script(name: &str) -> Option<(Vec2, Vec<ScriptLeg>)> {
    use Gait::*;
    let start = Vec2::new(crate::operator::OPERATOR_SPOT.0, crate::operator::OPERATOR_SPOT.1);
    let legs = match name {
        // The full drive: a closed loop — beach south at a run,
        // crouch at the water's edge, back north, the shelf at a
        // sprint, the slide, the aimed run home.
        "full" => vec![
            ScriptLeg::to(-13.0, 70.0, Walk),
            ScriptLeg::to(-13.5, 88.0, Run),
            ScriptLeg::to(-13.5, 92.0, CrouchWalk),
            ScriptLeg::to(-12.0, 76.0, Walk),
            ScriptLeg::to(-9.0, 66.0, Walk),
            ScriptLeg::to(-8.0, 40.0, Sprint),
            ScriptLeg::to(-8.0, 36.0, CrouchWalk).sliding(),
            ScriptLeg::to(-8.5, 44.0, CrouchWalk),
            ScriptLeg::to(-9.0, 58.0, Run).aiming(),
            ScriptLeg::to(-8.0, 64.0, Walk),
        ],
        // The motion A/B drive: the three gaits back to back up the
        // shelf, then a full stop — short enough to run at classic
        // scale, long enough to hold each gait steady.
        "ab" => vec![
            ScriptLeg::to(-9.0, 56.0, Walk),
            ScriptLeg::to(-8.0, 44.0, Run),
            ScriptLeg::to(-8.0, 36.0, Sprint),
            ScriptLeg::to(-8.0, 34.0, Stand),
        ],
        // Cycle bursts: one long steady leg per gait, up the shelf.
        "burst-walk" => vec![ScriptLeg::to(-8.0, 40.0, Walk)],
        "burst-run" => vec![ScriptLeg::to(-8.0, 38.0, Run)],
        // No legs: he stands at the spot. The static control.
        "stand" => vec![],
        _ => return None,
    };
    Some((start, legs))
}

