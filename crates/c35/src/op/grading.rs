//! Debrief grading: a calculator, not a balance.
//!
//! The grade is a score plus a per-factor breakdown.
//! Every factor is the op record's own numbers times
//! a weight the host passes. There is no invented
//! balance anywhere in this file:
//!
//! - `GradeWeights::default()` is all zeros, so an
//!   unweighted grade scores exactly 0.0 with every
//!   factor shown at 0 contribution. Zero effect
//!   unless set is the documented default, not a
//!   fallback.
//! - The breakdown lists every factor with its
//!   quantity, its weight, and its contribution, in
//!   fixed order — including factors that moved
//!   nothing. What moved the grade, and by how much,
//!   is fully visible, never summarized away.
//! - The engine assigns no band, no rank, no word:
//!   what a score means for progression is host
//!   policy (the demo maps scores to words and labels
//!   that mapping as demo policy, not engine truth).
//! - Weights may be negative (a penalty per unit) or
//!   positive (a bonus per unit); the engine just
//!   multiplies. Sign conventions are the host's
//!   documented choice, not the engine's hidden one.

use crate::op::lifecycle::OpRecord;

/// Weights the host passes to grading. All default
/// to zero effect unless set.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GradeWeights {
    /// Per completed objective.
    pub per_objective_complete: f32,
    /// Per failed objective.
    pub per_objective_failed: f32,
    /// Per objective still open at debrief
    /// (Pending or Active).
    pub per_objective_unclosed: f32,
    /// Per alert tier at op end (0, 1, or 2).
    pub per_alert_tier: f32,
    /// Per witness description filed.
    pub per_description: f32,
    /// Per priced witness point (economy prices:
    /// street 1.0, assault 3.0 per description).
    pub per_witness_price: f32,
    /// Per host-reported casualty.
    pub per_casualty: f32,
    /// Per second of Active op time.
    pub per_second: f32,
}

/// One factor's contribution to the grade, shown in
/// full: what it measured, what weight it carried,
/// and what it contributed.
#[derive(Debug, Clone, PartialEq)]
pub struct GradeFactor {
    /// Factor name, e.g. `"objectives_complete"`.
    pub name: &'static str,
    /// Measured quantity (count, tier, seconds, ...).
    pub quantity: f32,
    /// Host weight applied.
    pub weight: f32,
    /// quantity * weight.
    pub contribution: f32,
}

/// A grade: a score and the complete breakdown that
/// produced it, in fixed factor order.
#[derive(Debug, Clone, PartialEq)]
pub struct Grade {
    /// Sum of all factor contributions.
    pub score: f32,
    /// Every factor, even ones that contributed 0.
    pub factors: Vec<GradeFactor>,
}

/// Grade an op record with the host's weights.
/// Works on any record; hosts grade at debrief, after
/// close, when the tallies are final. The engine does
/// not require that — it calculates, it does not
/// police the ceremony.
pub fn grade(op: &OpRecord, weights: GradeWeights) -> Grade {
    let (_, _, complete, failed) = op.objectives.counts();
    let unclosed = op.objectives.unclosed();
    let raw: [(&'static str, f32, f32); 8] = [
        (
            "objectives_complete",
            complete as f32,
            weights.per_objective_complete,
        ),
        (
            "objectives_failed",
            failed as f32,
            weights.per_objective_failed,
        ),
        (
            "objectives_unclosed",
            unclosed as f32,
            weights.per_objective_unclosed,
        ),
        (
            "alert_tier_end",
            op.alert_tier as f32,
            weights.per_alert_tier,
        ),
        (
            "witness_descriptions",
            op.descriptions_filed as f32,
            weights.per_description,
        ),
        (
            "witness_price",
            op.witness_price,
            weights.per_witness_price,
        ),
        ("casualties", op.casualties as f32, weights.per_casualty),
        ("time_seconds", op.elapsed_s, weights.per_second),
    ];
    let mut factors = Vec::with_capacity(raw.len());
    let mut score = 0.0;
    for (name, quantity, weight) in raw {
        // Normalize negative zero: a zero contribution
        // prints as 0.00, never -0.00, in the breakdown.
        let contribution = if quantity * weight == 0.0 {
            0.0
        } else {
            quantity * weight
        };
        score += contribution;
        factors.push(GradeFactor {
            name,
            quantity,
            weight,
            contribution,
        });
    }
    Grade { score, factors }
}
