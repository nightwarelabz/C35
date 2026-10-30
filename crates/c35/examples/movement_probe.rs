//! Movement probe (M7): the movement proof's numbers, printed
//! plainly — the shore survey the proof script walks, the IK/FK
//! self-check, the foot-skate measurements (sampled sole velocities
//! at contact, plant-to-plant stride × cadence against ground
//! speed), the movement model's measured dynamics (time-to-speed,
//! stop distances, turn rates, the slide), and the joint-angle
//! census of the cycles. Nothing here renders; the headless host
//! drives the same controller and animator for the frames.

use std::time::Instant;

use c35::locomotion::{
    gait_params, proof_script, AnimInput, Animator, Controller, Gait, ScriptLeg,
    CROUCH_SPEED, RUN_SPEED, SPRINT_SPEED, WALK_SPEED,
};
use c35::operator::{build_operator_frame, foot_kinematics, root_matrix, PoseFrame, BONE_COUNT};
use c35::scene::height_at;
use glam::{Vec2, Vec3};

const DT: f32 = 1.0 / 240.0;

fn stats(samples: &[f32]) -> (f32, f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut s = samples.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = s.iter().sum::<f32>() / s.len() as f32;
    let p95 = s[((s.len() as f32 * 0.95) as usize).min(s.len() - 1)];
    (mean, p95, *s.last().unwrap())
}

/// World-space sole positions. The root's ground is the
/// ANIMATOR's smoothed reference — the same number the host hands
/// the mesh build — so measured feet and drawn feet agree.
fn sole_world(frame: &PoseFrame, pos: Vec2, yaw: f32, ground: f32) -> [Vec3; 2] {
    let root = root_matrix(Vec3::new(pos.x, 0.0, pos.y), ground, frame, yaw.to_degrees());
    [
        root.transform_point3(foot_kinematics(frame, true).sole),
        root.transform_point3(foot_kinematics(frame, false).sole),
    ]
}

fn ankle_world(frame: &PoseFrame, pos: Vec2, yaw: f32, ground: f32) -> [Vec3; 2] {
    let root = root_matrix(Vec3::new(pos.x, 0.0, pos.y), ground, frame, yaw.to_degrees());
    [
        root.transform_point3(foot_kinematics(frame, true).ankle),
        root.transform_point3(foot_kinematics(frame, false).ankle),
    ]
}

fn survey() {
    println!("== shore survey (height_at every 2 m along each line) ==");
    let lines: [(&str, Vec2, Vec2); 5] = [
        ("west z=56", Vec2::new(-8.0, 56.0), Vec2::new(-68.0, 56.0)),
        ("diag SW", Vec2::new(-8.0, 64.0), Vec2::new(-58.0, 24.0)),
        ("west z=44", Vec2::new(-8.0, 44.0), Vec2::new(-64.0, 44.0)),
        ("inland x=-8", Vec2::new(-8.0, 64.0), Vec2::new(-8.0, 4.0)),
        ("south x=-14", Vec2::new(-14.0, 64.0), Vec2::new(-14.0, 124.0)),
    ];
    for (name, a, b) in lines {
        let n = ((b - a).length() / 2.0) as usize;
        let mut out = String::new();
        for i in 0..=n {
            let p = a.lerp(b, i as f32 / n as f32);
            out.push_str(&format!(" {:.1}", height_at(p.x, p.y)));
        }
        println!("  {:12} ({}..{}):{}", name, a, b, out);
    }
}

struct Census {
    min: [[f32; 3]; BONE_COUNT],
    max: [[f32; 3]; BONE_COUNT],
    n: usize,
}

impl Census {
    fn new() -> Census {
        Census {
            min: [[f32::INFINITY; 3]; BONE_COUNT],
            max: [[f32::NEG_INFINITY; 3]; BONE_COUNT],
            n: 0,
        }
    }
    fn observe(&mut self, frame: &PoseFrame) {
        self.n += 1;
        for (i, a) in frame.angles.iter().enumerate() {
            for k in 0..3 {
                let v = [a.x, a.y, a.z][k];
                self.min[i][k] = self.min[i][k].min(v);
                self.max[i][k] = self.max[i][k].max(v);
            }
        }
    }
}

const BONE_NAMES: [&str; BONE_COUNT] = [
    "Pelvis", "Torso", "Head", "UpperArmL", "ForearmL", "HandL", "UpperArmR", "ForearmR",
    "HandR", "ThighL", "ShinL", "FootL", "ThighR", "ShinR", "FootR",
];

/// Isolation test: constant input on the gentle shelf, heading
/// north. If the planted ankle sits on its anchor here, the solver
/// and conversions are right and the drive's residuals live in the
/// turns; if not, the numbers below say by how much, per substep.
fn isolation() {
    println!("\n== ISOLATION: constant walk north at 1.5 m/s from (-8, 52) ==");
    let mut anim = Animator::new();
    let mut pos = Vec2::new(-8.0, 52.0);
    let yaw = std::f32::consts::PI;
    let mut prev_sole: Option<[Vec3; 2]> = None;
    let mut contact_speeds: Vec<f32> = Vec::new();
    for i in 0..(4.0 / DT) as usize {
        pos += Vec2::new(0.0, -1.0) * (1.5 * DT);
        let input = AnimInput {
            pos: Vec3::new(pos.x, 0.0, pos.y),
            yaw,
            speed: 1.5,
            crouch: 0.0,
            aim: 0.0,
            slide: 0.0,
            carry: true,
        };
        let frame = anim.update(DT, &input);
        let achieved = ankle_world(&frame, pos, yaw, anim.ground_y());
        let targets = anim.ankle_targets();
        let sole = sole_world(&frame, pos, yaw, anim.ground_y());
        if i % 120 == 0 {
            for l in 0..2 {
                println!(
                    "    t={:.2} leg {} contact={} target ({:.3},{:.3},{:.3}) achieved ({:.3},{:.3},{:.3}) err {:.4}",
                    i as f32 * DT, l, anim.contact(l == 0),
                    targets[l].x, targets[l].y, targets[l].z,
                    achieved[l].x, achieved[l].y, achieved[l].z,
                    (achieved[l] - targets[l]).length()
                );
            }
        }
        if let Some(prev) = prev_sole {
            for l in 0..2 {
                if anim.contact(l == 0) && i as f32 * DT > 1.0 {
                    let v = (sole[l] - prev[l]) / DT;
                    contact_speeds.push((v.x * v.x + v.z * v.z).sqrt());
                }
            }
        }
        prev_sole = Some(sole);
    }
    let (m, p, x) = stats(&contact_speeds);
    println!(
        "    isolation contact sole speed: mean {:.4} p95 {:.4} max {:.4} (n={})",
        m, p, x, contact_speeds.len()
    );
}

fn main() {
    isolation();
    survey();

    println!("\n== gait table (anchors interpolated; cadence = speed / step) ==");
    for v in [0.75, WALK_SPEED, 2.4, RUN_SPEED, 4.6, SPRINT_SPEED, 7.0] {
        let (step, duty, clr, drop, bob) = gait_params(v, 0.0);
        println!(
            "  v {:4.2} m/s: step {:.2} m, stride {:.2} m, duty {:.2}, clearance {:.3} m, ride drop {:.3}, bob {:.3}, cadence {:.2} steps/s",
            v, step, step * 2.0, duty, clr, drop, bob, v / step
        );
    }
    let (step, duty, clr, _, _) = gait_params(CROUCH_SPEED, 1.0);
    println!(
        "  crouch v {:.2}: step {:.2} m, duty {:.2}, clearance {:.3} m, cadence {:.2} steps/s",
        CROUCH_SPEED, step, duty, clr, CROUCH_SPEED / step
    );

    // ---- the full proof script at 240 Hz: skate, census, FK ----
    let (start, legs) = proof_script("full").expect("full script");
    let mut ctrl = Controller::new(start, legs);
    let mut anim = Animator::new();
    let mut prev_sole: Option<[Vec3; 2]> = None;
    let mut prev_contact = [false; 2];
    let mut steady_t = 0.0f32;
    // per gait bucket: 0 walk 1 run 2 sprint 3 crouch
    let mut skate_h: [Vec<f32>; 4] = Default::default();
    let mut skate_v: [Vec<f32>; 4] = Default::default();
    let mut ankle_h: [Vec<f32>; 4] = Default::default();
    let mut skate_edge: [Vec<f32>; 4] = Default::default();
    let mut skate_mid: [Vec<f32>; 4] = Default::default();
    let mut prev_ankle: Option<[Vec3; 2]> = None;
    let mut contact_age = [0u32; 2];
    // Plants carry their steady-window id: step/cadence pairs are
    // only ever formed inside one unbroken steady window, so a
    // dropped plant can never inflate a measured step.
    let mut plants: [Vec<(f32, Vec2, f32, u32)>; 4] = Default::default(); // (time, xz, yaw, window)
    let mut window_id: u32 = 0;
    let mut was_steady = false;
    let mut census: [Census; 4] = [Census::new(), Census::new(), Census::new(), Census::new()];
    let mut fk_err: Vec<f32> = Vec::new();
    let mut prev_yaw = ctrl.agent().yaw;
    let mut slide_entry = 0.0f32;
    let mut slide_start: Option<(f32, Vec2)> = None;
    let mut slide_result: Option<(f32, f32, f32)> = None; // (entry speed, distance, duration)
    let mut was_sliding = false;
    let mut path_lo = f32::INFINITY;
    let mut path_hi = f32::NEG_INFINITY;
    let mut t = 0.0f32;
    let mut frames = 0usize;
    let t0 = Instant::now();
    let mut build_us = 0.0f64;
    let mut build_n = 0usize;
    loop {
        let input: AnimInput = ctrl.update(DT);
        let frame = anim.update(DT, &input);
        let a = *ctrl.agent();
        t += DT;
        frames += 1;
        let h = height_at(a.pos.x, a.pos.y);
        path_lo = path_lo.min(h);
        path_hi = path_hi.max(h);

        // Slide bookkeeping (entry speed, distance, duration).
        if a.sliding && !was_sliding {
            slide_entry = a.speed;
            slide_start = Some((t, a.pos));
        }
        if !a.sliding && was_sliding {
            if let Some((st, sp)) = slide_start {
                slide_result = Some((slide_entry, (a.pos - sp).length(), t - st));
            }
        }
        was_sliding = a.sliding;

        // Steady state: at the gait's target speed, held.
        let tgt = a.gait.target_speed();
        if !a.done && (a.speed - tgt).abs() <= (0.03 * tgt).max(0.03) && a.speed > 0.1 {
            steady_t += DT;
        } else {
            steady_t = 0.0;
        }
        let bucket = match a.gait {
            Gait::Walk => Some(0),
            Gait::Run => Some(1),
            Gait::Sprint => Some(2),
            Gait::CrouchWalk => Some(3),
            Gait::Stand => None,
        };
        let sole = sole_world(&frame, a.pos, a.yaw, anim.ground_y());
        // The cycle census and the skate/plants measurements
        // describe the CYCLES: steady speed AND running straight
        // (turns are the metric runs' business, measured there).
        let yaw_rate = ((a.yaw - prev_yaw + std::f32::consts::PI * 3.0)
            % std::f32::consts::TAU
            - std::f32::consts::PI)
            .abs()
            / DT;
        prev_yaw = a.yaw;
        let steady = steady_t > 0.6 && yaw_rate < 0.08;
        if steady && !was_steady {
            window_id += 1;
        }
        was_steady = steady;
        if steady {
            if let Some(b) = bucket {
                census[b].observe(&frame);
                if let Some(prev) = prev_sole {
                    let ank_now = ankle_world(&frame, a.pos, a.yaw, anim.ground_y());
                    for leg_i in 0..2 {
                        let contact = anim.contact(leg_i == 0)
                            && (sole[leg_i].y - height_at(sole[leg_i].x, sole[leg_i].z)) < 0.012;
                        if contact {
                            let v = (sole[leg_i] - prev[leg_i]) / DT;
                            skate_h[b].push((v.x * v.x + v.z * v.z).sqrt());
                            skate_v[b].push(v.y.abs());
                            if let Some(pa) = prev_ankle {
                                let va = (ank_now[leg_i] - pa[leg_i]) / DT;
                                ankle_h[b].push((va.x * va.x + va.z * va.z).sqrt());
                            }
                            // Boundary split: samples within two
                            // substeps of contact start carry the
                            // swing's last motion; the rest is the
                            // stance proper.
                            if contact_age[leg_i] <= 2 {
                                skate_edge[b].push((v.x * v.x + v.z * v.z).sqrt());
                            } else {
                                skate_mid[b].push((v.x * v.x + v.z * v.z).sqrt());
                            }
                        }
                        let now_contact = anim.contact(leg_i == 0);
                        if now_contact && !prev_contact[leg_i] {
                            plants[b].push((
                                t,
                                Vec2::new(sole[leg_i].x, sole[leg_i].z),
                                a.yaw,
                                window_id,
                            ));
                        }
                    }
                }
            }
        }
        // FK vs IK target error: contacting legs only, in the
        // full-weight regime — a planted ankle must sit ON its
        // anchor; a swinging ankle owes nothing to its target.
        if a.speed > 0.7 && !a.sliding {
            let achieved = ankle_world(&frame, a.pos, a.yaw, anim.ground_y());
            let targets = anim.ankle_targets();
            for leg_i in 0..2 {
                if anim.contact(leg_i == 0) {
                    fk_err.push((achieved[leg_i] - targets[leg_i]).length());
                }
            }
        }
        for leg_i in 0..2 {
            if anim.contact(leg_i == 0) && prev_contact[leg_i] {
                contact_age[leg_i] += 1;
            } else if anim.contact(leg_i == 0) {
                contact_age[leg_i] = 0;
            }
        }
        prev_contact = [anim.contact(true), anim.contact(false)];
        prev_sole = Some(sole);
        prev_ankle = Some(ankle_world(&frame, a.pos, a.yaw, anim.ground_y()));

        // Mesh build cost (the per-frame CPU price of the pose).
        if frames.is_multiple_of(24) {
            let s = Instant::now();
            let mesh = build_operator_frame(&frame, Vec3::new(a.pos.x, 0.0, a.pos.y), anim.ground_y(), a.yaw.to_degrees());
            build_us += s.elapsed().as_secs_f64() * 1e6;
            build_n += 1;
            std::hint::black_box(mesh.vertices.len());
        }
        if a.done && a.speed <= 0.0 {
            break;
        }
        if t > 240.0 {
            println!("  !! script did not finish in 240 s");
            break;
        }
    }
    println!(
        "\n== full drive at 240 Hz: {:.1} s sim, {} substeps, path height range [{:.2}, {:.2}] ==",
        t, frames, path_lo, path_hi
    );
    println!(
        "  sim wall time {:.2} s ({:.0} substeps/s); mesh build mean {:.0} us/frame",
        t0.elapsed().as_secs_f64(),
        frames as f64 / t0.elapsed().as_secs_f64(),
        build_us / build_n.max(1) as f64
    );
    let (m, p, x) = stats(&fk_err);
    println!(
        "  IK/FK ankle error at full gait weight: mean {:.4} m, p95 {:.4}, max {:.4} (n={})",
        m, p, x, fk_err.len()
    );
    if let Some((entry, dist, dur)) = slide_result {
        println!(
            "  slide: entry {:.2} m/s, distance {:.2} m, duration {:.2} s",
            entry, dist, dur
        );
    }
    let names = ["walk", "run", "sprint", "crouch-walk"];
    for b in 0..4 {
        let (mh, ph, xh) = stats(&skate_h[b]);
        let (mv, pv, xv) = stats(&skate_v[b]);
        println!(
            "  {:11} contact sole speed: horizontal mean {:.4} p95 {:.4} max {:.4} m/s | vertical mean {:.4} p95 {:.4} max {:.4} (n={})",
            names[b], mh, ph, xh, mv, pv, xv, skate_h[b].len()
        );
        let (ma, pa, xa) = stats(&ankle_h[b]);
        let (me, _, _) = stats(&skate_edge[b]);
        let (mm, pm, xm) = stats(&skate_mid[b]);
        println!(
            "  {:11}   same samples: ANKLE horizontal mean {:.4} p95 {:.4} max {:.4} | sole mid-stance (>2 substeps) mean {:.4} p95 {:.4} max {:.4} (n={}) | sole at contact edge mean {:.4} (n={})",
            names[b], ma, pa, xa, mm, pm, xm, skate_mid[b].len(), me, skate_edge[b].len()
        );
        // Plant-to-plant: step length x cadence vs the speed on
        // record, pairs formed only inside one steady window.
        let pl = &plants[b];
        if pl.len() > 4 {
            let mut steps = Vec::new();
            let mut spans = 0.0f32;
            let mut counted = 0usize;
            for w in pl.windows(2) {
                if w[0].3 != w[1].3 {
                    continue;
                }
                let d = w[1].1 - w[0].1;
                let (s, c) = w[1].2.sin_cos();
                steps.push((d.x * s + d.y * c).abs());
                spans += w[1].0 - w[0].0;
                counted += 1;
            }
            if counted > 3 {
                let cad = counted as f32 / spans;
                let (ms, _, _) = stats(&steps);
                let tgt = [WALK_SPEED, RUN_SPEED, SPRINT_SPEED, CROUCH_SPEED][b];
                println!(
                    "  {:11} plants {} ({} in-window pairs): measured step {:.3} m x cadence {:.3} steps/s = {:.3} m/s vs gait {:.2} ({:+.1}%)",
                    names[b], pl.len(), counted, ms, cad, ms * cad, tgt,
                    (ms * cad / tgt - 1.0) * 100.0
                );
            }
        }
    }

    println!("\n== joint census (degrees, min..max over steady windows) ==");
    for b in 0..4 {
        if census[b].n == 0 {
            continue;
        }
        println!("  {} ({} samples):", names[b], census[b].n);
        for (i, bone_name) in BONE_NAMES.iter().enumerate() {
            let (mn, mx) = (census[b].min[i], census[b].max[i]);
            println!(
                "    {:10} x [{:7.1},{:7.1}]  y [{:7.1},{:7.1}]  z [{:7.1},{:7.1}]",
                bone_name, mn[0], mx[0], mn[1], mx[1], mn[2], mx[2]
            );
        }
    }

    // ---- dedicated metric runs ----
    println!("\n== movement metrics (dedicated runs, 240 Hz) ==");
    metric_accel_stop();
    metric_turn(Gait::Walk, 3.5);
    metric_turn(Gait::Run, 6.0);
}

/// Acceleration and braking: stand → run → sprint → full stop,
/// up the surveyed shelf.
fn metric_accel_stop() {
    let start = Vec2::new(-8.0, 64.0);
    let legs = vec![
        ScriptLeg::to(-8.0, 44.0, Gait::Run),
        ScriptLeg::to(-8.0, 37.0, Gait::Sprint),
        ScriptLeg::to(-8.0, 35.0, Gait::Stand),
    ];
    let mut ctrl = Controller::new(start, legs);
    let mut t = 0.0f32;
    let mut t_run90: Option<f32> = None;
    let mut sprint_leg_at: Option<f32> = None;
    let mut t_sprint90: Option<f32> = None;
    // The stop is measured from the first deceleration after full
    // sprint — the moment the brakes actually bite.
    let mut prev_speed = 0.0f32;
    let mut brake_mark: Option<(Vec2, f32)> = None;
    let mut stop_result: Option<f32> = None;
    loop {
        ctrl.update(DT);
        t += DT;
        let a = *ctrl.agent();
        if t_run90.is_none() && a.speed >= 0.9 * RUN_SPEED {
            t_run90 = Some(t);
        }
        if a.leg == 1 && sprint_leg_at.is_none() {
            sprint_leg_at = Some(t);
        }
        if let Some(st) = sprint_leg_at {
            if t_sprint90.is_none() && a.speed >= 0.9 * SPRINT_SPEED {
                t_sprint90 = Some(t - st);
            }
        }
        if brake_mark.is_none() && prev_speed >= 0.97 * SPRINT_SPEED && a.speed < prev_speed {
            brake_mark = Some((a.pos, prev_speed));
        }
        if let Some((bp, _)) = brake_mark {
            if stop_result.is_none() && a.speed < 0.05 {
                stop_result = Some((a.pos - bp).length());
            }
        }
        prev_speed = a.speed;
        if a.done || t > 60.0 {
            break;
        }
    }
    println!(
        "  0 -> run:    90% of {:.1} m/s at t = {:.2} s",
        RUN_SPEED,
        t_run90.unwrap_or(-1.0)
    );
    println!(
        "  run -> sprint: 90% of {:.1} m/s in {:.2} s from sprint-leg start",
        SPRINT_SPEED,
        t_sprint90.unwrap_or(-1.0)
    );
    if let (Some((_, v0)), Some(dist)) = (brake_mark, stop_result) {
        println!(
            "  sprint -> stand: brakes bite at {:.2} m/s, stop distance {:.2} m",
            v0, dist
        );
    }
}

/// A 180° reversal on an arc of the given radius, at the gait,
/// after a straight run-up so the turn is measured at speed, not
/// conflated with the acceleration onto it.
fn metric_turn(gait: Gait, radius: f32) {
    let start = Vec2::new(-8.0, 52.0);
    let center = Vec2::new(-8.0 + radius, 40.0);
    let mut legs = vec![ScriptLeg::to(-8.0, 40.0, gait)];
    for k in 1..=4 {
        let a = std::f32::consts::PI + (k as f32) * std::f32::consts::FRAC_PI_4;
        legs.push(ScriptLeg::to(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
            gait,
        ));
    }
    // A straight exit leg on the arc's exit tangent: the reversal
    // is measured against the accumulated heading change, and the
    // chorded arc's last leg ends ~20° short of the full tangent
    // reversal — the exit leg is where the heading completes it.
    legs.push(ScriptLeg::to(center.x + radius, center.y + 8.0, gait));
    let mut ctrl = Controller::new(start, legs);
    let mut t = 0.0f32;
    let mut arc_start: Option<(f32, f32)> = None; // (time, yaw)
    let mut prev_yaw = ctrl.agent().yaw;
    let mut peak_rate = 0.0f32;
    let mut turned_in: Option<f32> = None;
    let mut turned_acc = 0.0f32;
    loop {
        ctrl.update(DT);
        t += DT;
        let a = *ctrl.agent();
        if a.leg >= 1 && arc_start.is_none() {
            arc_start = Some((t, a.yaw));
        }
        if let Some((st, _)) = arc_start {
            let d = (a.yaw - prev_yaw + std::f32::consts::PI * 3.0) % std::f32::consts::TAU
                - std::f32::consts::PI;
            peak_rate = peak_rate.max(d.abs() / DT);
            turned_acc += d;
            if turned_in.is_none() && turned_acc.abs() >= std::f32::consts::PI - 0.15 {
                turned_in = Some(t - st);
            }
        }
        prev_yaw = a.yaw;
        if a.done || t > 60.0 {
            break;
        }
    }
    println!(
        "  180 deg turn at {} (arc r={:.1} m, at speed): reversed in {:.2} s, peak yaw rate {:.0} deg/s",
        gait.name(),
        radius,
        turned_in.unwrap_or(-1.0),
        peak_rate.to_degrees()
    );
}

