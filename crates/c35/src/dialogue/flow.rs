//! Dialogue flow: conversations in progress.
//!
//! A [`Dialogue`] is the session: it owns the
//! host's fact store and at most one active
//! conversation. Starting a second conversation over
//! an unfinished one is refused — the host finishes
//! (or keeps) the conversation it has.
//!
//! Flow is deterministic and data-driven:
//!
//! - Lines are visited in list order, or at the
//!   line a `next` / choice target names.
//! - A gated-off line is invisible: flow skips it
//!   (following its own `next`, or list order) as
//!   if it were not in the list. Skips are bounded
//!   by the line count, so even a cycle of gated
//!   lines ends the conversation instead of looping.
//! - A line with at least one available option is
//!   a choice point. `advance` refuses there; the
//!   host reports the player's pick with `choose`,
//!   indexing the available options in the order
//!   [`Dialogue::current`] presented them.
//! - Facts are evaluated live: a `set_fact` between
//!   two reads changes what the next `current`,
//!   `advance`, or `choose` sees. The engine stores
//!   facts; it never sets one itself.
//! - The session keeps a per-conversation record of
//!   every line flow arrives at (including repeat
//!   arrivals) and every successful choice. Starting
//!   a new conversation resets that record; facts do
//!   not reset.
//!
//! The cue handed to the UI box is
//! [`crate::ui::DialogueCue`], derived at each read:
//! `None` while a choice is pending (the box shows
//! no advance glyph; the host renders the options),
//! `More` when a visible line follows, `End` on the
//! last visible line.
//!
//! Every refused call returns a plain
//! [`DialogueError`] and changes nothing.

use crate::ui::DialogueCue;

use super::model::{ChoiceOption, FactSet, FactValue, Line};

/// A plain error for refused dialogue calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogueError {
    /// `start` was called while a conversation is
    /// still unfinished.
    AlreadyActive,
    /// `advance` / `choose` with no conversation
    /// started.
    NoConversation,
    /// `advance` / `choose` on a conversation that
    /// has already finished.
    Finished,
    /// `choose` on a line offering no available
    /// choice right now (a plain line, or a choice
    /// line whose options are all gated off).
    NoChoice,
    /// `advance` on a choice point: the host must
    /// report a selection with `choose` first.
    ChoiceRequired,
    /// `choose` index is outside the available
    /// options presented by `current`.
    OutOfRange,
    /// The chosen option's gate does not pass at
    /// choose time.
    GatedOff,
    /// A `next` or choice target names no line in
    /// the conversation (refused at `start`).
    UnknownTarget,
    /// Two lines share an id (refused at `start`).
    DuplicateId,
    /// `start` was given no lines at all.
    Empty,
}

/// The box-facing state of the current line:
/// exactly what the host feeds the UI dialogue box
/// (`speaker`, `text`, `cue`), plus the branch
/// options to present when the line is a choice
/// point.
#[derive(Debug, Clone, PartialEq)]
pub struct Current<'a> {
    /// Id of the current line (host word).
    pub id: &'a str,
    /// Speaker tag for the box.
    pub speaker: &'a str,
    /// Body text for the box.
    pub text: &'a str,
    /// Advance indicator for the box.
    pub cue: DialogueCue,
    /// Available option texts, in host order,
    /// gated options filtered out. `choose` indexes
    /// into this list. Empty unless the line is a
    /// choice point.
    pub options: Vec<&'a str>,
}

impl Current<'_> {
    /// True when the line offers a branch the host
    /// can pick from right now.
    pub fn is_choice_point(&self) -> bool {
        !self.options.is_empty()
    }
}

/// One conversation's runtime state: the lines,
/// where flow currently sits (`None` once finished),
/// and the tracking record for this conversation.
#[derive(Debug, Clone)]
struct Conversation {
    lines: Vec<Line>,
    pos: Option<usize>,
    visited: Vec<String>,
    visit_counts: Vec<(String, usize)>,
    choices_taken: Vec<(String, usize)>,
}

impl Conversation {
    /// Record an arrival at the current line. Repeat
    /// arrivals are appended and counted again.
    fn record_visit(&mut self) {
        let Some(idx) = self.pos else {
            return;
        };
        let id = self.lines[idx].id.clone();
        self.visited.push(id.clone());
        if let Some((_, count)) = self
            .visit_counts
            .iter_mut()
            .find(|(seen, _)| seen == &id)
        {
            *count += 1;
        } else {
            self.visit_counts.push((id, 1));
        }
    }
}

/// The dialogue session: the host's facts and the
/// one conversation in progress.
#[derive(Debug, Clone, Default)]
pub struct Dialogue {
    facts: FactSet,
    active: Option<Conversation>,
}

impl Dialogue {
    /// A session with no facts and no conversation.
    pub fn new() -> Dialogue {
        Dialogue {
            facts: FactSet::new(),
            active: None,
        }
    }

    /// Set a fact (bool or number). Facts gate lines
    /// and options, are evaluated live on the next
    /// read, and persist across conversations in
    /// this session. The host owns every fact; the
    /// engine never sets one.
    pub fn set_fact(&mut self, name: &str, value: FactValue) {
        self.facts.set(name, value);
    }

    /// Read a fact back. `None` if never set.
    pub fn fact(&self, name: &str) -> Option<FactValue> {
        self.facts.get(name)
    }

    /// Start a conversation from host-supplied
    /// lines. The data is validated once, here:
    /// duplicate ids and dangling targets are
    /// refused now, not mid-conversation. Refused
    /// while a previous conversation is unfinished;
    /// a finished one may be replaced freely.
    ///
    /// Flow starts at the first visible line; if
    /// every line is gated off, the conversation is
    /// finished from the start. Starting resets the
    /// visited and choices-taken records; the facts
    /// persist.
    pub fn start(&mut self, lines: Vec<Line>) -> Result<(), DialogueError> {
        if let Some(c) = &self.active {
            if c.pos.is_some() {
                return Err(DialogueError::AlreadyActive);
            }
        }
        if lines.is_empty() {
            return Err(DialogueError::Empty);
        }
        for (i, l) in lines.iter().enumerate() {
            if lines[..i].iter().any(|m| m.id == l.id) {
                return Err(DialogueError::DuplicateId);
            }
        }
        for l in &lines {
            if let Some(t) = &l.next {
                if !lines.iter().any(|m| &m.id == t) {
                    return Err(DialogueError::UnknownTarget);
                }
            }
            for o in &l.options {
                if !lines.iter().any(|m| m.id == o.target) {
                    return Err(DialogueError::UnknownTarget);
                }
            }
        }
        let pos = resolve_visible(&lines, &self.facts, Some(0));
        let mut conversation = Conversation {
            lines,
            pos,
            visited: Vec::new(),
            visit_counts: Vec::new(),
            choices_taken: Vec::new(),
        };
        conversation.record_visit();
        self.active = Some(conversation);
        Ok(())
    }

    /// The current line's box-facing state, or
    /// `None` when no conversation is running or the
    /// running one has finished.
    pub fn current(&self) -> Option<Current<'_>> {
        let conv = self.active.as_ref()?;
        let idx = conv.pos?;
        let line = &conv.lines[idx];
        let options = available_options(line, &self.facts);
        let cue = if !options.is_empty() {
            DialogueCue::None
        } else if has_next_visible(&conv.lines, &self.facts, idx) {
            DialogueCue::More
        } else {
            DialogueCue::End
        };
        Some(Current {
            id: &line.id,
            speaker: &line.speaker,
            text: &line.text,
            cue,
            options: options.iter().map(|o| o.text.as_str()).collect(),
        })
    }

    /// Move past the current line: to its `next`
    /// target, or the following visible line, or the
    /// end of the conversation. Refused with no
    /// conversation, after the finish, and on a
    /// choice point (use [`Dialogue::choose`]).
    pub fn advance(&mut self) -> Result<(), DialogueError> {
        let (lines, idx) = self.position()?;
        if !available_options(&lines[idx], &self.facts).is_empty() {
            return Err(DialogueError::ChoiceRequired);
        }
        let next = resolve_visible(&lines, &self.facts, successor(&lines, idx));
        let conv = self.active.as_mut().expect("position checked");
        conv.pos = next;
        conv.record_visit();
        Ok(())
    }

    /// Report the player's selection: `index` into
    /// the available options [`Dialogue::current`]
    /// presented, in order. Flow continues at the
    /// option's target line (skipping gated lines
    /// from there, as always). Refused with no
    /// conversation, after the finish, on a line
    /// offering no available choice, for an index
    /// outside the presented list, and if the picked
    /// option's gate fails at choose time.
    pub fn choose(&mut self, index: usize) -> Result<(), DialogueError> {
        let (lines, idx) = self.position()?;
        let available = available_options(&lines[idx], &self.facts);
        if available.is_empty() {
            return Err(DialogueError::NoChoice);
        }
        let picked = available
            .get(index)
            .copied()
            .ok_or(DialogueError::OutOfRange)?;
        if !picked.available(&self.facts) {
            return Err(DialogueError::GatedOff);
        }
        let target = index_of(&lines, &picked.target).expect("target validated at start");
        let next = resolve_visible(&lines, &self.facts, Some(target));
        let line_id = lines[idx].id.clone();
        let conv = self.active.as_mut().expect("position checked");
        conv.choices_taken.push((line_id, index));
        conv.pos = next;
        conv.record_visit();
        Ok(())
    }

    /// True when no conversation is running, or the
    /// running one has run past its last visible
    /// line.
    pub fn is_finished(&self) -> bool {
        match &self.active {
            None => true,
            Some(c) => c.pos.is_none(),
        }
    }

    /// Line ids in the order flow presented them in
    /// the current conversation. A line reached more
    /// than once appears once per arrival. The record
    /// remains readable after the conversation
    /// finishes, until the next `start` resets it.
    pub fn visited(&self) -> &[String] {
        match &self.active {
            Some(c) => &c.visited,
            None => &[],
        }
    }

    /// Per-line visit counts for the current
    /// conversation, in first-presentation order.
    pub fn visit_counts(&self) -> &[(String, usize)] {
        match &self.active {
            Some(c) => &c.visit_counts,
            None => &[],
        }
    }

    /// How many times flow has presented `id` in the
    /// current conversation.
    pub fn visit_count(&self, id: &str) -> usize {
        self.visit_counts()
            .iter()
            .find(|(seen, _)| seen == id)
            .map_or(0, |(_, count)| *count)
    }

    /// Choices taken in the current conversation, as
    /// `(line id, presented option index)` pairs in
    /// the order they were chosen.
    pub fn choices_taken(&self) -> &[(String, usize)] {
        match &self.active {
            Some(c) => &c.choices_taken,
            None => &[],
        }
    }

    /// The shared refusal checks for `advance` and
    /// `choose`, returning a snapshot of the lines
    /// and the current index.
    fn position(&self) -> Result<(Vec<Line>, usize), DialogueError> {
        let conv = self.active.as_ref().ok_or(DialogueError::NoConversation)?;
        let idx = conv.pos.ok_or(DialogueError::Finished)?;
        Ok((conv.lines.clone(), idx))
    }
}

/// Options of `line` available under `facts`, in
/// host order.
fn available_options<'a>(line: &'a Line, facts: &FactSet) -> Vec<&'a ChoiceOption> {
    line.options.iter().filter(|o| o.available(facts)).collect()
}

/// Index of the line named `id`.
fn index_of(lines: &[Line], id: &str) -> Option<usize> {
    lines.iter().position(|l| l.id == id)
}

/// The line flow moves to after `idx`, before any
/// visibility skipping: the `next` target if the
/// line declares one, else the following line in
/// list order, else nothing.
fn successor(lines: &[Line], idx: usize) -> Option<usize> {
    match &lines[idx].next {
        Some(t) => index_of(lines, t),
        None => {
            if idx + 1 < lines.len() {
                Some(idx + 1)
            } else {
                None
            }
        }
    }
}

/// From `start`, walk successors until a visible
/// line; gated lines are skipped through their own
/// successors. Bounded by the line count: a cycle
/// made only of gated lines resolves to `None`
/// (the conversation ends) rather than looping.
fn resolve_visible(lines: &[Line], facts: &FactSet, start: Option<usize>) -> Option<usize> {
    let mut cur = start;
    let mut steps = 0usize;
    while let Some(i) = cur {
        if lines[i].visible(facts) {
            return Some(i);
        }
        cur = successor(lines, i);
        steps += 1;
        if steps > lines.len() {
            return None;
        }
    }
    None
}

/// Whether any visible line follows `idx`.
fn has_next_visible(lines: &[Line], facts: &FactSet, idx: usize) -> bool {
    resolve_visible(lines, facts, successor(lines, idx)).is_some()
}
