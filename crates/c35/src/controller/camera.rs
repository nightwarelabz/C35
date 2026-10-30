//! The camera the controller hands the host each frame,
//! and the camera law's constants.
//!
//! [`CameraState`] is deliberately more than a view
//! matrix's ingredients: it carries pitch, posture and
//! aim state because the engine's viewmodel API
//! (`operator::build_viewmodel`) takes no pitch today —
//! this struct is the seam the DeBrief host pitches the
//! viewmodel from. The host reads `eye`, `yaw_deg`,
//! `fov_deg` and `roll` for its scene camera, and
//! `pitch` / `posture` / `aiming` for the viewmodel and
//! body-awareness placement, all off the one struct, so
//! the hands can never disagree with the eyes.

use crate::controller::input::Posture;
use crate::traversal::BodyMode;
use glam::Vec3;

/// Base vertical FOV (degrees): the standing, unhurried
/// register — wide enough for a street, narrow enough
/// that a doorway still reads as a decision.
pub const BASE_FOV_DEG: f32 = 75.0;
/// Aiming FOV (degrees): the narrowed register. A zoom
/// by framing, not by lens trickery — the aim pose and
/// this number are the whole of it.
pub const AIM_FOV_DEG: f32 = 55.0;
/// Sprint FOV (degrees): a few degrees of speed made
/// visible at the frame's edge.
pub const SPRINT_FOV_DEG: f32 = 82.0;
/// Grapple-swing FOV (degrees): the widest register —
/// the swing is the fastest honest motion the body has.
pub const SWING_FOV_DEG: f32 = 86.0;
/// FOV easing rate (1/s), exponential: frame-rate
/// independent, most of the travel inside ~0.3 s, and
/// never an overshoot.
pub const FOV_EASE_RATE: f32 = 8.0;

/// Pitch clamp (degrees either way from level). Past
/// this the neck — and the body-awareness figure under
/// it — stops being honest.
pub const PITCH_LIMIT_DEG: f32 = 85.0;

/// Lateral eye offset at full lean (m): a head and a
/// shoulder's worth around a corner, no more — lean is
/// for seeing, not for teleporting the eye through
/// walls.
pub const LEAN_OFFSET: f32 = 0.35;
/// Camera roll at full lean (degrees).
pub const LEAN_ROLL_DEG: f32 = 6.0;
/// Camera roll while wall-running (degrees), toward the
/// wall. Traversal admits walls leaning up to
/// `wallrun::MAX_WALL_LEAN_DEG` (25°) but exposes no
/// camera roll of its own; this declared value sits
/// inside that envelope.
pub const WALLRUN_ROLL_DEG: f32 = 12.0;
/// Roll easing rate (1/s), exponential — a wall-run
/// entry tilts in over ~0.2 s instead of snapping.
pub const ROLL_EASE_RATE: f32 = 10.0;

/// Head-bob peak vertical amplitude (m) at full walk.
/// Subtle on purpose: the bob is a felt rhythm, never a
/// visible bounce.
pub const BOB_AMPLITUDE: f32 = 0.035;
/// Ground distance (m) per full bob cycle: one cycle
/// per two steps at a walk stride.
pub const BOB_STRIDE: f32 = 1.9;
/// Below this ground speed (m/s) the bob is exactly
/// zero — the stealth threshold. Creeping must not
/// announce itself in the camera.
pub const BOB_STEALTH_SPEED: f32 = 1.6;
/// Bob weight attack rate (1/s): when the bob becomes
/// legal it fades in at this rate instead of popping.
/// (While illegal it is exactly 0.0 — see player.rs.)
pub const BOB_WEIGHT_RATE: f32 = 6.0;

/// One frame of camera truth, returned by
/// `PlayerController::update`.
#[derive(Copy, Clone, Debug)]
pub struct CameraState {
    /// World eye position: feet + posture eye height,
    /// plus the lean's lateral offset, plus the bob.
    pub eye: Vec3,
    /// The simulated body's feet position (world). The
    /// viewmodel placement contract keys off this:
    /// spot = feet XZ, ground_y = eye.y - eye_height.
    pub feet: Vec3,
    /// Camera yaw (radians; + turns right, wrapped to
    /// -PI..=PI). Facing is (-sin yaw, 0, -cos yaw).
    pub yaw: f32,
    /// Camera yaw in degrees — the exact value the
    /// viewmodel contract (`build_viewmodel`) takes.
    pub yaw_deg: f32,
    /// Camera pitch (radians, + looks up, clamped to
    /// +/- PITCH_LIMIT_DEG). Exposed for the viewmodel
    /// seam: the host pitches the hands by this.
    pub pitch: f32,
    /// Camera roll (radians; + tips the top of frame to
    /// the player's right). From lean and wall-run only;
    /// the controller invents no other roll sources.
    pub roll: f32,
    /// Current vertical FOV (degrees), eased.
    pub fov_deg: f32,
    /// Current posture (the target state; heights below
    /// may still be travelling toward it mid-transition).
    pub posture: Posture,
    /// Current eased eye height above the feet (m).
    pub eye_height: f32,
    /// Current eased capsule height (m) — the posture
    /// contract value (see mod.rs on the fixed sim
    /// capsule).
    pub capsule_height: f32,
    /// Horizontal ground speed (m/s), as simulated.
    pub speed: f32,
    /// The traversal mode this frame (Ground / Air /
    /// WallRun / Grapple / Mantle), mirrored so a host
    /// never reaches into the sim for it.
    pub mode: BodyMode,
    /// Aim held this frame (echoed for the viewmodel
    /// pose pick).
    pub aiming: bool,
    /// On the ground, as simulated.
    pub grounded: bool,
    /// The vertical bob offset folded into `eye` this
    /// frame (m). Exactly 0.0 whenever the bob is
    /// disabled by the stealth law.
    pub bob_offset: f32,
    /// Lean amount applied this frame (-1..=1, echoed).
    pub lean: f32,
}
