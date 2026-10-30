//! Missions and operations: the op lifecycle, the
//! grade, and the debrief record.
//!
//! DeBrief ops run brief -> active -> debrief, and
//! progression is paid by debrief grades. This module
//! is the machinery for that loop — and only that:
//!
//! - [`lifecycle`]: the op record. States (Briefed /
//!   Active / Debriefed / Failed / Aborted), the op
//!   clock, and the alert/witness tallies the host
//!   reports each tick. Terminal states are declared
//!   by the host, never inferred.
//! - [`objectives`]: typed state tracking with
//!   host-defined meaning. Kinds are host words, so
//!   new mission content needs no engine change.
//! - [`grading`]: the debrief grade calculator. Score
//!   plus a complete per-factor breakdown, from the
//!   op record times explicit host weights. No
//!   invented balance: default weights are all zero.
//! - [`debrief`]: the debrief record, shaped to file
//!   straight into persistence (journal -> Archive)
//!   with no translation glue.
//!
//! Seams the host owns:
//!
//! - Alert budget: the host passes level, tier, and
//!   witness readings each tick (see
//!   [`lifecycle::OpTick`]). The engine reports
//!   escalation facts ([`lifecycle::Escalation`]) —
//!   including the Type 2 threshold crossing — and
//!   never spends, buys, or acts on them. The
//!   music-bed intensity state reads from those
//!   facts; the op engine never touches sound.
//! - Objective definitions: kinds, labels, and what
//!   completion means are host data.
//! - Grade weights: every weight is a host input.
//!   What a score means for progression is host
//!   policy; the engine assigns no band or rank.
//! - Filing: the host pushes the debrief record into
//!   its snapshot journal and files it at debrief.
//!   Sequence numbering and Archive stamping stay
//!   persist's jobs.
//!
//! Deliberately NOT here: campaign scripting (the
//! 13-op campaign arc in the design docs is PROPOSED,
//! not settled — this module bakes no campaign, no
//! mission script, no story content); mission-specific
//! verbs (kinds are host words); balance tuning (all
//! weights host inputs); any filesystem touch (the
//! host saves snapshots).

pub mod debrief;
pub mod grading;
pub mod lifecycle;
pub mod objectives;

pub use debrief::DebriefRecord;
pub use grading::{grade, Grade, GradeFactor, GradeWeights};
pub use lifecycle::{Escalation, OpError, OpOutcome, OpRecord, OpState, OpTick};
pub use objectives::{Objective, ObjectiveError, ObjectiveSet, ObjectiveState};
