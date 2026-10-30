//! UI demo: a menu screen, built with the UI module and
//! rendered for real — not a geometry dump.
//!
//! The screen is a DeBrief-flavoured menu (title, list with
//! selection, a meter bar, a dialogue box) laid out in
//! points at the 480pt reference height. The engine's
//! renderer draws the (suppressed-vista) scene frame; the
//! UI pass is example-local scaffolding standing in for
//! the host's UI pass, exactly as the architecture intends:
//! the engine owns the scene pass, the host owns the UI
//! pass, the UiMesh is the seam. The pass draws the whole
//! mesh in one indexed draw, on top of the scene, into
//! the same output texture.
//!
//! Run: cargo run -q -p c35 --example ui_demo [--out dir]
//! Writes ui-menu.png (1280x720) and prints the ledger.
//! Env: VK_ICD_FILENAMES selects the Vulkan ICD, as in
//! the other rigs. Reality is pinned (Chicago observer,
//! 2026-04-16 23:30 -05:00) so runs cannot drift.

use c35::glam::Vec3;
use c35::scene::CameraPose;
use c35::wgpu::util::DeviceExt;
use c35::ui::{Align, Anchor, DialogueCue, Rect, UiScreen};
use c35::world::{days_from_civil, world_state, Observer, WorldState};

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

fn pinned_world() -> WorldState {
    let days = days_from_civil(2026, 4, 16) as f64;
    let local = days * 86400.0 + 23.0 * 3600.0 + 30.0 * 60.0;
    let unix = local - (-300.0) * 60.0;
    let observer = Observer {
        latitude_deg: Some(41.88),
        longitude_deg: Some(-87.63),
        utc_offset_minutes: Some(-300.0),
    };
    world_state(unix, &observer)
}

// ---- minimal PNG writer (zlib stored blocks; no dependencies) ----

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, e) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        *e = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) {
    // Stored-block zlib: each scanline is its own block, with
    // a filter byte 0 prefix. No compression, no dependency.
    let mut raw = Vec::new();
    for row in 0..h as usize {
        raw.push(0u8);
        raw.extend_from_slice(&rgba[row * w as usize * 4..(row + 1) * w as usize * 4]);
    }
    let mut z = Vec::new();
    z.extend_from_slice(&[0x78, 0x01]);
    let mut off = 0usize;
    while off < raw.len() || raw.is_empty() {
        let chunk = raw.len().min(off + 65535);
        let len = (chunk - off) as u16;
        let last = if chunk == raw.len() { 1u8 } else { 0u8 };
        z.push(last);
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(&raw[off..chunk]);
        off = chunk;
        if raw.is_empty() {
            break;
        }
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    let chunk = |tag: &[u8; 4], data: &[u8], png: &mut Vec<u8>| {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        png.extend_from_slice(tag);
        png.extend_from_slice(data);
        let mut crc_data = tag.to_vec();
        crc_data.extend_from_slice(data);
        png.extend_from_slice(&crc32(&crc_data).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr, &mut png);
    chunk(b"IDAT", &z, &mut png);
    chunk(b"IEND", &[], &mut png);
    std::fs::write(path, png).expect("write png");
}

// ---- the menu screen ----

const INK: [f32; 4] = [0.92, 0.90, 0.84, 1.0];
const DIM: [f32; 4] = [0.55, 0.55, 0.58, 1.0];
const SODIUM: [f32; 4] = [1.0, 0.62, 0.12, 1.0];
const PANEL_FILL: [f32; 4] = [0.05, 0.06, 0.09, 0.92];
const BACKDROP: [f32; 4] = [0.03, 0.03, 0.05, 1.0];
const HIGHLIGHT: [f32; 4] = [0.22, 0.10, 0.06, 1.0];
const TRACK: [f32; 4] = [0.12, 0.12, 0.14, 1.0];

fn build_screen(w: u32, h: u32) -> UiScreen {
    let mut s = UiScreen::new(w, h);
    let frame = s.frame();
    // Backdrop: the whole frame, near-black blue.
    s.panel(frame, BACKDROP, BACKDROP, 0.0);
    // Title, top-center.
    let title = s.anchor((400.0, 48.0), Anchor::TopCenter, 28.0);
    s.text_in("DEBRIEF", title, 42.0, INK, Align::Center);
    let sub = Rect::new(title.x, title.y + title.h + 2.0, title.w, 20.0);
    s.text_in("A CITY OF SPIES", sub, 14.0, SODIUM, Align::Center);
    // Menu list panel, center-left. Height is 4 rows plus
    // the 8pt inset on both sides, so all four rows fit
    // the inner rect exactly (the list clips rows past
    // the bottom, and an undersized panel would eat one).
    let list_rect = Rect::new(96.0, 130.0, 320.0, 4.0 * 36.0 + 16.0);
    s.panel(list_rect, PANEL_FILL, DIM, 2.0);
    s.list(
        &["CONTINUE", "NEW OPERATION", "COMPENDIUM", "OPTIONS"],
        1,
        list_rect.inset(8.0),
        36.0,
        16.0,
        DIM,
        INK,
        HIGHLIGHT,
    );
    // Meter bar, under the list.
    let bar_label = Rect::new(96.0, 300.0, 320.0, 18.0);
    s.text_in("SIGNAL", bar_label, 14.0, DIM, Align::Left);
    let bar_rect = Rect::new(96.0, 320.0, 320.0, 16.0);
    s.bar(bar_rect, 0.62, TRACK, SODIUM, DIM, 1.0);
    // Dialogue box, bottom, full width minus margins.
    // Ends at 444pt; the footer sits at 450pt, so the two
    // never share a pixel (the first layout overlapped).
    let dlg = Rect::new(64.0, 348.0, frame.w - 128.0, 96.0);
    s.dialogue(
        "HANDLER",
        "The city is listening. Move like you belong here, and it will never see you at all.",
        DialogueCue::More,
        dlg,
        PANEL_FILL,
        DIM,
        2.0,
        SODIUM,
        INK,
    );
    // Footer hint.
    let foot = s.anchor((400.0, 18.0), Anchor::BottomCenter, 12.0);
    s.text_in("ENTER - SELECT   ESC - BACK", foot, 12.0, DIM, Align::Center);
    s
}

const UI_WGSL: &str = r#"
struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};
@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) color: vec4<f32>) -> VertexOut {
    var out: VertexOut;
    out.pos = vec4<f32>(pos, 0.0, 1.0);
    out.color = color;
    return out;
}
@fragment
fn fs_main(@location(0) color: vec4<f32>) -> @location(0) vec4<f32> {
    return color;
}
"#;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_dir = arg_value(&args, "--out").unwrap_or_else(|| ".".to_string());
    std::fs::create_dir_all(&out_dir).expect("create out dir");

    let (width, height) = (1280u32, 720u32);

    // Build twice: determinism is one line of output, not a
    // harness — the same screen must batch identically.
    let (mesh_a, ledger_a) = build_screen(width, height).finish();
    let (mesh_b, _) = build_screen(width, height).finish();
    let det = bytemuck::cast_slice::<c35::ui::UiVertex, u8>(&mesh_a.vertices)
        == bytemuck::cast_slice::<c35::ui::UiVertex, u8>(&mesh_b.vertices)
        && mesh_a.indices == mesh_b.indices;
    println!("=== UI DEMO LEDGER ===");
    println!("frame            : {}x{} (scale {:.2})", width, height, height as f32 / 480.0);
    println!("quads            : {}", ledger_a.quads);
    println!("triangles        : {}", ledger_a.triangles);
    println!("vertices         : {}", ledger_a.vertices);
    println!("indices          : {}", ledger_a.indices);
    println!("draws            : {} (one indexed draw)", ledger_a.draws);
    println!("deterministic    : {}", det);

    // Scene frame first (suppressed vista = blank backdrop),
    // then the UI pass on top, into the same texture.
    let instance = c35::create_instance();
    let mut renderer = c35::pollster::block_on(c35::Renderer::new(
        &instance,
        c35::wgpu::TextureFormat::Rgba8UnormSrgb,
        width,
        height,
        1.0,
    ));
    renderer.set_present_mode(c35::PresentMode::Ps15, 2.0);
    renderer.set_vista_suppressed(true);
    renderer.pin_governor(Some(0));

    let out_tex = renderer.device.create_texture(&c35::wgpu::TextureDescriptor {
        label: Some("ui output"),
        size: c35::wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: c35::wgpu::TextureDimension::D2,
        format: c35::wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: c35::wgpu::TextureUsages::RENDER_ATTACHMENT | c35::wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let out_view = out_tex.create_view(&Default::default());
    let world = pinned_world();
    let pose = CameraPose {
        eye: Vec3::new(0.0, 2.0, 0.0),
        target: Vec3::new(0.0, 1.0, 10.0),
        fov_y_deg: 50.0,
    };
    renderer.render(&out_view, &pose, &world, 0.0);
    let _ = renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });

    // UI pass: one pipeline, one indexed draw, LoadOp::Load
    // so the scene frame survives underneath.
    let module = renderer.device.create_shader_module(c35::wgpu::ShaderModuleDescriptor {
        label: Some("ui"),
        source: c35::wgpu::ShaderSource::Wgsl(UI_WGSL.into()),
    });
    let pipeline = renderer.device.create_render_pipeline(&c35::wgpu::RenderPipelineDescriptor {
        label: Some("ui pipeline"),
        layout: None,
        vertex: c35::wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[c35::wgpu::VertexBufferLayout {
                array_stride: 24,
                step_mode: c35::wgpu::VertexStepMode::Vertex,
                attributes: &[
                    c35::wgpu::VertexAttribute {
                        format: c35::wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    c35::wgpu::VertexAttribute {
                        format: c35::wgpu::VertexFormat::Float32x4,
                        offset: 8,
                        shader_location: 1,
                    },
                ],
            }],
        },
        fragment: Some(c35::wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(c35::wgpu::ColorTargetState {
                format: c35::wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: Some(c35::wgpu::BlendState::ALPHA_BLENDING),
                write_mask: c35::wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: c35::wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: c35::wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    let vb = renderer.device.create_buffer_init(&c35::wgpu::util::BufferInitDescriptor {
        label: Some("ui vertices"),
        contents: bytemuck::cast_slice(&mesh_a.vertices),
        usage: c35::wgpu::BufferUsages::VERTEX,
    });
    let ib = renderer.device.create_buffer_init(&c35::wgpu::util::BufferInitDescriptor {
        label: Some("ui indices"),
        contents: bytemuck::cast_slice(&mesh_a.indices),
        usage: c35::wgpu::BufferUsages::INDEX,
    });
    let mut encoder = renderer.device.create_command_encoder(&c35::wgpu::CommandEncoderDescriptor {
        label: Some("ui encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&c35::wgpu::RenderPassDescriptor {
            label: Some("ui pass"),
            color_attachments: &[Some(c35::wgpu::RenderPassColorAttachment {
                view: &out_view,
                depth_slice: None,
                resolve_target: None,
                ops: c35::wgpu::Operations {
                    load: c35::wgpu::LoadOp::Load,
                    store: c35::wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), c35::wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh_a.indices.len() as u32, 0, 0..1);
    }
    renderer.queue.submit([encoder.finish()]);
    let _ = renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });

    // Readback + PNG.
    let bpr = (width * 4).div_ceil(256) * 256;
    let buf = renderer.device.create_buffer(&c35::wgpu::BufferDescriptor {
        label: Some("ui readback"),
        size: (bpr * height) as u64,
        usage: c35::wgpu::BufferUsages::MAP_READ | c35::wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&c35::wgpu::CommandEncoderDescriptor {
        label: Some("ui copy"),
    });
    encoder.copy_texture_to_buffer(
        c35::wgpu::TexelCopyTextureInfo {
            texture: &out_tex,
            mip_level: 0,
            origin: c35::wgpu::Origin3d::ZERO,
            aspect: c35::wgpu::TextureAspect::All,
        },
        c35::wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: c35::wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bpr),
                rows_per_image: Some(height),
            },
        },
        c35::wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    renderer.queue.submit([encoder.finish()]);
    buf.slice(..).map_async(c35::wgpu::MapMode::Read, |_| {});
    let _ = renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });
    let range = buf.slice(..).get_mapped_range();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height as usize {
        let start = row * bpr as usize;
        pixels.extend_from_slice(&range[start..start + (width * 4) as usize]);
    }
    drop(range);
    buf.unmap();
    let path = format!("{out_dir}/ui-menu.png");
    write_png(&path, width, height, &pixels);
    println!("saved            : {} ({} bytes)", path, std::fs::metadata(&path).unwrap().len());
}
