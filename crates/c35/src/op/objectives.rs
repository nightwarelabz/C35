//! Objectives: typed state tracking, host-defined
//! meaning.
//!
//! The engine tracks an objective's state; the host
//! defines what it means. An objective's `kind` is a
//! host-vocabulary word (`"cross"`, `"retrieve"`,
//! `"observe"` — the engine never interprets it), so
//! the type set is extensible by construction: new
//! mission content needs no engine change. The engine
//! refuses only impossible state moves, never
//! judgement calls:
//!
//! - Pending -> Active -> Complete, or
//!   Pending -> Active -> Failed.
//! - Pending -> Failed directly (an objective can
//!   fail before activation — a window closed, a
//!   contact gone).
//! - Complete and Failed are terminal: no move out.
//! - Activating, completing, or failing an unknown id
//!   is refused, not silently created.

/// State of one objective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectiveState {
    /// Defined, not yet started.
    Pending,
    /// Underway.
    Active,
    /// Achieved. Terminal.
    Complete,
    /// Failed. Terminal.
    Failed,
}

impl ObjectiveState {
    /// Terminal states accept no further transition.
    pub fn is_terminal(self) -> bool {
        matches!(self, ObjectiveState::Complete | ObjectiveState::Failed)
    }
}

/// A plain error for refused objective calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectiveError {
    /// No objective with this id exists.
    UnknownId,
    /// An objective with this id already exists.
    DuplicateId,
    /// The transition is impossible from the current
    /// state (e.g. completing a Complete objective,
    /// or activating a Failed one).
    ImpossibleTransition,
}

/// One objective: host meaning, engine state.
#[derive(Debug, Clone, PartialEq)]
pub struct Objective {
    /// Stable host id, e.g. `"obj.cross"`.
    pub id: String,
    /// Host-vocabulary kind word. Never interpreted.
    pub kind: String,
    /// Human-readable label for debrief display.
    pub label: String,
    /// Tracked state.
    pub state: ObjectiveState,
}

/// The op's objective set, in insertion order.
#[derive(Debug, Clone, Default)]
pub struct ObjectiveSet {
    objectives: Vec<Objective>,
}

impl ObjectiveSet {
    pub fn new() -> ObjectiveSet {
        ObjectiveSet {
            objectives: Vec::new(),
        }
    }

    /// Add an objective in Pending. Duplicate ids are
    /// refused: ids are the host's stable keys.
    pub fn add(&mut self, id: &str, kind: &str, label: &str) -> Result<(), ObjectiveError> {
        if self.objectives.iter().any(|o| o.id == id) {
            return Err(ObjectiveError::DuplicateId);
        }
        self.objectives.push(Objective {
            id: id.to_string(),
            kind: kind.to_string(),
            label: label.to_string(),
            state: ObjectiveState::Pending,
        });
        Ok(())
    }

    fn find_mut(&mut self, id: &str) -> Result<&mut Objective, ObjectiveError> {
        self.objectives
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or(ObjectiveError::UnknownId)
    }

    /// Pending -> Active.
    pub fn activate(&mut self, id: &str) -> Result<(), ObjectiveError> {
        let o = self.find_mut(id)?;
        if o.state != ObjectiveState::Pending {
            return Err(ObjectiveError::ImpossibleTransition);
        }
        o.state = ObjectiveState::Active;
        Ok(())
    }

    /// Pending or Active -> Complete.
    pub fn complete(&mut self, id: &str) -> Result<(), ObjectiveError> {
        let o = self.find_mut(id)?;
        if o.state.is_terminal() {
            return Err(ObjectiveError::ImpossibleTransition);
        }
        o.state = ObjectiveState::Complete;
        Ok(())
    }

    /// Pending or Active -> Failed.
    pub fn fail(&mut self, id: &str) -> Result<(), ObjectiveError> {
        let o = self.find_mut(id)?;
        if o.state.is_terminal() {
            return Err(ObjectiveError::ImpossibleTransition);
        }
        o.state = ObjectiveState::Failed;
        Ok(())
    }

    /// Objectives in insertion order.
    pub fn iter(&self) -> std::slice::Iter<'_, Objective> {
        self.objectives.iter()
    }

    /// Counts by state, in Pending / Active /
    /// Complete / Failed order.
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let mut pending = 0;
        let mut active = 0;
        let mut complete = 0;
        let mut failed = 0;
        for o in &self.objectives {
            match o.state {
                ObjectiveState::Pending => pending += 1,
                ObjectiveState::Active => active += 1,
                ObjectiveState::Complete => complete += 1,
                ObjectiveState::Failed => failed += 1,
            }
        }
        (pending, active, complete, failed)
    }

    /// Objectives still open at debrief: Pending or
    /// Active. These are the unclosed factor in the
    /// grade, not a silent failure.
    pub fn unclosed(&self) -> usize {
        let (pending, active, _, _) = self.counts();
        pending + active
    }
}
