//! Comigen native host — headless proof runner.
//!
//! Renders the vista offscreen (no window), prints the cost ledger to
//! stdout, and saves a PNG of the hero pose.
//!
//! Usage: comigen-native [--width 1280] [--height 720] [--frames 120]
//!        [--scale 1.0] [--out hero.png] [--drift]

use std::time::Instant;

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == name)
        .map(|w| w[1].clone())
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

    let instance = c35::create_instance();
    let mut renderer = c35::pollster::block_on(c35::Renderer::new(
        &instance,
        c35::wgpu::TextureFormat::Rgba8UnormSrgb,
        width,
        height,
        scale,
    ));

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
        renderer.render(&out_view, &c35::scene::camera_at(0.0), 0.0);
        let _ = renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });
    }
    let _ = renderer.drain_gpu_samples();

    let mut frame_ms: Vec<f64> = Vec::with_capacity(frames);
    for i in 0..frames {
        let t = if drift { i as f32 / 60.0 } else { 0.0 };
        let pose = c35::scene::camera_at(t);
        let start = Instant::now();
        renderer.render(&out_view, &pose, t);
        let _ = renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });
        frame_ms.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let gpu_ms = renderer.drain_gpu_samples();

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
    if gpu_ms.is_empty() {
        println!("gpu scene pass   : n/a (timestamp queries not exposed by this backend)");
    } else {
        println!(
            "gpu scene pass   : mean {:.2} ms over {} samples [wgpu timestamp queries]",
            c35::mean(&gpu_ms),
            gpu_ms.len()
        );
    }
    println!("draw calls/frame : {}", renderer.draw_calls_per_frame());
    println!("triangles/frame  : {}", renderer.triangles_per_frame());
    println!(
        "tracked vram     : {:.2} MiB (engine-tracked allocations, estimate)",
        renderer.tracked_vram_bytes() as f64 / (1024.0 * 1024.0)
    );
    println!("hero pose png    : {out}");
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
