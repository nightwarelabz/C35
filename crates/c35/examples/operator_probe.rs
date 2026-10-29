//! Operator probe (M6): the character pipeline's numbers, printed
//! plainly — the generated atlas's size and checksum, the proof
//! spot's terrain height, and per pose the triangle count, vertex /
//! index counts, and the full part inventory mapped to bones. The
//! M6 report quotes this output; nothing here renders.

use c35::operator::{
    build_operator, generate_atlas, OperatorPose, ATLAS_SIZE, OPERATOR_SPOT, OPERATOR_YAW_DEG,
};
use c35::scene;

fn atlas_checksum(px: &[u8]) -> u64 {
    // FNV-1a over the whole atlas: a byte-exact determinism witness
    // for the generated surfaces.
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in px {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn main() {
    let atlas = generate_atlas();
    println!(
        "atlas: {}x{} RGBA = {} bytes, FNV-1a checksum {:#018x}",
        ATLAS_SIZE,
        ATLAS_SIZE,
        atlas.len(),
        atlas_checksum(&atlas),
    );
    let ground = scene::height_at(OPERATOR_SPOT.0, OPERATOR_SPOT.1);
    println!(
        "proof spot: ({:.1}, {:.1}), terrain height {:.2}, yaw {:.0} deg",
        OPERATOR_SPOT.0, OPERATOR_SPOT.1, ground, OPERATOR_YAW_DEG
    );
    for pose in [
        OperatorPose::StandReady,
        OperatorPose::Crouch,
        OperatorPose::Aim,
    ] {
        let mesh = build_operator(
            pose,
            glam::Vec3::new(OPERATOR_SPOT.0, 0.0, OPERATOR_SPOT.1),
            ground,
            OPERATOR_YAW_DEG,
        );
        println!(
            "\npose {}: {} triangles, {} vertices, {} indices, {} parts",
            pose.name(),
            mesh.triangle_count(),
            mesh.vertices.len(),
            mesh.indices.len(),
            mesh.parts.len(),
        );
        // World-space bounds: a sanity witness for the skinning.
        let mut lo = glam::Vec3::splat(f32::INFINITY);
        let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
        for v in &mesh.vertices {
            let p = glam::Vec3::from(v.position);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        println!(
            "  bounds: x [{:.2}, {:.2}]  y [{:.2}, {:.2}]  z [{:.2}, {:.2}]  (height {:.2})",
            lo.x, hi.x, lo.y, hi.y, lo.z, hi.z, hi.y - lo.y,
        );
        for (i, p) in mesh.parts.iter().enumerate() {
            println!(
                "  {:2}. {:28} bone {:?} — {} tris",
                i + 1,
                p.name,
                p.bone,
                p.triangles
            );
        }
    }
}
