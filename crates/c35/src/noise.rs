//! Deterministic value noise — the only "asset factory" Comigen needs.
//!
//! Everything in the M1 vista is generated from these functions with a fixed
//! seed, so the native and browser hosts render the *same* world, bit for bit.

/// Fixed world seed for the M1 vista. Changing this changes the island.
pub const WORLD_SEED: u32 = 0xC35_0001;

/// Integer lattice hash -> [0, 1). Deterministic across platforms/targets:
/// pure integer math, no floats involved until the final scale.
fn lattice_hash(ix: i32, iz: i32, seed: u32) -> f32 {
    let mut h = seed ^ 0x9E37_79B9;
    h = h.wrapping_mul(0x85EB_CA6B) ^ (ix as u32).wrapping_mul(0xC2B2_AE35);
    h = h.wrapping_mul(0x27D4_EB2F) ^ (iz as u32).wrapping_mul(0x1656_6671);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Smooth value noise at (x, z) for the given octave seed.
pub fn value_noise(x: f32, z: f32, seed: u32) -> f32 {
    let ix = x.floor() as i32;
    let iz = z.floor() as i32;
    let fx = smooth(x - ix as f32);
    let fz = smooth(z - iz as f32);
    let a = lattice_hash(ix, iz, seed);
    let b = lattice_hash(ix + 1, iz, seed);
    let c = lattice_hash(ix, iz + 1, seed);
    let d = lattice_hash(ix + 1, iz + 1, seed);
    (a + (b - a) * fx + (c - a) * fz + (a - b - c + d) * fx * fz) * 2.0 - 1.0
}

/// Fractal Brownian motion, output roughly in [-1, 1).
pub fn fbm(x: f32, z: f32, octaves: u32, seed: u32) -> f32 {
    if octaves == 0 {
        return 0.0;
    }
    let mut sum = 0.0f32;
    let mut amp = 0.5f32;
    let mut norm = 0.0f32;
    let (mut px, mut pz) = (x, z);
    for o in 0..octaves {
        sum += amp * value_noise(px, pz, seed.wrapping_add(o * 0x1F1F_1F1F));
        norm += amp;
        // Rotate slightly per octave to hide lattice alignment.
        let (rx, rz) = (px * 0.8 - pz * 0.6, px * 0.6 + pz * 0.8);
        px = rx * 2.03;
        pz = rz * 2.03;
        amp *= 0.5;
    }
    if norm == 0.0 {
        return 0.0;
    }
    sum / norm
}

/// Ridged fBm — sharp crests, output in [0, 1]. The mountain maker.
pub fn ridged(x: f32, z: f32, octaves: u32, seed: u32) -> f32 {
    if octaves == 0 {
        return 0.0;
    }
    let mut sum = 0.0f32;
    let mut amp = 0.55f32;
    let mut norm = 0.0f32;
    let (mut px, mut pz) = (x, z);
    for o in 0..octaves {
        let n = 1.0 - value_noise(px, pz, seed.wrapping_add(o * 0x2B2B_2B2B)).abs();
        sum += amp * n * n;
        norm += amp;
        let (rx, rz) = (px * 0.83 - pz * 0.55, px * 0.55 + pz * 0.83);
        px = rx * 2.11;
        pz = rz * 2.11;
        amp *= 0.52;
    }
    if norm == 0.0 {
        return 0.0;
    }
    (sum / norm).clamp(0.0, 1.0)
}

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_octaves_are_finite() {
        assert_eq!(fbm(1.5, -2.0, 0, WORLD_SEED), 0.0);
        assert_eq!(ridged(1.5, -2.0, 0, WORLD_SEED), 0.0);
        assert!(fbm(0.2, 0.4, 4, WORLD_SEED).is_finite());
        let r = ridged(0.2, 0.4, 4, WORLD_SEED);
        assert!(r.is_finite() && (0.0..=1.0).contains(&r));
    }

    #[test]
    fn negative_coordinates_stay_finite() {
        let n = value_noise(-3.2, -0.1, WORLD_SEED);
        assert!(n.is_finite() && n.abs() <= 1.0);
    }
}
