//! Modeler proof rig: the A/B the system was ordered for.
//!
//! Builds the Greyport lamp standard twice —
//!   A: the shipped recipe (`greyport:lamp-post`, six boxes), and
//!   B: the modeled standard (modeler op script → `.c35m` save →
//!      load → bake through `modeler::to_prop_mesh`),
//! and renders both from one camera, at one spot, under the same
//! spring-night reality pin the other proof rigs use, in PS1.5
//! at its native 853×480 internal frame. B reaches the renderer
//! through exactly the path A uses (`Renderer::set_props`) — the
//! integration seam is that there is no other seam.
//!
//! Usage: modeler_demo --out <dir>
//! Env:   VK_ICD_FILENAMES selects the Vulkan ICD, as in the
//!        other proof rigs.
//!
//! Writes: lamp-primitive.png, lamp-modeled.png,
//!         lamp-ab-combined.png, lamp-standard.c35m

use c35::glam::Vec3;
use c35::modding::model::ParamValues;
use c35::modding::Registry;
use c35::modeler;
use c35::operator::OPERATOR_SPOT;
use c35::props::PropMesh;
use c35::scene::{height_at, CameraPose};
use c35::world::{days_from_civil, world_state, Observer, WorldState};

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

/// The spring-night pin (2026-04-16 23:30 -05:00, Chicago), the
/// same pin op_study uses, so the A/B cannot drift from the
/// house proofs.
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
// The proof rigs each carry this encoder; op_study's copy is the
// parent of this one.

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, e) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB88320 ^ (c >> 1)
            } else {
                c >> 1
            };
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
    let mut raw = Vec::with_capacity((w as usize * 4 + 1) * h as usize);
    for y in 0..h as usize {
        raw.push(0u8);
        raw.extend_from_slice(&rgba[y * w as usize * 4..(y + 1) * w as usize * 4]);
    }
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
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);
    std::fs::write(path, png).expect("write png");
}

fn save_png(renderer: &c35::Renderer, tex: &c35::wgpu::Texture, w: u32, h: u32, path: &str) {
    let bpr = (w * 4).div_ceil(256) * 256;
    let buf = renderer.device.create_buffer(&c35::wgpu::BufferDescriptor {
        label: Some("modeler readback"),
        size: (bpr * h) as u64,
        usage: c35::wgpu::BufferUsages::MAP_READ | c35::wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder =
        renderer
            .device
            .create_command_encoder(&c35::wgpu::CommandEncoderDescriptor {
                label: Some("modeler copy"),
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
        c35::wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue.submit([encoder.finish()]);
    buf.slice(..).map_async(c35::wgpu::MapMode::Read, |_| {});
    let _ = renderer.device.poll(c35::wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
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

/// Translate a baked mesh so its lowest vertex sits at `ground`
/// over (x, z).
fn place(mesh: &PropMesh, x: f32, ground: f32, z: f32) -> PropMesh {
    let min_y = mesh
        .vertices
        .iter()
        .map(|v| v.position[1])
        .fold(f32::INFINITY, f32::min);
    let mut out = PropMesh::new();
    for v in &mesh.vertices {
        let mut v = *v;
        v.position = [
            v.position[0] + x,
            v.position[1] - min_y + ground,
            v.position[2] + z,
        ];
        out.vertices.push(v);
    }
    out.indices = mesh.indices.clone();
    out
}

fn append(dst: &mut PropMesh, src: &PropMesh) {
    let base = dst.vertices.len() as u32;
    dst.vertices.extend_from_slice(&src.vertices);
    dst.indices.extend(src.indices.iter().map(|i| i + base));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_dir = arg_value(&args, "--out").unwrap_or_else(|| "modeler-stills".to_string());
    std::fs::create_dir_all(&out_dir).expect("create out dir");

    // ---- B's road: op script -> save -> load -> bake. ----
    let modeled = modeler::assets::lamp_standard();
    let faults = modeled.validate();
    assert!(
        faults.is_empty(),
        "proof asset must be watertight: {faults:?}"
    );
    let model_path = format!("{out_dir}/lamp-standard.c35m");
    modeler::save_to(&modeled, std::path::Path::new(&model_path)).expect("save model");
    let text = std::fs::read_to_string(&model_path).expect("read model back");
    let loaded = modeler::load(&text).expect("load model");
    assert_eq!(loaded, modeled, "load(save(m)) must be m");
    assert_eq!(
        modeler::save(&loaded),
        text,
        "re-save must be byte-identical"
    );
    let modeled_baked = modeler::to_prop_mesh(&loaded).expect("bake modeled lamp");
    println!(
        "modeled lamp : {} tris, {} verts, {} faces, .c35m {} bytes, round-trip byte-identical",
        modeled.triangle_count(),
        modeled.verts.len(),
        modeled.faces.len(),
        text.len()
    );

    // ---- A's road: the shipped recipe, untouched. ----
    let reg = Registry::new();
    let prim = reg
        .build("greyport:lamp-post", &ParamValues::new())
        .expect("build recipe lamp");
    println!(
        "recipe lamp  : {} tris, {} verts (registry census: {})",
        prim.mesh.triangle_count(),
        prim.mesh.vertices.len(),
        prim.triangles
    );

    // ---- One renderer, one night, one spot. ----
    let world = pinned_world();
    let (width, height) = (853u32, 480u32);
    let instance = c35::create_instance();
    let mut renderer = c35::pollster::block_on(c35::Renderer::new(
        &instance,
        c35::wgpu::TextureFormat::Rgba8UnormSrgb,
        width,
        height,
        1.0,
    ));
    renderer.set_present_mode(c35::PresentMode::Ps15, 2.0);
    renderer.pin_governor(Some(0));

    let ground = height_at(OPERATOR_SPOT.0, OPERATOR_SPOT.1);
    let base = Vec3::new(OPERATOR_SPOT.0, ground, OPERATOR_SPOT.1);

    let out_tex = renderer
        .device
        .create_texture(&c35::wgpu::TextureDescriptor {
            label: Some("modeler output"),
            size: c35::wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: c35::wgpu::TextureDimension::D2,
            format: c35::wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: c35::wgpu::TextureUsages::RENDER_ATTACHMENT | c35::wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
    let out_view = out_tex.create_view(&Default::default());

    let shoot = |renderer: &mut c35::Renderer, mesh: &PropMesh, cam: CameraPose, path: &str| {
        renderer.set_props(Some(mesh));
        for _ in 0..12 {
            renderer.render(&out_view, &cam, &world, 0.0);
            let _ = renderer.device.poll(c35::wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            });
        }
        save_png(renderer, &out_tex, width, height, path);
    };

    // Singles: the same framing for both, the lamp alone.
    // The modeled standard stands ~6.35 m tall against the
    // recipe's ~5.62 m; the camera sits far enough back that
    // the taller one fits whole.
    let single_cam = CameraPose {
        eye: base + Vec3::new(5.6, 3.2, 7.0),
        target: base + Vec3::new(0.6, 3.1, 0.0),
        fov_y_deg: 50.0,
    };
    let prim_here = place(&prim.mesh, base.x, ground, base.z);
    shoot(
        &mut renderer,
        &prim_here,
        single_cam,
        &format!("{out_dir}/lamp-primitive.png"),
    );
    let modeled_here = place(&modeled_baked, base.x, ground, base.z);
    shoot(
        &mut renderer,
        &modeled_here,
        single_cam,
        &format!("{out_dir}/lamp-modeled.png"),
    );

    // Combined: both in frame, recipe left, modeled right.
    let mut both = place(
        &prim.mesh,
        base.x - 2.1,
        height_at(base.x - 2.1, base.z - 0.8),
        base.z - 0.8,
    );
    append(
        &mut both,
        &place(
            &modeled_baked,
            base.x + 2.1,
            height_at(base.x + 2.1, base.z + 0.8),
            base.z + 0.8,
        ),
    );
    let both_cam = CameraPose {
        eye: base + Vec3::new(0.4, 3.1, 10.4),
        target: base + Vec3::new(0.4, 2.8, 0.0),
        fov_y_deg: 50.0,
    };
    shoot(
        &mut renderer,
        &both,
        both_cam,
        &format!("{out_dir}/lamp-ab-combined.png"),
    );
    renderer.set_props(None);

    let (iw, ih) = renderer.internal_size();
    println!("=== MODELER DEMO LEDGER ===");
    println!("backend          : {}", renderer.backend_name());
    println!(
        "output           : {}x{} (internal {}x{})",
        width, height, iw, ih
    );
    println!("presentation     : {}", renderer.present_description());
    println!(
        "triangles        : recipe {} vs modeled {}",
        prim.mesh.triangle_count(),
        modeled_baked.triangle_count()
    );
}
