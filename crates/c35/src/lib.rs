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
//! The house law: **if it costs, it shows.** Every frame is counted —
//! draw calls per pass, triangles, render scale, tracked memory, frame
//! time, and GPU pass times where the backend exposes timestamp queries.

pub mod noise;
pub mod scene;
pub mod world;

/// Re-exported so hosts build against exactly the engine's wgpu.
pub use wgpu;
pub use pollster;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

use scene::{CameraPose, TerrainMesh};
use world::WorldState;

pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Single-cascade sun shadow map edge (kept in sync with SHADOW_TEXEL in
/// shaders/common.wgsl). 2048 over a ±380-unit ortho box ≈ 0.37
/// units/texel — fine enough to carve the headwall's shadow, cheap enough
/// to stay one draw call. The ledger prints its cost like everything else.
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
    terrain_vb: wgpu::Buffer,
    terrain_ib: wgpu::Buffer,
    terrain_indices: u32,
    terrain_tris: u64,
    geometry_bytes: u64,
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

        // ---- terrain geometry (generated, seeded — the same on every host) ----
        let mesh: TerrainMesh = scene::generate_terrain();
        let terrain_tris = mesh.triangle_count();
        let terrain_indices = mesh.indices.len() as u32;
        let vb_bytes = (mesh.vertices.len() * std::mem::size_of::<scene::TerrainVertex>()) as u64;
        let ib_bytes = (mesh.indices.len() * 4) as u64;
        let terrain_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let terrain_ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // ---- GPU timers (optional; only where the adapter offered them) ----
        let (shadow_timer, scene_timer) = if features.contains(wgpu::Features::TIMESTAMP_QUERY) {
            (
                Some(PassTimer::new(&device, period_ns, "shadow timestamps")),
                Some(PassTimer::new(&device, period_ns, "scene timestamps")),
            )
        } else {
            (None, None)
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
            terrain_vb,
            terrain_ib,
            terrain_indices,
            terrain_tris,
            geometry_bytes: vb_bytes + ib_bytes,
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
        renderer.resize(width, height, render_scale);
        renderer
    }

    /// (Re)create the offscreen scene targets for an output of `width`×`height`
    /// rendered internally at `scale`.
    pub fn resize(&mut self, width: u32, height: u32, scale: f32) {
        let scale = scale.clamp(0.25, 1.0);
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

        // TAA chain: a resolved target (resolve output + blit source) and
        // a history texture (last frame's resolved image).
        let (blit_source_view, history_tex, tex_count) = if self.aa_mode == AaMode::Taa {
            let resolved = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("taa resolved"),
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
            let history = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("taa history"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SCENE_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            // Keep the resolved texture alive by leaking its handle into
            // the history slot's companion — the Targets struct keeps the
            // textures it needs below.
            (resolved.create_view(&Default::default()), Some((history, resolved)), 4u64)
        } else {
            (color_view.clone(), None, 2u64)
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
        // color(4) + depth(4) [+ resolved(4) + history(4)] bytes per pixel.
        let bytes = (sw * sh) as u64 * 4 * tex_count;
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

    pub fn triangles_per_frame(&self) -> u64 {
        self.terrain_tris + 2 /* water */ + 1 /* sky */ + 1 /* blit */
    }

    /// Triangles the shadow pass re-renders (the terrain, from the sun).
    pub fn shadow_triangles_per_frame(&self) -> u64 {
        if self.last_shadow_active {
            self.terrain_tris
        } else {
            0
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
        let targets = self.targets.as_ref().expect("renderer sized");
        let (internal_w, internal_h) = (targets.internal_w, targets.internal_h);
        let aspect = targets.width as f32 / targets.height as f32;
        let view = Mat4::look_at_rh(pose.eye, pose.target, Vec3::Y);
        let proj = Mat4::perspective_rh(pose.fov_y_deg.to_radians(), aspect, 0.5, 4000.0);
        let view_proj = proj * view;

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

        let sun_up = world.sun_elevation_deg > 0.0;
        let shadow_strength = smoothstep_cpu(1.0, 6.0, world.sun_elevation_deg);
        let light_vp = Self::light_view_proj(world.sun_dir);

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

        // ---- shadow pass (skipped outright when the sun is down) ----
        if sun_up {
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
            pass.set_vertex_buffer(0, self.terrain_vb.slice(..));
            pass.set_index_buffer(self.terrain_ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.terrain_indices, 0, 0..1);
            shadow_draws = 1;
            draw_calls += 1;
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
            pass.set_vertex_buffer(0, self.terrain_vb.slice(..));
            pass.set_index_buffer(self.terrain_ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.terrain_indices, 0, 0..1);
            pass.set_pipeline(&self.water_pipeline);
            pass.draw(0..6, 0..1);
            draw_calls += 3;
        }
        if let Some(t) = &self.shadow_timer {
            if sun_up {
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
                    width: internal_w,
                    height: internal_h,
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

        if sun_up {
            if let Some(t) = &self.shadow_timer {
                t.kick_readback();
            }
        }
        if let Some(t) = &self.scene_timer {
            t.kick_readback();
        }

        self.prev_view_proj = view_proj;
        if self.aa_mode == AaMode::Taa {
            self.history_valid = true;
        }
        self.frame_count += 1;
        self.last_draw_calls = draw_calls;
        self.last_shadow_draw_calls = shadow_draws;
        self.last_shadow_active = sun_up;
    }
}

fn smoothstep_cpu(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
