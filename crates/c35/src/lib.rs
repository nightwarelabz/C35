//! # Comigen (C35) — engine core
//!
//! A cost-accounted procedural graphics engine. M1: a wgpu renderer core and
//! one deterministic procedural vista (terrain + sky + water at golden hour),
//! rendered identically on native and in the browser from this single crate.
//!
//! The house law: **if it costs, it shows.** Every frame is counted — draw
//! calls, triangles, render scale, tracked memory, frame time, and GPU pass
//! time where the backend exposes timestamp queries.

pub mod noise;
pub mod scene;

/// Re-exported so hosts build against exactly the engine's wgpu.
pub use wgpu;
pub use pollster;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

use scene::{CameraPose, TerrainMesh};

pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Fog density (exponential). One of the vista's art knobs.
pub const FOG_DENSITY: f32 = 0.00135;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    misc: [f32; 4],
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

struct GpuTimer {
    query_set: wgpu::QuerySet,
    resolve_buf: wgpu::Buffer,
    readback_buf: Arc<wgpu::Buffer>,
    pending: Arc<AtomicBool>,
    samples_ms: Arc<Mutex<Vec<f64>>>,
    period_ns: f32,
}

struct Targets {
    width: u32,
    height: u32,
    scale: f32,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    blit_bind_group: wgpu::BindGroup,
    bytes: u64,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    adapter_info: wgpu::AdapterInfo,
    output_format: wgpu::TextureFormat,
    sky_pipeline: wgpu::RenderPipeline,
    terrain_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    blit_layout: wgpu::BindGroupLayout,
    blit_sampler: wgpu::Sampler,
    terrain_vb: wgpu::Buffer,
    terrain_ib: wgpu::Buffer,
    terrain_indices: u32,
    terrain_tris: u64,
    geometry_bytes: u64,
    targets: Option<Targets>,
    timer: Option<GpuTimer>,
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

    /// Surface build (web host): adapter is matched to `surface` and the
    /// output format is the surface's preferred format. Read it back with
    /// `output_format()` to configure the surface.
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

        // ---- scene bind group (uniforms) ----
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene layout"),
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
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene bind group"),
            layout: &scene_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
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
        let blit_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/blit.wgsl").into()),
        });

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

        // ---- GPU timer (optional; only where the adapter offered it) ----
        let timer = if features.contains(wgpu::Features::TIMESTAMP_QUERY) {
            let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("frame timestamps"),
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
            Some(GpuTimer {
                query_set,
                resolve_buf,
                readback_buf,
                pending: Arc::new(AtomicBool::new(false)),
                samples_ms: Arc::new(Mutex::new(Vec::new())),
                period_ns,
            })
        } else {
            None
        };

        let mut renderer = Renderer {
            device,
            queue,
            adapter_info,
            output_format,
            sky_pipeline,
            terrain_pipeline,
            water_pipeline,
            blit_pipeline,
            uniform_buf,
            scene_bind_group,
            blit_layout,
            blit_sampler,
            terrain_vb,
            terrain_ib,
            terrain_indices,
            terrain_tris,
            geometry_bytes: vb_bytes + ib_bytes,
            targets: None,
            timer,
        };
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&Default::default());
        let blit_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit bind group"),
            layout: &self.blit_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.blit_sampler),
                },
            ],
        });
        let bytes = (sw * sh * 4 + sw * sh * 4) as u64; // color + depth32
        self.targets = Some(Targets {
            width,
            height,
            scale,
            color_view,
            depth_view: depth.create_view(&Default::default()),
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

    pub fn draw_calls_per_frame(&self) -> u32 {
        4 // sky + terrain + water + blit
    }

    pub fn triangles_per_frame(&self) -> u64 {
        self.terrain_tris + 2 /* water */ + 1 /* sky */ + 1 /* blit */
    }

    /// Engine-tracked GPU memory: geometry + scene targets + uniforms.
    /// An estimate by construction — the ledger labels it as such.
    pub fn tracked_vram_bytes(&self) -> u64 {
        let t = self.targets.as_ref().map(|t| t.bytes).unwrap_or(0);
        self.geometry_bytes + t + std::mem::size_of::<Uniforms>() as u64
    }

    pub fn gpu_timestamps_available(&self) -> bool {
        self.timer.is_some()
    }

    /// Drain collected GPU scene-pass durations (milliseconds).
    pub fn drain_gpu_samples(&self) -> Vec<f64> {
        match &self.timer {
            Some(t) => {
                let mut s = t.samples_ms.lock().unwrap();
                std::mem::take(&mut *s)
            }
            None => Vec::new(),
        }
    }

    /// Render one frame of the vista into `out_view`.
    pub fn render(&mut self, out_view: &wgpu::TextureView, pose: &CameraPose, time: f32) {
        let targets = self.targets.as_ref().expect("renderer sized");
        let aspect = targets.width as f32 / targets.height as f32;
        let view = Mat4::look_at_rh(pose.eye, pose.target, Vec3::Y);
        let proj = Mat4::perspective_rh(
            pose.fov_y_deg.to_radians(),
            aspect,
            0.5,
            4000.0,
        );
        let view_proj = proj * view;
        let uniforms = Uniforms {
            view_proj: view_proj.to_cols_array_2d(),
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            cam_pos: Vec4::from((pose.eye, 1.0)).to_array(),
            sun_dir: Vec4::from((scene::sun_direction(), 0.0)).to_array(),
            sun_color: Vec4::from((scene::sun_color(), 0.0)).to_array(),
            misc: [time, FOG_DENSITY, targets.scale, aspect],
        };
        self.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&[uniforms]));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let ts_writes = self.timer.as_ref().map(|t| wgpu::RenderPassTimestampWrites {
                query_set: &t.query_set,
                beginning_of_pass_write_index: Some(0),
                end_of_pass_write_index: Some(1),
            });
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
                timestamp_writes: ts_writes,
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
        }
        if let Some(t) = &self.timer {
            if !t.pending.load(Ordering::Relaxed) {
                encoder.resolve_query_set(&t.query_set, 0..2, &t.resolve_buf, 0);
                encoder.copy_buffer_to_buffer(&t.resolve_buf, 0, &t.readback_buf, 0, 16);
            }
        }
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
        }
        self.queue.submit([encoder.finish()]);

        // If a resolve+copy rode this submission, kick off its readback. The
        // callback lands a frame or two later and banks the GPU pass time.
        if let Some(t) = &self.timer {
            if !t.pending.swap(true, Ordering::Relaxed) {
                let samples = t.samples_ms.clone();
                let pending = t.pending.clone();
                let buf_submit = t.readback_buf.clone();
                let buf_cb = t.readback_buf.clone();
                let period = t.period_ns;
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
    }
}
