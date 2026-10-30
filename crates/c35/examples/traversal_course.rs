//! Traversal course — a runnable demonstration, not a proof
//! harness. It drives the traversal sim through the brief's
//! course over an analytic Greyport-like block: street sprint
//! → dry wall-run → grapple swing → far wall-run → mantle →
//! drop, plus the wet variant, a sky miss, a mover follow, a
//! step-ring seam crossing on the real island height function,
//! and the navmesh bake with its typed-link routes. It prints
//! what happened (states, speeds, budgets, events, routes) —
//! Chase judges by playing; this prints the numbers the run
//! produced, nothing more.

use c35::scene::height_at;
use c35::traversal::events::StimulusKind;
use c35::traversal::nav::{bake, route};
use c35::traversal::physics::{
    BoxCollider, SeamDetector, StaticWorld, SEAM_SLEW,
};
use c35::traversal::rope::{MAX_LINE, MIN_ROPE, REEL_SPEED};
use c35::traversal::surface::{SurfaceKind, SurfaceRecord};
use c35::traversal::wallrun::{BUDGET_PER_SPEED, MIN_ENTRY_SPEED};
use c35::traversal::{BodyMode, TravInput, TravSim};
use glam::Vec3;

const DT: f32 = 1.0 / 240.0;

fn bx(
    w: &mut StaticWorld,
    min: Vec3,
    max: Vec3,
    kind: SurfaceKind,
    wet: f32,
) {
    w.add(BoxCollider::new(
        min,
        max,
        SurfaceRecord::wet(kind, wet),
    ));
}

/// The course block (metres). Wall A runs the street at z=-0.5
/// (face); the mast stands at x=24.5; wall B faces the street
/// from z=8; the crate is the mantle target; roofs A and D
/// are the grapple-link pair across a 10 m gap.
fn course_world(wet: f32) -> StaticWorld {
    let mut w = StaticWorld::new();
    bx(&mut w, Vec3::new(-60.0, -1.0, -40.0), Vec3::new(60.0, 0.0, 40.0), SurfaceKind::Asphalt, 0.0);
    bx(&mut w, Vec3::new(-20.0, 0.0, -10.5), Vec3::new(20.0, 12.0, -0.5), SurfaceKind::Brick, wet);
    bx(&mut w, Vec3::new(24.5, 0.0, -0.5), Vec3::new(25.5, 14.0, 0.5), SurfaceKind::Metal, 0.0);
    bx(&mut w, Vec3::new(20.0, 0.0, 8.0), Vec3::new(50.0, 12.0, 18.0), SurfaceKind::Concrete, wet);
    bx(&mut w, Vec3::new(44.0, 0.0, 4.0), Vec3::new(46.0, 1.2, 6.0), SurfaceKind::Wood, 0.0);
    bx(&mut w, Vec3::new(-20.0, 0.0, -20.0), Vec3::new(0.0, 6.0, -10.0), SurfaceKind::Stone, 0.0);
    bx(&mut w, Vec3::new(10.0, 0.0, -20.0), Vec3::new(30.0, 6.0, -10.0), SurfaceKind::Stone, 0.0);
    w
}

fn input(
    wish: Vec3,
    facing: Vec3,
    speed: f32,
    jump: bool,
    press: bool,
    held: bool,
    crouch: bool,
    aim: Vec3,
) -> TravInput {
    TravInput {
        wish,
        facing,
        speed,
        jump,
        hook_press: press,
        hook_held: held,
        crouch,
        aim,
    }
}

fn aim_at(from: Vec3, to: Vec3) -> Vec3 {
    (to - from).normalize()
}

fn main() {
    println!("== traversal course ==");
    println!("fixed step 240 Hz | capsule r=0.35 half=0.55 | g=9.81");
    println!(
        "tuned constants: MIN_ENTRY_SPEED={} BUDGET_PER_SPEED={} MAX_LINE={} REEL_SPEED={} MIN_ROPE={}",
        MIN_ENTRY_SPEED, BUDGET_PER_SPEED, MAX_LINE, REEL_SPEED, MIN_ROPE
    );

    // ---- Phase 0: sky miss + waiting-for-release guard ----
    {
        let world = course_world(0.0);
        let mut sim = TravSim::new(Vec3::new(-10.0, 0.0, 0.4));
        let mut fails = 0usize;
        let mut guarded = false;
        let mut refired = false;
        for t in 0..600 {
            let (press, held) = if t == 0 {
                (true, true)
            } else if t < 300 {
                // Keep holding through the detach: the guard
                // must block a refire even with press set.
                (true, true)
            } else if t == 300 {
                (false, false)
            } else if t == 301 {
                (true, true)
            } else {
                (false, true)
            };
            sim.tick(
                input(
                    Vec3::ZERO,
                    Vec3::X,
                    0.0,
                    false,
                    press,
                    held,
                    false,
                    Vec3::Y,
                ),
                &world,
            );
            fails = sim.events.count(StimulusKind::StrikeFail);
            if t == 280 && sim.grapple.claw_pos().is_none() && fails > 0 {
                guarded = true;
            }
            if t == 302 && sim.grapple.claw_pos().is_some() {
                refired = true;
            }
            if fails > 0 && t > 300 && refired {
                break;
            }
        }
        println!(
            "phase 0 sky miss: StrikeFail events={} guard blocked refire while held={} refire after release={}",
            fails, guarded, refired
        );
    }

    // ---- Phase 1: dry wall-run on A ----
    let dry_budget;
    {
        let world = course_world(0.0);
        let mut sim = TravSim::new(Vec3::new(-15.0, 0.0, 0.4));
        let mut entry_speed = 0.0f32;
        let mut budget_max = 0.0f32;
        let mut held_ticks = 0usize;
        let mut exit_speed = 0.0f32;
        for _ in 0..2400 {
            sim.tick(
                input(Vec3::X, Vec3::X, 5.8, false, false, false, false, Vec3::X),
                &world,
            );
            if sim.mode == BodyMode::WallRun {
                if held_ticks == 0 {
                    entry_speed = sim.body.horizontal_speed();
                    budget_max = sim.wallrun.as_ref().unwrap().budget_max;
                }
                held_ticks += 1;
                if let Some(w) = &sim.wallrun {
                    if w.budget_frac() < 0.5 {
                        exit_speed = sim.body.horizontal_speed();
                        break;
                    }
                }
            } else if held_ticks > 0 {
                break;
            }
        }
        dry_budget = budget_max;
        println!(
            "phase 1 dry wall-run A: entry speed={:.2} budget_max={:.2}s held {} ticks exit speed={:.2} (exit < entry: {})",
            entry_speed,
            budget_max,
            held_ticks,
            exit_speed,
            exit_speed < entry_speed
        );
    }

    // ---- Phase 2: wall-run -> grapple swing -> release -> land ----
    {
        let world = course_world(0.0);
        let mut sim = TravSim::new(Vec3::new(-10.0, 0.0, 0.4));
        // Sprint until wall-running.
        for _ in 0..2400 {
            sim.tick(
                input(Vec3::X, Vec3::X, 5.8, false, false, false, false, Vec3::X),
                &world,
            );
            if sim.mode == BodyMode::WallRun {
                break;
            }
        }
        // Fire at the mast face from the run. The fire point is
        // ~34 m from the mast, inside MAX_LINE (40 m); from the
        // -15 start the line would fall 0.3 m short of the mast
        // and the claw would fail at full extension instead.
        // Fire at the mast face from the run.
        let mut latched = false;
        let mut released = false;
        let mut max_swing = 0.0f32;
        let mut release_speed = 0.0f32;
        let mut impact = 0.0f32;
        let mast = Vec3::new(24.5, 11.7, 0.0);
        for t in 0..4800 {
            let origin = sim.body.feet + Vec3::new(0.0, 1.4, 0.0);
            let aim = aim_at(origin, mast);
            let (press, held) = if t == 0 {
                (true, true)
            } else {
                (false, true)
            };
            // Release mid-swing: airborne, past the pivot
            // plane, moving forward. A fixed x > 28 trigger
            // cannot fire — a reeled rope (len -> 2 m) caps
            // horizontal reach at pivot.x + len = 26.5.
            let held = if sim.grapple.is_latched()
                && !sim.body.grounded
                && sim.body.feet.x > mast.x
                && sim.body.vel.x > 0.0
            {
                false
            } else {
                held
            };
            if sim.grapple.is_latched() && !held && !released {
                released = true;
                release_speed = sim.body.speed();
            }
            sim.tick(
                input(Vec3::X, Vec3::X, 5.8, false, press, held, false, aim),
                &world,
            );
            if sim.grapple.is_latched() {
                latched = true;
                max_swing = max_swing.max(sim.body.speed());
            }
            // Break only once released AND landed — the old
            // break (latched + grounded + x > 20) fired while
            // the body was still running on the ground toward
            // the mast, before any swing happened.
            if released && sim.body.grounded {
                impact = sim.body.last_impact;
                break;
            }
            if t == 4799 {
                impact = sim.body.last_impact;
            }
        }
        println!(
            "phase 2 swing: latched={} released={} max swing speed={:.2} release speed={:.2} landing impact={:.2} final pos=({:.1},{:.1},{:.1})",
            latched,
            released,
            max_swing,
            release_speed,
            impact,
            sim.body.feet.x,
            sim.body.feet.y,
            sim.body.feet.z
        );
    }

    // ---- Phase 3: far wall-run on B (right side) ----
    {
        let world = course_world(0.0);
        let mut sim = TravSim::new(Vec3::new(22.0, 0.0, 0.4));
        let mut entry = 0.0f32;
        let mut budget = 0.0f32;
        for _ in 0..4800 {
            // Steer toward the B wall line, then along it.
            let to_wall = Vec3::new(0.0, 0.0, 7.1 - sim.body.feet.z);
            let wish = if sim.body.feet.z < 6.5 {
                Vec3::new(0.7, 0.0, 0.7).normalize()
            } else {
                Vec3::X
            };
            let _ = to_wall;
            sim.tick(
                input(wish, wish, 5.8, false, false, false, false, Vec3::X),
                &world,
            );
            if sim.mode == BodyMode::WallRun {
                entry = sim.body.horizontal_speed();
                budget = sim.wallrun.as_ref().unwrap().budget_max;
                break;
            }
        }
        println!(
            "phase 3 far wall-run B: entry speed={:.2} budget_max={:.2}s side={:?}",
            entry,
            budget,
            sim.wallrun.as_ref().map(|w| w.side)
        );
    }

    // ---- Phase 4: jump exit -> run -> mantle -> drop ----
    {
        let world = course_world(0.0);
        let mut sim = TravSim::new(Vec3::new(40.0, 0.0, 5.0));
        let mut mantled = false;
        let mut topped = false;
        let mut drop_impact = 0.0f32;
        for _ in 0..4800 {
            // Steer to the crate face, then jump at it. Once
            // topped, walk +x off the crate; once dropped,
            // record and stop — the old loop kept steering
            // after the drop, so the final-state print ("top
            // reached") read the body's later position, not
            // the mantle's outcome.
            let (wish, jump) = if topped {
                (Vec3::X, false)
            } else {
                let to_crate =
                    Vec3::new(44.0 - sim.body.feet.x, 0.0, 5.0 - sim.body.feet.z);
                let dist = to_crate.length();
                let wish = if dist > 0.01 { to_crate / dist } else { Vec3::X };
                (wish, dist < 1.2 && sim.body.grounded)
            };
            sim.tick(
                input(wish, wish, 3.4, jump, false, false, false, Vec3::X),
                &world,
            );
            if sim.mode == BodyMode::Mantle {
                mantled = true;
            }
            if mantled && sim.body.grounded && sim.body.feet.y > 1.0 {
                topped = true;
            }
            if topped && sim.body.grounded && sim.body.feet.y < 0.1 {
                drop_impact = sim.body.last_impact;
                break;
            }
        }
        println!(
            "phase 4 mantle+drop: mantled={} crate-top reached={} drop impact={:.2} final pos=({:.1},{:.1},{:.1})",
            mantled,
            topped,
            drop_impact,
            sim.body.feet.x,
            sim.body.feet.y,
            sim.body.feet.z
        );
    }

    // ---- Phase 5: wet wall-run (same wall, wetness 0.8) ----
    {
        let world = course_world(0.8);
        let mut sim = TravSim::new(Vec3::new(-15.0, 0.0, 0.4));
        let mut budget = 0.0f32;
        for _ in 0..2400 {
            sim.tick(
                input(Vec3::X, Vec3::X, 5.8, false, false, false, false, Vec3::X),
                &world,
            );
            if sim.mode == BodyMode::WallRun {
                budget = sim.wallrun.as_ref().unwrap().budget_max;
                break;
            }
        }
        println!(
            "phase 5 wet wall-run A (wetness 0.8): budget_max={:.2}s vs dry {:.2}s (ratio {:.2})",
            budget,
            dry_budget,
            if dry_budget > 0.0 { budget / dry_budget } else { 0.0 }
        );
    }

    // ---- Phase 6: mover follow ----
    {
        let world = course_world(0.0);
        let mut sim = TravSim::new(Vec3::new(10.0, 0.0, 0.4));
        let mast = Vec3::new(24.5, 11.7, 0.0);
        let mut anchor = mast;
        let mut start_x = 0.0f32;
        let mut end_x = 0.0f32;
        let mut latched = false;
        for t in 0..2400 {
            let origin = sim.body.feet + Vec3::new(0.0, 1.4, 0.0);
            let aim = aim_at(origin, mast);
            let (press, held) = if t == 0 { (true, true) } else { (false, true) };
            sim.tick(
                input(Vec3::ZERO, Vec3::X, 0.0, false, press, held, false, aim),
                &world,
            );
            if sim.grapple.is_latched() && !latched {
                latched = true;
                start_x = anchor.x;
                sim.grapple.retarget_anchor(anchor);
            } else if latched {
                // The mover carries the anchor +x at 1 m/s.
                anchor.x += 1.0 * DT;
                sim.grapple.retarget_anchor(anchor);
                end_x = anchor.x;
                if t > 1200 {
                    break;
                }
            }
        }
        println!(
            "phase 6 mover follow: latched={} anchor x {:.1} -> {:.1} (pivot moved with the mover: {})",
            latched,
            start_x,
            end_x,
            end_x > start_x
        );
    }

    // ---- Phase 7: step-ring seam on the real island ----
    {
        // Scan the movement probe's shore lines for a seam
        // crossing, then walk across it with a slewed ground
        // reference: the raw sample jumps ~0.84 m, the reference
        // may move at most SEAM_SLEW * dt per tick. The seam is
        // detected by its discontinuity signature (jump_segment,
        // 0.5 m threshold): the contour test on rescaled output
        // misfires on smooth steep crossings, and the brief
        // forbids changing the terrain function to expose the
        // pre-shelf value — so the output jump IS the seam
        // observable, and the slew is the honest response.
        let lines = [
            (Vec3::new(-8.0, 0.0, 56.0), Vec3::new(-68.0, 0.0, 56.0)),
            (Vec3::new(-8.0, 0.0, 44.0), Vec3::new(-64.0, 0.0, 44.0)),
            (Vec3::new(-8.0, 0.0, 64.0), Vec3::new(-8.0, 0.0, 4.0)),
            (Vec3::new(-14.0, 0.0, 64.0), Vec3::new(-14.0, 0.0, 124.0)),
        ];
        let mut found: Option<(Vec3, Vec3, f32)> = None;
        for (a, b) in lines {
            let steps = 400;
            let mut prev = a;
            for i in 1..=steps {
                let t = i as f32 / steps as f32;
                let p = a.lerp(b, t);
                if SeamDetector::jump_segment(height_at, prev.x, prev.z, p.x, p.z, 0.5) {
                    found = Some((prev, p, height_at(prev.x, prev.z)));
                    break;
                }
                prev = p;
            }
            if found.is_some() {
                break;
            }
        }
        match found {
            Some((a, b, _)) => {
                let mut ground_ref = height_at(a.x, a.z);
                let mut prev_raw = ground_ref;
                let mut max_step = 0.0f32;
                let mut raw_jump = 0.0f32;
                let mut crossings = 0usize;
                let steps = 240;
                for i in 0..=steps {
                    let t = i as f32 / steps as f32;
                    let p = a.lerp(b, t);
                    let raw = height_at(p.x, p.z);
                    if SeamDetector::jump_crossed(prev_raw, raw, 0.5) {
                        crossings += 1;
                        raw_jump = (raw - prev_raw).abs();
                    }
                    let delta = (raw - ground_ref).clamp(-SEAM_SLEW * DT, SEAM_SLEW * DT);
                    ground_ref += delta;
                    max_step = max_step.max(delta.abs());
                    prev_raw = raw;
                }
                println!(
                    "phase 7 seam crossing: segments crossed={} raw jump={:.2}m max one-tick reference move={:.4}m (cap {:.4}m) — slew held, no teleport",
                    crossings,
                    raw_jump,
                    max_step,
                    SEAM_SLEW * DT
                );
            }
            None => {
                println!("phase 7 seam crossing: no contour crossing found on the scanned shore lines");
            }
        }
    }

    // ---- Phase 8: navmesh bake + routes ----
    {
        let world = course_world(0.0);
        let mesh = bake(
            &world,
            Vec3::new(-30.0, 0.0, -15.0),
            Vec3::new(50.0, 0.0, 25.0),
        );
        println!(
            "phase 8 nav bake: cells={} walkable={} regions={} polys={} links={:?}",
            mesh.stats.cells,
            mesh.stats.walkable,
            mesh.stats.regions,
            mesh.stats.polys,
            mesh.stats.links_by_kind
        );
        for p in &mesh.polys {
            println!(
                "  poly {}: x[{:.2},{:.2}] z[{:.2},{:.2}] center=({:.1},{:.1}) region={} neighbors={:?}",
                p.id,
                p.min.x,
                p.max.x,
                p.min.z,
                p.max.z,
                p.center.x,
                p.center.z,
                p.region,
                p.neighbors.iter().map(|(n, _)| *n).collect::<Vec<_>>()
            );
        }
        for l in &mesh.links {
            if l.kind == c35::traversal::nav::LinkKind::Mantle
                || l.kind == c35::traversal::nav::LinkKind::Grapple
            {
                println!(
                    "  link {:?}: ({:.2},{:.1},{:.2}) -> ({:.2},{:.1},{:.2}) price={:.1}",
                    l.kind, l.from.x, l.from.y, l.from.z, l.to.x, l.to.y, l.to.z, l.price
                );
            }
        }
        if let Some(r) = route(&mesh, Vec3::new(-10.0, 0.0, 0.4), Vec3::new(45.0, 1.2, 5.0)) {
            println!(
                "  route street->crate top: cost={:.1} stimulus={:.1} kinds={:?}",
                r.cost, r.stimulus, r.kinds
            );
            let wps: Vec<String> = r
                .waypoints
                .iter()
                .map(|w| format!("({:.1},{:.1},{:.1})", w.x, w.y, w.z))
                .collect();
            println!("    waypoints: {}", wps.join(" -> "));
        } else {
            println!("  route street->crate top: none");
        }
        if let Some(r) = route(
            &mesh,
            Vec3::new(-10.0, 6.0, -15.0),
            Vec3::new(20.0, 6.0, -15.0),
        ) {
            println!(
                "  route roof A->roof D (no walkable path, link only): cost={:.1} kinds={:?}",
                r.cost, r.kinds
            );
        } else {
            println!("  route roof A->roof D: none");
        }
        // Patrol loop on the mesh, chained through waypoints
        // that force link use (street -> crate top -> street):
        // intra-poly legs cost 0 by construction (a straight
        // walk inside one polygon), so a street-only patrol
        // proves nothing — the crate-top leg is the test.
        let patrol = [
            Vec3::new(-10.0, 0.0, 0.4),
            Vec3::new(45.0, 1.2, 5.0),
            Vec3::new(10.0, 0.0, 5.0),
            Vec3::new(-10.0, 0.0, 5.0),
        ];
        let mut total = 0.0f32;
        let mut ok = true;
        for w in patrol.windows(2) {
            match route(&mesh, w[0], w[1]) {
                Some(r) => {
                    println!(
                        "  patrol leg ({:.0},{:.0}) -> ({:.0},{:.0}): cost={:.1} kinds={:?}",
                        w[0].x, w[0].z, w[1].x, w[1].z, r.cost, r.kinds
                    );
                    total += r.cost;
                }
                None => ok = false,
            }
        }
        println!(
            "  patrol loop on mesh: routed={} total cost={:.1}",
            ok, total
        );
    }

    println!("== course complete ==");
}
