//! The vista: one deterministic procedural island.
//!
//! Terrain is a generated heightfield mesh (no image assets anywhere). The
//! height function lives here in Rust; colour banding, sky, water and fog
//! are procedural in the shaders. Light, season and time itself belong to
//! the reality layer ([`crate::world`]) — this module owns only the land.
//!
//! M3: the heightfield is built as a **tile grid** (4×4 tiles of 64×64
//! cells) so the frame can frustum-cull per tile and pick a density LOD
//! per tile. LODs are index-density variants over the tile's own full-res
//! vertices — no second geometry, so a tile border is the *same vertices*
//! at every density. Where a dense tile meets a sparse one, the sparse
//! tile's border drops a **skirt** (a short vertical apron) that hides
//! the chord-vs-polyline sliver a T-junction would otherwise open; the
//! visibility system additionally keeps neighbours within one LOD step.

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};

use crate::noise::{fbm, ridged, smoothstep, WORLD_SEED};

/// World extent of the terrain square (world units, centred on origin).
pub const TERRAIN_SIZE: f32 = 520.0;
/// Grid resolution of the heightfield (vertices per side).
pub const TERRAIN_GRID: u32 = 257;
/// Sea level.
pub const WATER_LEVEL: f32 = 0.0;

/// Cells per tile side at full density. (TERRAIN_GRID − 1) / TILE_CELLS
/// tiles per side: 256 / 64 = 4 → a 4×4 grid.
pub const TILE_CELLS: u32 = 64;
pub const TILES_PER_SIDE: u32 = (TERRAIN_GRID - 1) / TILE_CELLS;
pub const TILE_COUNT: usize = (TILES_PER_SIDE * TILES_PER_SIDE) as usize;
/// How far a reduced-LOD tile's border skirt drops (world units). Deep
/// enough to cover the worst T-junction sliver at vista scale, shallow
/// enough to stay inside the terrain it aprons.
pub const SKIRT_DROP: f32 = 3.0;

/// The hero camera pose — THE proof pose, identical on every host.
/// Drift (when enabled) starts here.
#[derive(Copy, Clone, Debug)]
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

/// The grazing pose: low over the bay, looking across the island. The
/// M3 audit note: despite the name, frustum culling removes *nothing*
/// here — from this corner every tile centre sits within ~41° of the
/// view axis and the ~93-unit bounding spheres all intersect the
/// frustum (verified per-tile by the `frustum_probe` example). What
/// bites at this pose is distance LOD: the far shore is 400+ units out
/// and the pass submits ~43.5k of the 131k full-density triangles. The
/// hero pose is where culling earns (7 of 16 tiles behind/beside the
/// camera). Used by the headless host's `--pose grazing` proof run.
pub const GRAZING_POSE: CameraPose = CameraPose {
    eye: Vec3::new(238.0, 5.0, 238.0),
    target: Vec3::new(-60.0, 22.0, -80.0),
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
/// Terrain generation v2 (M4): the base landform ([`height_base`]),
/// domain-warped and displaced on its steep faces ([`cliff_relief`]).
/// The M3 art pass proved the headwall's flatness was geometric, and
/// measuring the v1 landform showed why: the bay-mask rim is a nearly
/// vertical band only ~2 grid cells wide, with a nearly constant normal
/// field — no albedo work, and no purely vertical displacement at
/// resolvable wavelengths, can model it. So on steep ground the sample
/// point itself is warped (the wall advances and retreats along its
/// length — buttresses and gullies in plan) and the warped height takes
/// a ridged vertical displacement (ribs, benches). Both are masked to
/// zero on gentle ground: the meadows, shores and bay floor are
/// vertex-for-vertex the M1–M3 island.
pub fn height_at(x: f32, z: f32) -> f32 {
    let raw = height_base(x, z);
    // Relief mask from the RAW landform at this point: nothing below
    // the shore band (beaches and the bay floor keep the exact M1–M3
    // profile, and the winter ice line with it), nothing on gentle
    // slopes, full on faces steeper than ~53°.
    if raw <= 1.0 {
        return raw;
    }
    let e = 2.0;
    let gx = (height_base(x + e, z) - height_base(x - e, z)) / (2.0 * e);
    let gz = (height_base(x, z + e) - height_base(x, z - e)) / (2.0 * e);
    let up = 1.0 / (1.0 + gx * gx + gz * gz).sqrt();
    let mask = smoothstep(0.90, 0.60, up) * smoothstep(1.0, 5.0, raw);
    if mask <= 0.0 {
        return raw;
    }
    // Domain warp at wall scale (principal wavelength ~26 units,
    // ±8-unit reach at full mask): steep faces sample the landform
    // from shifted points, so the rim juts and notches along its length
    // instead of running as one smooth curtain edge.
    let wx = fbm(x * 0.030 + 13.7, z * 0.030 + 3.1, 3, WORLD_SEED ^ 0x9A01) * 8.0;
    let wz = fbm(x * 0.030 + 7.9, z * 0.030 + 19.3, 3, WORLD_SEED ^ 0x9A02) * 8.0;
    let (sx, sz) = (x + wx * mask, z + wz * mask);
    let h0 = height_base(sx, sz);
    h0 + cliff_relief(sx, sz, h0) * mask
}

/// The vertical half of the cliff relief: ridged multifractal
/// displacement at wall scale (principal wavelengths ≈28 → 14 → 6.5
/// world units — the 257² grid's 2.03-unit step resolves all three,
/// and the headwall tiles draw at full density at the proof poses).
/// Gullies carve deeper than ribs rise — real cliff faces are more cut
/// than built. A low-amplitude bench term, phased by height and warped
/// by low-frequency noise, lays strata ledges across the face. The
/// caller applies the slope/shore mask.
fn cliff_relief(x: f32, z: f32, h0: f32) -> f32 {
    let r = ridged(x * 0.035, z * 0.035, 3, WORLD_SEED ^ 0xC11F);
    let ribs = (r - 0.52) * 2.0;
    let disp = ribs * 2.6 + ribs.min(0.0) * 1.6;
    let warp = fbm(x * 0.010, z * 0.010, 2, WORLD_SEED ^ 0xBE11) * 5.0;
    let bench = (h0 * 0.42 + warp).sin() * 0.6;
    disp + bench
}

/// The base landform (terrain generation v1, M1–M3): a crescent island —
/// massif to the north-west, a secondary headland to the north-east, and
/// a bay carved open to the south-east where the hero camera floats.
fn height_base(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let r = p.length();

    // Island body: radial falloff, carved away toward the south-east.
    let fall = smoothstep(1.10, 0.45, r / (TERRAIN_SIZE * 0.5));
    let dir = if r > 0.001 { p / r } else { Vec2::ZERO };
    // Four decimals, not `FRAC_1_SQRT_2`: this direction is part of the
    // pinned landform. Substituting the longer constant moves every
    // height sample.
    #[allow(clippy::approx_constant)]
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
    /// Baked horizon ambient occlusion in [0, 1] (M5): the fraction
    /// of the sky hemisphere visible from this vertex over the height
    /// field itself, horizon-mapped at fold scale. 1 = open sky.
    /// Computed by [`HeightGrid::bake_occlusion`]; consumed by the
    /// terrain shader against the hemisphere ambient — the term that
    /// lights shadowed folds, where the M4 relief otherwise read flat.
    pub ao: f32,
}

/// The generated heightfield sampled on its own lattice, kept around
/// so derived fields (normals already come from `height_at` directly;
/// M5 adds horizon occlusion) can be baked from the *actual* relief —
/// gullies, ribs and benches included — instead of re-deriving noise.
///
/// Occlusion is horizon mapping, heightfield-style: from each vertex,
/// eight azimuths are walked out to fold scale (≈28 world units — the
/// relief's longest wavelength) and the highest horizon angle per
/// azimuth, corrected by the local tangent plane's tilt in that
/// azimuth, gives the blocked fraction of that azimuth's sky as
/// cos²(effective horizon). The vertex value is the azimuth mean.
/// Pure f32 math in a fixed order over a fixed lattice — the bake is
/// deterministic, like everything else in this module.
pub struct HeightGrid {
    heights: Vec<f32>, // TERRAIN_GRID × TERRAIN_GRID, row-major (j = z)
    step: f32,
    half: f32,
}

impl Default for HeightGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl HeightGrid {
    pub fn new() -> Self {
        let n = TERRAIN_GRID as usize;
        let half = TERRAIN_SIZE * 0.5;
        let step = TERRAIN_SIZE / (TERRAIN_GRID - 1) as f32;
        let mut heights = Vec::with_capacity(n * n);
        for j in 0..n {
            for i in 0..n {
                heights.push(height_at(
                    -half + i as f32 * step,
                    -half + j as f32 * step,
                ));
            }
        }
        HeightGrid { heights, step, half }
    }

    /// Bilinear height at an arbitrary point (clamped to the field).
    fn height(&self, x: f32, z: f32) -> f32 {
        let n = TERRAIN_GRID as f32;
        let fx = ((x + self.half) / self.step).clamp(0.0, n - 1.001);
        let fz = ((z + self.half) / self.step).clamp(0.0, n - 1.001);
        let (i0, j0) = (fx.floor() as usize, fz.floor() as usize);
        let (tx, tz) = (fx - i0 as f32, fz - j0 as f32);
        let w = TERRAIN_GRID as usize;
        let h00 = self.heights[j0 * w + i0];
        let h10 = self.heights[j0 * w + i0 + 1];
        let h01 = self.heights[(j0 + 1) * w + i0];
        let h11 = self.heights[(j0 + 1) * w + i0 + 1];
        (h00 * (1.0 - tx) + h10 * tx) * (1.0 - tz) + (h01 * (1.0 - tx) + h11 * tx) * tz
    }

    /// Horizon ambient occlusion at a terrain point, in [0, 1]:
    /// 1 = the whole sky hemisphere reaches the point, lower = the
    /// folds around it block sky. See the type docs for the
    /// formulation. (The same walks yield a bent normal — the
    /// visibility-weighted average open-sky direction — but M5's A/B
    /// convicted it against this engine's two-stop hemisphere
    /// ambient: sampling the gradient along the bent direction
    /// raises the ambient almost exactly as much as the visibility
    /// dims it on steep faces — face mean |Δ| 1.5 vs 6.0 for the
    /// scalar alone — so the scalar ships and the bent direction
    /// waits for a directional ambient. See the M5 report §1.)
    pub fn bake_occlusion(&self, x: f32, z: f32) -> f32 {
        // Radii walk from the tight folds beside the vertex out
        // past the relief's longest wavelength (~28 units) to the
        // wall's own bulk at 40 — the massif is the largest occluder
        // its folds have. Geometrically spaced so the near folds are
        // sampled densest.
        const RADII: [f32; 8] = [1.5, 2.6, 4.4, 7.3, 12.0, 19.0, 28.0, 40.0];
        let h0 = self.height(x, z);
        // Local gradient (lattice central differences) — the tangent
        // plane the horizon angles are judged against.
        let e = self.step;
        let gx = (self.height(x + e, z) - self.height(x - e, z)) / (2.0 * e);
        let gz = (self.height(x, z + e) - self.height(x, z - e)) / (2.0 * e);
        let mut visible = 0.0f32;
        for k in 0..8 {
            let a = (k as f32) * std::f32::consts::FRAC_PI_4;
            let (ux, uz) = (a.cos(), a.sin());
            // Horizon angle above horizontal along this azimuth —
            // the terrain's silhouette elevation, which is NEGATIVE
            // where the ground falls away (hence no zero floor here:
            // flooring it would charge downhill azimuths occlusion
            // they do not have once the tilt is subtracted below).
            let mut horizon = f32::NEG_INFINITY;
            for &r in &RADII {
                let dh = self.height(x + ux * r, z + uz * r) - h0;
                horizon = horizon.max((dh / r).atan());
            }
            // Judge the horizon against the local tangent plane: an
            // obstacle only occludes past the plane's own tilt toward
            // that azimuth. (On an infinite plane this yields exactly
            // zero occlusion, as it must.)
            let tilt = (gx * ux + gz * uz).atan();
            let eff = (horizon - tilt).clamp(0.0, std::f32::consts::FRAC_PI_2);
            let c = eff.cos();
            visible += c * c;
        }
        (visible * 0.125).clamp(0.0, 1.0)
    }
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

/// Generate the heightfield mesh on the CPU from `height_at` — the whole
/// grid as one mesh. Kept for the `heights` dev example; the renderer
/// consumes the tiled form ([`generate_tiles`]).
pub fn generate_terrain() -> TerrainMesh {
    let grid = HeightGrid::new();
    let n = TERRAIN_GRID;
    let half = TERRAIN_SIZE * 0.5;
    let step = TERRAIN_SIZE / (n - 1) as f32;
    let mut vertices = Vec::with_capacity((n * n) as usize);
    for j in 0..n {
        for i in 0..n {
            let x = -half + i as f32 * step;
            let z = -half + j as f32 * step;
            vertices.push(sample_vertex(x, z, step, &grid));
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

fn sample_vertex(x: f32, z: f32, step: f32, grid: &HeightGrid) -> TerrainVertex {
    let y = height_at(x, z);
    // Normal from central differences of the height function.
    let e = step;
    let dx = height_at(x + e, z) - height_at(x - e, z);
    let dz = height_at(x, z + e) - height_at(x, z - e);
    let normal = Vec3::new(-dx, 2.0 * e, -dz).normalize();
    TerrainVertex {
        position: [x, y, z],
        normal: normal.to_array(),
        ao: grid.bake_occlusion(x, z),
    }
}

/// Density level of a tile draw. Index-density only: all three levels
/// index the tile's single full-res vertex buffer.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Lod {
    Full,
    Half,
    Quarter,
}

impl Lod {
    /// Vertex stride over the tile grid for this level.
    pub fn stride(&self) -> u32 {
        match self {
            Lod::Full => 1,
            Lod::Half => 2,
            Lod::Quarter => 4,
        }
    }
    pub fn slot(&self) -> usize {
        match self {
            Lod::Full => 0,
            Lod::Half => 1,
            Lod::Quarter => 2,
        }
    }
}

/// One tile's draw decision for a frame (produced by the visibility
/// system, consumed by the renderer).
#[derive(Copy, Clone, Debug)]
pub struct TileDraw {
    pub tile: usize,
    pub lod: Lod,
}

/// One terrain tile: its own full-res vertices (borders duplicate the
/// neighbour's edge vertices — the same heights by construction) plus
/// three index buffers, one per LOD. Reduced LODs carry border skirts.
pub struct TerrainTile {
    pub index: usize,
    /// Bounding sphere for frustum tests (centre, radius), from the
    /// tile's actual height range.
    pub center: Vec3,
    pub radius: f32,
    pub min_y: f32,
    pub max_y: f32,
    pub vertices: Vec<TerrainVertex>,
    /// Index buffers by LOD slot (0 = Full, 1 = Half, 2 = Quarter).
    pub lod_indices: [Vec<u32>; 3],
}

impl TerrainTile {
    pub fn lod_triangles(&self, lod: Lod) -> u64 {
        (self.lod_indices[lod.slot()].len() / 3) as u64
    }
}

/// Build one tile's index buffer at the given stride. Reduced strides get
/// a border skirt: the perimeter vertices are duplicated `SKIRT_DROP`
/// lower (appended to `vertices`) and stitched with quads. Skirt quads
/// are emitted in both windings — a skirt is gap insurance and must read
/// from either side; the cost is a few hundred triangles per tile.
fn build_lod_indices(
    stride: u32,
    vertices: &mut Vec<TerrainVertex>,
    local: u32, // vertices per side in the tile grid (TILE_CELLS + 1)
) -> Vec<u32> {
    let cells = TILE_CELLS / stride;
    let at = |a: u32, b: u32| -> u32 { (b * stride) * local + a * stride };
    let mut indices: Vec<u32> = Vec::with_capacity((cells * cells * 6) as usize);
    for b in 0..cells {
        for a in 0..cells {
            let v00 = at(a, b);
            let v10 = at(a + 1, b);
            let v01 = at(a, b + 1);
            let v11 = at(a + 1, b + 1);
            indices.extend_from_slice(&[v00, v01, v10, v10, v01, v11]);
        }
    }
    if stride > 1 {
        // Perimeter ring of the strided grid, in order: top row L→R,
        // right column T→B, bottom row R→L, left column B→T.
        let mut ring: Vec<u32> = Vec::with_capacity((cells * 4) as usize);
        for a in 0..=cells {
            ring.push(at(a, 0));
        }
        for b in 1..=cells {
            ring.push(at(cells, b));
        }
        for a in (0..cells).rev() {
            ring.push(at(a, cells));
        }
        for b in (1..cells).rev() {
            ring.push(at(0, b));
        }
        // Skirt vertices: perimeter positions dropped by SKIRT_DROP.
        let skirt_base = vertices.len() as u32;
        for &vi in &ring {
            let mut v = vertices[vi as usize];
            v.position[1] -= SKIRT_DROP;
            vertices.push(v);
        }
        let n = ring.len() as u32;
        for k in 0..n {
            let k2 = (k + 1) % n;
            let (r0, r1) = (ring[k as usize], ring[k2 as usize]);
            let (s0, s1) = (skirt_base + k, skirt_base + k2);
            indices.extend_from_slice(&[r0, s0, r1, r1, s0, s1]);
            indices.extend_from_slice(&[r0, r1, s0, r1, s1, s0]);
        }
    }
    indices
}

/// Generate one tile by its grid index (0..TILE_COUNT, row-major over
/// the 4×4 grid: ti = index % 4 east, tj = index / 4 south). The height
/// grid is shared across tiles so border vertices — duplicated per
/// tile by construction — bake identical occlusion from it.
pub fn generate_tile(index: usize, grid: &HeightGrid) -> TerrainTile {
    let half = TERRAIN_SIZE * 0.5;
    let step = TERRAIN_SIZE / (TERRAIN_GRID - 1) as f32;
    let local = TILE_CELLS + 1;
    let ti = (index as u32) % TILES_PER_SIDE;
    let tj = (index as u32) / TILES_PER_SIDE;
    let i0 = ti * TILE_CELLS;
    let j0 = tj * TILE_CELLS;
    let mut vertices: Vec<TerrainVertex> = Vec::with_capacity((local * local) as usize + 512);
    let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
    for b in 0..local {
        for a in 0..local {
            let x = -half + (i0 + a) as f32 * step;
            let z = -half + (j0 + b) as f32 * step;
            let v = sample_vertex(x, z, step, grid);
            min_y = min_y.min(v.position[1]);
            max_y = max_y.max(v.position[1]);
            vertices.push(v);
        }
    }
    // LOD index buffers; reduced levels append their skirt vertices to
    // the tile's vertex list as they are built.
    let full = build_lod_indices(1, &mut vertices, local);
    let half_lod = build_lod_indices(2, &mut vertices, local);
    let quarter = build_lod_indices(4, &mut vertices, local);
    let x0 = -half + i0 as f32 * step;
    let x1 = -half + (i0 + TILE_CELLS) as f32 * step;
    let z0 = -half + j0 as f32 * step;
    let z1 = -half + (j0 + TILE_CELLS) as f32 * step;
    let center = Vec3::new((x0 + x1) * 0.5, (min_y + max_y) * 0.5, (z0 + z1) * 0.5);
    let radius =
        Vec3::new((x1 - x0) * 0.5, (max_y - min_y) * 0.5, (z1 - z0) * 0.5).length() + 0.5;
    TerrainTile {
        index,
        center,
        radius,
        min_y,
        max_y,
        vertices,
        lod_indices: [full, half_lod, quarter],
    }
}

/// Generate the whole island as tiles. Same heights, same seed — the
/// tiled island is vertex-for-vertex the M1/M2 island along every seam.
pub fn generate_tiles() -> Vec<TerrainTile> {
    let grid = HeightGrid::new();
    (0..TILE_COUNT).map(|i| generate_tile(i, &grid)).collect()
}
