//! The camera each rig hands the host per tick, and
//! the camera law's constants.
//!
//! One [`CameraState`] serves both rigs: position,
//! target, yaw/pitch/roll and the eased FOV are all a
//! host needs to build its scene camera (or read
//! [`CameraState::pose`]), and the rig-specific fields
//! (bob offset, boom distances) say how the position
//! was arrived at, so a host can log or blend honestly.

use glam::Vec3;

/// First-person base vertical FOV (degrees): the
/// engine's standing register (the vista hero pose's
/// 55°), wide enough to read a doorway, a checkpoint
/// queue and a lamp pool's edge without turning.
pub const FP_FOV_BASE_DEG: f32 = 55.0;
/// First-person aim FOV (degrees): narrowed slightly —
/// the visor's focus, not a scope.
pub const FP_FOV_AIM_DEG: f32 = 50.0;
/// First-person FOV ceiling (degrees): sprint and the
/// grapple swing widen toward this and never past it.
/// The swing's widening is capped AT the sprint
/// widening, per the first-person note.
pub const FP_FOV_MAX_DEG: f32 = 60.0;
/// Third-person base vertical FOV (degrees): the house
/// follow rig's 50°.
pub const TP_FOV_BASE_DEG: f32 = 50.0;
/// Third-person aim FOV (degrees).
pub const TP_FOV_AIM_DEG: f32 = 45.0;
/// Third-person FOV ceiling (degrees).
pub const TP_FOV_MAX_DEG: f32 = 55.0;
/// FOV easing rate (1/s), exponential: frame-rate
/// independent, most of the travel inside ~0.3 s, no
/// overshoot, and never a snap — on latch, on release,
/// on toggle, anywhere.
pub const FOV_EASE_RATE: f32 = 8.0;

/// Pitch clamp (degrees either way from level).
pub const PITCH_LIMIT_DEG: f32 = 85.0;

/// Lateral eye offset at full lean (m) in first
/// person: a head and a shoulder's worth around a
/// corner, no more. Lean translates the eye; it never
/// rolls the world.
pub const LEAN_OFFSET: f32 = 0.35;

/// Head-bob peak vertical amplitude (m) at full sprint
/// speed; the offset scales down with speed from
/// there. Subtle on purpose: a felt rhythm, never a
/// visible bounce. Positional only — bob never pitches
/// or rolls the frame.
pub const BOB_AMPLITUDE: f32 = 0.03;
/// Ground distance (m) per full bob cycle: one cycle
/// per two steps at a walk stride.
pub const BOB_STRIDE: f32 = 1.9;
/// Bob weight attack rate (1/s): when the bob becomes
/// legal it fades in at this rate instead of popping.
/// (While illegal it is exactly 0.0 — see player.rs.)
pub const BOB_WEIGHT_RATE: f32 = 6.0;
/// Landing dip: deepest dip (m), and dip per unit of
/// impact speed (m per m/s). One eased downward
/// translation on landing, scaled by fall speed — a
/// dip, never a bounce, never a shake.
pub const DIP_MAX: f32 = 0.10;
pub const DIP_PER_IMPACT: f32 = 0.02;
/// Landing dip duration (s): recovered inside half a
/// second, well under the note's one-second bound.
pub const DIP_DURATION_S: f32 = 0.45;
/// Idle breath: the faintest standing-still eye
/// translation (m), on a slow period (s). It stops
/// entirely when aiming.
pub const BREATH_AMPLITUDE: f32 = 0.004;
pub const BREATH_PERIOD_S: f32 = 4.0;
/// Ticks of stillness that make a beat (0.25 s at the
/// fixed 240 Hz step): after this long without moving,
/// the gait bob is over and only the idle breath
/// remains.
pub const STILL_BEAT_TICKS: u32 = 60;

/// Camera roll while wall-running (degrees), toward
/// the wall: small and constant for the run's length.
pub const WALLRUN_ROLL_DEG: f32 = 12.0;
/// Swing bank: roll per (m/s) of lateral swing
/// velocity (degrees), capped at SWING_ROLL_CAP_DEG.
/// The horizon stays level in a swing except for this
/// capped bank.
pub const SWING_ROLL_PER_SPEED_DEG: f32 = 1.5;
pub const SWING_ROLL_CAP_DEG: f32 = 8.0;
/// Roll easing rate (1/s), exponential.
pub const ROLL_EASE_RATE: f32 = 10.0;

/// Third-person boom: the closest the collision
/// pull-in may bring the camera to the focus (m), and
/// the clearance kept in front of a blocking surface
/// (m) — the camera pulls in BEFORE the wall, never
/// clips through it.
pub const TP_MIN_BOOM: f32 = 0.35;
pub const TP_COLLISION_MARGIN: f32 = 0.25;
/// Boom length / shoulder easing rate (1/s): the boom
/// eases out to its wished length and tightens for
/// aim at this rate; collision pull-in is NOT eased
/// (it applies the same tick the geometry demands it).
pub const TP_BOOM_EASE_RATE: f32 = 10.0;

/// One tick of camera truth, returned inside
/// [`crate::controller::ControllerFrame`].
#[derive(Copy, Clone, Debug)]
pub struct CameraState {
    /// World camera position. First person: the eye
    /// (posture height + lean + bob folded in). Third
    /// person: the boom end, after collision pull-in.
    pub position: Vec3,
    /// World point the camera looks at: position +
    /// look direction × 10 m.
    pub target: Vec3,
    /// The point the rig is anchored to. First person:
    /// the eye itself. Third person: the boom's focus
    /// (feet + `tp_height`), before pull-in.
    pub focus: Vec3,
    /// Camera yaw (radians; facing =
    /// (sin yaw, 0, cos yaw), the locomotion yaw
    /// convention).
    pub yaw: f32,
    /// Camera pitch (radians, + looks up, clamped to
    /// ±PITCH_LIMIT_DEG).
    pub pitch: f32,
    /// Camera roll (radians). Sources are only the
    /// wall-run lean and the capped swing bank, in
    /// first person; third person holds the horizon
    /// (0.0). Lean never rolls the world.
    pub roll: f32,
    /// Current vertical FOV (degrees), eased.
    pub fov_deg: f32,
    /// The positional eye offset folded into
    /// `position` this tick (gait bob + landing dip +
    /// idle breath), first person only. Exactly zero
    /// in third person, and its gait component is
    /// exactly 0.0 whenever the stealth law silences
    /// the bob.
    pub bob_offset: Vec3,
    /// Third person: the boom length actually used,
    /// after easing and collision pull-in (m). First
    /// person: 0.0.
    pub boom_distance: f32,
    /// Third person: the boom length the rig wants
    /// (config, or the aim-tightened config). First
    /// person: 0.0.
    pub boom_requested: f32,
    /// Third person: true when geometry pulled the
    /// boom in short of `boom_requested` this tick.
    pub boom_blocked: bool,
}

impl CameraState {
    /// The engine scene camera for this state. Roll is
    /// not part of `scene::CameraPose`; a host that
    /// renders roll applies `roll` around its view axis
    /// itself.
    pub fn pose(&self) -> crate::scene::CameraPose {
        crate::scene::CameraPose {
            eye: self.position,
            target: self.target,
            fov_y_deg: self.fov_deg,
        }
    }
}
