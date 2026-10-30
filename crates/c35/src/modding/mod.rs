//! The mod engine (v1: props + placement).
//!
//! A mod is a folder of text files. It names props from the game's
//! own registry, or defines new props as recipes in the engine's
//! part vocabulary, and places them by anchor or position. No Rust,
//! no compiler, no code in v1 — the engine generates every mod prop
//! through the same painters and the same grade as the base game.
//!
//! Modules:
//! - [`json`]: the engine-owned strict JSON parser (one dialect).
//! - [`model`]: recipes, parameters, placement contracts, materials.
//! - [`mesh`]: part recipes into `PropMesh`.
//! - [`registry`]: the shipped prop table + the live registry.
//! - [`loader`]: folder loads, multi-mod ordering, conflicts,
//!   quarantine, placement resolution, the ledger line.
//! - [`check`]: the validator's report path (`--check-mod`).

pub mod check;
pub mod json;
pub mod loader;
pub mod mesh;
pub mod model;
pub mod registry;

pub use check::{check_mod_folder, CheckReport};
pub use loader::{load_mod_folder, load_mods, LoadReport, LoadedMod, QuarantinedMod};
pub use model::{CollisionClass, Footstep, PlacementContract, PropDef};
pub use registry::Registry;
