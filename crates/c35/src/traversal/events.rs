//! Stimulus events — traversal's output into the shared fields.
//!
//! Traversal never decides who heard or saw anything. It writes
//! events in the shape the (ABSENT) sound/perception layers read:
//! tick, kind, position, surface, magnitude. The animator's
//! footstep track and traversal's footfalls share this one log —
//! extended, not duplicated.

use crate::traversal::surface::SurfaceRecord;
use glam::Vec3;

/// What happened, in the vocabulary perception prices.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StimulusKind {
    /// A ground footfall (gait or wall-run step).
    Footstep,
    /// The hook strike at its impact point.
    Strike,
    /// A landing, magnitude priced by impact speed.
    Landing,
    /// A mantle completing onto a ledge.
    Mantle,
    /// A line severed by damage — a rare, priced failure.
    Sever,
    /// A step-ring seam crossing on height-field terrain.
    SeamCross,
    /// A strike that failed to bite (sky, out of reach).
    StrikeFail,
}

/// One priced event. Deterministic order: the log appends in tick
/// order, and ticks never repeat within a run.
#[derive(Copy, Clone, Debug)]
pub struct StimulusEvent {
    pub tick: u64,
    pub kind: StimulusKind,
    pub pos: Vec3,
    pub surface: SurfaceRecord,
    pub magnitude: f32,
}

/// The run's event log. Plain Vec, tick-ordered, allocation only
/// on push — the sim path never reorders it.
#[derive(Debug, Default)]
pub struct EventLog {
    pub events: Vec<StimulusEvent>,
}

impl EventLog {
    pub fn new() -> EventLog {
        EventLog { events: Vec::new() }
    }

    pub fn push(
        &mut self,
        tick: u64,
        kind: StimulusKind,
        pos: Vec3,
        surface: SurfaceRecord,
        magnitude: f32,
    ) {
        self.events.push(StimulusEvent {
            tick,
            kind,
            pos,
            surface,
            magnitude,
        });
    }

    pub fn count(&self, kind: StimulusKind) -> usize {
        self.events.iter().filter(|e| e.kind == kind).count()
    }
}
