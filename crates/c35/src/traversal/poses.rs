//! The traversal pose family, on the PoseFrame seam.
//!
//! The third-person pose-graph STAYS: these poses are new graph
//! state beside the M7 gaits, expressed through the same
//! PoseFrame (joint angles in degrees, root drop) — a mesh swap
//! must not touch any of this. The both-hands law is graph
//! state here: while the launcher is committed (or the hands
//! are on a ledge), the weapon layer is CLOSED, and the frame
//! reports that closure — the host gates firing on the report,
//! never on an input suggestion.

use crate::operator::{Bone, PoseFrame};

/// Traversal graph state. `Gait` means "no traversal override —
/// the M7 gait frame stands as built".
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TravPose {
    Gait,
    WallRunLeft,
    WallRunRight,
    GrappleRaise,
    GrappleHang,
    GrappleSwing,
    Mantle,
    Fall,
    Land,
}

/// Blend a traversal pose over a gait frame. `t` is the blend
/// weight (0 = gait, 1 = full traversal pose); the caller eases
/// `t` over frames — no camera or pose snap lives here.
/// Returns the frame and whether the weapon layer is OPEN.
/// Closed during grapple commitment and mantle: the hands are
/// on the launcher or the ledge, and the pistol does not speak.
pub fn traversal_pose(
    base: &PoseFrame,
    pose: TravPose,
    t: f32,
    hands_committed: bool,
) -> (PoseFrame, bool) {
    use Bone::*;
    let t = t.clamp(0.0, 1.0);
    let mut frame = *base;
    let mut weapon_open = !hands_committed;
    let set = |f: &mut PoseFrame, b: Bone, v: glam::Vec3| {
        let cur = f.get(b);
        f.set(b, cur.lerp(v, t));
    };
    let d = glam::Vec3::new;
    match pose {
        TravPose::Gait => {}
        // Lean INTO the wall (torso roll toward it), the
        // wall-side arm trailing for balance, coat streaming
        // in the family's signature read.
        TravPose::WallRunLeft => {
            set(&mut frame, Torso, d(8.0, 0.0, -14.0));
            set(&mut frame, UpperArmL, d(-20.0, 10.0, -30.0));
            set(&mut frame, ForearmL, d(-30.0, 0.0, 0.0));
            set(&mut frame, UpperArmR, d(-50.0, -8.0, 20.0));
            set(&mut frame, ForearmR, d(-25.0, 0.0, 0.0));
            set(&mut frame, ThighL, d(-25.0, 0.0, 0.0));
            set(&mut frame, ThighR, d(10.0, 0.0, 0.0));
            set(&mut frame, ShinL, d(30.0, 0.0, 0.0));
        }
        TravPose::WallRunRight => {
            set(&mut frame, Torso, d(8.0, 0.0, 14.0));
            set(&mut frame, UpperArmR, d(-20.0, -10.0, 30.0));
            set(&mut frame, ForearmR, d(-30.0, 0.0, 0.0));
            set(&mut frame, UpperArmL, d(-50.0, 8.0, -20.0));
            set(&mut frame, ForearmL, d(-25.0, 0.0, 0.0));
            set(&mut frame, ThighL, d(10.0, 0.0, 0.0));
            set(&mut frame, ThighR, d(-25.0, 0.0, 0.0));
            set(&mut frame, ShinR, d(30.0, 0.0, 0.0));
        }
        // Both hands on the launcher, raised two-handed.
        TravPose::GrappleRaise => {
            weapon_open = false;
            set(&mut frame, UpperArmR, d(-70.0, -5.0, 12.0));
            set(&mut frame, ForearmR, d(-20.0, 0.0, 0.0));
            set(&mut frame, UpperArmL, d(-65.0, 10.0, -18.0));
            set(&mut frame, ForearmL, d(-25.0, 0.0, 0.0));
            set(&mut frame, Torso, d(6.0, 0.0, 0.0));
        }
        // Hang: arms up holding the line, body straight.
        TravPose::GrappleHang => {
            weapon_open = false;
            set(&mut frame, UpperArmR, d(-150.0, -5.0, 10.0));
            set(&mut frame, ForearmR, d(-10.0, 0.0, 0.0));
            set(&mut frame, UpperArmL, d(-150.0, 5.0, -10.0));
            set(&mut frame, ForearmL, d(-10.0, 0.0, 0.0));
            set(&mut frame, Torso, d(2.0, 0.0, 0.0));
        }
        // Swing tuck: knees folded, arms holding.
        TravPose::GrappleSwing => {
            weapon_open = false;
            set(&mut frame, UpperArmR, d(-140.0, -5.0, 10.0));
            set(&mut frame, ForearmR, d(-15.0, 0.0, 0.0));
            set(&mut frame, UpperArmL, d(-140.0, 5.0, -10.0));
            set(&mut frame, ForearmL, d(-15.0, 0.0, 0.0));
            set(&mut frame, ThighL, d(-55.0, 0.0, 0.0));
            set(&mut frame, ThighR, d(-50.0, 0.0, 0.0));
            set(&mut frame, ShinL, d(75.0, 0.0, 0.0));
            set(&mut frame, ShinR, d(70.0, 0.0, 0.0));
            set(&mut frame, Torso, d(10.0, 0.0, 0.0));
        }
        // Both hands reaching the ledge, body committed.
        TravPose::Mantle => {
            weapon_open = false;
            set(&mut frame, UpperArmR, d(-160.0, -5.0, 8.0));
            set(&mut frame, ForearmR, d(-5.0, 0.0, 0.0));
            set(&mut frame, UpperArmL, d(-160.0, 5.0, -8.0));
            set(&mut frame, ForearmL, d(-5.0, 0.0, 0.0));
            set(&mut frame, Torso, d(15.0, 0.0, 0.0));
            set(&mut frame, ThighL, d(-30.0, 0.0, 0.0));
            set(&mut frame, ThighR, d(-20.0, 0.0, 0.0));
        }
        TravPose::Fall => {
            set(&mut frame, UpperArmR, d(-30.0, -5.0, 25.0));
            set(&mut frame, UpperArmL, d(-30.0, 5.0, -25.0));
            set(&mut frame, ThighL, d(-15.0, 0.0, 0.0));
            set(&mut frame, ThighR, d(-10.0, 0.0, 0.0));
        }
        TravPose::Land => {
            // Landing dip: root drops, knees fold — one eased
            // translation, never a bounce (the FP note's law,
            // kept here for the figure too).
            frame.root_y -= 0.15 * t;
            set(&mut frame, ThighL, d(-35.0, 0.0, 0.0));
            set(&mut frame, ThighR, d(-35.0, 0.0, 0.0));
            set(&mut frame, ShinL, d(45.0, 0.0, 0.0));
            set(&mut frame, ShinR, d(45.0, 0.0, 0.0));
        }
    }
    // Mantle always closes the weapon layer: both hands are on
    // the ledge, whatever the grapple says.
    if pose == TravPose::Mantle {
        weapon_open = false;
    }
    (frame, weapon_open)
}
