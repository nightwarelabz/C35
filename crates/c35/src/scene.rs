//! The M1 vista: one deterministic procedural island at golden hour.
//!
//! Terrain is a generated heightfield mesh (no image assets anywhere). The
//! height function lives here in Rust; colour banding, sky, water and fog are
//! procedural in the shaders.

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};

use crate::noise::{fbm, ridged, smoothstep, WORLD_SEED};

/// World extent of the terrain square (world units, centred on origin).
pub const TERRAIN_SIZE: f32 = 520.0;
/// Grid resolution of the heightfield (vertices per side).
pub const TERRAIN_GRID: u32 = 257;
/// Sea level.
pub const WATER_LEVEL: f32 = 0.0;

/// Golden-hour sun: direction *toward* the sun, and its colour (HDR).
/// Elevation ~9.5 degrees — long shadows, warm light.
pub fn sun_direction() -> Vec3 {
    // Azimuth chosen so the sun hangs over the bay in the hero pose.
    Vec3::new(-0.62, 0.165, -0.77).normalize()
}

pub fn sun_color() -> Vec3 {
    Vec3::new(1.0, 0.52, 0.24) * 3.1
}

/// The hero camera pose — THE proof pose, identical on every host.
/// Drift (when enabled) starts here.
pub struct CameraPose {
    pub eye: Vec3,
    pub target: Vec3,
    pub fov_y_deg: f32,
}

pub const HERO_POSE: CameraPose = CameraPose {
    eye: Vec3::new(58.0, 7.5, 80.0),
    target: Vec3::new(-45.0, 17.0, -70.0),
    fov_y_deg: 55.0,
};

/// Camera pose at time `t` seconds: hero pose plus a slow drift/orbit.
/// `t == 0` is exactly the hero pose.
pub fn camera_at(t: f32) -> CameraPose {
    let az = (t * 0.010).sin() * 0.055; // slow azimuth sway, ±3.2 degrees
    let bob = (t * 0.05).sin() * 0.7;
    let (s, c) = az.sin_cos();
    let rel = HERO_POSE.eye - HERO_POSE.target;
    let rotated = Vec3::new(
        rel.x * c - rel.z * s,
        rel.y,
        rel.x * s + rel.z * c,
    );
    CameraPose {
        eye: HERO_POSE.target + rotated + Vec3::new(0.0, bob, 0.0),
        target: HERO_POSE.target + Vec3::new(0.0, (t * 0.03).sin() * 1.2, 0.0),
        fov_y_deg: HERO_POSE.fov_y_deg,
    }
}

/// Island height function — the single source of truth for the terrain.
/// A crescent island: massif to the north-west, a secondary headland to the
/// north-east, and a bay carved open to the south-east where the hero
/// camera floats.
pub fn height_at(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let r = p.length();

    // Island body: radial falloff, carved away toward the south-east.
    let fall = smoothstep(1.10, 0.45, r / (TERRAIN_SIZE * 0.5));
    let dir = if r > 0.001 { p / r } else { Vec2::ZERO };
    let se_dot = dir.dot(Vec2::new(0.7071, 0.7071)); // +1 toward SE
    let bay = smoothstep(0.95, 0.05, se_dot); // 1 toward NW, 0 toward SE
    let land = (fall * (0.22 + 0.78 * bay)).clamp(0.0, 1.0);

    // Massif to the north-west; lower headland to the north-east.
    let massif = smoothstep(215.0, 45.0, (p - Vec2::new(-70.0, -85.0)).length());
    let headland = smoothstep(175.0, 25.0, (p - Vec2::new(185.0, -125.0)).length());

    let base = fbm(x * 0.0045, z * 0.0045, 5, WORLD_SEED) * 11.0;
    let ridge = ridged(x * 0.0058, z * 0.0058, 5, WORLD_SEED ^ 0x51AB);
    let ridge2 = ridged(x * 0.0072 + 9.0, z * 0.0072, 4, WORLD_SEED ^ 0x3D9E);
    let detail = fbm(x * 0.028, z * 0.028, 3, WORLD_SEED ^ 0x77C1) * 1.5;

    let mut h = -7.0 + land * 10.5;
    h += (base + detail) * (0.22 + 0.78 * land);
    h += ridge * ridge * 50.0 * massif * land + massif * 8.0 * land;
    h += headland * (11.0 + ridge2 * ridge2 * 30.0);

    // Gentle shore shelf: soften the crossing at sea level for wide beaches.
    if h.abs() < 2.2 {
        h *= 0.62;
    }
    h
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

pub struct TerrainMesh {
    pub vertices: Vec<TerrainVertex>,
    pub indices: Vec<u32>,
}

impl TerrainMesh {
    pub fn triangle_count(&self) -> u64 {
        (self.indices.len() / 3) as u64
    }
}

/// Generate the heightfield mesh on the CPU from `height_at`.
pub fn generate_terrain() -> TerrainMesh {
    let n = TERRAIN_GRID;
    let half = TERRAIN_SIZE * 0.5;
    let step = TERRAIN_SIZE / (n - 1) as f32;
    let mut vertices = Vec::with_capacity((n * n) as usize);
    for j in 0..n {
        for i in 0..n {
            let x = -half + i as f32 * step;
            let z = -half + j as f32 * step;
            let y = height_at(x, z);
            // Normal from central differences of the height function.
            let e = step;
            let dx = height_at(x + e, z) - height_at(x - e, z);
            let dz = height_at(x, z + e) - height_at(x, z - e);
            let normal = Vec3::new(-dx, 2.0 * e, -dz).normalize();
            vertices.push(TerrainVertex {
                position: [x, y, z],
                normal: normal.to_array(),
            });
        }
    }
    let mut indices = Vec::with_capacity(((n - 1) * (n - 1) * 6) as usize);
    for j in 0..n - 1 {
        for i in 0..n - 1 {
            let a = j * n + i;
            let b = a + 1;
            let c = a + n;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    TerrainMesh { vertices, indices }
}
