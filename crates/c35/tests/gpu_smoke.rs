//! Headless GPU smoke.
//!
//! `cargo test` skips this. The ignore line is the contract: without a
//! wgpu adapter the run must not look like a pass. The `gpu-smoke` CI
//! job installs Mesa lavapipe and runs it with `--ignored`. If that job
//! has no adapter, the test panics instead of returning.

use c35::scene::HERO_POSE;
use c35::world::{world_state, Observer};

#[test]
#[ignore = "needs a wgpu adapter (CI job gpu-smoke installs Vulkan lavapipe and passes --ignored). A missing adapter panics; it must not pass."]
fn headless_frame_is_not_blank() {
    let instance = c35::create_instance();
    let adapter =
        c35::pollster::block_on(instance.request_adapter(&c35::wgpu::RequestAdapterOptions {
            power_preference: c35::wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }));
    let adapter = match adapter {
        Ok(adapter) => adapter,
        Err(err) => panic!(
            "no wgpu adapter ({err}). Refusing to pass. Install mesa-vulkan-drivers (lavapipe) or a GPU, and leave this test ignored on machines without one."
        ),
    };
    let info = adapter.get_info();
    eprintln!(
        "gpu-smoke adapter: {:?} / {} ({:?})",
        info.backend, info.name, info.device_type
    );

    // 64² keeps the color target small. Init still allocates the 4096
    // shadow map; level 6 turns the shadow pass off so lavapipe does not
    // rasterize it. Daytime Chicago so the frame is not a night clear.
    let width = 64u32;
    let height = 64u32;
    let mut renderer = c35::pollster::block_on(c35::Renderer::new(
        &instance,
        c35::wgpu::TextureFormat::Rgba8UnormSrgb,
        width,
        height,
        0.55,
    ));
    renderer.pin_governor(Some(6));
    renderer.set_cull_mode(c35::CullMode::Cpu);

    let world = world_state(1_784_134_800.0, &Observer::new(41.88, -87.63));
    let tex = renderer
        .device
        .create_texture(&c35::wgpu::TextureDescriptor {
            label: Some("gpu-smoke output"),
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
    let view = tex.create_view(&c35::wgpu::TextureViewDescriptor::default());
    renderer.render(&view, &HERO_POSE, &world, 0.0);
    assert!(
        renderer.draw_calls_per_frame() > 0,
        "frame recorded no draw calls on {}",
        renderer.backend_name()
    );
    assert!(
        renderer.triangles_per_frame() > 0,
        "frame submitted no triangles on {}",
        renderer.backend_name()
    );

    let bpr = (width * 4).div_ceil(256) * 256;
    let buf = renderer.device.create_buffer(&c35::wgpu::BufferDescriptor {
        label: Some("gpu-smoke readback"),
        size: (bpr * height) as u64,
        usage: c35::wgpu::BufferUsages::MAP_READ | c35::wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder =
        renderer
            .device
            .create_command_encoder(&c35::wgpu::CommandEncoderDescriptor {
                label: Some("gpu-smoke copy"),
            });
    encoder.copy_texture_to_buffer(
        c35::wgpu::TexelCopyTextureInfo {
            texture: &tex,
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
    let _ = renderer.device.poll(c35::wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    let range = buf.slice(..).get_mapped_range();
    let mut nonzero = 0u32;
    for row in 0..height as usize {
        let start = row * bpr as usize;
        let row_px = &range[start..start + (width * 4) as usize];
        for px in row_px.chunks_exact(4) {
            if px[0] != 0 || px[1] != 0 || px[2] != 0 {
                nonzero += 1;
            }
        }
    }
    drop(range);
    buf.unmap();
    assert!(
        nonzero > 0,
        "readback was entirely black on {}; frame did not render",
        renderer.backend_name()
    );
    eprintln!(
        "gpu-smoke ok: {} draws, {} tris, {nonzero} nonzero pixels, {}",
        renderer.draw_calls_per_frame(),
        renderer.triangles_per_frame(),
        renderer.backend_name()
    );
}
