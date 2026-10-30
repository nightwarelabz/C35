//! Economies — the alert budget and the witness ledger.
//!
//! The alert budget is an economy, not an AI: income
//! (witnessed violence, hard mecha kills, failed
//! crossings, reports) and decay (clean ops, quiet
//! crossings, time). Thresholds buy persistent unlocks
//! (denser patrols, Weathervane coverage); one-shot
//! events (a Type 2 appearance) are SPENT from the
//! level. A budget that only rises is a difficulty
//! slider with extra steps — decay is what makes
//! clean play pay.
//!
//! The witness ledger is the other archive's input:
//! descriptions priced by configuration. An assault
//! description is the most expensive in the city — a
//! man in black at a checkpoint is a story the whole
//! district repeats, and the checkpoints hear it.

use crate::perception::enemies::Description;

/// What the target was seen wearing as kit. This is
/// behavior-visible configuration, not a label: the
/// street coat passes checkpoints, the black set does
/// not, and the city prices the difference.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ConfigKind {
    /// The street coat — passes, unremarkable.
    Street,
    /// The black set — cannot pass, most expensive.
    Assault,
}

/// The city-level alert budget. Level 0.0–100.0.
/// Thresholds are levels, not purchases: crossing one
/// unlocks its escalation until decay drops below it
/// again. One-shot events spend.
pub struct AlertBudget {
    pub level: f32,
}

impl AlertBudget {
    /// Patrol density unlocks here: more boots, tighter
    /// routes. The first thing a risen city buys.
    pub const PATROL_AT: f32 = 25.0;
    /// Weathervane coverage unlocks here: rooftops
    /// watched, the vertical layer contested.
    pub const WEATHERVANE_AT: f32 = 50.0;
    /// A Type 2 event unlocks here — and costs 25 to
    /// spend. Escalation bought, never population: a
    /// Type 2 never joins a standing patrol.
    pub const TYPE2_AT: f32 = 75.0;
    pub const TYPE2_COST: f32 = 25.0;

    pub fn new() -> AlertBudget {
        AlertBudget { level: 0.0 }
    }

    fn add(&mut self, amount: f32) {
        self.level = (self.level + amount).clamp(0.0, 100.0);
    }

    /// Witnessed violence: the city saw it happen.
    pub fn witnessed_violence(&mut self) {
        self.add(8.0);
    }

    /// A hard mecha kill: the city heard the machine die.
    pub fn hard_mecha_kill(&mut self) {
        self.add(12.0);
    }

    /// A failed crossing: caught, turned back, or fled.
    pub fn failed_crossing(&mut self) {
        self.add(6.0);
    }

    /// A Weathervane report received by the district.
    pub fn report_received(&mut self) {
        self.add(5.0);
    }

    /// A clean op: nothing witnessed, nothing raised.
    pub fn clean_op(&mut self) {
        self.add(-10.0);
    }

    /// A quiet crossing: passed, unremarked.
    pub fn quiet_crossing(&mut self) {
        self.add(-4.0);
    }

    /// One fixed-step tick of time decay: the city
    /// forgets, slowly. ~1 point per 100 seconds.
    pub fn tick(&mut self) {
        self.add(-0.000_042);
    }

    /// Patrol density tier from the level: 0 (normal),
    /// 1 (dense), 2 (dense + Weathervane coverage).
    pub fn patrol_tier(&self) -> u32 {
        if self.level >= Self::WEATHERVANE_AT {
            2
        } else if self.level >= Self::PATROL_AT {
            1
        } else {
            0
        }
    }

    /// Is a Type 2 event available to buy?
    pub fn type2_available(&self) -> bool {
        self.level >= Self::TYPE2_AT
    }

    /// Spend a Type 2 event. Returns false when the
    /// budget cannot afford it — escalation is bought,
    /// never granted.
    pub fn spend_type2(&mut self) -> bool {
        if self.level >= Self::TYPE2_AT {
            self.level -= Self::TYPE2_COST;
            true
        } else {
            false
        }
    }
}

/// The witness ledger: descriptions the city holds.
/// Descriptions price checkpoint reads and block the
/// Wanderer while active. The ledger never forgets a
/// description that is still within its TTL — the host
/// ages descriptions out through the Neighbour brain.
#[derive(Debug, Default)]
pub struct WitnessLedger {
    pub descriptions: Vec<Description>,
}

impl WitnessLedger {
    pub fn new() -> WitnessLedger {
        WitnessLedger {
            descriptions: Vec::new(),
        }
    }

    /// File a description (from the Neighbour brain, or
    /// any witness the host models). Duplicates by
    /// configuration refresh rather than stack — the
    /// city holds one story per configuration, told
    /// louder, not twice.
    pub fn file(&mut self, d: Description) {
        for old in self.descriptions.iter_mut() {
            if old.config == d.config {
                *old = d;
                return;
            }
        }
        self.descriptions.push(d);
    }

    /// Price of a witnessed configuration: what the
    /// city charges for being seen in it. Assault is
    /// the most expensive description in the witness
    /// economy — three times the street coat.
    pub fn price(config: ConfigKind) -> f32 {
        match config {
            ConfigKind::Street => 1.0,
            ConfigKind::Assault => 3.0,
        }
    }

    /// Checkpoint strictness multiplier from held
    /// descriptions: each active description tightens
    /// the checkpoint by a quarter. The host multiplies
    /// papers scrutiny and HALT probability by this.
    pub fn checkpoint_strictness(&self) -> f32 {
        1.0 + 0.25 * self.descriptions.len() as f32
    }

    /// The Wanderer appears only while no description
    /// is active. An active description is a city that
    /// knows your face — grace does not visit a face
    /// the city has named.
    pub fn blocks_wanderer(&self) -> bool {
        !self.descriptions.is_empty()
    }

    /// Drop expired descriptions (TTL spent). The host
    /// calls this when the Neighbour brain ages one out
    /// — the ledger and the brain agree on expiry, or
    /// the city remembers what the informer forgot.
    pub fn prune_expired(&mut self) {
        self.descriptions.retain(|d| d.ttl_ticks > 0);
    }
}
