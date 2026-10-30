//! FP controller demo: one scripted, deterministic
//! input sequence over a flat street slab, printed
//! step by step — walk, sprint, aim, crouch, lean,
//! stand. No rendering, no files; the numbers are the
//! demonstration.
//!
//! Run: cargo run -q -p c35 --example fp_demo
//!
//! What to watch: sprint widens the FOV toward 82 and
//! the speed toward 5.8; aiming narrows the FOV toward
//! 55 and the bob reads exactly +0.000; crouching drops
//! the eye toward 1.05 with the bob still exactly zero;
//! leaning shifts the eye sideways by the lean offset
//! and rolls the camera; standing restores the eye to
//! 1.62.

use c35::controller::camera::CameraState;
use c35::controller::{PlayerController, PlayerInput};
use c35::traversal::physics::{BoxCollider, StaticWorld};
use c35::traversal::surface::{SurfaceKind, SurfaceRecord};
use glam::Vec3;

const DT: f32 = 1.0 / 60.0;

fn print_state(t: f32, cam: &CameraState) {
    println!(
        "  t={:4.2}s posture={:<6?} eye_h={:.2} spd={:.2} fov={:.1} bob={:+.3} roll={:+.2}deg eye=({:+.2},{:.2},{:+.2})",
        t,
        cam.posture,
        cam.eye_height,
        cam.speed,
        cam.fov_deg,
        cam.bob_offset,
        cam.roll.to_degrees(),
        cam.eye.x,
        cam.eye.y,
        cam.eye.z,
    );
}

/// Run one scripted phase: the same input every frame
/// for `seconds`, printing every quarter second and at
/// the phase's end. Returns the elapsed clock.
fn phase(
    ctrl: &mut PlayerController,
    world: &StaticWorld,
    label: &str,
    input: PlayerInput,
    seconds: f32,
    t: &mut f32,
) {
    println!("--- {label} ---");
    let frames = (seconds / DT).round() as u32;
    for i in 0..frames {
        let cam = ctrl.update(&input, world, DT);
        *t += DT;
        if i % 15 == 0 || i + 1 == frames {
            print_state(*t, &cam);
        }
    }
}

fn main() {
    // One flat street slab, top at y = 0.
    let mut world = StaticWorld::new();
    world.add(BoxCollider::new(
        Vec3::new(-40.0, -1.0, -40.0),
        Vec3::new(40.0, 0.0, 40.0),
        SurfaceRecord::dry(SurfaceKind::Asphalt),
    ));

    let mut ctrl = PlayerController::new(Vec3::new(0.0, 0.0, 0.0));
    let mut t = 0.0f32;
    let walk = PlayerInput {
        move_forward: 1.0,
        ..Default::default()
    };

    phase(&mut ctrl, &world, "walk forward", walk, 2.0, &mut t);
    phase(
        &mut ctrl,
        &world,
        "sprint (FOV widens)",
        PlayerInput {
            sprint_held: true,
            ..walk
        },
        2.0,
        &mut t,
    );
    phase(
        &mut ctrl,
        &world,
        "aim while walking (FOV narrows, bob zero)",
        PlayerInput {
            aim_held: true,
            ..walk
        },
        2.0,
        &mut t,
    );
    phase(
        &mut ctrl,
        &world,
        "crouch-walk (eye drops, bob zero)",
        PlayerInput {
            crouch_held: true,
            ..walk
        },
        1.5,
        &mut t,
    );
    phase(
        &mut ctrl,
        &world,
        "lean right, standing still (eye offset + roll)",
        PlayerInput {
            lean: 1.0,
            ..Default::default()
        },
        1.0,
        &mut t,
    );
    phase(
        &mut ctrl,
        &world,
        "stand again, idle (eye back to 1.62)",
        PlayerInput::default(),
        1.0,
        &mut t,
    );
    println!("done: final posture {:?}", ctrl.posture());
}
