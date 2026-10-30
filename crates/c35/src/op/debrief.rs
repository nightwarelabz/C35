//! The debrief record: shaped for filing, not for
//! translation.
//!
//! The op engine outputs a [`DebriefRecord`] whose
//! journal lines drop straight into persistence: the
//! host calls `file_into` on its [`Snapshot`], then
//! `file_journal` at debrief, and the lines land in
//! the Archive with the filing stamp — no translation
//! glue, no reshaping, no second copy of the shapes.
//!
//! The record reuses persist's own types for the
//! filing path: lines are pushed through
//! [`Snapshot::journal_push`], so sequence numbering
//! stays the host's (the caller passes the next
//! sequence counter), and filing stays persist's
//! (`file_journal` stamps career and day). The op
//! engine never assigns a sequence, never stamps a
//! career, never touches the Archive directly: those
//! are persist's jobs, and this module does not
//! duplicate them.

use crate::op::grading::Grade;
use crate::op::lifecycle::{OpRecord, OpState};
use crate::op::objectives::{Objective, ObjectiveState};
use crate::persist::model::Snapshot;

/// One op's debrief: outcome, grade, objectives, and
/// tallies, frozen at close.
#[derive(Debug, Clone)]
pub struct DebriefRecord {
    /// Op id, carried over.
    pub op_id: String,
    /// Op title, carried over.
    pub title: String,
    /// Terminal state the host declared at close.
    pub outcome: OpState,
    /// Grade with its full per-factor breakdown.
    pub grade: Grade,
    /// Objective results, in insertion order.
    pub objectives: Vec<Objective>,
    /// Seconds of Active op time.
    pub elapsed_s: f32,
    /// Alert tier at op end.
    pub alert_tier: u32,
    /// Highest alert level seen this op.
    pub peak_level: f32,
    /// Witness descriptions filed at op end.
    pub descriptions_filed: u32,
    /// Priced witness total at op end.
    pub witness_price: f32,
    /// Host-reported casualties.
    pub casualties: u32,
}

fn state_word(s: ObjectiveState) -> &'static str {
    match s {
        ObjectiveState::Pending => "pending",
        ObjectiveState::Active => "active",
        ObjectiveState::Complete => "complete",
        ObjectiveState::Failed => "failed",
    }
}

fn outcome_word(s: OpState) -> &'static str {
    match s {
        OpState::Debriefed => "completed",
        OpState::Failed => "failed",
        OpState::Aborted => "aborted",
        // Briefed/Active are not debrief outcomes;
        // the word is still honest if a host files
        // early — it says what the state was.
        OpState::Briefed => "briefed",
        OpState::Active => "active",
    }
}

impl DebriefRecord {
    /// Freeze an op record and its grade into a
    /// debrief record. The grade is the host's
    /// calculation (see grading.rs); this only
    /// carries it.
    pub fn from_op(op: &OpRecord, grade: Grade) -> DebriefRecord {
        DebriefRecord {
            op_id: op.id.clone(),
            title: op.title.clone(),
            outcome: op.state,
            grade,
            objectives: op.objectives.iter().cloned().collect(),
            elapsed_s: op.elapsed_s,
            alert_tier: op.alert_tier,
            peak_level: op.peak_level,
            descriptions_filed: op.descriptions_filed,
            witness_price: op.witness_price,
            casualties: op.casualties,
        }
    }

    /// Journal lines for this record: (text, tags)
    /// pairs in fixed order — header, grade, one line
    /// per grade factor, one line per objective, then
    /// the tally lines. The host files these; the
    /// engine assigns no sequence numbers here.
    pub fn journal_lines(&self) -> Vec<(String, Vec<String>)> {
        let mut lines: Vec<(String, Vec<String>)> = Vec::new();
        let tag = |t: &str| t.to_string();
        lines.push((
            format!(
                "debrief {} {}: {}",
                self.op_id,
                self.title,
                outcome_word(self.outcome)
            ),
            vec![tag("debrief"), tag(outcome_word(self.outcome))],
        ));
        lines.push((
            format!("grade {:.2}", self.grade.score),
            vec![tag("debrief"), tag("grade")],
        ));
        for f in &self.grade.factors {
            lines.push((
                format!(
                    "factor {}: {:.2} x {:.2} = {:.2}",
                    f.name, f.quantity, f.weight, f.contribution
                ),
                vec![tag("debrief"), tag("grade"), tag("factor")],
            ));
        }
        for o in &self.objectives {
            lines.push((
                format!(
                    "objective {} ({}): {}",
                    o.id,
                    o.label,
                    state_word(o.state)
                ),
                vec![tag("objective"), tag(state_word(o.state))],
            ));
        }
        lines.push((
            format!(
                "alert: tier {} at end, peak {:.1}",
                self.alert_tier, self.peak_level
            ),
            vec![tag("debrief"), tag("alert")],
        ));
        lines.push((
            format!(
                "witness: {} descriptions, price {:.1}",
                self.descriptions_filed, self.witness_price
            ),
            vec![tag("debrief"), tag("witness")],
        ));
        lines.push((
            format!("casualties: {}", self.casualties),
            vec![tag("debrief"), tag("casualties")],
        ));
        lines.push((
            format!("time: {:.1}s active", self.elapsed_s),
            vec![tag("debrief"), tag("time")],
        ));
        lines
    }

    /// Push this record's lines into the host's
    /// snapshot journal, taking sequence numbers from
    /// the caller's counter. Returns the number of
    /// lines pushed. The host then calls
    /// `Snapshot::file_journal(day)` to file them
    /// into the Archive — filing stays persist's job.
    pub fn file_into(&self, snap: &mut Snapshot, day: u32, next_seq: &mut u64) -> usize {
        let lines = self.journal_lines();
        let count = lines.len();
        for (text, tags) in &lines {
            let refs: Vec<&str> = tags.iter().map(|t| t.as_str()).collect();
            snap.journal_push(*next_seq, day, text, &refs);
            *next_seq += 1;
        }
        count
    }
}
