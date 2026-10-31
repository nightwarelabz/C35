//! Dev probe: per-tile frustum visibility + LOD for a pose, printed from
//! the engine's own math (used to audit the M3 culling counts).

use c35::ecs::JobPool;
use c35::scene::{generate_tiles, GRAZING_POSE, HERO_POSE};
use c35::vista::Vista;
use c35::world::{world_state, Observer};
use glam::Mat4;

fn main() {
    let tiles = generate_tiles();
    let meta: Vec<_> = tiles
        .iter()
        .map(|t| {
            (
                t.center,
                t.radius,
                [
                    t.lod_triangles(c35::scene::Lod::Full),
                    t.lod_triangles(c35::scene::Lod::Half),
                    t.lod_triangles(c35::scene::Lod::Quarter),
                ],
            )
        })
        .collect();
    let pool = JobPool::new();
    let world = world_state(1768485600.0, &Observer::new(41.88, -87.63));
    for (name, pose) in [("HERO", HERO_POSE), ("GRAZING", GRAZING_POSE)] {
        let mut vista = Vista::new(&meta);
        let view = Mat4::look_at_rh(pose.eye, pose.target, glam::Vec3::Y);
        let proj = Mat4::perspective_rh(pose.fov_y_deg.to_radians(), 1280.0 / 720.0, 0.5, 4000.0);
        let stats = vista.update(&pool, &pose, &world, proj * view, Mat4::IDENTITY, false);
        println!("{name}: drawn={:?}", vista.draws());
        println!(
            "{name}: culled tiles: {:?}",
            (0..16)
                .filter(|i| !vista.draws().iter().any(|d| d.tile == *i))
                .collect::<Vec<_>>()
        );
        println!("{name}: core stats: {stats:?}");
        for (i, t) in tiles.iter().enumerate() {
            let to_c = t.center - pose.eye;
            let fwd = (pose.target - pose.eye).normalize();
            let ang = to_c.normalize().dot(fwd).acos().to_degrees();
            println!(
                "  tile {i:2}: center {:?} r {:.1} dist {:.0} ang {:.1} deg",
                t.center,
                t.radius,
                to_c.length(),
                ang
            );
        }
    }
}
