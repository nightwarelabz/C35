use c35::glam::Vec3;
use c35::locomotion::{AnimInput, Animator};
use c35::operator::{build_operator_frame, FigureTemplate, OperatorPose, PoseFrame};

#[test]
fn cached_skinning_matches_original_geometry_and_reuses_storage() {
    let template = FigureTemplate::operator();
    let mut mesh = template.mesh();
    let ptr = mesh.vertices.as_ptr();
    let iptr = mesh.indices.as_ptr();
    for pose in [
        OperatorPose::StandReady,
        OperatorPose::Aim,
        OperatorPose::Crouch,
    ] {
        let frame = PoseFrame::from_pose(pose);
        for yaw in [-170.0, 0.0, 43.0, 180.0] {
            let spot = Vec3::new(12.0, 0.0, -9.0);
            let reference = build_operator_frame(&frame, spot, 1.5, yaw);
            template.skin_into(&mut mesh, &frame, spot, 1.5, yaw);
            assert_eq!(mesh.indices, reference.indices);
            assert_eq!(mesh.vertices.as_ptr(), ptr);
            assert_eq!(mesh.indices.as_ptr(), iptr);
            for (a, b) in mesh.vertices.iter().zip(&reference.vertices) {
                assert!(Vec3::from_array(a.position).distance(Vec3::from_array(b.position)) < 1e-5);
                assert!(Vec3::from_array(a.normal).distance(Vec3::from_array(b.normal)) < 1e-5);
                assert_eq!(a.uv, b.uv);
                assert_eq!(a.mat, b.mat);
            }
        }
    }
}

#[test]
fn footsteps_are_contact_edges_at_the_ground_surface() {
    let mut animator = Animator::new();
    animator.set_ground_fn(Some(|_, _| 2.0));
    let mut input = AnimInput {
        pos: Vec3::ZERO,
        yaw: 0.0,
        speed: 1.5,
        crouch: 0.0,
        aim: 0.0,
        slide: 0.0,
        carry: true,
    };
    let mut count = 0;
    let mut feet = [0; 2];
    for _ in 0..600 {
        input.pos.z += input.speed / 120.0;
        animator.update(1.0 / 120.0, &input);
        for step in animator.footsteps() {
            count += 1;
            feet[usize::from(!step.left)] += 1;
            assert_eq!(step.position.y, 2.0);
            assert!(step.position.is_finite());
        }
    }
    assert!(
        (10..=14).contains(&count),
        "five seconds of walk produced {count} contacts"
    );
    assert!((feet[0] as i32 - feet[1] as i32).abs() <= 1);
    input.speed = 0.0;
    for _ in 0..120 {
        animator.update(1.0 / 120.0, &input);
        assert_eq!(animator.footsteps().count(), 0);
    }
}
