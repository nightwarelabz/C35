//! DeBrief — Slice 1 host: one Greyport block on the C35 engine.
//!
//! A headless proof runner in the house pattern (the native vista
//! host's rig, pointed at a city): PS1.5 presentation by default,
//! the governor pinned, the vista suppressed, the block placed as
//! props, and both men — the operator and the patrol — merged
//! through the engine's character slot, posed through the
//! PoseFrame seam.
//!
//! Modes:
//!   --mode still   hero street still + the patrol-echo still
//!   --mode poses   the soldier's pose vocabulary (action sheet #15)
//!                  as stills, + the HALT staged at the checkpoint
//!   --mode drive   the two scripted drives (open / shadow) with
//!                  the light read logged per frame against the
//!                  patrol's facing
//!   --mode survey  no renderer: block census, the grapple anchor
//!                  survey, and the light field's route profiles
//!   --mode windowed  the same block LIVE in a real window
//!                  (see windowed.rs; --proof / --smoke <secs> run
//!                  it unattended for the proof rig)
//!
//! Reality rig (same contract as the vista host — pins are read by
//! this host only, never by the engine crate):
//!   COMIGEN_PIN_ISO / COMIGEN_LAT / COMIGEN_LON / COMIGEN_UTC_OFFSET_MIN

mod block;
mod light;
mod soldier;
mod windowed;

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use c35::glam::{Vec2, Vec3};
use c35::locomotion::{Animator, Controller, Gait, ScriptLeg};
use c35::operator::{build_operator_frame, OperatorMesh, PoseFrame};
use c35::world::{iso_utc, world_state, Observer, WorldState};

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

fn env_f32(name: &str) -> Option<f32> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

/// Parse the rig's pin: ISO-8601 local time with an explicit offset.
/// (Copied from the vista host — the rig contract is shared.)
fn parse_pin_iso(s: &str) -> Option<(f64, f32)> {
    let (dt, offset_min) = if let Some(idx) = s.rfind('Z') {
        (&s[..idx], 0.0f32)
    } else if let Some(idx) = s.rfind(['+', '-']) {
        if idx < 10 {
            return None;
        }
        let tail = &s[idx..];
        let sign: f32 = if tail.starts_with('-') { -1.0 } else { 1.0 };
        let mut parts = tail[1..].split(':');
        let h: f32 = parts.next()?.parse().ok()?;
        let m: f32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0.0);
        (&s[..idx], sign * (h * 60.0 + m))
    } else {
        (s, 0.0f32)
    };
    let mut it = dt.split('T');
    let date = it.next()?;
    let time = it.next().unwrap_or("00:00:00");
    let mut d = date.split('-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: i64 = d.next()?.parse().ok()?;
    let day: i64 = d.next()?.parse().ok()?;
    let mut t = time.split(':');
    let hh: f64 = t.next()?.parse().ok()?;
    let mm: f64 = t.next().and_then(|p| p.parse().ok()).unwrap_or(0.0);
    let ss: f64 = t.next().and_then(|p| p.parse().ok()).unwrap_or(0.0);
    // Days since epoch (civil calendar, Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let local = days as f64 * 86400.0 + hh * 3600.0 + mm * 60.0 + ss;
    Some((local - offset_min as f64 * 60.0, offset_min))
}

pub(crate) struct Reality {
    pub(crate) world: WorldState,
    clock_line: String,
    observer_line: String,
    solar_line: String,
}

pub(crate) fn resolve_reality() -> Reality {
    let mut offset_env = env_f32("COMIGEN_UTC_OFFSET_MIN");
    let (unix, clock_line) = match std::env::var("COMIGEN_PIN_ISO") {
        Ok(pin) => match parse_pin_iso(&pin) {
            Some((unix, pin_offset)) => {
                if offset_env.is_none() {
                    offset_env = Some(pin_offset);
                }
                (unix, format!("rig pin (COMIGEN_PIN_ISO={pin})"))
            }
            None => {
                eprintln!("warning: COMIGEN_PIN_ISO not parseable, using system clock");
                (system_now(), "system clock (pin unparseable)".to_string())
            }
        },
        Err(_) => (system_now(), "system clock".to_string()),
    };
    let lat = env_f32("COMIGEN_LAT");
    let lon = env_f32("COMIGEN_LON");
    let observer = Observer { latitude_deg: lat, longitude_deg: lon, utc_offset_minutes: offset_env };
    let world = world_state(unix, &observer);
    let observer_line = match (lat, lon) {
        (Some(la), Some(lo)) => format!("lat {la:.2} (COMIGEN_LAT), lon {lo:.2} (COMIGEN_LON)"),
        (Some(la), None) => format!("lat {la:.2} (COMIGEN_LAT), no longitude"),
        _ => "latitude unknown — equator fallback in force".to_string(),
    };
    let solar_line = if lon.is_some() {
        "longitude".to_string()
    } else if let Some(off) = offset_env {
        format!("utc offset {off:.0} min")
    } else {
        "UTC (no longitude or offset given)".to_string()
    };
    Reality { world, clock_line, observer_line, solar_line }
}

fn system_now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

pub(crate) fn print_reality(r: &Reality) {
    let w = &r.world;
    println!("--- reality reading ---");
    println!("clock source     : {}", r.clock_line);
    println!("instant (UTC)    : {}", iso_utc(w.unix_seconds));
    println!("observer         : {}", r.observer_line);
    println!("solar time src   : {}", r.solar_line);
    println!(
        "sun              : elevation {:+.2} deg, azimuth {:.1} deg, day length {:.1} h",
        w.sun_elevation_deg, w.sun_azimuth_deg, w.day_length_hours
    );
    println!(
        "season           : cold {:.2} / autumn {:.2} / spring {:.2} / summer {:.2}",
        w.cold, w.autumn, w.spring, w.summer
    );
}

/// The follow rig — the vista host's contract, unchanged (DESIGN
/// §8's open-ground law lives in these offsets).
pub(crate) fn follow_pose(anchor: Vec3, yaw: f32) -> c35::scene::CameraPose {
    let fwd = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
    c35::scene::CameraPose {
        eye: anchor + Vec3::new(0.0, 1.52, 0.0) - fwd * 3.4 + right * 0.62,
        target: anchor + Vec3::new(0.0, 1.48, 0.0) + fwd * 4.0,
        fov_y_deg: 50.0,
    }
}

pub(crate) fn flat_ground(_x: f32, _z: f32) -> f32 {
    0.0
}

/// The hero-street camera rig — the still's own composition,
/// shared verbatim with the windowed host's F1 bookmark (one
/// definition: if a still and the window disagree, the window is
/// wrong).
pub(crate) fn hero_pose() -> c35::scene::CameraPose {
    c35::scene::CameraPose {
        eye: Vec3::new(0.8, 1.6, 21.0),
        target: Vec3::new(-1.2, 2.0, -24.0),
        fov_y_deg: 50.0,
    }
}

/// The patrol-echo camera rig (golden #11's echo) — the still's
/// composition, shared with the windowed host's F2 bookmark.
pub(crate) fn echo_pose() -> c35::scene::CameraPose {
    c35::scene::CameraPose {
        eye: Vec3::new(-5.2, 0.9, 29.5),
        target: Vec3::new(3.2, 2.4, 4.0),
        fov_y_deg: 50.0,
    }
}

/// The two crossings' routes — start + scripted legs, shared
/// verbatim with the windowed host's live drives (F3 / F4).
pub(crate) fn drive_route(which: &str) -> (Vec2, Vec<ScriptLeg>) {
    if which == "open" {
        (
            Vec2::new(-2.0, -44.0),
            vec![
                ScriptLeg::to(-2.0, -10.0, Gait::Walk),
                ScriptLeg::to(-2.0, 30.0, Gait::Run),
                ScriptLeg::to(-2.0, 44.0, Gait::Walk),
            ],
        )
    } else {
        (Vec2::new(8.9, -44.0), vec![ScriptLeg::to(8.9, 44.0, Gait::CrouchWalk)])
    }
}

/// Merge the two men's meshes into the engine's one character
/// slot: vertices concatenated, the second mesh's indices offset.
pub(crate) fn merge_meshes(a: &OperatorMesh, b: &OperatorMesh) -> OperatorMesh {
    let mut vertices = Vec::with_capacity(a.vertices.len() + b.vertices.len());
    vertices.extend_from_slice(&a.vertices);
    vertices.extend_from_slice(&b.vertices);
    let base = a.vertices.len() as u32;
    let mut indices = Vec::with_capacity(a.indices.len() + b.indices.len());
    indices.extend_from_slice(&a.indices);
    indices.extend(b.indices.iter().map(|i| i + base));
    let mut parts: Vec<c35::operator::PartInfo> = Vec::with_capacity(a.parts.len() + b.parts.len());
    for p in a.parts.iter().chain(b.parts.iter()) {
        parts.push(c35::operator::PartInfo { name: p.name, bone: p.bone, triangles: p.triangles });
    }
    OperatorMesh { vertices, indices, parts }
}

fn save_png(renderer: &c35::Renderer, tex: &c35::wgpu::Texture, width: u32, height: u32, path: &str) {
    let bpr = (width * 4).div_ceil(256) * 256;
    let buf = renderer.device.create_buffer(&c35::wgpu::BufferDescriptor {
        label: Some("png readback"),
        size: (bpr * height) as u64,
        usage: c35::wgpu::BufferUsages::MAP_READ | c35::wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&c35::wgpu::CommandEncoderDescriptor {
        label: Some("png copy"),
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
    let img = image::RgbaImage::from_raw(width, height, pixels).expect("pixel buffer size");
    img.save(path).expect("save png");
    println!("saved {path}");
}

/// THE scene build — one definition for every host mode: the
/// presentation, the pinned governor, the suppressed vista, the
/// block placed as props, and the wet pass's lamp set mirrored
/// from the light field's own list (so the pools in the frame and
/// the exposures in the light read are one geography). The
/// headless rig and the windowed host both configure through
/// here; a mode that wants a different scene is a different mode,
/// not a quiet edit.
pub(crate) fn configure_scene(renderer: &mut c35::Renderer, blk: &block::Block, present: &str) {
    if present == "ps15" {
        renderer.set_present_mode(c35::PresentMode::Ps15, 2.0);
    }
    renderer.pin_governor(Some(0));
    renderer.set_vista_suppressed(true);
    renderer.set_props(Some(&blk.mesh));
    let lamp_set: Vec<[f32; 4]> = blk
        .field
        .lamps
        .iter()
        .map(|l| [l.head.x, l.head.y, l.head.z, light::LAMP_INTENSITY])
        .collect();
    renderer.set_lamp_set(&lamp_set);
}

struct Rig {
    renderer: c35::Renderer,
    out_tex: c35::wgpu::Texture,
    width: u32,
    height: u32,
}

impl Rig {
    fn new(width: u32, height: u32, present: &str, blk: &block::Block) -> Rig {
        let instance = c35::create_instance();
        let mut renderer = c35::pollster::block_on(c35::Renderer::new(
            &instance,
            c35::wgpu::TextureFormat::Rgba8UnormSrgb,
            width,
            height,
            1.0,
        ));
        configure_scene(&mut renderer, blk, present);
        let out_tex = renderer.device.create_texture(&c35::wgpu::TextureDescriptor {
            label: Some("debrief output"),
            size: c35::wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: c35::wgpu::TextureDimension::D2,
            format: c35::wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: c35::wgpu::TextureUsages::RENDER_ATTACHMENT | c35::wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Rig { renderer, out_tex, width, height }
    }

    fn still(&mut self, world: &WorldState, pose: c35::scene::CameraPose, mesh: Option<&OperatorMesh>, path: &str) {
        match mesh {
            Some(m) => self.renderer.update_operator(m),
            None => self.renderer.clear_operator(),
        }
        let view = self.out_tex.create_view(&Default::default());
        for _ in 0..12 {
            self.renderer.render(&view, &pose, world, 0.0);
            let _ = self.renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });
        }
        save_png(&self.renderer, &self.out_tex, self.width, self.height, path);
    }

    fn ledger(&self, label: &str) {
        let r = &self.renderer;
        let (iw, ih) = r.internal_size();
        println!("=== DEBRIEF COST LEDGER ({label}) ===");
        println!("backend          : {}", r.backend_name());
        println!("output           : {}x{} (internal {}x{})", self.width, self.height, iw, ih);
        println!("presentation     : {}", r.present_description());
        println!("draw calls/frame : {}", r.draw_calls_per_frame());
        println!("triangles/frame  : {}", r.triangles_per_frame());
        println!(
            "tracked vram     : {:.2} MiB (engine-tracked allocations, estimate)",
            r.tracked_vram_bytes() as f64 / (1024.0 * 1024.0)
        );
        println!("governor         : pinned at level 0 (proof rig law)");
    }
}

pub(crate) fn op_mesh(frame: &PoseFrame, spot: Vec3, yaw_deg: f32) -> OperatorMesh {
    build_operator_frame(frame, spot, 0.0, yaw_deg)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = arg_value(&args, "--mode").unwrap_or_else(|| "survey".to_string());
    let out_dir = arg_value(&args, "--out").unwrap_or_else(|| ".".to_string());
    let width: u32 = arg_value(&args, "--width").and_then(|v| v.parse().ok()).unwrap_or(1280);
    let height: u32 = arg_value(&args, "--height").and_then(|v| v.parse().ok()).unwrap_or(720);
    let present = arg_value(&args, "--present").unwrap_or_else(|| "ps15".to_string());
    std::fs::create_dir_all(&out_dir).expect("create out dir");

    let blk = block::build_block();
    println!("{}", blk.census);
    println!(
        "light field      : {} lamps, lit threshold {:.2}",
        blk.field.lamps.len(),
        light::LIT_THRESHOLD
    );

    if mode == "survey" {
        println!("--- anchor survey ({} candidates) ---", blk.anchors.len());
        print!("{}", block::anchors_csv(&blk.anchors));
        println!("--- light profiles (exposure along the drive routes) ---");
        for (name, x) in [("open route x=-2.0", -2.0f32), ("shadow route x=8.9", 8.9f32)] {
            let mut line = format!("{name}: ");
            let mut z = -44.0f32;
            while z <= 44.0 {
                let e = blk.field.exposure(Vec3::new(x, 1.15, z));
                line.push_str(&format!("[{z:+.0}:{e:.2}{}] ", if e >= light::LIT_THRESHOLD { "L" } else { "." }));
                z += 4.0;
            }
            println!("{line}");
        }
        return;
    }

    let reality = resolve_reality();
    let world = reality.world;

    if mode == "windowed" {
        let params = windowed::Params {
            out_dir: out_dir.clone(),
            present: present.clone(),
            width,
            height,
            proof: args.iter().any(|a| a == "--proof"),
            smoke_secs: arg_value(&args, "--smoke").and_then(|v| v.parse().ok()),
        };
        windowed::run(blk, &reality, params);
        return;
    }

    let mut rig = Rig::new(width, height, &present, &blk);

    match mode.as_str() {
        "still" => {
            // HERO: down the street from the south mouth — lamp
            // pools receding, the crane in fog at the far end, both
            // men in the street.
            let op = op_mesh(&PoseFrame::from_pose(c35::OperatorPose::StandReady), Vec3::new(-2.0, 0.0, 3.0), 168.0);
            let so = soldier::soldier_mesh(&soldier::pose_low_ready(), Vec3::new(-8.4, 0.0, -9.0), 0.0, 190.0);
            let both = merge_meshes(&op, &so);
            rig.still(&world, hero_pose(), Some(&both), &format!("{out_dir}/hero-street.png"));
            // ECHO of golden #11: low camera, the patrol posted under
            // the east lamp with the banner on the wall behind him,
            // the operator crouched in the dark corner, foreground.
            let op = op_mesh(&PoseFrame::from_pose(c35::OperatorPose::Crouch), Vec3::new(-7.4, 0.0, 21.0), 180.0);
            let so = soldier::soldier_mesh(&soldier::pose_low_ready(), Vec3::new(6.0, 0.0, 19.2), 0.0, 0.0);
            let both = merge_meshes(&op, &so);
            rig.still(&world, echo_pose(), Some(&both), &format!("{out_dir}/echo-patrol.png"));
            rig.ledger("stills");
        }
        "poses" => {
            for (name, frame) in soldier::pose_vocabulary() {
                let so = soldier::soldier_mesh(&frame, Vec3::new(-2.0, 0.0, 0.0), 0.0, 0.0);
                rig.still(
                    &world,
                    c35::scene::CameraPose {
                        eye: Vec3::new(1.3, 1.25, 4.6),
                        target: Vec3::new(-2.0, 1.0, 0.0),
                        fov_y_deg: 50.0,
                    },
                    Some(&so),
                    &format!("{out_dir}/pose-{name}.png"),
                );
            }
            // The HALT staged where it belongs: at the checkpoint,
            // in the lane on the barrier's near side, facing the
            // traffic he is stopping — the camera sits in that
            // traffic, with nothing between lens and raised hand
            // (the first staging put the barrier arm across his
            // chest and the verb read as hiding).
            let so = soldier::soldier_mesh(&soldier::pose_halt(), Vec3::new(6.6, 0.0, 27.2), 0.0, 0.0);
            rig.still(
                &world,
                c35::scene::CameraPose {
                    eye: Vec3::new(4.6, 1.5, 21.0),
                    target: Vec3::new(6.6, 1.2, 27.2),
                    fov_y_deg: 50.0,
                },
                Some(&so),
                &format!("{out_dir}/pose-halt-checkpoint.png"),
            );
            rig.ledger("poses");
        }
        "drive" => {
            run_drive(&mut rig, &world, &blk, &out_dir, "open");
            run_drive(&mut rig, &world, &blk, &out_dir, "shadow");
            rig.ledger("drives");
        }
        other => {
            eprintln!("unknown --mode '{other}' (still|poses|drive|survey|windowed)");
            std::process::exit(2);
        }
    }
    print_reality(&reality);
}

/// The patrol's loop (UNAWARE): the west sidewalk north, across the
/// north end, back down the street's east lane, across the south —
/// a rectangle that closes on its own start, so the loop restarts
/// seamlessly.
pub(crate) fn patrol_legs() -> Vec<ScriptLeg> {
    vec![
        ScriptLeg::to(-8.6, 40.0, Gait::Walk),
        ScriptLeg::to(2.5, 40.0, Gait::Walk),
        ScriptLeg::to(2.5, -40.0, Gait::Walk),
        ScriptLeg::to(-8.6, -40.0, Gait::Walk),
    ]
}

fn run_drive(rig: &mut Rig, world: &WorldState, blk: &block::Block, out_dir: &str, which: &str) {
    let (start, legs) = drive_route(which);
    let mut ctrl = Controller::new(start, legs);
    let mut anim = Animator::new();
    anim.set_ground_fn(Some(flat_ground));
    let mut p_ctrl = Controller::new(Vec2::new(-8.6, -40.0), patrol_legs());
    let mut p_anim = Animator::new();
    p_anim.set_ground_fn(Some(flat_ground));

    let view = rig.out_tex.create_view(&Default::default());
    // Warm both animators standing, place both men, warm the pipes.
    let a0 = *ctrl.agent();
    let warm = c35::locomotion::AnimInput {
        pos: Vec3::new(a0.pos.x, 0.0, a0.pos.y),
        yaw: a0.yaw,
        speed: 0.0,
        crouch: 0.0,
        aim: 0.0,
        slide: 0.0,
        carry: true,
    };
    let f0 = anim.update(0.0, &warm);
    let pa0 = *p_ctrl.agent();
    let pwarm = c35::locomotion::AnimInput { pos: Vec3::new(pa0.pos.x, 0.0, pa0.pos.y), yaw: pa0.yaw, ..warm };
    let pf0 = p_anim.update(0.0, &pwarm);
    let m0 = merge_meshes(
        &build_operator_frame(&f0, Vec3::new(a0.pos.x, 0.0, a0.pos.y), 0.0, a0.yaw.to_degrees()),
        &soldier::soldier_mesh(&pf0, Vec3::new(pa0.pos.x, 0.0, pa0.pos.y), 0.0, pa0.yaw.to_degrees()),
    );
    rig.renderer.update_operator(&m0);
    let mut anchor = Vec3::new(a0.pos.x, 0.0, a0.pos.y);
    for _ in 0..10 {
        rig.renderer.render(&view, &follow_pose(anchor, a0.yaw), world, 0.0);
        let _ = rig.renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });
    }
    let _ = rig.renderer.drain_gpu_samples();

    let mut csv = String::from(
        "frame,t,op_x,op_z,gait,speed,exposure,lit,lamp,soldier_x,soldier_z,soldier_yaw_deg,facing_dot,soldier_dist\n",
    );
    let mut frame_ms: Vec<f64> = Vec::new();
    let mut settle = 0usize;
    let mut frames_run = 0usize;
    let mut lit_frames = 0usize;
    let dt = 1.0f32 / 60.0;
    for i in 0..20000usize {
        let input = ctrl.update(dt);
        let pframe = anim.update(dt, &input);
        let a = *ctrl.agent();
        let p_input = p_ctrl.update(dt);
        let p_frame = p_anim.update(dt, &p_input);
        let pa = *p_ctrl.agent();
        if pa.done {
            p_ctrl = Controller::new(Vec2::new(-8.6, -40.0), patrol_legs());
        }
        let op_pos = Vec3::new(a.pos.x, 0.0, a.pos.y);
        let so_pos = Vec3::new(pa.pos.x, 0.0, pa.pos.y);
        let mesh = merge_meshes(
            &build_operator_frame(&pframe, op_pos, anim.ground_y(), a.yaw.to_degrees()),
            &soldier::soldier_mesh(&p_frame, so_pos, p_anim.ground_y(), pa.yaw.to_degrees()),
        );
        rig.renderer.update_operator(&mesh);
        let target_anchor = Vec3::new(a.pos.x, anim.ground_y(), a.pos.y);
        let k = 1.0 - (-dt / 0.14f32).exp();
        anchor = anchor + (target_anchor - anchor) * k;
        let pose = follow_pose(anchor, a.yaw);
        let t0 = Instant::now();
        rig.renderer.render(&view, &pose, world, i as f32 / 60.0);
        let _ = rig.renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });
        frame_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
        rig.renderer.observe_frame_ms(frame_ms.last().copied().unwrap_or(0.0));
        frames_run = i + 1;

        // THE LIGHT READ — the same field that baked the street,
        // sampled at the operator's chest, logged against the
        // patrol's facing. Not an AI: a measurement.
        let sample_h = 1.15 - 0.35 * input.crouch;
        let sample = Vec3::new(a.pos.x, sample_h, a.pos.y);
        let exposure = blk.field.exposure(sample);
        let lit = blk.field.lit(sample);
        if lit {
            lit_frames += 1;
        }
        let lamp = blk.field.nearest_lamp(sample).map(|(idx, _)| idx).unwrap_or(usize::MAX);
        let s_fwd = Vec3::new(pa.yaw.sin(), 0.0, pa.yaw.cos());
        let to_op = Vec3::new(a.pos.x - pa.pos.x, 0.0, a.pos.y - pa.pos.y);
        let dist = to_op.length();
        let facing_dot = if dist > 1e-4 { s_fwd.dot(to_op / dist) } else { 0.0 };
        csv.push_str(&format!(
            "{},{:.3},{:.2},{:.2},{},{:.2},{:.3},{},{},{:.2},{:.2},{:.1},{:.3},{:.2}\n",
            i,
            i as f32 / 60.0,
            a.pos.x,
            a.pos.y,
            a.gait.name(),
            a.speed,
            exposure,
            if lit { 1 } else { 0 },
            lamp,
            pa.pos.x,
            pa.pos.y,
            pa.yaw.to_degrees(),
            facing_dot,
            dist
        ));
        if i % 300 == 0 || (a.done && settle == 0) {
            save_png(&rig.renderer, &rig.out_tex, rig.width, rig.height, &format!("{out_dir}/drive-{which}-f{i}.png"));
        }
        if a.done && a.speed < 0.05 {
            settle += 1;
            if settle >= 45 {
                break;
            }
        }
    }
    save_png(&rig.renderer, &rig.out_tex, rig.width, rig.height, &format!("{out_dir}/drive-{which}-final.png"));
    std::fs::write(format!("{out_dir}/drive-{which}-lightread.csv"), &csv).expect("write light read csv");
    println!(
        "drive {which}: {frames_run} frames, {:.1} m travelled, lit {}/{} frames ({:.1}%), frame mean {:.2} ms p95 {:.2} ms",
        ctrl.agent().distance,
        lit_frames,
        frames_run,
        100.0 * lit_frames as f64 / frames_run.max(1) as f64,
        c35::mean(&frame_ms),
        c35::percentile(&frame_ms, 95.0)
    );
}
