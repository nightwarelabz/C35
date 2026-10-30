//! Run explicitly on a Vulkan-capable machine (CI uses Mesa lavapipe):
//! cargo test -p c35 --test render -- --ignored --nocapture
use c35::scene::HERO_POSE;
use c35::world::{world_state, Observer};
use c35::{wgpu, CullMode, PresentMode, Renderer, SCENE_FORMAT};

fn target(renderer: &Renderer, w: u32, h: u32) -> wgpu::Texture {
    renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("regression output"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SCENE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn pixels(renderer: &Renderer, texture: &wgpu::Texture, w: u32, h: u32) -> Vec<u8> {
    let stride = (w * 4).div_ceil(256) * 256;
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("regression readback"),
        size: u64::from(stride) * u64::from(h),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    renderer
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range();
    let mut out = Vec::new();
    for row in 0..h as usize {
        out.extend_from_slice(
            &mapped[row * stride as usize..row * stride as usize + w as usize * 4],
        );
    }
    drop(mapped);
    buffer.unmap();
    out
}

#[test]
#[ignore = "requires a Vulkan adapter; exercised by the gpu CI job"]
fn render_paths_culling_parity_and_storage_reuse() {
    let instance = c35::create_instance_with(wgpu::Backends::VULKAN);
    let world = world_state(
        1_783_003_200.0,
        &Observer {
            latitude_deg: Some(41.88),
            longitude_deg: Some(-87.63),
            utc_offset_minutes: None,
        },
    );
    let mut cpu =
        c35::pollster::block_on(Renderer::try_new(&instance, SCENE_FORMAT, 128, 72, 0.7)).unwrap();
    cpu.pin_governor(Some(0));
    cpu.set_cull_mode(CullMode::Cpu);
    let output = target(&cpu, 128, 72);
    let view = output.create_view(&Default::default());
    cpu.render(&view, &HERO_POSE, &world, 0.0);
    let reference = pixels(&cpu, &output, 128, 72);
    assert!(
        reference
            .chunks_exact(4)
            .any(|p| p[0] != p[1] || p[1] != p[2]),
        "frame must contain a colored scene"
    );
    assert!(reference.chunks_exact(4).all(|p| p[3] == 255));

    let mut gpu =
        c35::pollster::block_on(Renderer::try_new(&instance, SCENE_FORMAT, 128, 72, 0.7)).unwrap();
    gpu.pin_governor(Some(0));
    gpu.set_cull_mode(CullMode::Gpu);
    assert_eq!(gpu.cull_mode(), CullMode::Gpu);
    let gpu_output = target(&gpu, 128, 72);
    gpu.render(
        &gpu_output.create_view(&Default::default()),
        &HERO_POSE,
        &world,
        0.0,
    );
    assert_eq!(
        pixels(&gpu, &gpu_output, 128, 72),
        reference,
        "CPU and compute culling must draw identical pixels"
    );

    let template = c35::operator::FigureTemplate::operator();
    let mut mesh = template.mesh();
    cpu.update_operator(&mesh);
    let first = cpu.mesh_upload_stats();
    mesh.vertices[0].position[0] += 0.1;
    cpu.update_operator(&mesh);
    let second = cpu.mesh_upload_stats();
    assert_eq!(first.buffer_allocations, 2);
    assert_eq!(second.buffer_allocations, first.buffer_allocations);
    assert_eq!(second.index_bytes, first.index_bytes);
    assert!(second.vertex_bytes > first.vertex_bytes);
    mesh.indices.truncate(mesh.indices.len() - 3);
    cpu.update_operator(&mesh);
    assert_eq!(
        cpu.mesh_upload_stats().buffer_allocations,
        2,
        "shrinking a mesh must reuse capacity"
    );

    // Two-frame history, camera cut, presentation switch, ladder reallocation,
    // zero-sized resize, and removing a mesh all run with wgpu validation on.
    cpu.render(&view, &HERO_POSE, &world, 0.0);
    cpu.render(&view, &HERO_POSE, &world, 0.0);
    cpu.reset_history();
    let bytes = cpu.tracked_vram_bytes();
    cpu.resize(128, 72);
    assert_eq!(cpu.tracked_vram_bytes(), bytes);
    cpu.set_present_mode(PresentMode::Ps15, 2.0);
    cpu.render(&view, &HERO_POSE, &world, 0.0);
    cpu.pin_governor(Some(4));
    cpu.render(&view, &HERO_POSE, &world, 0.0);
    cpu.resize(0, 0);
    assert_eq!(cpu.internal_size(), (1, 1));
    cpu.render(&view, &HERO_POSE, &world, 0.0);
    cpu.clear_operator();
    assert_eq!(cpu.operator_triangles(), 0);
    cpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    println!(
        "Vulkan render validation passed; CPU/GPU frame byte parity; dynamic mesh allocations=2"
    );
}
