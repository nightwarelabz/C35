//! First-person player controller — input + camera
//! above the traversal sim.
//!
//! The traversal milestone owns the BODY: capsule,
//! gravity, grapple, wall-run, mantle, and the mode
//! priority (Mantle > Grapple > WallRun > Ground/Air).
//! This module owns the two things the body cannot:
//! what the player MEANT (axes, edges, posture, lean —
//! [`PlayerInput`], filled by the host each frame) and
//! what the player SEES ([`CameraState`]: eye, yaw,
//! pitch, roll, eased FOV, and a head-bob the stealth
//! law can silence to exactly zero).
//!
//! [`PlayerController`] is the whole surface: construct
//! it at a feet position, call `update` once per
//! rendered frame with the frame's input, the static
//! world, and the frame's dt. It steps the sim at its
//! fixed 240 Hz, mirrors the sim's mode rather than
//! re-deciding it, and returns the camera. While a
//! traversal mode owns the body, ground input is still
//! sampled and forwarded — the sim decides what it may
//! steer — and locomotion resumes on exit by itself.
//!
//! One seam, stated plainly: the traversal collision
//! capsule is a fixed 1.80 m shape in this slice
//! (`physics::CAPSULE_HALF` is a constant, and
//! `resolve_capsule` takes no height). Posture therefore
//! drives eye height, speed gear, the traversal crouch
//! flag and the headroom RULE today, and the capsule
//! heights declared in [`input`] are the contract the
//! camera and the headroom check honour — but the body
//! does not yet physically fit under a 1.5 m lintel by
//! crouching. When traversal grows a variable-height
//! capsule, the declared heights are the values it
//! should take; nothing here changes. See the build
//! report for the full seam list.

pub mod camera;
pub mod input;
pub mod player;

pub use camera::CameraState;
pub use input::{ControllerConfig, PlayerInput, Posture};
pub use player::PlayerController;

/// The traversal mode, re-exported so a host reading a
/// [`CameraState`] needs no second import to name it.
pub use crate::traversal::BodyMode;
