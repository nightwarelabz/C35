//! Per-frame player intent, the posture set, and the
//! controller's tuning constants.
//!
//! The host fills one [`PlayerInput`] per rendered frame;
//! nothing here reads a device, a clock, or a key. Bindings
//! are the host's business entirely: lean arrives as a
//! number in -1..=1 (DeBrief's grapple lives on E, and E is
//! never lean), the grapple arrives as press/held
//! pass-throughs, and crouch arrives as both a held level
//! and a pressed edge so the controller can own the toggle
//! latch (see [`ControllerConfig`]).

use crate::traversal::physics::{CAPSULE_HALF, CAPSULE_RADIUS};

/// Player walk speed (m/s): the unhurried gear — fast
/// enough to cross a street, slow enough to read it. This
/// is the PLAYER's table, not the NPC gait table in
/// `locomotion` (whose walk is a patrol's 1.5 m/s).
pub const WALK_SPEED: f32 = 3.0;
/// Player sprint speed (m/s): the locomotion table's
/// sprint, taken by reference so the player and the
/// figure's sprint gait can never drift apart.
pub const SPRINT_SPEED: f32 = crate::locomotion::SPRINT_SPEED;
/// Crouch-walk speed (m/s). Prone crawls in this band too:
/// the controller declares three speed gears, not a
/// tuning table, and prone is the crouch gear lying down.
pub const CROUCH_WALK_SPEED: f32 = 1.4;

/// Standing capsule height (m): the traversal capsule's
/// own total shape, 2 * (half + radius) = 1.80, taken
/// from the physics constants by reference.
pub const STAND_CAPSULE: f32 = 2.0 * (CAPSULE_HALF + CAPSULE_RADIUS);
/// Crouching capsule height (m): head and shoulders under
/// a desk, a counter, a window line.
pub const CROUCH_CAPSULE: f32 = 1.20;
/// Prone capsule height (m): the body flat — a torso's
/// thickness plus clearance.
pub const PRONE_CAPSULE: f32 = 0.60;

/// Standing eye height (m): the engine's viewmodel eye
/// law, taken by reference — the camera and the viewmodel
/// placement contract key off the same number.
pub const STAND_EYE: f32 = crate::operator::VIEWMODEL_EYE_Y;
/// Crouching eye height (m).
pub const CROUCH_EYE: f32 = 1.05;
/// Prone eye height (m): cheek near the floor.
pub const PRONE_EYE: f32 = 0.45;

/// Seconds a full stand-to-prone posture change takes.
/// Eye and capsule heights slew at a constant rate set by
/// the full range over this time, so a stand-to-crouch
/// (a shorter distance) lands sooner; no transition ever
/// snaps.
pub const POSTURE_TRANSITION_S: f32 = 0.25;

/// The body's posture. The sim's collision capsule does
/// not change with posture in this slice (see the module
/// docs in `mod.rs`); posture drives eye height, speed
/// gear, the traversal crouch flag, and the camera law.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Posture {
    Stand,
    Crouch,
    Prone,
}

impl Posture {
    /// The capsule height this posture declares (m).
    pub fn capsule_height(self) -> f32 {
        match self {
            Posture::Stand => STAND_CAPSULE,
            Posture::Crouch => CROUCH_CAPSULE,
            Posture::Prone => PRONE_CAPSULE,
        }
    }

    /// The eye height this posture declares (m above feet).
    pub fn eye_height(self) -> f32 {
        match self {
            Posture::Stand => STAND_EYE,
            Posture::Crouch => CROUCH_EYE,
            Posture::Prone => PRONE_EYE,
        }
    }
}

/// Controller configuration, set once by the host.
#[derive(Copy, Clone, Debug)]
pub struct ControllerConfig {
    /// Crouch mode. false (default): hold-to-crouch —
    /// `crouch_held` is the posture, releasing stands.
    /// true: toggle — each `crouch_pressed` edge flips a
    /// latch the controller owns, and `crouch_held` is
    /// ignored. The latch lives here, not host-side, so
    /// the headroom rule (a blocked stand defers, it does
    /// not unlatch) has exactly one owner.
    pub crouch_toggle: bool,
}

impl Default for ControllerConfig {
    fn default() -> ControllerConfig {
        ControllerConfig {
            crouch_toggle: false,
        }
    }
}

/// One frame of player intent, filled by the host.
///
/// Axes are analog: -1..=1, camera-relative (the
/// controller turns them into a wish direction with the
/// camera yaw). Look deltas are radians of mouse/stick
/// travel THIS frame — the controller applies them once
/// per update, never per physics substep, so look speed
/// cannot depend on how many substeps a frame buys.
#[derive(Copy, Clone, Debug, Default)]
pub struct PlayerInput {
    /// Forward axis: +1 walks toward the camera's facing.
    pub move_forward: f32,
    /// Strafe axis: +1 moves to the camera's right.
    pub move_strafe: f32,
    /// Yaw travel this frame (radians, + turns right).
    pub look_yaw_delta: f32,
    /// Pitch travel this frame (radians, + looks up).
    pub look_pitch_delta: f32,
    /// Sprint gear held. Sprint is a forward gear: it
    /// engages only with a real forward component, and
    /// aiming cancels it.
    pub sprint_held: bool,
    /// Crouch level (hold mode).
    pub crouch_held: bool,
    /// Crouch pressed edge (toggle mode).
    pub crouch_pressed: bool,
    /// Prone pressed edge: toggles prone. Prone exits
    /// only through this edge or a jump press (which
    /// stands instead of jumping) — crouch input is
    /// ignored while prone.
    pub prone_pressed: bool,
    /// Jump pressed edge. Consumed by the first physics
    /// substep of the frame, never repeated into later
    /// substeps.
    pub jump_pressed: bool,
    /// Aim held: narrows the FOV, kills the head-bob,
    /// and caps the speed gear at walk.
    pub aim_held: bool,
    /// Lean amount, -1 (full left) ..= 1 (full right).
    /// A number, never a key: which keys make it is an
    /// open design question the host owns.
    pub lean: f32,
    /// Grapple press edge, passed through to the
    /// traversal sim (DeBrief binds it to E; the binding
    /// is the host's, the edge is forwarded untouched).
    pub hook_press: bool,
    /// Grapple held level, passed through.
    pub hook_held: bool,
}
