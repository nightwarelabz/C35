//! Dialogue: the conversation system behind the
//! UI dialogue box.
//!
//! The UI module provides only the box — a speaker
//! tag, wrapped text, and a More / End / None cue
//! (`ui::widgets::UiScreen::dialogue`). This module
//! is the system that fills it:
//!
//! - [`model`]: the host's conversation data —
//!   lines, branch options, gates — plus the fact
//!   store the gates read. All host words and host
//!   values; the engine interprets none of it.
//! - [`flow`]: the session in progress ([`flow::Dialogue`]).
//!   It tracks flow state only and hands the host a
//!   [`flow::Current`] carrying exactly the box's
//!   inputs; the host renders it with one
//!   `dialogue()` call and feeds the player's pick
//!   back as a choice index.
//!
//! Seams the host owns:
//!
//! - All content: line ids, speakers, texts, option
//!   labels, branch targets, and when a conversation
//!   starts are host data. New conversations need
//!   no engine change.
//! - All facts: the host sets named facts (bool or
//!   number) with `set_fact`; gates are data (a fact
//!   name plus a comparison) evaluated against them.
//!   What a fact means in the world is the host's
//!   business; the engine never sets a fact and
//!   never reads game state.
//! - Voice and audio: which line is spoken, by which
//!   voice, and when, is the sound module's job.
//!   This module returns text and state; it routes
//!   nothing to a speaker.
//! - Presentation beyond the box: portraits, name
//!   colours, and choice-list layout are the host's
//!   rendering choices. Timing and animation belong
//!   to the host too — flow moves when the host
//!   calls, never on a clock.
//!
//! Deliberately NOT here: story content (no script,
//! no campaign material, no baked conversation —
//! conversations are host data); outcome effects
//! (a choice leads to a line — what else it changes
//! in the world is the host's business, applied
//! host-side from its own facts); an abandon verb
//! (a started conversation runs to its finish —
//! starting another over it is refused); any
//! filesystem touch.

pub mod flow;
pub mod model;

pub use flow::{Current, Dialogue, DialogueError};
pub use model::{ChoiceOption, FactSet, FactValue, Gate, GateOp, Line};
