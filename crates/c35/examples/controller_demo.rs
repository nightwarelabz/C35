//! Dual-mode controller demo — a runnable demonstration,
//! not a proof harness. It drives ONE player controller
//! through one movement script over an analytic block:
//! walk -> run -> sprint in first person, a mode toggle
//! MID-SPRINT into third person (body, velocity and rope
//! untouched — only the rig changes), a crouch-walk in
//! third person, a toggle back, an aim stretch, then a
//! grapple latch + swing + release in first person. A
//! short annex runs a mantle in third person, and a
//! second annex walks the third-person boom into a wall
//! to show the collision pull-in. It prints camera/eye
//! positions, FOV, mode and body state at marked ticks —
//! Chase judges by playing; this prints what the run
//! produced, nothing more.

use c35::controller::{ControllerConfig, PlayerController, PlayerInput, SpeedTier, ViewMode};
use c35::traversal::physics::{BoxCollider, StaticWorld};
use c35::traversal::surface::{SurfaceKind, SurfaceRecord};
use glam::{Vec2, Vec3};

fn bx(w: &mut StaticWorld, min: Vec3, max: Vec3, kind: SurfaceKind) {
    w.add(BoxCollider::new(min, max, SurfaceRecord::dry(kind)));
}

/// The course block (metres): open street along +X, the
/// mast at x=24.5 (the grapple target), the crate at
/// x=10..12, z=4..6 (the mantle target).
fn course_world() -> StaticWorld {
    let mut w = StaticWorld::new();
    bx(&mut w, Vec3::new(-80.0, -1.0, -40.0), Vec3::new(80.0, 0.0, 40.0), SurfaceKind::Asphalt);
    bx(&mut w, Vec3::new(24.5, 0.0, -0.5), Vec3::new(25.5, 14.0, 0.5), SurfaceKind::Metal);
    bx(&mut w, Vec3::new(10.0, 0.0, 4.0), Vec3::new(12.0, 1.2, 6.0), SurfaceKind::Wood);
    w
}

fn flat(_x: f32, _z: f32) -> f32 {
    0.0
}

fn input(wish: Vec2, speed: SpeedTier) -> PlayerInput {
    PlayerInput {
        wish,
        speed,
        ..PlayerInput::default()
    }
}

fn show(tag: &str, f: &c35::controller::ControllerFrame) {
    println!(
        "{:<26} view={:?} trav={:?} posture={:?} feet=({:6.2},{:5.2},{:6.2}) vel=({:5.2},{:5.2},{:5.2}) speed={:4.2} cam=({:6.2},{:5.2},{:6.2}) fov={:5.1} bob_y={:+.3} boom={:4.2}{} vm={:?} weapon_open={}",
        tag,
        f.mode,
        f.traversal_mode,
        f.posture,
        f.feet.x,
        f.feet.y,
        f.feet.z,
        f.velocity.x,
        f.velocity.y,
        f.velocity.z,
        f.speed,
        f.camera.position.x,
        f.camera.position.y,
        f.camera.position.z,
        f.camera.fov_deg,
        f.camera.bob_offset.y,
        f.camera.boom_distance,
        if f.camera.boom_blocked { " BLOCKED" } else { "" },
        f.viewmodel_pose,
        f.weapon_open,
    );
}

fn main() {
    println!("== dual-mode controller demo ==");
    println!("one movement core (TravSim @ 240 Hz), two rigs; toggle carries all body state");
    let world = course_world();
    let east = Vec2::new(0.0, 1.0); // camera-space: y = forward
    // Face +X: yaw = atan2(1, 0) = PI/2 under forward=(sin,cos).
    let face_east = Vec2::new(std::f32::consts::FRAC_PI_2, 0.0);

    let mut pc = PlayerController::new(Vec3::new(-14.0, 0.0, 0.0));
    pc.set_ground_fn(Some(flat));

    // Settle + face east.
    pc.tick(&PlayerInput::default(), &world);
    let mut look = PlayerInput::default();
    look.look_delta = face_east;
    let mut frame = pc.tick(&look, &world);
    show("start (FP)", &frame);

    // Walk 2 s, run 2 s — first person.
    for _ in 0..480 {
        frame = pc.tick(&input(east, SpeedTier::Walk), &world);
    }
    show("after walk 2s (FP)", &frame);
    for _ in 0..480 {
        frame = pc.tick(&input(east, SpeedTier::Run), &world);
    }
    show("after run 2s (FP)", &frame);

    // Sprint 1 s in FP, then toggle MID-SPRINT and sprint
    // 1 s more in TP: the body must not notice.
    for _ in 0..240 {
        frame = pc.tick(&input(east, SpeedTier::Sprint), &world);
    }
    show("sprint 1s (FP)", &frame);
    let before = frame;
    let mut t = input(east, SpeedTier::Sprint);
    t.toggle_mode = true;
    frame = pc.tick(&t, &world);
    println!(
        "toggle tick: feet moved ({:.3},{:.3}) in one tick, vel ({:.2},{:.2},{:.2}) -> ({:.2},{:.2},{:.2}), view {:?} -> {:?}",
        frame.feet.x - before.feet.x,
        frame.feet.z - before.feet.z,
        before.velocity.x,
        before.velocity.y,
        before.velocity.z,
        frame.velocity.x,
        frame.velocity.y,
        frame.velocity.z,
        before.mode,
        frame.mode,
    );
    show("toggle tick (TP)", &frame);
    for _ in 0..240 {
        frame = pc.tick(&input(east, SpeedTier::Sprint), &world);
    }
    show("sprint +1s (TP)", &frame);

    // Crouch-walk in third person.
    for _ in 0..360 {
        frame = pc.tick(&input(east, SpeedTier::Crouch), &world);
    }
    show("crouch-walk 1.5s (TP)", &frame);

    // Toggle back to FP; aim stretch, standing.
    let mut t = input(Vec2::ZERO, SpeedTier::Walk);
    t.toggle_mode = true;
    t.aim_held = true;
    frame = pc.tick(&t, &world);
    for _ in 0..240 {
        let mut a = input(Vec2::ZERO, SpeedTier::Walk);
        a.aim_held = true;
        frame = pc.tick(&a, &world);
    }
    show("aim 1s (FP, back)", &frame);

    // Pitch up toward the mast top and fire the grapple.
    // From the aim point the mast strike (24.5, ~11.5, 0)
    // sits ~0.58 rad above level.
    let mut p = PlayerInput::default();
    p.look_delta = Vec2::new(0.0, 0.58);
    pc.tick(&p, &world);
    let mut fire = PlayerInput::default();
    fire.hook_press = true;
    fire.hook_held = true;
    frame = pc.tick(&fire, &world);
    show("grapple fired (FP)", &frame);
    let mut latched_at = None;
    for i in 0..240 {
        let mut h = PlayerInput::default();
        h.hook_held = true;
        frame = pc.tick(&h, &world);
        if pc.sim.grapple.is_latched() && latched_at.is_none() {
            latched_at = Some(i);
            show("grapple latched (FP)", &frame);
        }
    }
    show("swing held 1s (FP)", &frame);
    for _ in 0..240 {
        let mut h = PlayerInput::default();
        h.hook_held = true;
        frame = pc.tick(&h, &world);
    }
    show("swing held 2s (FP)", &frame);
    // Release: momentum kept, rope gone this tick.
    let mut r = PlayerInput::default();
    r.hook_release = true;
    frame = pc.tick(&r, &world);
    show("grapple released (FP)", &frame);
    for _ in 0..480 {
        frame = pc.tick(&PlayerInput::default(), &world);
    }
    show("settled 2s after release", &frame);
    println!(
        "events: strikes={} strike_fails={} landings={}",
        pc.sim.events.count(c35::traversal::events::StimulusKind::Strike),
        pc.sim.events.count(c35::traversal::events::StimulusKind::StrikeFail),
        pc.sim.events.count(c35::traversal::events::StimulusKind::Landing),
    );

    // ---- annex: mantle in third person ----
    let mut cfg = ControllerConfig::default();
    cfg.initial_mode = ViewMode::ThirdPerson;
    let mut pc2 = PlayerController::with_config(Vec3::new(8.2, 0.0, 5.0), cfg);
    pc2.set_ground_fn(Some(flat));
    pc2.tick(&PlayerInput::default(), &world);
    let mut look2 = PlayerInput::default();
    look2.look_delta = face_east;
    let mut f2 = pc2.tick(&look2, &world);
    show("annex start (TP)", &f2);
    // Walk east a half-second: the body facing chases the
    // movement direction at its turn rate, and the mantle
    // probe reads the BODY's facing — jumping before the
    // turn completes probes the wrong way and just jumps.
    for _ in 0..120 {
        f2 = pc2.tick(&input(Vec2::new(0.0, 1.0), SpeedTier::Walk), &world);
    }
    show("annex walked to crate (TP)", &f2);
    let mut j = input(Vec2::new(0.0, 1.0), SpeedTier::Walk);
    j.jump = true;
    f2 = pc2.tick(&j, &world);
    let mut saw_mantle = matches!(f2.traversal_mode, c35::traversal::BodyMode::Mantle);
    for _ in 0..200 {
        f2 = pc2.tick(&input(Vec2::new(0.0, 1.0), SpeedTier::Walk), &world);
        saw_mantle = saw_mantle || matches!(f2.traversal_mode, c35::traversal::BodyMode::Mantle);
    }
    show("annex after mantle run (TP)", &f2);
    println!("annex: mantle mode seen = {}", saw_mantle);

    // ---- annex: boom collision — geometry between the
    // focus and the wished boom end pulls the camera in. ----
    let mut pc3 = PlayerController::with_config(Vec3::new(0.0, 0.0, -3.0), cfg);
    pc3.set_ground_fn(Some(flat));
    let mut w2 = StaticWorld::new();
    bx(&mut w2, Vec3::new(-80.0, -1.0, -40.0), Vec3::new(80.0, 0.0, 40.0), SurfaceKind::Asphalt);
    bx(&mut w2, Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 6.0, 2.0), SurfaceKind::Brick);
    pc3.tick(&PlayerInput::default(), &w2);
    let mut look3 = PlayerInput::default();
    look3.look_delta = Vec2::new(std::f32::consts::PI, 0.0);
    let mut f3 = pc3.tick(&look3, &w2);
    for _ in 0..30 {
        f3 = pc3.tick(&PlayerInput::default(), &w2);
    }
    show("annex boom vs wall (TP)", &f3);
    println!(
        "annex: boom requested {:.2}, used {:.2}, blocked = {}",
        f3.camera.boom_requested, f3.camera.boom_distance, f3.camera.boom_blocked
    );
}
