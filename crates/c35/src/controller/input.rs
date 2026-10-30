//! Per-tick player intent, the speed-tier and posture
//! vocabulary, and the controller's configuration.
//!
//! The host fills one [`PlayerInput`] per fixed tick;
//! nothing here reads a device, a clock, or a key.
//! Bindings are the host's business entirely: the
//! grapple arrives as press / held / release edges and
//! levels (DeBrief binds it to E by game law — the
//! controller takes the edges, never the key), lean
//! arrives as a number in -1..=1, and look arrives as
//! per-tick yaw/pitch deltas in radians.

use glam::Vec2;

/// Which camera rig presents the one movement core.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ViewMode {
    FirstPerson,
    ThirdPerson,
}

/// The wish speed tier. The speeds are the locomotion
/// gait table's own constants, taken by reference, so
/// the player's gears and the figure's gaits can never
/// drift apart.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SpeedTier {
    Crouch,
    Walk,
    Run,
    Sprint,
}

impl SpeedTier {
    /// Target ground speed (m/s) for this tier.
    pub fn speed(self) -> f32 {
        match self {
            SpeedTier::Crouch => crate::locomotion::CROUCH_SPEED,
            SpeedTier::Walk => crate::locomotion::WALK_SPEED,
            SpeedTier::Run => crate::locomotion::RUN_SPEED,
            SpeedTier::Sprint => crate::locomotion::SPRINT_SPEED,
        }
    }

    /// The crouch tier IS the crouch posture: it sets
    /// the posture, the crouch speed, and the traversal
    /// crouch flag (which stops the grapple reel without
    /// stopping the swing).
    pub fn is_crouch(self) -> bool {
        matches!(self, SpeedTier::Crouch)
    }
}

/// Standing eye height (m above the feet): the engine's
/// viewmodel eye law, taken by reference — the camera
/// and the viewmodel placement contract key off the
/// same number.
pub const STAND_EYE: f32 = crate::operator::VIEWMODEL_EYE_Y;
/// The crouch eye drop (m): the staged crouch pose's
/// root drop (`PoseFrame::from_pose(OperatorPose::Crouch)
/// .root_y == -0.50`), so the eye falls by exactly the
/// amount the posed body falls.
pub const CROUCH_DROP: f32 = 0.50;
/// Crouching eye height (m above the feet).
pub const CROUCH_EYE: f32 = STAND_EYE - CROUCH_DROP;

/// Seconds a full stand-to-crouch eye travel takes.
/// The eye slews at a constant rate set by the full
/// drop over this time; no transition ever snaps.
pub const POSTURE_TRANSITION_S: f32 = 0.25;

/// The body's posture, as the frame reports it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Posture {
    Stand,
    Crouch,
}

impl Posture {
    /// The eye height this posture declares (m).
    pub fn eye_height(self) -> f32 {
        match self {
            Posture::Stand => STAND_EYE,
            Posture::Crouch => CROUCH_EYE,
        }
    }
}

/// Controller configuration, set once by the host.
/// The third-person boom numbers are host-tunable
/// here; their defaults are the house follow rig's
/// own contract (3.4 m back, 0.62 m to his right,
/// chest height), carried over from the DeBrief host's
/// `follow_pose`.
#[derive(Copy, Clone, Debug)]
pub struct ControllerConfig {
    /// The rig the controller starts in.
    pub initial_mode: ViewMode,
    /// Boom length (m) in third person, unaimed.
    pub tp_distance: f32,
    /// Shoulder offset (m, toward his right) in third
    /// person, unaimed.
    pub tp_shoulder: f32,
    /// Height of the boom's focus point above the feet
    /// (m) in third person.
    pub tp_height: f32,
    /// Boom length (m) while aiming: the boom tightens.
    pub tp_aim_distance: f32,
    /// Shoulder offset (m) while aiming: nearer centre.
    pub tp_aim_shoulder: f32,
    /// Which kit the first-person viewmodel wears.
    pub viewmodel: crate::operator::ViewmodelConfig,
}

impl Default for ControllerConfig {
    fn default() -> ControllerConfig {
        ControllerConfig {
            initial_mode: ViewMode::FirstPerson,
            tp_distance: 3.4,
            tp_shoulder: 0.62,
            tp_height: 1.52,
            tp_aim_distance: 2.0,
            tp_aim_shoulder: 0.28,
            viewmodel: crate::operator::ViewmodelConfig::Street,
        }
    }
}

/// One fixed tick of player intent, filled by the host.
///
/// `wish` is in camera space: `x` strafes toward the
/// camera's right, `y` moves toward where the camera
/// faces; the controller turns it into a world wish
/// direction with the camera yaw (the orbit yaw in
/// third person, the eye yaw in first — the movement
/// math is the same). Its length is the analog
/// magnitude, clamped to 1.
#[derive(Copy, Clone, Debug)]
pub struct PlayerInput {
    /// Camera-space wish: x = right, y = forward.
    pub wish: Vec2,
    /// Look travel this tick (radians): x = yaw
    /// (positive rotates the facing from +Z toward +X,
    /// the yaw convention of `locomotion::facing`),
    /// y = pitch (positive looks up). Applied once per
    /// tick — there are no substeps here; one call is
    /// one fixed step.
    pub look_delta: Vec2,
    /// The wish speed tier; also the posture source.
    pub speed: SpeedTier,
    /// Jump pressed edge for this tick. Mantle pre-empts
    /// it at a ledge, exactly as the sim orders.
    pub jump: bool,
    /// Grapple press edge (DeBrief: E press).
    pub hook_press: bool,
    /// Grapple held level (DeBrief: E held). Hold keeps
    /// the rope; the tick it goes false the rope
    /// detaches, momentum kept.
    pub hook_held: bool,
    /// Grapple release edge (DeBrief: E released). A
    /// release edge forces `hook_held` false for this
    /// tick even if the host still reports the level.
    pub hook_release: bool,
    /// Aim held: narrows the FOV, silences the head-bob,
    /// faces the body with the camera, and tightens the
    /// third-person boom.
    pub aim_held: bool,
    /// Lean amount, -1 (full left) ..= 1 (full right).
    /// A number, never a key. In first person the eye
    /// translates sideways by this; the world never
    /// rolls for lean.
    pub lean: f32,
    /// Mode toggle edge: switch rigs this tick. The
    /// body, the rope, the posture and the look all
    /// carry; only the presentation changes.
    pub toggle_mode: bool,
}

impl Default for PlayerInput {
    fn default() -> PlayerInput {
        PlayerInput {
            wish: Vec2::ZERO,
            look_delta: Vec2::ZERO,
            speed: SpeedTier::Walk,
            jump: false,
            hook_press: false,
            hook_held: false,
            hook_release: false,
            aim_held: false,
            lean: 0.0,
            toggle_mode: false,
        }
    }
}
