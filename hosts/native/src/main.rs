//! Comigen native host — headless proof runner.
//!
//! Renders the vista offscreen (no window), prints the cost ledger to
//! stdout, and saves a PNG of the hero pose.
//!
//! The vista is lit by the reality layer: the host reads the **system
//! clock** and the observer location from the environment, and the engine
//! computes the sun, sky and season from them. There are no time or
//! season controls. The only pinning seam is this measurement rig:
//!
//!   COMIGEN_PIN_ISO   e.g. 2026-01-15T08:00:00-06:00 — pin the instant
//!                     (ISO-8601 with offset; read by this host only,
//!                     never by the engine crate)
//!   COMIGEN_LAT       observer latitude in degrees (hemisphere = sign)
//!   COMIGEN_LON       observer longitude in degrees (solar-time source)
//!   COMIGEN_UTC_OFFSET_MIN  UTC offset in minutes, if no longitude
//!
//! Usage: comigen-native [--width 1280] [--height 720] [--frames 120]
//!        [--scale 1.0] [--out hero.png] [--drift]
//!        [--pose hero|grazing]
//!        [--pin-level N | --governor] [--target-ms 16.7]
//!        [--cull cpu|gpu] [--save-at 60,120] [--start-level N]
//!        [--drift-rate R]  (rig-only: multiply the drift clock by R;
//!        the M5 fast-pan stress. Shipped runs never set it.)
//!
//! M3 rig notes: the quality governor is **pinned at level 0 by
//! default** in this host, so a proof run's output is a deterministic
//! function of its pin (the ladder cannot wander mid-measurement).
//! `--pin-level N` pins another level (the A/B reconstruction proof is
//! level 0 vs level 2 at the same pose). `--governor` unpins it: the
//! governor then governs live against `--target-ms`, printing every
//! adjustment as it happens; the saved PNG is the final frame at
//! whatever level the ladder reached.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use c35::world::{iso_utc, world_state, Observer, WorldState};

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == name)
        .map(|w| w[1].clone())
}

fn env_f32(name: &str) -> Option<f32> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

/// Parse the rig's pin: ISO-8601 local time with an explicit offset
/// (`2026-01-15T08:00:00-06:00`, `...Z`, seconds optional). Returns
/// (unix seconds, offset minutes from the string).
fn parse_pin_iso(s: &str) -> Option<(f64, f32)> {
    let (dt, offset_min) = if let Some(idx) = s.rfind('Z') {
        (&s[..idx], 0.0f32)
    } else if let Some(idx) = s.rfind(['+', '-']) {
        if idx < 10 {
            return None; // a date separator, not an offset
        }
        let tail = &s[idx..];
        let sign: f32 = if tail.starts_with('-') { -1.0 } else { 1.0 };
        let mut parts = tail[1..].split(':');
        let oh: f32 = parts.next()?.parse().ok()?;
        let om: f32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0.0);
        (&s[..idx], sign * (oh * 60.0 + om))
    } else {
        (s, 0.0f32)
    };
    let mut halves = dt.split('T');
    let date = halves.next()?;
    let time = halves.next().unwrap_or("00:00:00");
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let mo: u32 = dp.next()?.parse().ok()?;
    let d: u32 = dp.next()?.parse().ok()?;
    let mut tp = time.split(':');
    let hh: f64 = tp.next()?.parse().ok()?;
    let mm: f64 = tp.next().and_then(|p| p.parse().ok()).unwrap_or(0.0);
    let ss: f64 = tp.next().and_then(|p| p.parse().ok()).unwrap_or(0.0);
    let days = c35::world::days_from_civil(y, mo, d) as f64;
    let local = days * 86400.0 + hh * 3600.0 + mm * 60.0 + ss;
    Some((local - offset_min as f64 * 60.0, offset_min))
}

struct Reality {
    world: WorldState,
    clock_line: String,
    observer_line: String,
    solar_line: String,
}

/// Resolve clock + observer from the environment, exactly once, and say
/// precisely what was used — the ledger prints these lines verbatim.
fn resolve_reality() -> Reality {
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
    let observer = Observer {
        latitude_deg: lat,
        longitude_deg: lon,
        utc_offset_minutes: offset_env,
    };
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
    Reality {
        world,
        clock_line,
        observer_line,
        solar_line,
    }
}

fn system_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn print_reality(r: &Reality) {
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
    println!(
        "land & water     : snow line {:+.1}, water ice {:.2}, wave scale {:.2}",
        w.snow_line, w.water_ice, w.water_wave_scale
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let width: u32 = arg_value(&args, "--width")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1280);
    let height: u32 = arg_value(&args, "--height")
        .and_then(|v| v.parse().ok())
        .unwrap_or(720);
    let frames: usize = arg_value(&args, "--frames")
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let scale: f32 = arg_value(&args, "--scale")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);
    let out = arg_value(&args, "--out").unwrap_or_else(|| "hero.png".to_string());
    let drift = args.iter().any(|a| a == "--drift");
    let drift_rate: f32 = arg_value(&args, "--drift-rate")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);
    let pose_name = arg_value(&args, "--pose").unwrap_or_else(|| "hero".to_string());
    let pin_level: Option<usize> = arg_value(&args, "--pin-level").and_then(|v| v.parse().ok());
    let governor_live = args.iter().any(|a| a == "--governor");
    let target_ms: Option<f64> = arg_value(&args, "--target-ms").and_then(|v| v.parse().ok());
    // M4 rig seams: culling path override, governor start level (for
    // the step-up proof), and per-frame PNG saves (for the motion A/B).
    let cull_arg = arg_value(&args, "--cull");
    let start_level: Option<usize> =
        arg_value(&args, "--start-level").and_then(|v| v.parse().ok());
    let save_at: Vec<usize> = arg_value(&args, "--save-at")
        .map(|v| {
            v.split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default();

    let reality = resolve_reality();
    let world = reality.world;

    let instance = c35::create_instance();
    let mut renderer = c35::pollster::block_on(c35::Renderer::new(
        &instance,
        c35::wgpu::TextureFormat::Rgba8UnormSrgb,
        width,
        height,
        scale,
    ));

    // Culling path (before any frame): the renderer's default is the
    // GPU compute path where the backend carries it; the rig can force
    // either path for the parity proofs.
    match cull_arg.as_deref() {
        Some("cpu") => renderer.set_cull_mode(c35::CullMode::Cpu),
        Some("gpu") => renderer.set_cull_mode(c35::CullMode::Gpu),
        _ => {}
    }

    // Governor setup (before any frame): target, then pin or live.
    if let Some(t) = target_ms {
        renderer.set_governor_target(t);
    }
    let gov_mode = if governor_live {
        if let Some(lvl) = start_level {
            renderer.set_governor_start_level(lvl);
            format!("live (unpinned), starting at level {lvl}")
        } else {
            "live (unpinned)".to_string()
        }
    } else {
        let lvl = pin_level.unwrap_or(0);
        renderer.pin_governor(Some(lvl));
        format!("pinned at level {lvl}")
    };

    // Proof pose: the hero pose (drift orbits it) or the fixed grazing
    // pose where culling bites (drift does not apply there).
    let fixed_pose = if pose_name == "grazing" {
        c35::scene::GRAZING_POSE
    } else {
        c35::scene::HERO_POSE
    };
    let pose_at = |t: f32| -> c35::scene::CameraPose {
        if pose_name == "grazing" {
            fixed_pose
        } else {
            c35::scene::camera_at(t)
        }
    };

    let out_tex = renderer.device.create_texture(&c35::wgpu::TextureDescriptor {
        label: Some("host output"),
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

    // Warmup: shader compilation and pipeline setup land here, not in the
    // measured run.
    for _ in 0..10 {
        renderer.render(&out_view, &pose_at(0.0), &world, 0.0);
        let _ = renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });
    }
    let _ = renderer.drain_gpu_samples();

    let mut frame_ms: Vec<f64> = Vec::with_capacity(frames);
    let mut update_ms: Vec<f64> = Vec::with_capacity(frames);
    let mut printed_adj = 0usize;
    for i in 0..frames {
        let t = if drift { i as f32 / 60.0 * drift_rate } else { 0.0 };
        let pose = pose_at(t);
        let start = Instant::now();
        renderer.render(&out_view, &pose, &world, t);
        let _ = renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        frame_ms.push(ms);
        update_ms.push(renderer.core_stats().update_ms);
        if save_at.contains(&i) {
            // Paired-frame evidence (the motion A/B): the same frame
            // index in two runs is the same drift instant, bit for bit.
            let stem = out.strip_suffix(".png").unwrap_or(&out);
            save_png(&renderer, &out_tex, width, height, &format!("{stem}-f{i}.png"));
        }
        // The governor eats the same number the ledger reports; in live
        // mode its adjustments print as they happen.
        renderer.observe_frame_ms(ms);
        let adj = renderer.governor().adjustments();
        while printed_adj < adj.len() {
            let a = adj[printed_adj];
            println!(
                "[governor] frame {} : level {} -> {} (frame EMA {:.2} ms vs target {:.1} ms)",
                a.frame, a.from_level, a.to_level, a.ema_ms,
                renderer.governor().target_ms()
            );
            printed_adj += 1;
        }
    }
    let (shadow_ms, gpu_ms) = renderer.drain_gpu_samples();
    let cull_ms = renderer.drain_cull_samples();

    save_png(&renderer, &out_tex, width, height, &out);

    // ---- the cost ledger ----
    let iw = ((width as f32 * renderer.render_scale()) as u32).max(1);
    let ih = ((height as f32 * renderer.render_scale()) as u32).max(1);
    println!("=== COMIGEN COST LEDGER (native, headless offscreen) ===");
    println!("backend          : {}", renderer.backend_name());
    println!("output           : {width}x{height}");
    println!(
        "render scale     : {:.2} (internal {}x{})",
        renderer.render_scale(),
        iw,
        ih
    );
    println!("frames measured  : {frames} (after 10 warmup)");
    println!(
        "frame time       : mean {:.2} ms, p95 {:.2} ms  [frame-loop wall time: encode+submit+wait]",
        c35::mean(&frame_ms),
        c35::percentile(&frame_ms, 95.0)
    );
    if renderer.shadow_active() {
        if shadow_ms.is_empty() {
            println!("gpu shadow pass  : n/a (timestamp queries not exposed by this backend)");
        } else {
            println!(
                "gpu shadow pass  : mean {:.2} ms over {} samples [wgpu timestamp queries]",
                c35::mean(&shadow_ms),
                shadow_ms.len()
            );
        }
        println!(
            "shadow pass      : {} draw calls, {} triangles (single cascade, {}x{})",
            renderer.shadow_draw_calls_per_frame(),
            renderer.shadow_triangles_per_frame(),
            renderer.shadow_map_size(),
            renderer.shadow_map_size()
        );
    } else if world.sun_elevation_deg > 0.0 {
        println!("shadow pass      : shed by the governor (sun is up, shadow map off at this level)");
    } else {
        println!("shadow pass      : skipped — sun below horizon (shadows are sun-only)");
    }
    if gpu_ms.is_empty() {
        println!("gpu scene pass   : n/a (timestamp queries not exposed by this backend)");
    } else {
        println!(
            "gpu scene pass   : mean {:.2} ms over {} samples [wgpu timestamp queries]",
            c35::mean(&gpu_ms),
            gpu_ms.len()
        );
    }
    println!("anti-aliasing    : {}", renderer.aa_description());
    println!("draw calls/frame : {}", renderer.draw_calls_per_frame());
    println!("triangles/frame  : {}", renderer.triangles_per_frame());
    println!(
        "tracked vram     : {:.2} MiB (engine-tracked allocations, estimate)",
        renderer.tracked_vram_bytes() as f64 / (1024.0 * 1024.0)
    );
    println!("hero pose png    : {out}");

    // ---- the M3 core: ECS + visibility accounting ----
    let core = renderer.core_stats();
    let cull = renderer.cull_stats();
    println!("--- engine core ---");
    println!(
        "entities         : {} (camera, world state, {} tiles, water, sky)",
        core.entities, cull.tiles_total
    );
    println!(
        "systems          : {} [{}]",
        core.systems,
        renderer.system_names().join(", ")
    );
    println!(
        "job pool         : {} workers, {} jobs executed",
        core.pool_size, core.jobs_executed
    );
    println!(
        "scene update     : mean {:.3} ms CPU (bind + frustum/LOD over {} tiles)",
        c35::mean(&update_ms),
        cull.tiles_total
    );
    println!("culling          : {}", renderer.culling_description());
    if renderer.cull_mode() == c35::CullMode::Gpu {
        if cull_ms.is_empty() {
            println!("gpu cull pass    : n/a (timestamp queries not exposed by this backend)");
        } else {
            println!(
                "gpu cull pass    : mean {:.3} ms over {} samples [wgpu timestamp queries]",
                c35::mean(&cull_ms),
                cull_ms.len()
            );
        }
    }
    println!(
        "tiles (camera)   : {} drawn, {} culled of {}",
        cull.tiles_drawn, cull.tiles_culled, cull.tiles_total
    );
    println!(
        "tiles (shadow)   : {} drawn, {} culled of {}",
        cull.shadow_drawn, cull.shadow_culled, cull.tiles_total
    );
    println!(
        "triangles        : {} at full density -> {} submitted (camera pass)",
        cull.tris_before, cull.tris_after
    );

    // ---- the governor's own account ----
    let gov = renderer.governor();
    let qs = gov.state();
    let shadow_desc = if qs.shadow_size > 0 {
        format!("{}x{}", qs.shadow_size, qs.shadow_size)
    } else {
        "off".to_string()
    };
    println!("--- governor ---");
    println!("mode             : {gov_mode}");
    println!("target           : {:.1} ms/frame", gov.target_ms());
    println!(
        "final level      : {} (scale {:.2}, shadow {}, detail x{:.2}, water {} octaves)",
        gov.level(),
        qs.render_scale,
        shadow_desc,
        qs.detail_fade_mul,
        qs.water_octaves
    );
    println!("frame EMA        : {:.2} ms", gov.ema_ms());
    println!("adjustments      : {}", gov.adjustments().len());
    for a in gov.adjustments() {
        println!(
            "  frame {} : level {} -> {} (EMA {:.2} ms)",
            a.frame, a.from_level, a.to_level, a.ema_ms
        );
    }
    if gov.is_pinned() {
        println!(
            "pinned overruns  : {} frames with EMA over target (reported, never acted on)",
            gov.pinned_overruns()
        );
    }
    print_reality(&reality);
}

fn save_png(
    renderer: &c35::Renderer,
    tex: &c35::wgpu::Texture,
    width: u32,
    height: u32,
    path: &str,
) {
    let bpr = (width * 4).div_ceil(256) * 256;
    let buf = renderer.device.create_buffer(&c35::wgpu::BufferDescriptor {
        label: Some("png readback"),
        size: (bpr * height) as u64,
        usage: c35::wgpu::BufferUsages::MAP_READ | c35::wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = renderer
        .device
        .create_command_encoder(&c35::wgpu::CommandEncoderDescriptor {
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
        c35::wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
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
}
