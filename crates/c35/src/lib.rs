//! # Comigen (C35) — engine core
//!
//! A cost-accounted procedural graphics engine. M1: a wgpu renderer core
//! and one deterministic procedural vista. M2: the vista obeys the
//! **reality layer** ([`world`]) — a real solar model drives sun, sky,
//! fog, seasons and ice from the clock and the observer's latitude — plus
//! a real directional shadow map, a fragment terrain detail pass, and
//! temporal AA where the backend can carry it (FXAA on the WebGL2
//! fallback path; the ledger names which ran).
//!
//! M3: the renderer becomes an engine core. The scene lives in a
//! data-oriented ECS ([`ecs`], [`vista`]) — camera, world state, terrain
//! tiles, water and sky are components; a visibility system frustum-culls
//! the 4×4 tile grid and picks a density LOD per tile. The TAA resolve
//! becomes **temporal reconstruction**: the scene renders at the
//! governor's internal scale and the history resolve writes at output
//! resolution. And the frame governs itself ([`governor`]): an EMA of
//! frame time is held against a target and the engine sheds its own
//! ranked costs — render scale, shadow resolution, detail reach, water
//! octaves — before it ever drops a frame it could have afforded. A pin
//! is a pin: pinned, the governor only watches and reports.
//!
//! M4 pays M3's named debts. The terrain's steep faces get real relief
//! (terrain generation v2, [`scene`]): domain warp + ridged displacement
//! masked by slope, so the headwall has geometry for light to model.
//! And culling moves onto the GPU where the backend carries compute:
//! all tiles live in merged buffers, a compute pass runs the same
//! visibility law as the CPU system and writes indirect draw arguments
//! (the CPU system still owns the shadow list, and is the whole story
//! on WebGL2). Every ledger names which culling ran.
//!
//! The house law: **if it costs, it shows.** Every frame is counted —
//! draw calls per pass, triangles, render scale, tracked memory, frame
//! time, and GPU pass times where the backend exposes timestamp queries.

pub mod ecs;
pub mod governor;
pub mod noise;
pub mod scene;
pub mod vista;
pub mod world;

/// Re-exported so hosts build against exactly the engine's wgpu.
pub use wgpu;
pub use pollster;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

use scene::CameraPose;
use world::WorldState;

use ecs::{CoreStats, JobPool};
use governor::{Governor, QualityState};
use scene::TerrainTile;
use vista::Vista;

pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Single-cascade sun shadow map edge at governor level 0 (the PCF texel
/// size itself travels in the `quality` uniform — see shaders/common.wgsl).
/// 2048 over a ±380-unit ortho box ≈ 0.37 units/texel — fine enough to
/// carve the headwall's shadow, cheap enough to stay one cascade. The
/// governor's ladder resizes it (2048 → 1024 → off); the ledger prints
/// its cost like everything else.
pub const SHADOW_MAP_SIZE: u32 = 2048;
/// Half-extent of the shadow ortho box (bounding-sphere fit of the vista).
const SHADOW_RADIUS: f32 = 380.0;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    light_view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    sun_glow: [f32; 4],
    moon_dir: [f32; 4],
    moon_color: [f32; 4],
    amb_sky: [f32; 4],
    amb_ground: [f32; 4],
    sky_zenith: [f32; 4],
    sky_mid: [f32; 4],
    sky_horizon: [f32; 4],
    fog: [f32; 4],
    season: [f32; 4],
    water_p: [f32; 4],
    misc: [f32; 4],
    quality: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct ResolveUniforms {
    inv_view_proj: [[f32; 4]; 4],
    prev_view_proj: [[f32; 4]; 4],
    params: [f32; 4],
}

/// Which anti-aliasing the renderer runs — decided from the adapter's
/// backend at build time, printed by every ledger.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AaMode {
    /// Jittered projection + history resolve (Vulkan/Metal/DX12/WebGPU).
    Taa,
    /// FXAA in the blit pass (the WebGL2 fallback path).
    Fxaa,
}

impl AaMode {
    pub fn describe(&self) -> &'static str {
        match self {
            AaMode::Taa => "TAA (jitter + history resolve)",
            AaMode::Fxaa => "FXAA (blit pass; TAA unavailable on this backend)",
        }
    }
}

/// Simple mean / percentile helpers shared by both hosts' ledgers.
pub fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.iter().sum::<f64>() / xs.len() as f64
}

pub fn percentile(xs: &[f64], p: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v: Vec<f64> = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((p / 100.0) * (v.len() - 1) as f64).round() as usize;
    v[idx.min(v.len() - 1)]
}

fn shader_src(body: &str) -> String {
    format!("{}\n{}", include_str!("shaders/common.wgsl"), body)
}

/// One render pass's GPU timer: a 2-slot timestamp query set with the
/// resolve/copy/readback dance M1 proved out. Absent where the adapter
/// offered no TIMESTAMP_QUERY feature.
struct PassTimer {
    query_set: wgpu::QuerySet,
    resolve_buf: wgpu::Buffer,
    readback_buf: Arc<wgpu::Buffer>,
    pending: Arc<AtomicBool>,
    samples_ms: Arc<Mutex<Vec<f64>>>,
    period_ns: f32,
}

impl PassTimer {
    fn new(device: &wgpu::Device, period_ns: f32, label: &str) -> PassTimer {
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some(label),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let resolve_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timestamp resolve"),
            size: 16,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback_buf = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timestamp readback"),
            size: 16,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        PassTimer {
            query_set,
            resolve_buf,
            readback_buf,
            pending: Arc::new(AtomicBool::new(false)),
            samples_ms: Arc::new(Mutex::new(Vec::new())),
            period_ns,
        }
    }

    fn writes(&self) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(0),
            end_of_pass_write_index: Some(1),
        }
    }

    fn compute_writes(&self) -> wgpu::ComputePassTimestampWrites<'_> {
        wgpu::ComputePassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(0),
            end_of_pass_write_index: Some(1),
        }
    }

    /// Encode resolve+copy (only when no readback is already in flight).
    fn arm(&self, encoder: &mut wgpu::CommandEncoder) {
        if !self.pending.load(Ordering::Relaxed) {
            encoder.resolve_query_set(&self.query_set, 0..2, &self.resolve_buf, 0);
            encoder.copy_buffer_to_buffer(&self.resolve_buf, 0, &self.readback_buf, 0, 16);
        }
    }

    /// After submit: if this frame's resolve rode along, start the async
    /// map whose callback banks the sample a frame or two later.
    fn kick_readback(&self) {
        if !self.pending.swap(true, Ordering::Relaxed) {
            let samples = self.samples_ms.clone();
            let pending = self.pending.clone();
            let buf_submit = self.readback_buf.clone();
            let buf_cb = self.readback_buf.clone();
            let period = self.period_ns;
            buf_submit
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    if result.is_ok() {
                        {
                            let range = buf_cb.slice(..).get_mapped_range();
                            let lo = u64::from_le_bytes(range[0..8].try_into().unwrap());
                            let hi = u64::from_le_bytes(range[8..16].try_into().unwrap());
                            if hi > lo {
                                let ms = (hi - lo) as f64 * period as f64 / 1.0e6;
                                samples.lock().unwrap().push(ms);
                            }
                        }
                        buf_cb.unmap();
                    }
                    pending.store(false, Ordering::Relaxed);
                });
        }
    }

    fn drain(&self) -> Vec<f64> {
        let mut s = self.samples_ms.lock().unwrap();
        std::mem::take(&mut *s)
    }
}

struct Targets {
    width: u32,
    height: u32,
    scale: f32,
    internal_w: u32,
    internal_h: u32,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    /// TAA: the history texture (previous resolved frame) + its view.
    history_tex: Option<wgpu::Texture>,
    blit_bind_group: wgpu::BindGroup,
    bytes: u64,
}

/// One tile's draw range inside the merged geometry buffers. The
/// merged index buffer holds absolute indices (each tile's vertex
/// offset baked in at build), so `base_vertex` is always 0 — WebGL2
/// has no base-vertex draw and its backend panics if asked for one.
/// Mirrored on the CPU (for the CPU cull path's draws) and in the GPU
/// tile params.
#[derive(Copy, Clone)]
struct TileDrawParams {
    first_index: u32,
    index_count: u32,
    base_vertex: i32,
}

/// Which visibility path draws the camera pass (M4).
/// The CPU path is the M3 visibility system: it always runs (it owns
/// the shadow-pass list) and it is the only path on WebGL2, which has
/// no compute. The GPU path runs the same law in a compute pass
/// (shaders/cull.wgsl) feeding indirect draws.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CullMode {
    Cpu,
    Gpu,
}

/// GPU mirror of the cull shader's TileParam (shaders/cull.wgsl).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GpuTileParam {
    sphere: [f32; 4],
    lod: [[u32; 4]; 3],
}

/// GPU mirror of the cull shader's CullUniforms.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CullUniforms {
    planes: [[f32; 4]; 6],
    eye: [f32; 4],
    dims: [u32; 4],
}

/// One indirect draw's arguments — the layout wgpu's
/// draw_indexed_indirect reads, and the cull shader writes.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct DrawArgs {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

const DRAW_ARGS_BYTES: u64 = 20;

/// The GPU culling apparatus (native-class backends only): the compute
/// pipeline, its buffers, and the readback that lets the ledger report
/// the GPU path's own counts (decoded from the final frame's argument
/// buffer — the render itself never waits on it).
struct GpuCull {
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    args_buf: wgpu::Buffer,
    readback_buf: Arc<wgpu::Buffer>,
    pending: Arc<AtomicBool>,
    snapshot: Arc<Mutex<Option<(usize, u64)>>>, // (tiles drawn, terrain tris)
    timer: Option<PassTimer>,
}

impl GpuCull {
    /// Encode the args→readback copy, but only when no map is in flight
    /// (the PassTimer discipline: a mapped buffer cannot be a copy
    /// target).
    fn arm_readback(&self, encoder: &mut wgpu::CommandEncoder, bytes: u64) {
        if !self.pending.load(Ordering::Relaxed) {
            encoder.copy_buffer_to_buffer(&self.args_buf, 0, &self.readback_buf, 0, bytes);
        }
    }

    /// After submit: map the readback and decode (tiles drawn, terrain
    /// triangles) into the snapshot the ledger reads.
    fn kick_readback(&self) {
        if !self.pending.swap(true, Ordering::Relaxed) {
            let pending = self.pending.clone();
            let snapshot = self.snapshot.clone();
            let buf_submit = self.readback_buf.clone();
            let buf_cb = self.readback_buf.clone();
            buf_submit
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    if result.is_ok() {
                        {
                            let range = buf_cb.slice(..).get_mapped_range();
                            let args: &[DrawArgs] = bytemuck::cast_slice(&range);
                            let mut drawn = 0usize;
                            let mut tris = 0u64;
                            for a in args {
                                if a.index_count > 0 {
                                    drawn += 1;
                                    tris += (a.index_count / 3) as u64;
                                }
                            }
                            *snapshot.lock().unwrap() = Some((drawn, tris));
                        }
                        buf_cb.unmap();
                    }
                    pending.store(false, Ordering::Relaxed);
                });
        }
    }

    fn snapshot(&self) -> Option<(usize, u64)> {
        *self.snapshot.lock().unwrap()
    }
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    adapter_info: wgpu::AdapterInfo,
    output_format: wgpu::TextureFormat,
    aa_mode: AaMode,
    sky_pipeline: wgpu::RenderPipeline,
    terrain_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    depth_pipeline: wgpu::RenderPipeline,
    resolve_pipeline: Option<wgpu::RenderPipeline>,
    blit_pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    resolve_uniform_buf: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    depth_bind_group: wgpu::BindGroup,
    blit_layout: wgpu::BindGroupLayout,
    resolve_layout: Option<wgpu::BindGroupLayout>,
    resolve_bind_group: Option<wgpu::BindGroup>,
    resolved_tex: Option<wgpu::Texture>,
    resolved_view: Option<wgpu::TextureView>,
    shadow_attach: Option<(wgpu::Texture, Arc<wgpu::TextureView>)>,
    blit_sampler: wgpu::Sampler,
    linear_sampler: wgpu::Sampler,
    shadow_bytes: u64,
    /// M4: all tiles live in ONE vertex buffer and ONE index buffer
    /// (every LOD of every tile), so an indirect draw can address any
    /// tile/LOD by first_index alone (indices are absolute) — the
    /// precondition for GPU culling. The CPU path draws ranges from
    /// the same buffers.
    merged_vb: wgpu::Buffer,
    merged_ib: wgpu::Buffer,
    tile_params: Vec<[TileDrawParams; 3]>,
    geometry_bytes: u64,
    /// GPU culling apparatus (None on the WebGL2 path).
    gpu_cull: Option<GpuCull>,
    cull_mode: CullMode,
    // The M3 core: job pool, the ECS vista, the quality governor.
    pool: JobPool,
    vista: Vista,
    governor: Governor,
    /// The quality state currently applied to the GPU.
    quality: QualityState,
    /// The host's own render-scale request; the governor's ladder scale
    /// multiplies it (hosts normally pass 1.0).
    base_scale: f32,
    /// Current shadow map edge (governor-driven; 0 = pass off).
    shadow_size: u32,
    scene_layout: wgpu::BindGroupLayout,
    shadow_sampler: wgpu::Sampler,
    core_stats: CoreStats,
    out_size: (u32, u32),
    last_tris: u64,
    last_shadow_tris: u64,
    targets: Option<Targets>,
    shadow_timer: Option<PassTimer>,
    scene_timer: Option<PassTimer>,
    frame_count: u64,
    prev_view_proj: Mat4,
    history_valid: bool,
    last_draw_calls: u32,
    last_shadow_draw_calls: u32,
    last_shadow_active: bool,
}

pub fn create_instance() -> wgpu::Instance {
    create_instance_with(wgpu::Backends::all())
}

pub fn create_instance_with(backends: wgpu::Backends) -> wgpu::Instance {
    wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    })
}

/// Halton low-discrepancy sequence — the TAA jitter source. Deterministic
/// in the frame index, so pinned proof runs reproduce bit-for-bit.
fn halton(index: u32, base: u32) -> f32 {
    let mut f = 1.0f32;
    let mut r = 0.0f32;
    let mut i = index;
    while i > 0 {
        f /= base as f32;
        r += f * (i % base) as f32;
        i /= base;
    }
    r
}

impl Renderer {
    /// Headless build (native proof host): renders into owned textures of
    /// `output_format`.
    pub async fn new(
        instance: &wgpu::Instance,
        output_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        render_scale: f32,
    ) -> Renderer {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .expect("no GPU adapter available");
        Self::build(adapter, output_format, width, height, render_scale).await
    }

    /// Surface build (web + windowed hosts): adapter is matched to
    /// `surface` and the output format is the surface's preferred format.
    /// Read it back with `output_format()` to configure the surface.
    pub async fn new_for_surface(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        render_scale: f32,
    ) -> Renderer {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no GPU adapter available");
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats[0];
        Self::build(adapter, format, width, height, render_scale).await
    }

    pub fn output_format(&self) -> wgpu::TextureFormat {
        self.output_format
    }

    async fn build(
        adapter: wgpu::Adapter,
        output_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        render_scale: f32,
    ) -> Renderer {
        let adapter_info = adapter.get_info();
        // TAA needs a sampled depth texture + a history resolve; the
        // WebGL2 backend is the one path where we do not trust that
        // combination, so it takes FXAA in the blit instead. Everywhere
        // else — Vulkan, Metal, DX12, real WebGPU — TAA runs.
        let aa_mode = if adapter_info.backend == wgpu::Backend::Gl {
            AaMode::Fxaa
        } else {
            AaMode::Taa
        };
        let mut features = wgpu::Features::empty();
        if adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            features |= wgpu::Features::TIMESTAMP_QUERY;
        }
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("comigen device"),
                required_features: features,
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .expect("failed to create device");
        let period_ns = queue.get_timestamp_period();

        // The job pool comes up first: tile generation runs through it.
        let pool = JobPool::new();

        // ---- shadow map (fixed size; allocated once) ----
        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sun shadow map"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&Default::default());
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_bytes = (SHADOW_MAP_SIZE * SHADOW_MAP_SIZE * 4) as u64;
        // Stash the shadow view where resize() (which does not rebuild the
        // scene bind group) never needs it: the bind group is built now.
        let shadow_view = std::sync::Arc::new(shadow_view);
        let shadow_attach_view = shadow_view.clone();

        // ---- scene bind group (uniforms + shadow map) ----
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene bind group"),
            layout: &scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(shadow_view.as_ref()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });
        // The shadow-caster pass gets its own uniform-only bind group: a
        // bind group that samples the shadow map cannot also be bound
        // inside the pass that writes it (usage-scope conflict).
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("depth bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let depth_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("depth bind group"),
            layout: &depth_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });
        let depth_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("depth pipeline layout"),
                bind_group_layouts: &[&depth_layout],
                push_constant_ranges: &[],
            });

        let scene_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("scene pipeline layout"),
                bind_group_layouts: &[&scene_layout],
                push_constant_ranges: &[],
            });

        // ---- blit bind group layout ----
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let blit_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("blit pipeline layout"),
                bind_group_layouts: &[&blit_layout],
                push_constant_ranges: &[],
            });
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ---- resolve (TAA) layout + uniforms, TAA paths only ----
        let (resolve_layout, resolve_uniform_buf) = if aa_mode == AaMode::Taa {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("resolve layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
            let buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("resolve uniforms"),
                size: std::mem::size_of::<ResolveUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            (Some(layout), buf)
        } else {
            // FXAA path: no resolve pass, but keep a 1-byte stand-in buffer
            // so the struct shape stays uniform across paths.
            let buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("resolve uniforms (unused)"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            (None, buf)
        };

        // ---- pipelines ----
        let sky_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky"),
            source: wgpu::ShaderSource::Wgsl(shader_src(include_str!("shaders/sky.wgsl")).into()),
        });
        let terrain_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain"),
            source: wgpu::ShaderSource::Wgsl(
                shader_src(include_str!("shaders/terrain.wgsl")).into(),
            ),
        });
        let water_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("water"),
            source: wgpu::ShaderSource::Wgsl(
                shader_src(include_str!("shaders/water.wgsl")).into(),
            ),
        });
        let depth_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("depth"),
            source: wgpu::ShaderSource::Wgsl(
                shader_src(include_str!("shaders/depth.wgsl")).into(),
            ),
        });
        let blit_shader = if aa_mode == AaMode::Fxaa {
            include_str!("shaders/fxaa.wgsl")
        } else {
            include_str!("shaders/blit.wgsl")
        };
        let blit_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit"),
            source: wgpu::ShaderSource::Wgsl(blit_shader.into()),
        });

        let terrain_layout = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<scene::TerrainVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
                // M5: baked horizon occlusion
                // (scene::HeightGrid::bake_occlusion).
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 24,
                    shader_location: 2,
                },
            ],
        }];

        let scene_color_target = [Some(wgpu::ColorTargetState {
            format: SCENE_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky pipeline"),
            layout: Some(&scene_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &sky_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &sky_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &scene_color_target,
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let terrain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain pipeline"),
            layout: Some(&scene_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &terrain_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &terrain_layout,
            },
            fragment: Some(wgpu::FragmentState {
                module: &terrain_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &scene_color_target,
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let water_color_target = [Some(wgpu::ColorTargetState {
            format: SCENE_FORMAT,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let water_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water pipeline"),
            layout: Some(&scene_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &water_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &water_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &water_color_target,
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Depth-only caster pipeline for the shadow pass. Front faces are
        // culled so the map stores back-face depth (the standard trick
        // against heightfield self-acne).
        let depth_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow depth pipeline"),
            layout: Some(&depth_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &depth_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &terrain_layout,
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Front),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let resolve_pipeline = if aa_mode == AaMode::Taa {
            let layout = resolve_layout.as_ref().unwrap();
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("resolve pipeline layout"),
                bind_group_layouts: &[layout],
                push_constant_ranges: &[],
            });
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("resolve"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("shaders/resolve.wgsl").into(),
                ),
            });
            Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("taa resolve pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &scene_color_target,
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            }))
        } else {
            None
        };

        let blit_color_target = [Some(wgpu::ColorTargetState {
            format: output_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit pipeline"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &blit_color_target,
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ---- terrain geometry: the island as 16 tiles, generated in
        // parallel through the job pool (seeded — the same island on
        // every host, vertex-for-vertex the M1/M2 island at the seams).
        // The M5 occlusion bake reads one shared height grid, built
        // once here, so duplicated border vertices bake identical AO.
        let height_grid = scene::HeightGrid::new();
        let mut tile_slots: Vec<Option<TerrainTile>> =
            (0..scene::TILE_COUNT).map(|_| None).collect();
        pool.for_each_chunk_mut(&mut tile_slots, pool.size(), &|start, chunk| {
            for (i, slot) in chunk.iter_mut().enumerate() {
                *slot = Some(scene::generate_tile(start + i, &height_grid));
            }
        });
        let tiles: Vec<TerrainTile> = tile_slots
            .into_iter()
            .map(|t| t.expect("every tile generated"))
            .collect();
        // Merged geometry: every tile's vertices concatenated into one
        // buffer, every LOD index list into another, each tile/LOD
        // addressed by (first_index, index_count) into the merged index
        // buffer. The indices are ABSOLUTE — each tile's vertex offset
        // is baked into its index values at this merge — so every draw
        // on every path carries base_vertex 0. That is a portability
        // law, not a style choice: WebGL2 has no base-vertex draw, and
        // wgpu's GL backend (glow) panics outright if a draw asks for
        // one — the M4 web proof convicted exactly that. Both cull
        // paths draw from these buffers: the CPU path by explicit
        // ranges, the GPU path by indirect arguments the compute pass
        // writes.
        let mut tile_params: Vec<[TileDrawParams; 3]> = Vec::with_capacity(tiles.len());
        let mut tile_meta: Vec<(Vec3, f32, [u64; 3])> = Vec::with_capacity(tiles.len());
        let mut gpu_params: Vec<GpuTileParam> = Vec::with_capacity(tiles.len());
        let mut all_vertices: Vec<scene::TerrainVertex> = Vec::new();
        let mut all_indices: Vec<u32> = Vec::new();
        for tile in &tiles {
            let base_vertex = all_vertices.len() as u32;
            all_vertices.extend_from_slice(&tile.vertices);
            let mut params = [TileDrawParams {
                first_index: 0,
                index_count: 0,
                base_vertex: 0,
            }; 3];
            let mut gpu_lod = [[0u32; 4]; 3];
            for (slot, idx) in tile.lod_indices.iter().enumerate() {
                let first_index = all_indices.len() as u32;
                all_indices.extend(idx.iter().map(|&i| i + base_vertex));
                params[slot] = TileDrawParams {
                    first_index,
                    index_count: idx.len() as u32,
                    base_vertex: 0,
                };
                gpu_lod[slot] = [first_index, idx.len() as u32, 0, 0];
            }
            tile_params.push(params);
            gpu_params.push(GpuTileParam {
                sphere: [tile.center.x, tile.center.y, tile.center.z, tile.radius],
                lod: gpu_lod,
            });
            tile_meta.push((
                tile.center,
                tile.radius,
                [
                    tile.lod_triangles(scene::Lod::Full),
                    tile.lod_triangles(scene::Lod::Half),
                    tile.lod_triangles(scene::Lod::Quarter),
                ],
            ));
        }
        let geometry_bytes = (all_vertices.len() * std::mem::size_of::<scene::TerrainVertex>()
            + all_indices.len() * 4) as u64;
        let merged_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain vertices (merged)"),
            contents: bytemuck::cast_slice(&all_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let merged_ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain indices (merged)"),
            contents: bytemuck::cast_slice(&all_indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let full_tris: u64 = tile_meta.iter().map(|m| m.2[0]).sum();
        let vista = Vista::new(&tile_meta);
        let governor = Governor::new(16.7);
        let quality = governor.state();

        // ---- GPU timers (optional; only where the adapter offered them) ----
        let (shadow_timer, scene_timer) = if features.contains(wgpu::Features::TIMESTAMP_QUERY) {
            (
                Some(PassTimer::new(&device, period_ns, "shadow timestamps")),
                Some(PassTimer::new(&device, period_ns, "scene timestamps")),
            )
        } else {
            (None, None)
        };

        // ---- GPU culling apparatus (M4): every backend with compute —
        // i.e. everything except the WebGL2 fallback, which keeps the
        // CPU visibility system as its whole path. ----
        let gpu_cull = if adapter_info.backend != wgpu::Backend::Gl {
            let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("cull tile params"),
                contents: bytemuck::cast_slice(&gpu_params),
                usage: wgpu::BufferUsages::STORAGE,
            });
            let args_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cull indirect args"),
                size: DRAW_ARGS_BYTES * scene::TILE_COUNT as u64,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::INDIRECT
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let readback_buf = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cull args readback"),
                size: DRAW_ARGS_BYTES * scene::TILE_COUNT as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cull uniforms"),
                size: std::mem::size_of::<CullUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cull layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cull bind group"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: params_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: args_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform_buf.as_entire_binding(),
                    },
                ],
            });
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("cull"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/cull.wgsl").into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("cull pipeline layout"),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("cull pipeline"),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
            let timer = if features.contains(wgpu::Features::TIMESTAMP_QUERY) {
                Some(PassTimer::new(&device, period_ns, "cull timestamps"))
            } else {
                None
            };
            Some(GpuCull {
                pipeline,
                bind_group,
                uniform_buf,
                args_buf,
                readback_buf,
                pending: Arc::new(AtomicBool::new(false)),
                snapshot: Arc::new(Mutex::new(None)),
                timer,
            })
        } else {
            None
        };
        let cull_mode = if gpu_cull.is_some() {
            CullMode::Gpu
        } else {
            CullMode::Cpu
        };

        // The shadow attachment view must outlive build(); keep it in the
        // targets-independent field via a leaked Arc companion — simpler:
        // store it on the renderer through the scene bind group build
        // above, and keep the texture alive here.
        let mut renderer = Renderer {
            device,
            queue,
            adapter_info,
            output_format,
            aa_mode,
            sky_pipeline,
            terrain_pipeline,
            water_pipeline,
            depth_pipeline,
            resolve_pipeline,
            blit_pipeline,
            uniform_buf,
            resolve_uniform_buf,
            scene_bind_group,
            depth_bind_group,
            blit_layout,
            resolve_layout,
            resolve_bind_group: None,
            resolved_tex: None,
            resolved_view: None,
            shadow_attach: None,
            blit_sampler,
            linear_sampler,
            shadow_bytes,
            merged_vb,
            merged_ib,
            tile_params,
            geometry_bytes,
            gpu_cull,
            cull_mode,
            pool,
            vista,
            governor,
            quality,
            base_scale: render_scale,
            shadow_size: SHADOW_MAP_SIZE,
            scene_layout,
            shadow_sampler,
            core_stats: CoreStats::default(),
            out_size: (width, height),
            last_tris: full_tris + 4,
            last_shadow_tris: full_tris,
            targets: None,
            shadow_timer,
            scene_timer,
            frame_count: 0,
            prev_view_proj: Mat4::IDENTITY,
            history_valid: false,
            last_draw_calls: 0,
            last_shadow_draw_calls: 0,
            last_shadow_active: false,
        };
        renderer.shadow_attach = Some((shadow_tex, shadow_attach_view));
        renderer.resize(width, height);
        renderer
    }

    /// (Re)create the offscreen scene targets for an output of
    /// `width`×`height`. The effective render scale is the host's base
    /// scale times the governor's current ladder scale — `resize` never
    /// second-guesses the governor.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.out_size = (width, height);
        let scale = (self.base_scale * self.quality.render_scale).clamp(0.25, 1.0);
        let sw = ((width as f32 * scale) as u32).max(1);
        let sh = ((height as f32 * scale) as u32).max(1);
        let size = wgpu::Extent3d {
            width: sw,
            height: sh,
            depth_or_array_layers: 1,
        };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCENE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());

        // Temporal reconstruction chain: the resolve writes at OUTPUT
        // resolution — the resolved texture and the history are output-
        // sized, so the resolve's per-output-pixel history blend is the
        // upsampler, and the blit after it is a straight copy (FXAA path
        // excepted: it blits the internal target directly, as before).
        let (blit_source_view, history_tex, extra_bytes) = if self.aa_mode == AaMode::Taa {
            let out_extent = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            let resolved = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("reconstruction resolved"),
                size: out_extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SCENE_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let history = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("reconstruction history"),
                size: out_extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SCENE_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            (
                resolved.create_view(&Default::default()),
                Some((history, resolved)),
                (width * height) as u64 * 8, // resolved + history, 4 B/px each
            )
        } else {
            (color_view.clone(), None, 0u64)
        };
        let blit_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit bind group"),
            layout: &self.blit_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&blit_source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.blit_sampler),
                },
            ],
        });
        // Internal colour+depth at 4 B/px each, plus the output-sized
        // reconstruction pair where TAA runs.
        let bytes = (sw * sh) as u64 * 8 + extra_bytes;
        // Rebuild the resolve bind group against the fresh textures.
        if self.aa_mode == AaMode::Taa {
            let (hist, resolved) = history_tex.as_ref().unwrap();
            let hist_view = hist.create_view(&Default::default());
            let resolved_view = resolved.create_view(&Default::default());
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("resolve bind group"),
                layout: self.resolve_layout.as_ref().unwrap(),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.resolve_uniform_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&color_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&depth_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&hist_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                    },
                ],
            });
            self.resolve_bind_group = Some(bg);
            self.resolved_tex = Some(resolved.clone());
            self.resolved_view = Some(resolved_view);
        }
        self.history_valid = false;
        self.targets = Some(Targets {
            width,
            height,
            scale,
            internal_w: sw,
            internal_h: sh,
            color_view,
            depth_view,
            history_tex: history_tex.map(|(h, _)| h),
            blit_bind_group,
            bytes,
        });
    }

    pub fn render_scale(&self) -> f32 {
        self.targets.as_ref().map(|t| t.scale).unwrap_or(1.0)
    }

    pub fn backend_name(&self) -> String {
        format!(
            "{:?} / {} ({:?})",
            self.adapter_info.backend, self.adapter_info.name, self.adapter_info.device_type
        )
    }

    pub fn aa_mode(&self) -> AaMode {
        self.aa_mode
    }

    /// Total draw calls issued by the most recent frame.
    pub fn draw_calls_per_frame(&self) -> u32 {
        self.last_draw_calls
    }

    /// Draw calls the shadow pass cost in the most recent frame (0 at
    /// night — the pass is skipped outright when the sun is down).
    pub fn shadow_draw_calls_per_frame(&self) -> u32 {
        self.last_shadow_draw_calls
    }

    pub fn shadow_active(&self) -> bool {
        self.last_shadow_active
    }

    /// Scene-pass triangles submitted by the most recent frame (terrain
    /// after culling + LOD, plus water, sky and the blit triangle).
    pub fn triangles_per_frame(&self) -> u64 {
        self.last_tris
    }

    /// Triangles the shadow pass re-rendered in the most recent frame
    /// (the tiles the light frustum kept — casters always draw at full
    /// density; see the vista module docs).
    pub fn shadow_triangles_per_frame(&self) -> u64 {
        if self.last_shadow_active {
            self.last_shadow_tris
        } else {
            0
        }
    }

    /// The ECS core stats from the most recent frame's scene update.
    pub fn core_stats(&self) -> CoreStats {
        self.core_stats
    }

    /// Names of the scene systems that run per frame, in order.
    pub fn system_names(&self) -> Vec<&'static str> {
        self.vista.system_names()
    }

    /// The frame's culling accounting. On the GPU path the camera-pass
    /// fields come from the GPU's own argument buffer (readback of the
    /// final frame's draws — the render never waits on it); the shadow
    /// fields always come from the CPU visibility system, which owns
    /// the caster list on every path.
    pub fn cull_stats(&self) -> vista::CullStats {
        let mut c = self.vista.cull;
        if self.cull_mode == CullMode::Gpu {
            if let Some(gc) = &self.gpu_cull {
                if let Some((drawn, tris)) = gc.snapshot() {
                    c.tiles_drawn = drawn;
                    c.tiles_culled = c.tiles_total - drawn;
                    c.tris_after = tris;
                }
            }
        }
        c
    }

    /// Which culling path is drawing the camera pass.
    pub fn cull_mode(&self) -> CullMode {
        self.cull_mode
    }

    /// Ask for a culling path. A GPU request on a backend without the
    /// apparatus (WebGL2) is refused silently — `cull_mode()` reports
    /// the path actually in force, and every ledger prints it.
    pub fn set_cull_mode(&mut self, mode: CullMode) {
        self.cull_mode = match mode {
            CullMode::Gpu if self.gpu_cull.is_none() => CullMode::Cpu,
            m => m,
        };
    }

    /// The honest one-liner every ledger prints about culling.
    pub fn culling_description(&self) -> &'static str {
        match self.cull_mode {
            CullMode::Gpu => {
                "GPU compute (tile frustum + LOD in a compute pass, indirect draws)"
            }
            CullMode::Cpu => "CPU visibility system",
        }
    }

    /// Drain collected GPU cull-pass durations (empty unless the GPU
    /// path ran with timestamp queries available).
    pub fn drain_cull_samples(&self) -> Vec<f64> {
        self.gpu_cull
            .as_ref()
            .and_then(|gc| gc.timer.as_ref().map(|t| t.drain()))
            .unwrap_or_default()
    }

    pub fn governor(&self) -> &Governor {
        &self.governor
    }

    /// Set the governor's frame-time target (proof rigs use this to make
    /// shed behaviour observable; the shipped default is 16.7 ms).
    pub fn set_governor_target(&mut self, target_ms: f64) {
        self.governor.set_target(target_ms);
    }

    /// Pin (or release) the governor. A pin applies immediately — the
    /// pinned level's quality state goes to the GPU at once — and from
    /// there the governor only observes and reports (its law).
    pub fn pin_governor(&mut self, level: Option<usize>) {
        match level {
            Some(l) => self.governor.pin_to(l),
            None => self.governor.unpin(),
        }
        let s = self.governor.state();
        self.apply_quality(s);
    }

    /// Place the governor's ladder at a level without pinning (rig
    /// setup — the step-up proof starts partway down the ladder and
    /// governs live from there). Not an adjustment; nothing is logged.
    pub fn set_governor_start_level(&mut self, level: usize) {
        self.governor.force_level(level);
        let s = self.governor.state();
        self.apply_quality(s);
    }

    /// Feed one frame's measured cost (the host's frame-loop wall time)
    /// to the governor; if it stepped the ladder, apply the new state.
    pub fn observe_frame_ms(&mut self, ms: f64) {
        if self.governor.observe(ms) {
            let s = self.governor.state();
            self.apply_quality(s);
        }
    }

    fn apply_quality(&mut self, state: QualityState) {
        let prev = self.quality;
        self.quality = state;
        if state.shadow_size != prev.shadow_size {
            self.rebuild_shadow(state.shadow_size);
        }
        let eff = (self.base_scale * state.render_scale).clamp(0.25, 1.0);
        let cur = self.targets.as_ref().map(|t| t.scale).unwrap_or(0.0);
        if (eff - cur).abs() > 0.001 {
            let (w, h) = self.out_size;
            self.resize(w, h);
        }
    }

    /// Rebuild the shadow map at a new edge (0 = governor shed shadows
    /// entirely: a 1×1 stand-in keeps the scene bind group valid and the
    /// pass is skipped, with shadow strength zeroed in the uniforms).
    fn rebuild_shadow(&mut self, size: u32) {
        let dim = size.max(1);
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sun shadow map"),
            size: wgpu::Extent3d {
                width: dim,
                height: dim,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let view = Arc::new(view);
        self.scene_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene bind group"),
            layout: &self.scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view.as_ref()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                },
            ],
        });
        self.shadow_bytes = (dim as u64) * (dim as u64) * 4;
        self.shadow_size = size;
        self.shadow_attach = Some((tex, view));
    }

    /// Current shadow map edge in texels (0 = governor shed the pass).
    pub fn shadow_map_size(&self) -> u32 {
        self.shadow_size
    }

    /// What anti-aliasing / reconstruction actually ran, at the current
    /// quality state — the honest string every ledger prints.
    pub fn aa_description(&self) -> String {
        match self.aa_mode {
            AaMode::Taa => {
                let s = self.render_scale();
                if s < 0.999 {
                    format!(
                        "temporal reconstruction {s:.2} -> 1.00 (jitter + history resolve at output resolution)"
                    )
                } else {
                    "TAA (jitter + history resolve)".to_string()
                }
            }
            AaMode::Fxaa => AaMode::Fxaa.describe().to_string(),
        }
    }

    /// Engine-tracked GPU memory: geometry + scene targets + shadow map +
    /// uniforms. An estimate by construction — the ledger labels it as such.
    pub fn tracked_vram_bytes(&self) -> u64 {
        let t = self.targets.as_ref().map(|t| t.bytes).unwrap_or(0);
        self.geometry_bytes
            + t
            + self.shadow_bytes
            + std::mem::size_of::<Uniforms>() as u64
            + std::mem::size_of::<ResolveUniforms>() as u64
    }

    pub fn gpu_timestamps_available(&self) -> bool {
        self.scene_timer.is_some()
    }

    /// Drain collected GPU pass durations (shadow_ms, scene_ms).
    pub fn drain_gpu_samples(&self) -> (Vec<f64>, Vec<f64>) {
        let shadow = self.shadow_timer.as_ref().map(|t| t.drain()).unwrap_or_default();
        let scene = self.scene_timer.as_ref().map(|t| t.drain()).unwrap_or_default();
        (shadow, scene)
    }

    /// Sun shadow transform: bounding-sphere ortho fit around the vista.
    fn light_view_proj(sun_dir: Vec3) -> Mat4 {
        let center = Vec3::new(0.0, 10.0, 0.0);
        let eye = center - sun_dir * (SHADOW_RADIUS + 80.0);
        let up = if sun_dir.y.abs() > 0.95 { Vec3::X } else { Vec3::Y };
        let view = Mat4::look_at_rh(eye, center, up);
        let proj = Mat4::orthographic_rh(
            -SHADOW_RADIUS,
            SHADOW_RADIUS,
            -SHADOW_RADIUS,
            SHADOW_RADIUS,
            1.0,
            2.0 * SHADOW_RADIUS + 240.0,
        );
        proj * view
    }

    /// Render one frame of the vista into `out_view`, lit and seasoned by
    /// `world` (the reality layer's reading for this instant and place).
    pub fn render(&mut self, out_view: &wgpu::TextureView, pose: &CameraPose, world: &WorldState, time: f32) {
        let (internal_w, internal_h, aspect, out_w, out_h) = {
            let t = self.targets.as_ref().expect("renderer sized");
            (
                t.internal_w,
                t.internal_h,
                t.width as f32 / t.height as f32,
                t.width,
                t.height,
            )
        };
        let view = Mat4::look_at_rh(pose.eye, pose.target, Vec3::Y);
        let proj = Mat4::perspective_rh(pose.fov_y_deg.to_radians(), aspect, 0.5, 4000.0);
        let view_proj = proj * view;

        // ---- scene update through the ECS core: bind reality, cull ----
        let sun_up = world.sun_elevation_deg > 0.0;
        let light_vp = Self::light_view_proj(world.sun_dir);
        let shadow_light = sun_up && self.shadow_size > 0;
        let stats = self.vista.update(
            &self.pool,
            pose,
            world,
            view_proj,
            light_vp,
            shadow_light,
        );
        self.core_stats = stats;
        let targets = self.targets.as_ref().expect("renderer sized");

        // TAA jitter: sub-pixel, Halton in the frame index (deterministic
        // for pinned runs). Jitter is baked into the rasterization matrix;
        // the resolve pass un-jitters when it samples the current frame.
        let (jitter_ndc, jitter_uv) = if self.aa_mode == AaMode::Taa {
            let idx = (self.frame_count % 32) as u32 + 1;
            let jx = halton(idx, 2) - 0.5; // pixels, [-0.5, 0.5]
            let jy = halton(idx, 3) - 0.5;
            (
                Vec3::new(
                    jx * 2.0 / internal_w as f32,
                    -jy * 2.0 / internal_h as f32,
                    0.0,
                ),
                glam::Vec2::new(jx / internal_w as f32, jy / internal_h as f32),
            )
        } else {
            (Vec3::ZERO, glam::Vec2::ZERO)
        };
        let jittered_vp = if self.aa_mode == AaMode::Taa {
            let j = Mat4::from_cols(
                Vec4::new(1.0, 0.0, 0.0, 0.0),
                Vec4::new(0.0, 1.0, 0.0, 0.0),
                Vec4::new(0.0, 0.0, 1.0, 0.0),
                Vec4::new(jitter_ndc.x, jitter_ndc.y, 0.0, 1.0),
            );
            j * view_proj
        } else {
            view_proj
        };

        let shadow_strength = if self.shadow_size > 0 {
            smoothstep_cpu(1.0, 6.0, world.sun_elevation_deg)
        } else {
            0.0 // governor shed the shadow pass: no shadow term at all
        };

        let uniforms = Uniforms {
            view_proj: jittered_vp.to_cols_array_2d(),
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            light_view_proj: light_vp.to_cols_array_2d(),
            cam_pos: Vec4::from((pose.eye, 1.0)).to_array(),
            sun_dir: Vec4::from((world.sun_dir, 0.0)).to_array(),
            sun_color: Vec4::from((world.sun_color, 0.0)).to_array(),
            sun_glow: [
                world.sun_glow.x,
                world.sun_glow.y,
                world.sun_glow.z,
                world.sun_disc,
            ],
            moon_dir: Vec4::from((world.moon_dir, 0.0)).to_array(),
            moon_color: [
                world.moon_color.x,
                world.moon_color.y,
                world.moon_color.z,
                world.moon_disc,
            ],
            amb_sky: Vec4::from((world.ambient_sky, 0.0)).to_array(),
            amb_ground: Vec4::from((world.ambient_ground, 0.0)).to_array(),
            sky_zenith: Vec4::from((world.sky_zenith, 0.0)).to_array(),
            sky_mid: Vec4::from((world.sky_mid, 0.0)).to_array(),
            sky_horizon: Vec4::from((world.sky_horizon, 0.0)).to_array(),
            fog: [
                world.fog_color.x,
                world.fog_color.y,
                world.fog_color.z,
                world.fog_density,
            ],
            season: [
                world.cold,
                world.autumn,
                world.spring,
                world.snow_line,
            ],
            water_p: [
                world.water_ice,
                world.water_wave_scale,
                world.star_intensity,
                world.daylight,
            ],
            misc: [time, shadow_strength, targets.scale, aspect],
            // Governor-driven quality: x detail fade multiplier ·
            // y water wave octaves · z shadow PCF texel size (0 = off).
            quality: [
                self.quality.detail_fade_mul,
                self.quality.water_octaves as f32,
                if self.shadow_size > 0 {
                    1.0 / self.shadow_size as f32
                } else {
                    0.0
                },
                0.0,
            ],
        };
        self.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&[uniforms]));

        if self.aa_mode == AaMode::Taa {
            let ru = ResolveUniforms {
                inv_view_proj: view_proj.inverse().to_cols_array_2d(),
                prev_view_proj: self.prev_view_proj.to_cols_array_2d(),
                params: [
                    jitter_uv.x,
                    jitter_uv.y,
                    if self.history_valid { 1.0 } else { 0.0 },
                    0.1,
                ],
            };
            self.queue
                .write_buffer(&self.resolve_uniform_buf, 0, bytemuck::cast_slice(&[ru]));
        }

        let mut draw_calls = 0u32;
        let mut shadow_draws = 0u32;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        // ---- GPU culling (M4): the compute pass runs the visibility
        // law for the camera pass and writes the indirect arguments.
        // The CPU system above already ran: it owns the shadow list on
        // every path, and its counts are the fallback the ledger shows
        // until the GPU readback lands. ----
        if self.cull_mode == CullMode::Gpu {
            let gc = self.gpu_cull.as_ref().expect("gpu cull apparatus");
            let planes = vista::Frustum::from_view_proj(&view_proj).planes();
            let cu = CullUniforms {
                planes: planes.map(|p| p.to_array()),
                eye: [pose.eye.x, pose.eye.y, pose.eye.z, 1.0],
                dims: [scene::TILES_PER_SIDE, scene::TILE_COUNT as u32, 0, 0],
            };
            self.queue
                .write_buffer(&gc.uniform_buf, 0, bytemuck::cast_slice(&[cu]));
            let ts = gc.timer.as_ref().map(|t| t.compute_writes());
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("gpu cull pass"),
                timestamp_writes: ts,
            });
            pass.set_pipeline(&gc.pipeline);
            pass.set_bind_group(0, &gc.bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
            drop(pass);
            gc.arm_readback(&mut encoder, DRAW_ARGS_BYTES * scene::TILE_COUNT as u64);
        }

        // ---- shadow pass (skipped outright when the sun is down, or
        // when the governor has shed it) ----
        if shadow_light {
            let ts = self.shadow_timer.as_ref().map(|t| t.writes());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.shadow_attach.as_ref().unwrap().1.as_ref(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: ts,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.depth_bind_group, &[]);
            pass.set_pipeline(&self.depth_pipeline);
            pass.set_vertex_buffer(0, self.merged_vb.slice(..));
            pass.set_index_buffer(self.merged_ib.slice(..), wgpu::IndexFormat::Uint32);
            for d in self.vista.shadow_draws() {
                let p = self.tile_params[d.tile][d.lod.slot()];
                pass.draw_indexed(
                    p.first_index..p.first_index + p.index_count,
                    p.base_vertex,
                    0..1,
                );
                shadow_draws += 1;
            }
            draw_calls += shadow_draws;
        }

        // ---- scene pass ----
        {
            let ts = self.scene_timer.as_ref().map(|t| t.writes());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: ts,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&self.terrain_pipeline);
            pass.set_vertex_buffer(0, self.merged_vb.slice(..));
            pass.set_index_buffer(self.merged_ib.slice(..), wgpu::IndexFormat::Uint32);
            let mut terrain_draws = 0u32;
            if self.cull_mode == CullMode::Gpu {
                // One indirect draw per tile slot; culled tiles carry a
                // zero index count written by the cull pass.
                let args = &self.gpu_cull.as_ref().expect("gpu cull apparatus").args_buf;
                for slot in 0..scene::TILE_COUNT {
                    pass.draw_indexed_indirect(args, slot as u64 * DRAW_ARGS_BYTES);
                    terrain_draws += 1;
                }
            } else {
                for d in self.vista.draws() {
                    let p = self.tile_params[d.tile][d.lod.slot()];
                    pass.draw_indexed(
                        p.first_index..p.first_index + p.index_count,
                        p.base_vertex,
                        0..1,
                    );
                    terrain_draws += 1;
                }
            }
            pass.set_pipeline(&self.water_pipeline);
            pass.draw(0..6, 0..1);
            draw_calls += 1 + terrain_draws + 1;
        }
        if self.cull_mode == CullMode::Gpu {
            if let Some(gc) = &self.gpu_cull {
                if let Some(t) = &gc.timer {
                    t.arm(&mut encoder);
                }
            }
        }
        if let Some(t) = &self.shadow_timer {
            if shadow_light {
                t.arm(&mut encoder);
            }
        }
        if let Some(t) = &self.scene_timer {
            t.arm(&mut encoder);
        }

        // ---- TAA resolve + history copy ----
        if self.aa_mode == AaMode::Taa {
            let resolved_view = self.resolved_view.as_ref().unwrap().clone();
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("taa resolve pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &resolved_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_bind_group(0, self.resolve_bind_group.as_ref().unwrap(), &[]);
                pass.set_pipeline(self.resolve_pipeline.as_ref().unwrap());
                pass.draw(0..3, 0..1);
                draw_calls += 1;
            }
            let resolved_tex = self.resolved_tex.as_ref().unwrap();
            let history_tex = targets.history_tex.as_ref().unwrap();
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: resolved_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: history_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: out_w,
                    height: out_h,
                    depth_or_array_layers: 1,
                },
            );
        }

        // ---- blit / FXAA to the output ----
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: out_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &targets.blit_bind_group, &[]);
            pass.set_pipeline(&self.blit_pipeline);
            pass.draw(0..3, 0..1);
            draw_calls += 1;
        }
        self.queue.submit([encoder.finish()]);

        if shadow_light {
            if let Some(t) = &self.shadow_timer {
                t.kick_readback();
            }
        }
        if let Some(t) = &self.scene_timer {
            t.kick_readback();
        }
        if self.cull_mode == CullMode::Gpu {
            if let Some(gc) = &self.gpu_cull {
                if let Some(t) = &gc.timer {
                    t.kick_readback();
                }
                gc.kick_readback();
            }
        }

        self.prev_view_proj = view_proj;
        if self.aa_mode == AaMode::Taa {
            self.history_valid = true;
        }
        self.frame_count += 1;
        self.last_draw_calls = draw_calls;
        self.last_shadow_draw_calls = shadow_draws;
        self.last_shadow_active = shadow_light;
        // Scene triangles: culled/LOD'd terrain + water (2) + sky (1) +
        // blit (1). On the GPU path the terrain term is the readback
        // snapshot (a frame behind while the camera moves; exact at
        // any held pose).
        self.last_tris = self.cull_stats().tris_after + 2 + 1 + 1;
        self.last_shadow_tris = self.vista.cull.shadow_tris;
    }
}

fn smoothstep_cpu(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
