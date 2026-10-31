//! Op lifecycle: brief, active, debriefed — and the
//! terminal states the host declares, never inferred.
//!
//! An op is a record the host opens, ticks, and closes.
//! The engine tracks state and tallies; it never judges
//! what an objective means, never declares failure on
//! its own, and never acts on escalation. Every
//! judgement below is a fact the host reads:
//!
//! - State transitions are explicit host calls:
//!   `open` (Briefed), `begin` (Briefed -> Active),
//!   `close` (Active -> a terminal state the host
//!   names). There is no auto-fail, no timeout, no
//!   silent abort. An op with failed objectives is
//!   still Active until the host closes it.
//! - The op clock advances only on `tick`, only while
//!   Active. Time is host-supplied seconds, never the
//!   wall clock — the engine is deterministic.
//! - Alert readings arrive per tick from the host
//!   (level, tier, descriptions filed, priced witness
//!   total). The engine stores the latest, tracks the
//!   peak, and reports threshold crossings as facts.
//!   It never spends a budget, never buys an
//!   escalation, never touches sound or music: the
//!   music-bed intensity state the host drives reads
//!   from `escalation()`, it is not driven here.

use crate::perception::economy::AlertBudget;

/// Lifecycle state of one op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpState {
    /// Briefed, not yet started. Objectives may be
    /// added; the clock does not run.
    Briefed,
    /// Underway. The clock runs; ticks apply.
    Active,
    /// Closed by the host as completed. Terminal.
    Debriefed,
    /// Closed by the host as failed. Terminal.
    Failed,
    /// Closed by the host as aborted. Terminal.
    Aborted,
}

impl OpState {
    /// Terminal states accept no further transition
    /// and no further ticks.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            OpState::Debriefed | OpState::Failed | OpState::Aborted
        )
    }
}

/// The outcome the host declares at close. This is
/// the host's judgement, not the engine's: the engine
/// never maps objective results to an outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpOutcome {
    /// The op completed; grade at debrief.
    Completed,
    /// The op failed.
    Failed,
    /// The op was aborted.
    Aborted,
}

impl OpOutcome {
    fn state(self) -> OpState {
        match self {
            OpOutcome::Completed => OpState::Debriefed,
            OpOutcome::Failed => OpState::Failed,
            OpOutcome::Aborted => OpState::Aborted,
        }
    }
}

/// A plain error for refused lifecycle calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpError {
    /// `begin` called on a non-Briefed op.
    NotBriefed,
    /// `close` called on a non-Active op.
    NotActive,
    /// `tick` called on a terminal op.
    Terminal,
    /// An objective transition the objective refuses
    /// (see objectives.rs); carried through verbatim.
    Objective(crate::op::objectives::ObjectiveError),
}

impl From<crate::op::objectives::ObjectiveError> for OpError {
    fn from(e: crate::op::objectives::ObjectiveError) -> OpError {
        OpError::Objective(e)
    }
}

/// One tick's readings, supplied by the host. All
/// values are the host's measurements from its own
/// systems (alert budget, witness ledger); the engine
/// stores them, it does not recompute them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpTick {
    /// Seconds elapsed since the last tick.
    pub dt_s: f32,
    /// Alert budget level, 0.0–100.0.
    pub alert_level: f32,
    /// Alert tier at this tick (0, 1, or 2).
    pub alert_tier: u32,
    /// Witness descriptions currently filed.
    pub descriptions_filed: u32,
    /// Priced witness total (economy prices: street
    /// 1.0, assault 3.0 per description).
    pub witness_price: f32,
}

/// Escalation facts for one tick. Reported, never
/// acted on: the host decides whether a crossing buys
/// anything, and the music bed reads these facts if
/// it wants an intensity input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Escalation {
    /// Patrol tier from the level (0 normal, 1 dense,
    /// 2 dense + Weathervane coverage), mirroring
    /// the economy's thresholds.
    pub patrol_tier: u32,
    /// A Type 2 event is affordable at this level.
    /// Escalation bought, never population.
    pub type2_available: bool,
    /// The level crossed the patrol threshold upward
    /// on this tick.
    pub crossed_patrol: bool,
    /// The level crossed the Weathervane threshold
    /// upward on this tick.
    pub crossed_weathervane: bool,
    /// The level crossed the Type 2 threshold upward
    /// on this tick. This is the crossing that would
    /// buy a Type 2 escalation — reported as a fact,
    /// never spent here.
    pub crossed_type2: bool,
}

/// One op's record: objectives, clock, and tallies.
#[derive(Debug, Clone)]
pub struct OpRecord {
    /// Host id, e.g. `"op.greyport.07"`.
    pub id: String,
    /// Host title, e.g. `"Cross the checkpoint"`.
    pub title: String,
    /// Lifecycle state.
    pub state: OpState,
    /// Seconds of Active time.
    pub elapsed_s: f32,
    /// Latest alert level reported by the host.
    pub alert_level: f32,
    /// Latest alert tier reported by the host.
    pub alert_tier: u32,
    /// Highest alert level seen this op.
    pub peak_level: f32,
    /// Latest descriptions-filed count.
    pub descriptions_filed: u32,
    /// Latest priced witness total.
    pub witness_price: f32,
    /// Casualties, host-reported only. The engine
    /// never counts bodies; the host tells it.
    pub casualties: u32,
    /// Objectives, owned by the objectives module.
    pub objectives: crate::op::objectives::ObjectiveSet,
    /// Escalation facts from the most recent tick.
    /// All false before the first tick.
    pub last_escalation: Escalation,
    /// Previous tick's level, for crossing detection.
    prev_level: f32,
    /// Whether any tick has been applied yet.
    ticked: bool,
}

impl OpRecord {
    /// Open an op: Briefed, clock at zero, no tallies.
    pub fn open(id: &str, title: &str) -> OpRecord {
        OpRecord {
            id: id.to_string(),
            title: title.to_string(),
            state: OpState::Briefed,
            elapsed_s: 0.0,
            alert_level: 0.0,
            alert_tier: 0,
            peak_level: 0.0,
            descriptions_filed: 0,
            witness_price: 0.0,
            casualties: 0,
            objectives: crate::op::objectives::ObjectiveSet::new(),
            last_escalation: Escalation {
                patrol_tier: 0,
                type2_available: false,
                crossed_patrol: false,
                crossed_weathervane: false,
                crossed_type2: false,
            },
            prev_level: 0.0,
            ticked: false,
        }
    }

    /// Begin the op: Briefed -> Active. Refused from
    /// any other state.
    pub fn begin(&mut self) -> Result<(), OpError> {
        if self.state != OpState::Briefed {
            return Err(OpError::NotBriefed);
        }
        self.state = OpState::Active;
        Ok(())
    }

    /// Apply one tick's readings. Only while Active;
    /// terminal ops refuse ticks, and Briefed ops
    /// refuse them too — the clock does not run
    /// before begin.
    pub fn tick(&mut self, t: OpTick) -> Result<(), OpError> {
        if self.state.is_terminal() {
            return Err(OpError::Terminal);
        }
        if self.state != OpState::Active {
            return Err(OpError::NotActive);
        }
        self.elapsed_s += t.dt_s.max(0.0);
        self.alert_level = t.alert_level;
        self.alert_tier = t.alert_tier;
        if t.alert_level > self.peak_level {
            self.peak_level = t.alert_level;
        }
        self.descriptions_filed = t.descriptions_filed;
        self.witness_price = t.witness_price;
        let crossed_patrol = self.ticked
            && self.prev_level < AlertBudget::PATROL_AT
            && t.alert_level >= AlertBudget::PATROL_AT;
        let crossed_weathervane = self.ticked
            && self.prev_level < AlertBudget::WEATHERVANE_AT
            && t.alert_level >= AlertBudget::WEATHERVANE_AT;
        let crossed_type2 = self.ticked
            && self.prev_level < AlertBudget::TYPE2_AT
            && t.alert_level >= AlertBudget::TYPE2_AT;
        self.last_escalation = Escalation {
            patrol_tier: if t.alert_level >= AlertBudget::WEATHERVANE_AT {
                2
            } else if t.alert_level >= AlertBudget::PATROL_AT {
                1
            } else {
                0
            },
            type2_available: t.alert_level >= AlertBudget::TYPE2_AT,
            crossed_patrol,
            crossed_weathervane,
            crossed_type2,
        };
        self.prev_level = t.alert_level;
        self.ticked = true;
        Ok(())
    }

    /// Host-reported casualties. Set, not counted:
    /// the host owns the truth about bodies.
    pub fn set_casualties(&mut self, n: u32) {
        self.casualties = n;
    }

    /// Close the op with the outcome the host
    /// declares. Only from Active; terminal states
    /// are final.
    pub fn close(&mut self, outcome: OpOutcome) -> Result<(), OpError> {
        if self.state != OpState::Active {
            return Err(OpError::NotActive);
        }
        self.state = outcome.state();
        Ok(())
    }

    /// Current escalation facts, as of the last tick.
    /// A pure read: the host drives music intensity,
    /// escalation purchases, or nothing from this.
    pub fn escalation(&self) -> Escalation {
        self.last_escalation
    }
}
