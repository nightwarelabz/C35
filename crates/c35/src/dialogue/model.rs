//! Dialogue model: the host-supplied data a
//! conversation is made of, and the facts lines and
//! options gate on.
//!
//! Everything here is host data. A line's speaker,
//! text, branch targets, and gates are words and
//! values the host supplied; the engine stores them,
//! evaluates gates against facts, and never
//! interprets what any of it means. There is no
//! story content in this module — no characters, no
//! script, nothing to interpret.

/// A fact value. Facts are the generic named values
/// the host sets and gates read: a flag, a count, a
/// clearance level. The engine compares them; it
/// never invents one and never changes one on its
/// own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FactValue {
    /// A true/false flag.
    Bool(bool),
    /// A number (counts, levels, tallies).
    Number(f64),
}

/// How a gate compares a fact to its expected value.
/// Ordering comparisons (`Lt`..`Ge`) apply to
/// numbers only; against a bool or a missing fact
/// they fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateOp {
    /// Fact equals the expected value.
    Eq,
    /// Fact differs from the expected value.
    Ne,
    /// Fact is a number below the expected number.
    Lt,
    /// Fact is a number at or below the expected.
    Le,
    /// Fact is a number above the expected number.
    Gt,
    /// Fact is a number at or above the expected.
    Ge,
}

/// One gate, as data: a fact name, a comparison, an
/// expected value. A line or option carrying a gate
/// is visible / available only while the gate
/// passes. A gate on a fact the host never set never
/// passes — the host owns its facts, and an unset
/// fact is not a hidden default.
#[derive(Debug, Clone, PartialEq)]
pub struct Gate {
    /// Host fact name, e.g. `"clearance"`.
    pub fact: String,
    /// The comparison to run.
    pub op: GateOp,
    /// The value to compare against.
    pub value: FactValue,
}

impl Gate {
    /// A gate that passes when `fact` equals `value`.
    pub fn eq(fact: &str, value: FactValue) -> Gate {
        Gate {
            fact: fact.to_string(),
            op: GateOp::Eq,
            value,
        }
    }

    /// A gate that passes when the bool fact `fact`
    /// is exactly `value`.
    pub fn flag(fact: &str, value: bool) -> Gate {
        Gate::eq(fact, FactValue::Bool(value))
    }

    /// A gate that passes when the number fact
    /// `fact` is at least `value`.
    pub fn at_least(fact: &str, value: f64) -> Gate {
        Gate {
            fact: fact.to_string(),
            op: GateOp::Ge,
            value: FactValue::Number(value),
        }
    }

    /// Evaluate against the fact store. Missing
    /// fact: false, for every comparison. Mismatched
    /// kinds (bool against number, ordering against
    /// a bool): false.
    pub fn passes(&self, facts: &FactSet) -> bool {
        let actual = match facts.get(&self.fact) {
            Some(v) => v,
            None => return false,
        };
        match (actual, self.value) {
            (FactValue::Bool(a), FactValue::Bool(e)) => match self.op {
                GateOp::Eq => a == e,
                GateOp::Ne => a != e,
                _ => false,
            },
            (FactValue::Number(a), FactValue::Number(e)) => match self.op {
                GateOp::Eq => a == e,
                GateOp::Ne => a != e,
                GateOp::Lt => a < e,
                GateOp::Le => a <= e,
                GateOp::Gt => a > e,
                GateOp::Ge => a >= e,
            },
            _ => false,
        }
    }
}

/// The fact store: named values in insertion order.
/// The host sets facts; the engine reads them for
/// gates. Setting an existing name replaces its
/// value in place.
#[derive(Debug, Clone, Default)]
pub struct FactSet {
    facts: Vec<(String, FactValue)>,
}

impl FactSet {
    pub fn new() -> FactSet {
        FactSet { facts: Vec::new() }
    }

    /// Set a fact, replacing any previous value.
    pub fn set(&mut self, name: &str, value: FactValue) {
        if let Some(slot) = self.facts.iter_mut().find(|(n, _)| n == name) {
            slot.1 = value;
        } else {
            self.facts.push((name.to_string(), value));
        }
    }

    /// Read a fact. `None` if the host never set it.
    pub fn get(&self, name: &str) -> Option<FactValue> {
        self.facts.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
    }

    /// Facts in insertion order.
    pub fn iter(&self) -> std::slice::Iter<'_, (String, FactValue)> {
        self.facts.iter()
    }

    /// How many distinct facts are set.
    pub fn len(&self) -> usize {
        self.facts.len()
    }

    /// Whether no fact is set.
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }
}

/// One branch option on a line. The host supplies
/// the text and the target; the engine presents
/// available options in the order given and, on
/// selection, continues at the target. It never
/// reads what an option means.
#[derive(Debug, Clone, PartialEq)]
pub struct ChoiceOption {
    /// Option text, shown to the player as written.
    pub text: String,
    /// Id of the line flow continues at.
    pub target: String,
    /// Availability gate; `None` means always
    /// available.
    pub gate: Option<Gate>,
}

impl ChoiceOption {
    /// An always-available option.
    pub fn new(text: &str, target: &str) -> ChoiceOption {
        ChoiceOption {
            text: text.to_string(),
            target: target.to_string(),
            gate: None,
        }
    }

    /// An option available only while `gate` passes.
    pub fn gated(text: &str, target: &str, gate: Gate) -> ChoiceOption {
        ChoiceOption {
            text: text.to_string(),
            target: target.to_string(),
            gate: Some(gate),
        }
    }

    /// Whether this option is available under
    /// `facts`.
    pub fn available(&self, facts: &FactSet) -> bool {
        self.gate.as_ref().map_or(true, |g| g.passes(facts))
    }
}

/// One line of conversation: who speaks, what is
/// said, where flow goes next, and any branch
/// options. All host data.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Stable host id, e.g. `"brief.open"`. Branch
    /// targets and `next` name lines by id.
    pub id: String,
    /// Speaker tag, shown in the dialogue box as
    /// written. A host word; never interpreted.
    pub speaker: String,
    /// The line's text, shown as written.
    pub text: String,
    /// Visibility gate; `None` means always visible.
    /// A gated-off line is skipped by flow, as if it
    /// were not in the list.
    pub gate: Option<Gate>,
    /// Where `advance` goes from this line: a line
    /// id. `None` means the following line in list
    /// order (or the end of the conversation).
    pub next: Option<String>,
    /// Branch options. A line with at least one
    /// available option is a choice point: flow
    /// continues by `choose`, not `advance`.
    pub options: Vec<ChoiceOption>,
}

impl Line {
    /// A plain line: no gate, no options, flow falls
    /// to the following line.
    pub fn new(id: &str, speaker: &str, text: &str) -> Line {
        Line {
            id: id.to_string(),
            speaker: speaker.to_string(),
            text: text.to_string(),
            gate: None,
            next: None,
            options: Vec::new(),
        }
    }

    /// This line, visible only while `gate` passes.
    pub fn gated(mut self, gate: Gate) -> Line {
        self.gate = Some(gate);
        self
    }

    /// This line, with `advance` continuing at the
    /// line named `next` instead of the following
    /// line.
    pub fn then(mut self, next: &str) -> Line {
        self.next = Some(next.to_string());
        self
    }

    /// This line, offering `options` as its branch
    /// choices (in the order given).
    pub fn choices(mut self, options: Vec<ChoiceOption>) -> Line {
        self.options = options;
        self
    }

    /// Whether this line is visible under `facts`.
    pub fn visible(&self, facts: &FactSet) -> bool {
        self.gate.as_ref().map_or(true, |g| g.passes(facts))
    }
}
