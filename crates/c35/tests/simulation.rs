use c35::acoustics::{SoundEvent, SoundField, SoundKind};
use c35::glam::Vec3;
use c35::physics::{Particle, Rope};
use c35::spatial::{Aabb, Ray, Triangle, TriangleBvh};

fn wall(z: f32, tag: u32) -> Triangle {
    Triangle {
        vertices: [
            Vec3::new(-2.0, -2.0, z),
            Vec3::new(2.0, -2.0, z),
            Vec3::new(0.0, 2.0, z),
        ],
        tag,
    }
}

#[test]
fn raycast_is_two_sided_bounded_and_preserves_identity() {
    let bvh = TriangleBvh::new(vec![wall(4.0, 7), wall(2.0, 3)]).unwrap();
    let ray = Ray::new(Vec3::ZERO, Vec3::Z * 10.0, 10.0).unwrap();
    let hit = bvh.raycast(&ray).unwrap();
    assert_eq!(hit.tag, 3);
    assert!((hit.distance - 2.0).abs() < 1e-6);
    assert!((hit.barycentric.element_sum() - 1.0).abs() < 1e-6);
    assert!(bvh
        .raycast(&Ray::new(Vec3::ZERO, Vec3::Z, 1.0).unwrap())
        .is_none());
    assert!(bvh
        .raycast(&Ray::new(Vec3::new(0.0, 0.0, 3.0), -Vec3::Z, 2.0).unwrap())
        .is_some());
    assert!(Ray::new(Vec3::ZERO, Vec3::ZERO, 1.0).is_none());
    assert!(Ray::new(Vec3::NAN, Vec3::Z, 1.0).is_none());
    assert!(bvh.occluded(Vec3::ZERO, Vec3::Z * 5.0));
    assert!(!bvh.occluded(Vec3::ZERO, Vec3::Z));
}

#[test]
fn bvh_agrees_with_brute_force_for_many_rays() {
    let mut triangles = Vec::new();
    for i in 0..64 {
        let offset = Vec3::new(
            (i % 8) as f32 * 5.0 - 20.0,
            (i / 8) as f32 * 5.0 - 20.0,
            (i % 5) as f32,
        );
        let mut t = wall(20.0, i);
        for p in &mut t.vertices {
            *p += offset;
        }
        triangles.push(t);
    }
    let bvh = TriangleBvh::new(triangles.clone()).unwrap();
    let individual: Vec<_> = triangles
        .into_iter()
        .map(|t| TriangleBvh::new(vec![t]).unwrap())
        .collect();
    for x in -24..24 {
        for y in -24..24 {
            let ray = Ray::new(
                Vec3::new(x as f32 + 0.13, y as f32 + 0.17, 0.0),
                Vec3::Z,
                100.0,
            )
            .unwrap();
            let expected = individual
                .iter()
                .filter_map(|b| b.raycast(&ray))
                .min_by(|a, b| a.distance.total_cmp(&b.distance).then(a.tag.cmp(&b.tag)));
            assert_eq!(
                bvh.raycast(&ray).map(|h| (h.tag, h.distance)),
                expected.map(|h| (h.tag, h.distance))
            );
        }
    }
    let mut found = Vec::new();
    bvh.query_aabb(Aabb::new(Vec3::splat(-100.0), Vec3::splat(100.0)), |t| {
        found.push(t.tag)
    });
    found.sort_unstable();
    assert_eq!(found, (0..64).collect::<Vec<_>>());
}

#[test]
fn malformed_and_empty_geometry_are_safe() {
    assert!(TriangleBvh::from_indexed(&[Vec3::ZERO], &[1, 0, 0]).is_err());
    assert!(TriangleBvh::from_indexed(&[Vec3::ZERO], &[0, 0]).is_err());
    let empty = TriangleBvh::default();
    assert!(empty
        .raycast(&Ray::new(Vec3::ZERO, Vec3::Z, 1.0).unwrap())
        .is_none());
    let degenerate = TriangleBvh::from_indexed(&[Vec3::ZERO], &[0, 0, 0]).unwrap();
    assert!(degenerate
        .raycast(&Ray::new(-Vec3::Z, Vec3::Z, 2.0).unwrap())
        .is_none());
}

fn sound(position: Vec3) -> SoundEvent {
    SoundEvent {
        source: 5,
        kind: SoundKind::Footstep,
        position,
        radius: 10.0,
        loudness: 1.0,
        duration: 1.0,
    }
}

#[test]
fn sound_distance_decay_occlusion_and_equal_power_pan_share_one_level() {
    let mut field = SoundField::new(4);
    field.emit(sound(Vec3::X * 5.0));
    let open = field
        .sample(Vec3::ZERO, Vec3::X, 0.2, |_, _| false)
        .next()
        .unwrap();
    assert_eq!(open.level, 0.25);
    assert!(open.left_gain.abs() < 1e-6);
    assert!((open.right_gain - open.level).abs() < 1e-6);
    let bvh = TriangleBvh::new(vec![wall(2.0, 0)]).unwrap();
    let mut blocked = SoundField::new(1);
    blocked.emit(sound(Vec3::Z * 5.0));
    let heard = blocked
        .sample(Vec3::ZERO, Vec3::X, 0.2, |a, b| bvh.occluded(a, b))
        .next()
        .unwrap();
    assert!(heard.occluded);
    assert!((heard.level - 0.05).abs() < 1e-6);
    assert!(
        (heard.left_gain.powi(2) + heard.right_gain.powi(2) - heard.level.powi(2)).abs() < 1e-6
    );
    field.advance(0.5);
    assert_eq!(
        field
            .sample(Vec3::ZERO, Vec3::X, 1.0, |_, _| false)
            .next()
            .unwrap()
            .level,
        0.125
    );
    assert_eq!(
        field
            .sample(Vec3::X * 16.0, Vec3::X, 1.0, |_, _| false)
            .count(),
        0
    );
    field.advance(0.5);
    assert!(field.is_empty());
}

#[test]
fn sound_field_stays_bounded_and_rejects_invalid_input() {
    let mut field = SoundField::new(2);
    for _ in 0..5 {
        field.emit(sound(Vec3::ZERO));
    }
    assert_eq!(field.len(), 2);
    assert_eq!(field.evicted(), 3);
    assert_eq!(
        field
            .sample(Vec3::ZERO, Vec3::ZERO, 1.0, |_, _| false)
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    assert!(field.emit(sound(Vec3::NAN)).is_none());
    field.advance(f32::NAN);
    assert_eq!(field.len(), 2);
    assert!(SoundField::new(0).emit(sound(Vec3::ZERO)).is_none());
}

#[test]
fn rope_preserves_tangent_and_release_momentum() {
    let rope = Rope::new(Vec3::ZERO, 2.0).unwrap();
    let mut body = Particle {
        position: Vec3::X * 3.0,
        velocity: Vec3::new(4.0, 0.0, 5.0),
    };
    let report = rope.constrain(&mut body);
    assert!(report.taut);
    assert_eq!(body.position, Vec3::X * 2.0);
    assert_eq!(body.velocity, Vec3::Z * 5.0);
    assert_eq!(report.removed_outward_speed, 4.0);
    let velocity = body.velocity;
    body.step(Vec3::ZERO, 1.0 / 120.0, None);
    assert_eq!(body.velocity, velocity);
    assert!(body.position.z > 0.0);
}

#[test]
fn pendulum_stays_finite_inside_rope_length() {
    let rope = Rope::new(Vec3::ZERO, 4.0).unwrap();
    let mut body = Particle {
        position: Vec3::X * 4.0,
        velocity: Vec3::ZERO,
    };
    for _ in 0..1200 {
        body.step(Vec3::new(0.0, -9.81, 0.0), 1.0 / 120.0, Some(&rope));
        assert!(body.position.is_finite() && body.velocity.is_finite());
        assert!(body.position.length() <= 4.00001);
        assert!(body.velocity.length() < 15.0);
    }
    assert!(Rope::new(Vec3::ZERO, 0.0).is_none());
}

#[test]
fn invalid_or_overflowing_physics_step_does_not_corrupt_body() {
    let mut body = Particle {
        position: Vec3::X,
        velocity: Vec3::ZERO,
    };
    for dt in [f32::NAN, -1.0, f32::INFINITY, f32::MAX] {
        body.step(Vec3::splat(f32::MAX), dt, None);
        assert_eq!(body.position, Vec3::X);
        assert_eq!(body.velocity, Vec3::ZERO);
    }
}
