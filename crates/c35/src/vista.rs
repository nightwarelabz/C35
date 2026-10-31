//! The vista, expressed through the data-oriented core.
//!
//! M3 moves the scene out of the renderer's hard-coded draw list and
//! into the ECS ([`crate::ecs`]): the camera, the reality layer's world
//! state, the sixteen terrain tiles, the water and the sky are entities
//! with components in flat arenas, and two named systems run over them
//! each frame —
//!
//! - `reality-bind` writes this frame's camera pose and world state
//!   into their components (the renderer consumes components, never
//!   ad-hoc arguments, from here on);
//! - `visibility` frustum-culls the terrain tiles against the camera
//!   and picks a density LOD per tile by distance — fanned out over the
//!   job pool in chunks — then repeats the frustum test against the
//!   sun's shadow frustum for the caster pass, clamps neighbouring
//!   tiles to within one LOD step of each other (index-density LODs
//!   share border vertices, and the reduced levels carry skirts, so
//!   seams stay shut), and collects the frame's two draw lists.
//!
//!   One asymmetry, convicted by the M3 winter-morning proof: **shadow
//!   casters always draw at full density.** The first cut picked caster
//!   LODs by distance from the light like the camera's — and the
//!   headwall's self-shadow eroded (a stride-4 caster is a sieve at
//!   shadow-texel scale; the lit face leaked warm through it). Camera
//!   LOD is a resolution choice; shadow LOD is a correctness choice.
//!   The caster pass still frustum-culls against the light.
//!
//! The update allocates nothing per frame: arenas and scratch are sized
//! at build, draw lists are cleared and refilled.

use glam::{Mat4, Vec3, Vec4};

use crate::ecs::{Arena, CoreStats, Entity, JobPool, SystemDef};
use crate::scene::{CameraPose, Lod, TileDraw, TILES_PER_SIDE, TILE_COUNT};
use crate::world::WorldState;

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Terrain tile component: identity + bounding sphere (the geometry
/// itself lives in [`crate::scene::TerrainTile`] and on the GPU).
#[derive(Copy, Clone, Debug)]
pub struct TileComp {
    pub index: usize,
    pub center: Vec3,
    pub radius: f32,
}

/// Unit components: the water and the sky are single entities whose
/// presence in the world is the component.
#[derive(Copy, Clone, Debug)]
pub struct WaterComp;
#[derive(Copy, Clone, Debug)]
pub struct SkyComp;

/// Per-tile draw accounting for one frame, printed in the ledger.
#[derive(Copy, Clone, Default, Debug)]
pub struct CullStats {
    pub tiles_total: usize,
    /// Camera pass.
    pub tiles_drawn: usize,
    pub tiles_culled: usize,
    /// Triangles if every tile drew at full density (the M1/M2 total).
    pub tris_before: u64,
    /// Triangles actually submitted to the camera pass after culling+LOD.
    pub tris_after: u64,
    /// Shadow (caster) pass.
    pub shadow_drawn: usize,
    pub shadow_culled: usize,
    pub shadow_tris: u64,
}

// ---------------------------------------------------------------------------
// Frustum
// ---------------------------------------------------------------------------

/// Six planes extracted from a view-projection matrix (Gribb–Hartmann),
/// wgpu clip conventions (z ∈ [0, w]). Planes point inward: a sphere is
/// visible when its signed distance to every plane is ≥ −radius.
#[derive(Copy, Clone, Debug)]
pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    pub fn from_view_proj(m: &Mat4) -> Frustum {
        let a = m.to_cols_array();
        let row = |r: usize| Vec4::new(a[r], a[4 + r], a[8 + r], a[12 + r]);
        let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
        let norm = |p: Vec4| {
            let l = Vec3::new(p.x, p.y, p.z).length().max(1e-9);
            p / l
        };
        Frustum {
            planes: [
                norm(r3 + r0), // left
                norm(r3 - r0), // right
                norm(r3 + r1), // bottom
                norm(r3 - r1), // top
                norm(r2),      // near (z ≥ 0 in wgpu clip space)
                norm(r3 - r2), // far
            ],
        }
    }

    pub fn sphere_visible(&self, center: Vec3, radius: f32) -> bool {
        self.planes
            .iter()
            .all(|p| p.x * center.x + p.y * center.y + p.z * center.z + p.w >= -radius)
    }

    /// The six inward planes as vec4s — the exact floats the GPU cull
    /// pass (M4) tests against, so both paths judge the same frustum.
    pub fn planes(&self) -> [Vec4; 6] {
        self.planes
    }
}

/// LOD step (0 = Full, 1 = Half, 2 = Quarter) for a sphere at `dist`
/// (surface distance: centre distance minus radius, clamped at 0).
/// Bands are wide against the 130-unit tile size: a tile stays full
/// density while it can fill real screen space, and the far shore —
/// 300+ units out at the hero pose — drops to quarter density, where
/// the detail fade has already calmed its shading.
fn lod_step_for(dist: f32) -> u8 {
    if dist < 140.0 {
        0
    } else if dist < 300.0 {
        1
    } else {
        2
    }
}

fn lod_of(step: u8) -> Lod {
    match step {
        0 => Lod::Full,
        1 => Lod::Half,
        _ => Lod::Quarter,
    }
}

#[derive(Copy, Clone)]
struct TileVis {
    visible: bool,
    step: u8,
}

impl Default for TileVis {
    fn default() -> Self {
        TileVis {
            visible: false,
            step: 2,
        }
    }
}

// ---------------------------------------------------------------------------
// The world
// ---------------------------------------------------------------------------

/// Per-frame inputs the renderer hands to the systems (the systems are
/// plain `fn` pointers, so the frame's arguments ride on the world).
struct FrameInputs {
    pose: CameraPose,
    world: WorldState,
    view_proj: Mat4,
    light_view_proj: Mat4,
    shadow_light: bool,
}

pub struct Vista {
    pub cameras: Arena<CameraPose>,
    pub worlds: Arena<WorldState>,
    pub tiles: Arena<TileComp>,
    pub waters: Arena<WaterComp>,
    pub skies: Arena<SkyComp>,
    camera_e: Entity,
    world_e: Entity,
    /// Bounding spheres in tile-index order, chunk-friendly for the pool.
    spheres: Vec<(Vec3, f32)>,
    /// Per-LOD triangle counts per tile (from the generated geometry).
    lod_tris: Vec<[u64; 3]>,
    systems: Vec<SystemDef<Vista>>,
    frame: Option<FrameInputs>,
    vis: Vec<TileVis>,
    light_vis: Vec<TileVis>,
    draws: Vec<TileDraw>,
    shadow_draws: Vec<TileDraw>,
    pub cull: CullStats,
}

impl Vista {
    /// Build the scene's entities from generated tile metadata:
    /// (centre, radius, per-LOD triangle counts) per tile, in tile order.
    pub fn new(tile_meta: &[(Vec3, f32, [u64; 3])]) -> Vista {
        let mut v = Vista {
            cameras: Arena::new(),
            worlds: Arena::new(),
            tiles: Arena::new(),
            waters: Arena::new(),
            skies: Arena::new(),
            camera_e: Entity(0),
            world_e: Entity(1),
            spheres: Vec::with_capacity(tile_meta.len()),
            lod_tris: Vec::with_capacity(tile_meta.len()),
            systems: Vec::new(),
            frame: None,
            vis: vec![TileVis::default(); TILE_COUNT],
            light_vis: vec![TileVis::default(); TILE_COUNT],
            draws: Vec::with_capacity(TILE_COUNT),
            shadow_draws: Vec::with_capacity(TILE_COUNT),
            cull: CullStats::default(),
        };
        v.cameras.insert(v.camera_e, crate::scene::HERO_POSE);
        // Placeholder world state until the first update binds reality
        // (a fixed epoch instant at the equator fallback observer).
        let placeholder = crate::world::world_state(0.0, &crate::world::Observer::unknown());
        v.worlds.insert(v.world_e, placeholder);
        for (i, &(center, radius, tris)) in tile_meta.iter().enumerate() {
            v.tiles.insert(
                Entity(2 + i as u32),
                TileComp {
                    index: i,
                    center,
                    radius,
                },
            );
            v.spheres.push((center, radius));
            v.lod_tris.push(tris);
        }
        v.waters.insert(Entity(2 + TILE_COUNT as u32), WaterComp);
        v.skies.insert(Entity(3 + TILE_COUNT as u32), SkyComp);
        v.cull.tiles_total = tile_meta.len();
        v.cull.tris_before = tile_meta.iter().map(|t| t.2[0]).sum();
        v.systems.push(SystemDef {
            name: "reality-bind",
            run: sys_reality_bind,
        });
        v.systems.push(SystemDef {
            name: "visibility",
            run: sys_visibility,
        });
        v
    }

    pub fn system_names(&self) -> Vec<&'static str> {
        self.systems.iter().map(|s| s.name).collect()
    }

    /// The camera pose component, as bound by the last update.
    pub fn camera_pose(&self) -> CameraPose {
        *self.cameras.get(self.camera_e).expect("camera component")
    }

    /// The world-state component, as bound by the last update.
    pub fn world_state(&self) -> WorldState {
        *self.worlds.get(self.world_e).expect("world component")
    }

    /// Camera-pass draw list from the last update.
    pub fn draws(&self) -> &[TileDraw] {
        &self.draws
    }

    /// Shadow-pass draw list from the last update (empty at night).
    pub fn shadow_draws(&self) -> &[TileDraw] {
        &self.shadow_draws
    }

    /// Run one frame of scene systems. `light_view_proj` describes the
    /// sun's shadow camera; `shadow_light` says whether a caster pass
    /// will run at all this frame (sun up + map enabled).
    pub fn update(
        &mut self,
        pool: &JobPool,
        pose: &CameraPose,
        world: &WorldState,
        view_proj: Mat4,
        light_view_proj: Mat4,
        shadow_light: bool,
    ) -> CoreStats {
        let t0 = now();
        self.frame = Some(FrameInputs {
            pose: *pose,
            world: *world,
            view_proj,
            light_view_proj,
            shadow_light,
        });
        // Systems are fn pointers over &mut Vista; run them in order by
        // taking the registry out (no allocation — the Vec just moves).
        let systems = std::mem::take(&mut self.systems);
        for sys in &systems {
            (sys.run)(self, pool);
        }
        let n_systems = systems.len();
        self.systems = systems;
        self.frame = None;
        CoreStats {
            entities: self.cameras.len()
                + self.worlds.len()
                + self.tiles.len()
                + self.waters.len()
                + self.skies.len(),
            systems: n_systems,
            jobs_executed: pool.executed(),
            pool_size: pool.size(),
            update_ms: elapsed_ms(t0),
        }
    }
}

/// Bind this frame's reality into the components.
fn sys_reality_bind(v: &mut Vista, _pool: &JobPool) {
    let f = v.frame.as_ref().expect("frame inputs");
    let pose = f.pose;
    let world = f.world;
    v.cameras.insert(v.camera_e, pose);
    v.worlds.insert(v.world_e, world);
}

/// Frustum + LOD visibility for both passes, neighbour-clamped.
fn sys_visibility(v: &mut Vista, pool: &JobPool) {
    let (view_proj, light_vp, eye, shadow_light) = {
        let f = v.frame.as_ref().expect("frame inputs");
        (f.view_proj, f.light_view_proj, f.pose.eye, f.shadow_light)
    };
    let frustum = Frustum::from_view_proj(&view_proj);
    let spheres = &v.spheres;
    pool.for_each_chunk_mut(&mut v.vis, pool.size(), &|start, chunk| {
        for (i, slot) in chunk.iter_mut().enumerate() {
            let (c, r) = spheres[start + i];
            slot.visible = frustum.sphere_visible(c, r);
            let d = ((c - eye).length() - r).max(0.0);
            slot.step = lod_step_for(d);
        }
    });
    if shadow_light {
        let lfrustum = Frustum::from_view_proj(&light_vp);
        pool.for_each_chunk_mut(&mut v.light_vis, pool.size(), &|start, chunk| {
            for (i, slot) in chunk.iter_mut().enumerate() {
                let (c, r) = spheres[start + i];
                slot.visible = lfrustum.sphere_visible(c, r);
                // Casters are always full density — see the module docs
                // (a reduced caster erodes thin self-shadows).
                slot.step = 0;
            }
        });
    } else {
        for s in v.light_vis.iter_mut() {
            s.visible = false;
        }
    }
    clamp_neighbours(&mut v.vis);
    // (No neighbour clamp on the caster list: casters are uniformly
    // full density by the module-docs law.)

    // Collect the draw lists + the ledger's counts.
    v.draws.clear();
    v.shadow_draws.clear();
    let mut drawn = 0usize;
    let mut tris_after = 0u64;
    for (i, s) in v.vis.iter().enumerate() {
        if s.visible {
            drawn += 1;
            tris_after += v.lod_tris[i][s.step as usize];
            v.draws.push(TileDraw {
                tile: i,
                lod: lod_of(s.step),
            });
        }
    }
    let mut sdrawn = 0usize;
    let mut stris = 0u64;
    if shadow_light {
        for (i, s) in v.light_vis.iter().enumerate() {
            if s.visible {
                sdrawn += 1;
                stris += v.lod_tris[i][s.step as usize];
                v.shadow_draws.push(TileDraw {
                    tile: i,
                    lod: lod_of(s.step),
                });
            }
        }
    }
    v.cull.tiles_drawn = drawn;
    v.cull.tiles_culled = v.cull.tiles_total - drawn;
    v.cull.tris_after = tris_after;
    v.cull.shadow_drawn = sdrawn;
    v.cull.shadow_culled = v.cull.tiles_total - sdrawn;
    v.cull.shadow_tris = stris;
}

/// Keep neighbouring tiles within one LOD step of each other: a tile may
/// not be coarser than (finest neighbour step + 1). Relaxation over the
/// grid to its fixed point (cap 8 passes, early-out when stable) — the
/// GPU cull pass (M4, shaders/cull.wgsl) runs the identical in-place
/// relaxation in the identical scan order, so both paths land on the
/// same LOD assignment, not merely similar ones.
fn clamp_neighbours(vis: &mut [TileVis]) {
    let n = TILES_PER_SIDE as usize;
    for _ in 0..8 {
        let mut changed = false;
        for tj in 0..n {
            for ti in 0..n {
                let i = tj * n + ti;
                if !vis[i].visible {
                    continue;
                }
                let mut finest = vis[i].step;
                for (di, dj) in [
                    (1i32, 0i32),
                    (-1, 0),
                    (0, 1),
                    (0, -1),
                    (1, 1),
                    (1, -1),
                    (-1, 1),
                    (-1, -1),
                ] {
                    let (ai, aj) = (ti as i32 + di, tj as i32 + dj);
                    if ai < 0 || aj < 0 || ai >= n as i32 || aj >= n as i32 {
                        continue;
                    }
                    let j = aj as usize * n + ai as usize;
                    if vis[j].visible {
                        finest = finest.min(vis[j].step);
                    }
                }
                let max_step = finest + 1;
                if vis[i].step > max_step {
                    vis[i].step = max_step;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Frame timing for CoreStats.update_ms. std::time::Instant is not
// implemented on wasm32-unknown-unknown (it panics), so the web host's
// updates report 0 — its ledger does not print a CPU update line anyway;
// native targets measure for real.
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
fn now() -> std::time::Instant {
    std::time::Instant::now()
}
#[cfg(not(target_arch = "wasm32"))]
fn elapsed_ms(t0: std::time::Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}
#[cfg(target_arch = "wasm32")]
fn now() -> u32 {
    0
}
#[cfg(target_arch = "wasm32")]
fn elapsed_ms(_t0: u32) -> f64 {
    0.0
}
