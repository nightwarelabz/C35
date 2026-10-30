//! The practical-light field — ONE FIELD LAW (DESIGN.md, perception).
//!
//! The lamps that light the block are data: head positions over the
//! street. This module evaluates the field at any point — falloff
//! with distance from the head, a downward cone factor, and true
//! occlusion by the block's building volumes (segment vs AABB). The
//! SAME evaluation does three jobs, so the game can never lie with
//! light:
//!
//! 1. it is baked into the block mesh's vertex `light` terms (the
//!    pools the frame shows),
//! 2. it scores streak decals and window spill at build time,
//! 3. it answers the light read: lit / unlit at the operator's
//!    position, logged per frame of the proof drives.
//!
//! If a frame looks safe, it is safe — the read and the render are
//! one function of one field.

use c35::glam::Vec3;

/// Exposure at or above this value is LIT for the read. Tuned once
/// against the field's own numbers (see block.rs): pool centres
/// read 0.7–0.95, the shadow route reads 0.00–0.06 except its two
/// honest lamp blips (~0.39) — the threshold sits in the empty band
/// between the street's two regimes.
pub const LIT_THRESHOLD: f32 = 0.22;

/// Field reach: past this 3D distance from a head, a lamp
/// contributes exactly nothing.
pub const FIELD_RADIUS: f32 = 11.0;
/// Lamp strength (the field's unit exposure at a pool centre is
/// ~0.9 with this intensity).
pub const LAMP_INTENSITY: f32 = 1.6;

/// One sodium lamp: the head's world position (the fixture hangs
/// over the street from its pole).
#[derive(Copy, Clone, Debug)]
pub struct Lamp {
    pub head: Vec3,
}

/// An occluding volume (axis-aligned): a building, the kiosk, a
/// container — anything that stands between a lamp and a point.
#[derive(Copy, Clone, Debug)]
pub struct Occluder {
    pub min: Vec3,
    pub max: Vec3,
}

impl Occluder {
    pub fn new(min: Vec3, max: Vec3) -> Occluder {
        Occluder { min, max }
    }
}

/// The block's light field: lamps + occluders, evaluated anywhere.
pub struct LightField {
    pub lamps: Vec<Lamp>,
    pub occluders: Vec<Occluder>,
}

/// Segment vs AABB (slab method). `t0`/`t1` trim the segment's ends:
/// a lamp head inside its own pole's volume, or a point standing
/// inside a doorway's reveal, must not self-occlude — the caller
/// passes the sample point slightly raised and the test ignores the
/// first/last sliver of the path.
fn segment_blocked(a: Vec3, b: Vec3, occ: &Occluder) -> bool {
    let d = b - a;
    let mut tmin = 0.04f32;
    let mut tmax = 0.96f32;
    for axis in 0..3 {
        let (a_c, d_c, lo, hi) = match axis {
            0 => (a.x, d.x, occ.min.x, occ.max.x),
            1 => (a.y, d.y, occ.min.y, occ.max.y),
            _ => (a.z, d.z, occ.min.z, occ.max.z),
        };
        if d_c.abs() < 1e-8 {
            if a_c < lo || a_c > hi {
                return false;
            }
        } else {
            let inv = 1.0 / d_c;
            let mut t1 = (lo - a_c) * inv;
            let mut t2 = (hi - a_c) * inv;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return false;
            }
        }
    }
    true
}

impl LightField {
    /// Exposure at a world point from one lamp (0 when out of reach
    /// or occluded). The sample point is the thing being lit — the
    /// read samples the operator at chest height; the bake samples
    /// surface vertices at their own positions.
    pub fn lamp_exposure(&self, lamp: &Lamp, p: Vec3) -> f32 {
        let to = p - lamp.head;
        let d = to.length();
        if d >= FIELD_RADIUS || d < 1e-4 {
            return 0.0;
        }
        // Falloff: a smooth pool — full near the head's ground
        // point, gone at the field radius.
        let fall = (1.0 - (d / FIELD_RADIUS) * (d / FIELD_RADIUS)).powf(1.5);
        // Cone: the fixture throws down. The factor is the squared
        // cosine of the angle off straight-down, so wall tops across
        // the street gather spill, not pools.
        let down_cos = (-to.y / d).clamp(0.0, 1.0);
        let cone = down_cos * down_cos;
        for occ in &self.occluders {
            if segment_blocked(lamp.head, p, occ) {
                return 0.0;
            }
        }
        LAMP_INTENSITY * fall * cone
    }

    /// Total exposure at a point: the sum over lamps, clamped to
    /// the bake range the shader expects.
    pub fn exposure(&self, p: Vec3) -> f32 {
        let mut e = 0.0f32;
        for lamp in &self.lamps {
            e += self.lamp_exposure(lamp, p);
        }
        e.min(1.2)
    }

    /// The read: is this point lit?
    pub fn lit(&self, p: Vec3) -> bool {
        self.exposure(p) >= LIT_THRESHOLD
    }

    /// Nearest lamp head to a point (for the ledger's colour: which
    /// pool a reading belongs to).
    pub fn nearest_lamp(&self, p: Vec3) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32)> = None;
        for (i, lamp) in self.lamps.iter().enumerate() {
            let d = lamp.head.distance(p);
            if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((i, d));
            }
        }
        best
    }
}
