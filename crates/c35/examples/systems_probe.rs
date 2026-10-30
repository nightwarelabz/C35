//! CPU-only end-to-end engine proof: fixed ticks -> contacts -> hearing,
//! with the same triangle geometry deciding occlusion; plus rope dynamics.
use c35::acoustics::{SoundEvent, SoundField, SoundKind};
use c35::glam::Vec3;
use c35::locomotion::{AnimInput, Animator};
use c35::physics::{Particle, Rope};
use c35::spatial::{Triangle, TriangleBvh};
use c35::timing::FixedClock;
use std::time::Duration;

fn main() {
    let geometry = TriangleBvh::new(vec![Triangle {
        vertices: [
            Vec3::new(-20.0, -2.0, 4.0),
            Vec3::new(20.0, -2.0, 4.0),
            Vec3::new(0.0, 20.0, 4.0),
        ],
        tag: 0,
    }])
    .unwrap();
    let mut clock = FixedClock::new(120, 8);
    let mut animator = Animator::new();
    animator.set_ground_fn(Some(|_, _| 0.0));
    let mut field = SoundField::new(32);
    let mut input = AnimInput {
        pos: Vec3::ZERO,
        yaw: 0.0,
        speed: 1.5,
        crouch: 0.0,
        aim: 0.0,
        slide: 0.0,
        carry: true,
    };
    let rope = Rope::new(Vec3::new(0.0, 6.0, 0.0), 4.0).unwrap();
    let mut particle = Particle {
        position: Vec3::new(4.0, 6.0, 0.0),
        velocity: Vec3::ZERO,
    };
    let mut contacts = 0;
    let mut occluded = 0;
    // Deliberately render at 144 Hz while simulation runs at 120 Hz.
    for frame in 0..720u64 {
        let a = frame * 1_000_000_000 / 144;
        let b = (frame + 1) * 1_000_000_000 / 144;
        for _ in clock.advance(Duration::from_nanos(b - a)).ticks {
            let dt = clock.step_seconds();
            field.advance(dt);
            input.pos.z += input.speed * dt;
            animator.update(dt, &input);
            for step in animator.footsteps() {
                contacts += 1;
                field.emit(SoundEvent {
                    source: 1,
                    kind: SoundKind::Footstep,
                    position: step.position,
                    radius: 20.0,
                    loudness: step.speed / 3.4,
                    duration: 0.4,
                });
            }
            occluded += field
                .sample(Vec3::new(0.0, 1.0, 10.0), Vec3::X, 0.2, |a, b| {
                    geometry.occluded(a, b)
                })
                .filter(|s| s.occluded)
                .count();
            particle.step(Vec3::new(0.0, -9.81, 0.0), dt, Some(&rope));
        }
    }
    assert_eq!(clock.tick(), 600);
    assert!((10..=14).contains(&contacts));
    assert!(occluded > 0);
    assert!((particle.position - rope.anchor).length() <= rope.length + 1e-5);
    println!("ticks={} contacts={} occluded_samples={} live_sounds={} evicted={} dropped={:?} rope_distance={:.5}",
        clock.tick(), contacts, occluded, field.len(), field.evicted(), clock.dropped_time(),
        (particle.position - rope.anchor).length());
}
