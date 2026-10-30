//! Compare one-shot mesh construction with reusable skinning, CPU only.
use c35::glam::Vec3;
use c35::operator::{build_operator_frame, FigureTemplate, OperatorPose, PoseFrame};
use std::{hint::black_box, time::Instant};

fn main() {
    let template = FigureTemplate::operator();
    let mut mesh = template.mesh();
    let frames = 2000;
    let pose = PoseFrame::from_pose(OperatorPose::Aim);
    let start = Instant::now();
    for i in 0..frames {
        black_box(build_operator_frame(
            black_box(&pose),
            Vec3::ZERO,
            0.0,
            i as f32,
        ));
    }
    let old = start.elapsed();
    let ptr = mesh.vertices.as_ptr();
    let start = Instant::now();
    for i in 0..frames {
        template.skin_into(
            black_box(&mut mesh),
            black_box(&pose),
            Vec3::ZERO,
            0.0,
            i as f32,
        );
        black_box(&mesh);
    }
    let new = start.elapsed();
    assert_eq!(ptr, mesh.vertices.as_ptr());
    println!("frames={frames} triangles={} one_shot_us={:.2} cached_us={:.2} ratio={:.2}x output_storage_reused=true",
        mesh.triangle_count(), old.as_secs_f64() * 1e6 / frames as f64,
        new.as_secs_f64() * 1e6 / frames as f64, old.as_secs_f64() / new.as_secs_f64());
}
