//! Sound propagation — traversal prices, perception judges.
//!
//! The division of labor is the whole design: traversal
//! writes events with position, surface, and magnitude. It
//! never decides who heard anything. This module propagates:
//! distance falloff per kind, occlusion by the same field
//! geometry the light uses (a wall that blocks light muffles
//! sound — the one-field law extended to hearing), and the
//! district-level scream, which is not ordinary sound at all.
//!
//! If it sounds loud, it is loud, to every ear in the field.
//! There is no "AI hearing" radius separate from the
//! propagation law — an ear hears what the law delivers to it.

use crate::perception::field::LightField;
use crate::traversal::events::{StimulusEvent, StimulusKind};
use glam::Vec3;

/// What an ear received: the event, the heard magnitude
/// after falloff and occlusion, and the direction it came
/// from (so a perceiver can turn toward it, or search
/// toward it, without any icon).
#[derive(Copy, Clone, Debug)]
pub struct HeardEvent {
    pub kind: StimulusKind,
    pub pos: Vec3,
    /// Magnitude at the ear, 0.0 (inaudible) upward.
    pub heard: f32,
    /// Unit direction from the ear toward the source.
    pub dir: Vec3,
}

/// Base hearing radius per kind, in metres. A mantle is a
/// soft close sound; a severed line rings down the street.
/// These are propagation constants, not perceiver stats —
/// the same event reaches every ear by the same law.
pub fn hear_radius(kind: StimulusKind) -> f32 {
    match kind {
        StimulusKind::Footstep => 18.0,
        StimulusKind::Strike => 30.0,
        StimulusKind::Landing => 25.0,
        StimulusKind::Mantle => 15.0,
        StimulusKind::Sever => 40.0,
        StimulusKind::SeamCross => 8.0,
        StimulusKind::StrikeFail => 12.0,
    }
}

/// Propagate one event to one ear. Returns None when the
/// event does not reach this ear at all. Falloff is linear
/// to the kind radius; an occluded path muffles to a
/// quarter — sound leaks through walls, it does not stop
/// at them, and it never arrives louder than it left.
pub fn propagate_one(
    event: &StimulusEvent,
    ear: Vec3,
    field: &LightField,
) -> Option<HeardEvent> {
    let d = (event.pos - ear).length();
    let radius = hear_radius(event.kind);
    if d >= radius {
        return None;
    }
    let fall = 1.0 - d / radius;
    let mut heard = event.magnitude * fall;
    if field.segment_blocked(event.pos, ear) {
        heard *= 0.25;
    }
    if heard < 0.02 {
        return None;
    }
    let dir = if d > 1e-6 {
        (event.pos - ear) / d
    } else {
        Vec3::X
    };
    Some(HeardEvent {
        kind: event.kind,
        pos: event.pos,
        heard,
        dir,
    })
}

/// Propagate a whole log to one ear. The caller passes the
/// events since the last tick (the log is tick-ordered;
/// perception never reorders it). Output order is input
/// order — determinism rides on the log, not on this pass.
pub fn propagate(
    events: &[StimulusEvent],
    ear: Vec3,
    field: &LightField,
) -> Vec<HeardEvent> {
    events
        .iter()
        .filter_map(|e| propagate_one(e, ear, field))
        .collect()
}

/// A district-level stimulus: the Type 2's scream, a
/// Weathervane's telephone report heard as a district
/// event, a mecha's ground tremor. District stimuli do not
/// obey room acoustics — they reach every ear in the
/// district, fading slowly with distance, because the
/// district itself is the medium. Radius is the district:
/// 500 metres, linear falloff, no occlusion (a scream
/// does not stop at a crate).
#[derive(Copy, Clone, Debug)]
pub struct DistrictStimulus {
    pub pos: Vec3,
    pub magnitude: f32,
    /// What the district is hearing: scream, report, tremor.
    /// A perceiver reacts to the kind, not to a number.
    pub kind: DistrictKind,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DistrictKind {
    /// The Type 2's scream — soul-trapped, district-wide.
    Scream,
    /// A Weathervane's telephone report, as the district
    /// hears it: someone, somewhere, has been reported.
    Report,
    /// Mecha ground tremor — felt more than heard.
    Tremor,
}

impl DistrictStimulus {
    pub const RADIUS: f32 = 500.0;

    /// Magnitude at an ear anywhere in the district.
    /// Never zero inside the radius — a district stimulus
    /// that fades to nothing is just a loud sound.
    pub fn heard_at(&self, ear: Vec3) -> f32 {
        let d = (self.pos - ear).length();
        if d >= Self::RADIUS {
            return 0.0;
        }
        (self.magnitude * (1.0 - d / Self::RADIUS)).max(0.05)
    }
}
