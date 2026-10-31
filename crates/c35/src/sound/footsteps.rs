//! Footsteps: the animator's foot-plant events, made audible.
//!
//! The timbre comes from the SAME surface record traversal
//! prices from — `SurfaceRecord` kind + wetness — so the ear
//! hears what the field priced. Gain is NOT recomputed here:
//! the caller passes the heard magnitude the propagation
//! layer delivered (falloff + occlusion already applied),
//! and the mixer multiplies it. Playback never re-prices.
//!
//! Per-surface reads, deliberately distinct at 20 metres:
//! wood knocks hollow, metal rings bright, concrete thuds
//! dull, stone crunches gravel, asphalt thuds soft, brick
//! sits between stone and concrete. Wetness adds a splash
//! (a second, slower noise burst) and stretches the decay —
//! wet stone reads differently from dry stone, never just
//! louder.
//!
//! Mod props flow through the same fields: the mod registry's
//! placement contract carries a footstep surface per prop
//! (`modding::Footstep`), and `from_mod_footstep` maps it onto
//! this same record — a mod's fruit cart on wet stone sounds
//! like wet stone, because it IS wet stone to the field.

use crate::modding::Footstep as ModFootstep;
use crate::sound::synth::{CompositeVoice, NoiseBurst, Ring, Thump, VoiceGen};
use crate::traversal::surface::{SurfaceKind, SurfaceRecord};

/// Build a footstep voice for a surface, or None when the
/// surface makes no sound (sky, or a mod prop with footstep
/// surface None). The caller supplies the seed; the mixer
/// supplies one per spawn, deterministically.
pub fn footstep_voice(
    surface: SurfaceRecord,
    seed: u64,
    sample_rate: f32,
) -> Option<Box<dyn VoiceGen>> {
    let wet = surface.wetness.clamp(0.0, 1.0);
    // Wetness stretches the body decay a little and adds a
    // splash burst scaled by wetness. Dry surfaces: no splash.
    let decay_stretch = 1.0 + 0.4 * wet;
    let mut parts: Vec<(Box<dyn VoiceGen>, f32)> = Vec::new();

    match surface.kind {
        SurfaceKind::Wood => {
            // Hollow knock: pitch-drop thump + a short woody
            // resonance partial, both quick.
            parts.push((
                Box::new(Thump::new(
                    160.0,
                    70.0,
                    0.06 * decay_stretch,
                    0.20,
                    sample_rate,
                )),
                0.9,
            ));
            parts.push((
                Box::new(Ring::new(320.0, &[(1.0, 0.35)], 0.12, 0.04, sample_rate)),
                0.5,
            ));
            parts.push((
                Box::new(NoiseBurst::new(seed, 900.0, 0.08, 0.002, 0.03, sample_rate)),
                0.25,
            ));
        }
        SurfaceKind::Metal => {
            // Bright ring: inharmonic partials, long decay.
            parts.push((
                Box::new(Ring::new(
                    220.0,
                    &[(1.0, 0.6), (2.76, 0.35), (5.40, 0.2)],
                    0.45 * decay_stretch,
                    0.12,
                    sample_rate,
                )),
                0.8,
            ));
            parts.push((
                Box::new(NoiseBurst::new(
                    seed,
                    4000.0,
                    0.04,
                    0.001,
                    0.012,
                    sample_rate,
                )),
                0.3,
            ));
        }
        SurfaceKind::Concrete => {
            // Dull thud: low drop + heavily lowpassed noise.
            parts.push((
                Box::new(Thump::new(
                    100.0,
                    45.0,
                    0.07 * decay_stretch,
                    0.20,
                    sample_rate,
                )),
                1.0,
            ));
            parts.push((
                Box::new(NoiseBurst::new(seed, 400.0, 0.10, 0.002, 0.04, sample_rate)),
                0.4,
            ));
        }
        SurfaceKind::Stone => {
            // Gravel crunch: thump + brighter noise than concrete.
            parts.push((
                Box::new(Thump::new(
                    120.0,
                    55.0,
                    0.06 * decay_stretch,
                    0.18,
                    sample_rate,
                )),
                0.9,
            ));
            parts.push((
                Box::new(NoiseBurst::new(
                    seed,
                    1400.0,
                    0.14,
                    0.002,
                    0.05,
                    sample_rate,
                )),
                0.55,
            ));
        }
        SurfaceKind::Brick => {
            // Between stone and concrete: crunch, duller.
            parts.push((
                Box::new(Thump::new(
                    110.0,
                    50.0,
                    0.06 * decay_stretch,
                    0.18,
                    sample_rate,
                )),
                0.9,
            ));
            parts.push((
                Box::new(NoiseBurst::new(
                    seed,
                    900.0,
                    0.12,
                    0.002,
                    0.045,
                    sample_rate,
                )),
                0.5,
            ));
        }
        SurfaceKind::Asphalt => {
            // Soft thud: the quietest surface at the same speed.
            parts.push((
                Box::new(Thump::new(
                    90.0,
                    45.0,
                    0.06 * decay_stretch,
                    0.16,
                    sample_rate,
                )),
                0.8,
            ));
            parts.push((
                Box::new(NoiseBurst::new(
                    seed,
                    300.0,
                    0.09,
                    0.002,
                    0.035,
                    sample_rate,
                )),
                0.3,
            ));
        }
        SurfaceKind::Sky => return None,
    }

    if wet > 0.0 {
        // Splash: a slower, brighter noise burst, gain and
        // length both scaled by wetness.
        parts.push((
            Box::new(NoiseBurst::new(
                seed ^ 0x5DEECE66D,
                2500.0,
                0.10 + 0.20 * wet,
                0.005,
                0.08 + 0.10 * wet,
                sample_rate,
            )),
            0.4 * wet,
        ));
    }

    Some(Box::new(CompositeVoice::new(parts)))
}

/// Map a mod registry footstep surface onto the shared
/// record. WetStone is stone at full wetness — the weather
/// field's value when the record is written, not a second
/// surface kind. None means the prop makes no footstep
/// sound at all (dressing with no tread).
pub fn from_mod_footstep(footstep: ModFootstep) -> Option<SurfaceRecord> {
    match footstep {
        ModFootstep::Wood => Some(SurfaceRecord::dry(SurfaceKind::Wood)),
        ModFootstep::Metal => Some(SurfaceRecord::dry(SurfaceKind::Metal)),
        ModFootstep::Concrete => Some(SurfaceRecord::dry(SurfaceKind::Concrete)),
        ModFootstep::WetStone => Some(SurfaceRecord::wet(SurfaceKind::Stone, 1.0)),
        ModFootstep::None => None,
    }
}
