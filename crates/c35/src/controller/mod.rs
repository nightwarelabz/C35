//! Dual-mode player controller — one movement core, two
//! camera rigs (Chase's ruling, 2026-10-30: the game can
//! be played in either first person or third person).
//!
//! The split of labour is strict:
//!
//! * The MOVEMENT CORE is shared and singular: every tick,
//!   in either mode, the controller turns [`PlayerInput`]
//!   into one `TravInput` and steps [`TravSim`] exactly
//!   once at its fixed 240 Hz step. Capsule, gravity,
//!   grapple, wall-run, mantle and the mode priority
//!   (Mantle > Grapple > WallRun > Ground/Air) are the
//!   sim's, mirrored here, never re-decided. Traversal
//!   overrides own the body exactly as the sim orders
//!   them; ground input is still sampled and forwarded —
//!   the sim decides what it may steer.
//! * The RIGS are presentation only. First person: the
//!   eye sits at the operator's eye plane, the figure is
//!   the viewmodel plus look-down body awareness. Third
//!   person: an over-shoulder boom with collision
//!   pull-in, and the full operator figure posed by the
//!   locomotion animator under the traversal pose family.
//!   Neither rig feeds anything back into the body.
//!
//! Toggling mode — by the input edge or by
//! [`PlayerController::set_mode`] — switches the rig and
//! NOTHING else: body position, velocity, rope state,
//! posture, camera yaw/pitch and FOV all carry across
//! the toggle untouched.
//!
//! Deterministic at the fixed step: no RNG, no wall
//! clock, no IO. Look deltas and edges are consumed
//! once per tick, so one press can never fire twice.
//!
//! Seams, stated plainly:
//! * The traversal collision capsule is a fixed 1.80 m
//!   shape (`physics::CAPSULE_HALF` is a constant and
//!   `resolve_capsule` takes no height). Posture drives
//!   eye height, speed tier, the traversal crouch flag
//!   (which stops the grapple reel) and the pose blend
//!   today; the crouch eye drop is the staged crouch
//!   pose's root drop (0.50 m), the contract value a
//!   future variable-height capsule should take.
//! * `operator::build_viewmodel` takes no pitch: the
//!   frame hands the host `viewmodel_pitch` beside the
//!   pose hint, so the hands can be pitched by exactly
//!   the camera's pitch and never disagree with the eye.
//! * The locomotion [`Animator`](crate::locomotion::Animator)
//!   samples the vista heightfield by default; a host
//!   walking the figure on box ground installs its own
//!   ground function through
//!   [`PlayerController::set_ground_fn`].

pub mod camera;
pub mod input;
pub mod player;

pub use camera::CameraState;
pub use input::{ControllerConfig, PlayerInput, Posture, SpeedTier, ViewMode};
pub use player::{ControllerFrame, PlayerController};

/// The traversal mode, re-exported so a host reading a
/// [`ControllerFrame`] needs no second import to name it.
pub use crate::traversal::BodyMode;
