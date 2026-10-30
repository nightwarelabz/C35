//! The sense view — the visor's presentation of the
//! same fields, never a second truth.
//!
//! The sense view renders the world as signals: emitters
//! bloom by tone, attention reads as tone and sound,
//! mecha components read as states at sense range. It
//! NEVER shows an enemy's state as an icon — posture
//! and sound still carry that, per the perception law —
//! and it NEVER shows the player's own scent picture.
//! That last rule is structural, not polite: this module
//! takes no scent input at all, so there is nothing to
//! leak. The debrief grades scent after the fact; the
//! visor does not grade it live.
//!
//! The visor rim is the FP stealth read: a sodium rim
//! on the visor edge, driven by the light field at the
//! eye. If the frame looks lit, the perceivers' field
//! says lit — same field, same answer, no second sense.

use crate::perception::field::LightField;
use crate::perception::mecha::{
    CannonState, CoolantState, LocoState, SensorState, Type1Governor,
};
use glam::Vec3;

/// An emitter as the sense view blooms it: position,
/// intensity at the eye, tone. Bloom is presentation —
/// the field already priced the light; this shapes how
/// the visor glass shows it.
#[derive(Copy, Clone, Debug)]
pub struct Bloom {
    pub pos: Vec3,
    pub intensity: f32,
    pub sodium: f32,
}

/// Attention as tone, not icon: the direction attention
/// is coming from, and whether it reads calm, wary, or
/// alarmed. A tone is analog and uncertain — an icon
/// would be certain, and certainty is what the law
/// forbids the visor to claim.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AttentionTone {
    Calm,
    Wary,
    Alarmed,
}

#[derive(Copy, Clone, Debug)]
pub struct AttentionRead {
    pub dir: Vec3,
    pub tone: AttentionTone,
}

/// A mecha as the sense view reads it: position and
/// component states, at sense range only. Component
/// states are the visor's legitimate read — the design
/// grants them, and the ladder is built from them.
/// No health, no number, no bar.
#[derive(Copy, Clone, Debug)]
pub struct MechaRead {
    pub pos: Vec3,
    pub sensors: SensorState,
    pub locomotion: LocoState,
    pub cannon: CannonState,
    pub coolant: CoolantState,
}

/// The sense view for one eye at one moment: blooms,
/// attention tones, mecha reads, and the visor rim.
/// Built from the same fields the perceivers read.
/// No scent. No icons. No states-as-markers.
#[derive(Clone, Debug, Default)]
pub struct SenseView {
    pub blooms: Vec<Bloom>,
    pub tones: Vec<AttentionRead>,
    pub mechas: Vec<MechaRead>,
    /// Visor rim intensity 0.0–1.0: sodium light at
    /// the eye, edging the visor glass. The FP stealth
    /// read — lit frame, lit field.
    pub rim: f32,
}

impl SenseView {
    /// Sense range for mecha component reads, in
    /// metres. Beyond it, the visor shows blooms and
    /// tones only — components are a close read, not
    /// a radar.
    pub const MECHA_RANGE: f32 = 60.0;

    /// Build the sense view. `emitter_positions` are
    /// the field's emitters (the host passes the field
    /// itself and this reads its list — one field, one
    /// list). `attentions` are (direction, level)
    /// pairs from nearby perceivers. `mechas` are
    /// (position, governor) pairs. Own scent is not
    /// an argument, so it cannot appear.
    pub fn build(
        field: &LightField,
        eye: Vec3,
        attentions: &[(Vec3, f32)],
        mechas: &[(Vec3, &Type1Governor)],
    ) -> SenseView {
        let mut view = SenseView::default();
        for e in &field.emitters {
            let d = (e.pos - eye).length();
            if d >= e.radius * 2.0 {
                continue;
            }
            let fall = (1.0 - d / (e.radius * 2.0)).max(0.0);
            let mut intensity = e.intensity * fall;
            if field.segment_blocked(e.pos, eye) {
                intensity *= 0.15;
            }
            if intensity > 0.02 {
                view.blooms.push(Bloom {
                    pos: e.pos,
                    intensity,
                    sodium: if e.tone == crate::perception::field::Tone::Sodium {
                        1.0
                    } else {
                        0.0
                    },
                });
            }
        }
        for (dir, level) in attentions {
            let tone = if *level >= 0.85 {
                AttentionTone::Alarmed
            } else if *level >= 0.45 {
                AttentionTone::Wary
            } else {
                AttentionTone::Calm
            };
            view.tones.push(AttentionRead {
                dir: *dir,
                tone,
            });
        }
        for (pos, gov) in mechas {
            if (*pos - eye).length() <= Self::MECHA_RANGE {
                view.mechas.push(MechaRead {
                    pos: *pos,
                    sensors: gov.sensors,
                    locomotion: gov.locomotion,
                    cannon: gov.cannon,
                    coolant: gov.coolant,
                });
            }
        }
        view.rim = visor_rim(field, eye);
        view
    }
}

/// The visor rim: sodium-weighted light at the eye,
/// 0.0–1.0. A sodium pool at the eye rims the glass
/// warm; moonlight barely rims it at all. The rim is
/// the stealth read the player trusts — because it is
/// the field the perceivers trust, at the same point.
pub fn visor_rim(field: &LightField, eye: Vec3) -> f32 {
    let sample = field.sample(eye);
    (sample.level * sample.sodium).clamp(0.0, 1.0)
}
