//! Dev probe: the M5 horizon-occlusion bake, in numbers. Prints the
//! AO distribution over the whole island and over the steep band
//! (where the relief lives — slope factor as computed in
//! `scene::height_at`'s mask), so a bake change can be judged by its
//! statistics before anyone renders a frame.
//!
//! Run: cargo run --release -p c35 --example ao_probe

use c35::scene::{generate_tiles, HeightGrid, TERRAIN_GRID, TERRAIN_SIZE};

fn main() {
    let grid = HeightGrid::new();
    let tiles = generate_tiles();
    let half = TERRAIN_SIZE * 0.5;
    let step = TERRAIN_SIZE / (TERRAIN_GRID - 1) as f32;

    let mut all: Vec<f32> = Vec::new();
    let mut steep: Vec<f32> = Vec::new();
    let mut ao_by_tile_min = f32::INFINITY;
    for tile in &tiles {
        for v in &tile.vertices {
            all.push(v.ao);
            // Steep band: recompute the slope factor the relief mask
            // uses (normal.y of the geometric normal, 1 = flat).
            let up = v.normal[1];
            if up < 0.78 && v.position[1] > 1.0 {
                steep.push(v.ao);
            }
            ao_by_tile_min = ao_by_tile_min.min(v.ao);
        }
    }
    let _ = grid;
    let _ = (half, step);

    fn stats(name: &str, v: &[f32]) {
        if v.is_empty() {
            println!("{name}: (empty)");
            return;
        }
        let mut s = v.to_vec();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = s.iter().sum::<f32>() / s.len() as f32;
        let p = |q: f32| s[((s.len() - 1) as f32 * q) as usize];
        println!(
            "{name}: n={} min={:.3} p10={:.3} p50={:.3} mean={:.3} p90={:.3} max={:.3}",
            s.len(),
            s[0],
            p(0.10),
            p(0.50),
            mean,
            p(0.90),
            s[s.len() - 1]
        );
        let below = |t: f32| s.iter().filter(|&&a| a < t).count();
        println!(
            "    fraction below 0.9: {:.3} · below 0.75: {:.3} · below 0.6: {:.3}",
            below(0.9) as f32 / s.len() as f32,
            below(0.75) as f32 / s.len() as f32,
            below(0.6) as f32 / s.len() as f32
        );
    }
    stats("all vertices ", &all);
    stats("steep band   ", &steep);
    println!("global min AO: {ao_by_tile_min:.3}");
}
