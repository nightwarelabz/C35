//! The ONE FIELD — practical light, grace, and occlusion.
//!
//! The one-field law: a single practical-light field serves the
//! renderer AND every perceiver. There is no separate "AI vision"
//! number anywhere in this crate. If a screenshot looks safe, it
//! is safe — because the perceiver samples the same lamp data
//! the renderer draws.
//!
//! Discipline, stated plainly: the host feeds the SAME lamp
//! positions, radii, and intensities to [`LightField`] and to
//! the renderer's lamp set. This module cannot enforce that from
//! inside the crate — it is the host's one-field duty — but
//! every perceiver in this module reads only this field, never
//! a private copy. A second field is a second truth, and the
//! law forbids it.
//!
//! Grace is a field like light: the Wanderer's ground. Near it,
//! suspicion rises slower, scent breaks, the Neighbour lowers
//! his notebook, and the visor barely registers her. It is
//! sampled the same way, from the same substrate.

use glam::Vec3;

/// The tone of a practical light. Tone matters twice: sodium
/// pools read warm to the eye (and to the visor rim), and the
/// sense view blooms emitters by tone, not by brightness alone.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Sodium street lamps — Greyport's night signature.
    Sodium,
    /// Moonlight — the cool key.
    Moon,
    /// Interior / white practicals.
    White,
}

/// One practical light: position, reach, strength, tone.
/// Strength 1.0 is a full lamp pool; 0.0 is unlit.
#[derive(Copy, Clone, Debug)]
pub struct Emitter {
    pub pos: Vec3,
    pub radius: f32,
    pub intensity: f32,
    pub tone: Tone,
}

/// An axis-aligned occluder box: a wall, a crate stack, a
/// building mass. Occluders dim light across them and muffle
/// sound through them — the same geometry both channels read.
#[derive(Copy, Clone, Debug)]
pub struct Occluder {
    pub min: Vec3,
    pub max: Vec3,
}

impl Occluder {
    /// Slab test: does segment a→b pass through this box?
    /// Deterministic, no allocation. A segment grazing exactly
    /// on a face counts as blocked — a grazing ray is not a
    /// sightline anyone should trust.
    pub fn blocks(&self, a: Vec3, b: Vec3) -> bool {
        let d = b - a;
        let mut tmin = 0.0f32;
        let mut tmax = 1.0f32;
        for i in 0..3 {
            let o = a[i];
            let dir = d[i];
            let lo = self.min[i];
            let hi = self.max[i];
            if dir.abs() < 1e-8 {
                if o < lo || o > hi {
                    return false;
                }
            } else {
                let inv = 1.0 / dir;
                let mut t0 = (lo - o) * inv;
                let mut t1 = (hi - o) * inv;
                if t0 > t1 {
                    std::mem::swap(&mut t0, &mut t1);
                }
                if t0 > tmin {
                    tmin = t0;
                }
                if t1 < tmax {
                    tmax = t1;
                }
                if tmin > tmax {
                    return false;
                }
            }
        }
        true
    }
}

/// What a light sample returns: overall level plus the sodium
/// share (the visor rim and the sense view both want the
/// sodium share, not just the level).
#[derive(Copy, Clone, Debug, Default)]
pub struct LightSample {
    /// 0.0 (void) to ~1.0+ (full lamp pool). Clamped at 1.5
    /// so overlapping pools cannot run away.
    pub level: f32,
    /// 0.0 (no sodium) to 1.0 (pure sodium pool).
    pub sodium: f32,
}

/// The practical-light field: ambient night + emitters +
/// occluders. Sampled at any position, including the eye.
#[derive(Debug, Default)]
pub struct LightField {
    /// Night ambient: the floor no shadow goes below. Deep
    /// blue-grey readable, never void black.
    pub ambient: f32,
    pub emitters: Vec<Emitter>,
    pub occluders: Vec<Occluder>,
}

impl LightField {
    pub fn new(ambient: f32) -> LightField {
        LightField {
            ambient: ambient.clamp(0.0, 1.0),
            emitters: Vec::new(),
            occluders: Vec::new(),
        }
    }

    pub fn add_emitter(&mut self, e: Emitter) {
        self.emitters.push(e);
    }

    pub fn add_occluder(&mut self, o: Occluder) {
        self.occluders.push(o);
    }

    /// Is the segment a→b blocked by any occluder? The one
    /// line-of-sight question, shared by vision and sound.
    pub fn segment_blocked(&self, a: Vec3, b: Vec3) -> bool {
        self.occluders.iter().any(|o| o.blocks(a, b))
    }

    /// Sample the field at a position. Each emitter
    /// contributes intensity × linear falloff; an emitter
    /// whose light must cross an occluder to reach the point
    /// contributes only its leak (×0.15) — practical light
    /// spills, it does not teleport. A sample is never
    /// brighter than 1.5: overlapping pools saturate, they
    /// do not stack into a second sun.
    pub fn sample(&self, pos: Vec3) -> LightSample {
        let mut level = self.ambient;
        let mut sodium_acc = 0.0f32;
        let mut sodium_w = 0.0f32;
        for e in &self.emitters {
            let d = (e.pos - pos).length();
            if d >= e.radius {
                continue;
            }
            let fall = 1.0 - d / e.radius;
            let mut contrib = e.intensity * fall;
            if self.segment_blocked(e.pos, pos) {
                contrib *= 0.15;
            }
            level += contrib;
            if e.tone == Tone::Sodium {
                sodium_acc += contrib;
            }
            sodium_w += contrib;
        }
        let sodium = if sodium_w > 1e-6 {
            (sodium_acc / sodium_w).clamp(0.0, 1.0)
        } else {
            0.0
        };
        LightSample {
            level: level.clamp(0.0, 1.5),
            sodium,
        }
    }

    /// Line of sight from an eye to a point: clear when no
    /// occluder crosses the segment. Vision needs BOTH light
    /// and a clear line; this is the line half.
    pub fn los_clear(&self, eye: Vec3, target: Vec3) -> bool {
        !self.segment_blocked(eye, target)
    }
}

/// Grace — the Wanderer's field. Sources are grounds she has
/// blessed: a temple courtyard, raked gravel she has walked.
/// Grace is sampled like light and priced like weather: it
/// does not hide the player, it slows the world's certainty
/// about him.
#[derive(Debug, Default)]
pub struct GraceField {
    /// (position, radius) per grace source.
    pub sources: Vec<(Vec3, f32)>,
}

impl GraceField {
    pub fn new() -> GraceField {
        GraceField {
            sources: Vec::new(),
        }
    }

    pub fn add_source(&mut self, pos: Vec3, radius: f32) {
        self.sources.push((pos, radius));
    }

    /// Grace at a position, 0.0 (none) to 1.0 (at her feet).
    /// Overlapping sources saturate, they do not stack.
    pub fn grace_at(&self, pos: Vec3) -> f32 {
        let mut g = 0.0f32;
        for (p, r) in &self.sources {
            let d = (*p - pos).length();
            if d < *r {
                let v = 1.0 - d / *r;
                if v > g {
                    g = v;
                }
            }
        }
        g.clamp(0.0, 1.0)
    }
}
