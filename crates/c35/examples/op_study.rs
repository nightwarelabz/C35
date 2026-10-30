//! Operator study rig (operator-perfection phase 1 proof seam).
//!
//! Renders the procedural operator on the island proof spot from
//! dedicated study cameras — rear three-quarter (the primary
//! angle), front, and side, at close range and at patrol distance
//! (~20 m) — under the spring-night reality pin the M6/M7/Slice 1
//! proofs use, in PS1.5 and classic presentations. It exists so a
//! model pass on `operator.rs` can be judged before/after from
//! identical cameras; it renders only what the public operator and
//! renderer APIs already expose, and writes its PNGs with a small
//! self-contained encoder (no new dependencies).
//!
//! Usage: op_study --out <dir> [--present ps15|classic]
//!                 [--pose stand|crouch|aim]
//! Env:   VK_ICD_FILENAMES selects the Vulkan ICD, as in the other
//!        proof rigs. Reality is pinned in-code (Chicago observer,
//!        2026-04-16 23:30 -05:00) so before/after runs cannot drift.

use c35::glam::Vec3;
use c35::operator::{build_operator, OperatorPose, OPERATOR_SPOT, OPERATOR_YAW_DEG};
use c35::scene::{CameraPose, height_at};
use c35::world::{days_from_civil, world_state, Observer, WorldState};

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

/// The spring-night pin, computed the way the native host's
/// COMIGEN_PIN_ISO parser computes it (2026-04-16T23:30:00-05:00,
/// Chicago observer).
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

fn chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(name);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) {
    // Raw scanlines with a filter byte (0) per row.
    let mut raw = Vec::with_capacity((w as usize * 4 + 1) * h as usize);
    for y in 0..h as usize {
        raw.push(0u8);
        raw.extend_from_slice(&rgba[y * w as usize * 4..(y + 1) * w as usize * 4]);
    }
    // zlib stream: header + stored blocks + adler32.
    let mut z = Vec::with_capacity(raw.len() + raw.len() / 65535 * 5 + 6);
    z.extend_from_slice(&[0x78, 0x01]);
    let mut off = 0usize;
    while off < raw.len() {
        let n = (raw.len() - off).min(65535);
        let last = off + n >= raw.len();
        z.push(if last { 1 } else { 0 });
        z.extend_from_slice(&(n as u16).to_le_bytes());
        z.extend_from_slice(&(!(n as u16)).to_le_bytes());
        z.extend_from_slice(&raw[off..off + n]);
        off += n;
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);
    std::fs::write(path, png).expect("write png");
}

fn save_png(renderer: &c35::Renderer, tex: &c35::wgpu::Texture, w: u32, h: u32, path: &str) {
    let bpr = (w * 4).div_ceil(256) * 256;
    let buf = renderer.device.create_buffer(&c35::wgpu::BufferDescriptor {
        label: Some("study readback"),
        size: (bpr * h) as u64,
        usage: c35::wgpu::BufferUsages::MAP_READ | c35::wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&c35::wgpu::CommandEncoderDescriptor {
        label: Some("study copy"),
    });
    encoder.copy_texture_to_buffer(
        c35::wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: c35::wgpu::Origin3d::ZERO,
            aspect: c35::wgpu::TextureAspect::All,
        },
        c35::wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: c35::wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bpr),
                rows_per_image: Some(h),
            },
        },
        c35::wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    renderer.queue.submit([encoder.finish()]);
    buf.slice(..).map_async(c35::wgpu::MapMode::Read, |_| {});
    let _ = renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });
    let range = buf.slice(..).get_mapped_range();
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h as usize {
        let start = row * bpr as usize;
        pixels.extend_from_slice(&range[start..start + (w * 4) as usize]);
    }
    drop(range);
    buf.unmap();
    write_png(path, w, h, &pixels);
    println!("saved {path}");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_dir = arg_value(&args, "--out").unwrap_or_else(|| ".".to_string());
    let present = arg_value(&args, "--present").unwrap_or_else(|| "ps15".to_string());
    let pose_name = arg_value(&args, "--pose").unwrap_or_else(|| "stand".to_string());
    std::fs::create_dir_all(&out_dir).expect("create out dir");

    let pose = match pose_name.as_str() {
        "crouch" => OperatorPose::Crouch,
        "aim" => OperatorPose::Aim,
        _ => OperatorPose::StandReady,
    };

    let world = pinned_world();
    println!(
        "reality pin: sun elevation {:+.2} deg, azimuth {:.1} deg",
        world.sun_elevation_deg, world.sun_azimuth_deg
    );

    let (width, height) = (1280u32, 720u32);
    let instance = c35::create_instance();
    let mut renderer = c35::pollster::block_on(c35::Renderer::new(
        &instance,
        c35::wgpu::TextureFormat::Rgba8UnormSrgb,
        width,
        height,
        1.0,
    ));
    if present == "ps15" {
        renderer.set_present_mode(c35::PresentMode::Ps15, 2.0);
    }
    renderer.pin_governor(Some(0));

    let ground = height_at(OPERATOR_SPOT.0, OPERATOR_SPOT.1);
    let spot = Vec3::new(OPERATOR_SPOT.0, 0.0, OPERATOR_SPOT.1);
    let mesh = build_operator(pose, spot, ground, OPERATOR_YAW_DEG);
    println!(
        "operator: {} triangles, {} parts",
        mesh.triangle_count(),
        mesh.parts.len()
    );
    renderer.update_operator(&mesh);

    let out_tex = renderer.device.create_texture(&c35::wgpu::TextureDescriptor {
        label: Some("study output"),
        size: c35::wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: c35::wgpu::TextureDimension::D2,
        format: c35::wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: c35::wgpu::TextureUsages::RENDER_ATTACHMENT | c35::wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let out_view = out_tex.create_view(&Default::default());

    // Study cameras, built from the operator's own frame: fwd is
    // where he faces, left is his left (the pack side). Targets sit
    // at mid-torso; heights are above the proof spot's ground.
    let yaw = OPERATOR_YAW_DEG.to_radians();
    let fwd = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let left = Vec3::new(fwd.z, 0.0, -fwd.x);
    let base = Vec3::new(OPERATOR_SPOT.0, ground, OPERATOR_SPOT.1);
    let target = base + Vec3::new(0.0, 1.12, 0.0);
    let cam = |off: Vec3| CameraPose { eye: base + off, target, fov_y_deg: 50.0 };
    let views: Vec<(&str, CameraPose)> = vec![
        // The primary angle: behind him, off his left shoulder —
        // pack, whips, pouches, slung gun.
        ("rear3q-close", cam(-fwd * 3.1 + left * 1.5 + Vec3::new(0.0, 1.55, 0.0))),
        ("rear3q-far", cam(-fwd * 19.4 + left * 4.5 + Vec3::new(0.0, 2.2, 0.0))),
        ("front-close", cam(fwd * 3.1 - left * 1.3 + Vec3::new(0.0, 1.5, 0.0))),
        ("front-far", cam(fwd * 19.4 - left * 5.0 + Vec3::new(0.0, 2.2, 0.0))),
        ("side-close", cam(left * 3.4 + Vec3::new(0.0, 1.35, 0.0))),
        ("side-far", cam(left * 19.8 + Vec3::new(0.0, 2.0, 0.0))),
    ];

    for (name, pose_cam) in &views {
        for _ in 0..12 {
            renderer.render(&out_view, pose_cam, &world, 0.0);
            let _ = renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });
        }
        save_png(
            &renderer,
            &out_tex,
            width,
            height,
            &format!("{out_dir}/study-{name}-{pose_name}-{present}.png"),
        );
    }

    let (iw, ih) = renderer.internal_size();
    println!("=== OP STUDY LEDGER ({present}) ===");
    println!("backend          : {}", renderer.backend_name());
    println!("output           : {}x{} (internal {}x{})", width, height, iw, ih);
    println!("presentation     : {}", renderer.present_description());
    println!("draw calls/frame : {}", renderer.draw_calls_per_frame());
    println!("triangles/frame  : {}", renderer.triangles_per_frame());
    println!(
        "tracked vram     : {:.2} MiB",
        renderer.tracked_vram_bytes() as f64 / (1024.0 * 1024.0)
    );
    println!("governor         : pinned at level 0");
}
