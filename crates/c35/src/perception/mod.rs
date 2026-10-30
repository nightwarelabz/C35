//! Perception + enemy AI — the brains that read the
//! one-field law.
//!
//! The division of labor is the whole module:
//! traversal PRICES into the fields (footstep gains,
//! strike magnitudes, landing impacts, scent deposits
//! the host lays as the body moves), and perception
//! JUDGES from the fields (visibility, hearing, scent,
//! memory, states). Nothing here renders, nothing here
//! moves a body — brains output intents (move_to,
//! posture, voice, records) and the host walks them.
//!
//! The law, restated for implementers: one practical-
//! light field serves renderer and perceivers; sound
//! propagates through the same geometry; scent is a
//! ground field with age and weather; grace is a field
//! like light; states live in posture and sound, never
//! icons; the visor's sense view is a presentation of
//! these fields, never a second source of truth, and it
//! never shows the player's own scent.
//!
//! Determinism: every brain ticks at the fixed step,
//! every price is a closed-form function of field data,
//! the conscript's fallibility is a hash of tick and
//! id — never a hidden die roll. A run reproduces.

pub mod economy;
pub mod enemies;
pub mod field;
pub mod mecha;
pub mod pricing;
pub mod scent;
pub mod soldier;
pub mod sound;
pub mod visor;

pub use economy::{AlertBudget, ConfigKind, WitnessLedger};
pub use enemies::{HoundBrain, NeighbourBrain, WeathervaneBrain};
pub use field::{Emitter, GraceField, LightField, Occluder, Tone};
pub use mecha::{Type1Governor, Type2Governor};
pub use pricing::{Attention, Memory, Posture};
pub use scent::ScentField;
pub use soldier::{PapersState, SoldierBrain, SoldierState};
pub use sound::{DistrictKind, DistrictStimulus};
pub use visor::SenseView;

/// The district: the fields and economies a host holds
/// once and hands to every brain. Brains are ticked
/// individually (they are heterogeneous — a soldier is
/// not a Hound), but the fields they read are shared,
/// and this struct is where they live.
pub struct District {
    pub light: LightField,
    pub grace: GraceField,
    pub scent: ScentField,
    pub alert: AlertBudget,
    pub witnesses: WitnessLedger,
}

impl District {
    pub fn new(ambient: f32) -> District {
        District {
            light: LightField::new(ambient),
            grace: GraceField::new(),
            scent: ScentField::new(),
            alert: AlertBudget::new(),
            witnesses: WitnessLedger::new(),
        }
    }

    /// One fixed-step tick of the fields that age on
    /// their own: scent decays, washes, and drifts;
    /// the alert budget forgets. Brains tick separately
    /// — fields age, brains decide.
    pub fn tick_fields(&mut self) {
        self.scent.tick();
        self.alert.tick();
    }
}
