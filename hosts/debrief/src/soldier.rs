//! The Regime soldier — the patrol.
//!
//! A second procedural character on the operator's part system and
//! the SAME 15-bone skeleton (the pose interface is the contract:
//! [`PoseFrame`] in, geometry out). Re-proportioned per PREP §1 —
//! broader and heavier than the operator: coat bulk, not muscle.
//! Built from the canon: front (#4), rear (#7), action sheet (#15).
//!
//! Non-negotiables carried in geometry, not texture alone:
//! - the greatcoat's SPLIT SKIRT is two rear panels with a real gap
//!   at the centre back (#7), hanging from the pelvis;
//! - the helmet + blank mask: the face carries NO readable features
//!   — a goggle band and a vented mask plate (#4);
//! - the back harness rides over the coat (#7): two verticals, a
//!   horizontal, buckles;
//! - the rifle is American-pattern, carried low (#15), modelled on
//!   the right hand's axis like the operator's USP;
//! - the red shoulder and the three-bars armband are on his LEFT
//!   (#4/#7/#15 all agree) — the pauldron as red cloth, the armband
//!   as a thin box wrapped on the left upper arm so the mark reads
//!   from every side at patrol distance.
//!
//! His surfaces live in regions the Slice 1 atlas extension painted
//! into the operator atlas's unused texels (COAT, ARMBAND, REDCLOTH,
//! MASK) plus the operator's own dark cloth/metal regions — the two
//! men share a texture economy the way they share a skeleton.

use c35::glam::Vec3;
use c35::operator::{region, uv_rect, Bone, OperatorMesh, PartSpec, PoseFrame, MAT_STANDARD};

fn spec(
    name: &'static str,
    bone: Bone,
    c: (f32, f32, f32),
    s: (f32, f32, f32),
    r: (u32, u32, u32, u32),
) -> PartSpec {
    PartSpec {
        name,
        bone,
        center: Vec3::new(c.0, c.1, c.2),
        size: Vec3::new(s.0, s.1, s.2),
        uv: uv_rect(r),
        mat: MAT_STANDARD,
    }
}

/// The soldier, as geometry: ~90 parts on the shared skeleton.
pub fn soldier_parts() -> Vec<PartSpec> {
    use region::*;
    use Bone::*;
    let mut v: Vec<PartSpec> = Vec::new();
    // ---- pelvis: belt, pouches, and the split skirt ----
    v.push(spec("pelvis", Pelvis, (0.0, 0.96, 0.0), (0.42, 0.22, 0.27), COAT, ));
    v.push(spec("belt", Pelvis, (0.0, 1.065, 0.0), (0.44, 0.07, 0.29), STRAP));
    v.push(spec("belt buckle", Pelvis, (0.0, 1.065, 0.15), (0.09, 0.05, 0.02), METAL));
    v.push(spec("rear pouch L", Pelvis, (0.12, 1.01, -0.155), (0.11, 0.13, 0.05), VEST));
    v.push(spec("rear pouch R", Pelvis, (-0.12, 1.01, -0.155), (0.11, 0.13, 0.05), VEST));
    // The skirt: front panel, two side panels, and the SPLIT rear —
    // two panels with a centre gap, per the rear canon (#7). The
    // gap is real geometry: you can see the legs through it.
    v.push(spec("skirt front", Pelvis, (0.0, 0.665, 0.115), (0.42, 0.53, 0.05), COAT));
    v.push(spec("skirt side L", Pelvis, (0.215, 0.665, 0.0), (0.05, 0.53, 0.26), COAT));
    v.push(spec("skirt side R", Pelvis, (-0.215, 0.665, 0.0), (0.05, 0.53, 0.26), COAT));
    v.push(spec("skirt rear L", Pelvis, (0.112, 0.655, -0.115), (0.205, 0.55, 0.05), COAT));
    v.push(spec("skirt rear R", Pelvis, (-0.112, 0.655, -0.115), (0.205, 0.55, 0.05), COAT));
    // ---- torso: the greatcoat's body, pouches, harness ----
    v.push(spec("coat torso", Torso, (0.0, 1.28, 0.0), (0.52, 0.46, 0.34), COAT));
    v.push(spec("coat collar", Torso, (0.0, 1.525, 0.0), (0.30, 0.13, 0.28), COAT));
    v.push(spec("chest pouch 1", Torso, (-0.15, 1.26, 0.185), (0.12, 0.15, 0.05), VEST));
    v.push(spec("chest pouch 2", Torso, (0.0, 1.26, 0.19), (0.12, 0.15, 0.05), VEST));
    v.push(spec("chest pouch 3", Torso, (0.15, 1.26, 0.185), (0.12, 0.15, 0.05), VEST));
    v.push(spec("coat button band", Torso, (0.0, 1.30, 0.175), (0.05, 0.40, 0.02), STRAP));
    // The back harness (#7): two verticals with buckles + a
    // horizontal, over the coat's back.
    v.push(spec("harness vertical L", Torso, (0.115, 1.31, -0.185), (0.07, 0.44, 0.03), STRAP));
    v.push(spec("harness vertical R", Torso, (-0.115, 1.31, -0.185), (0.07, 0.44, 0.03), STRAP));
    v.push(spec("harness horizontal", Torso, (0.0, 1.335, -0.185), (0.36, 0.06, 0.03), STRAP));
    v.push(spec("harness buckle L", Torso, (0.115, 1.475, -0.19), (0.09, 0.07, 0.02), METAL));
    v.push(spec("harness buckle R", Torso, (-0.115, 1.475, -0.19), (0.09, 0.07, 0.02), METAL));
    v.push(spec("harness keeper L", Torso, (0.115, 1.13, -0.19), (0.09, 0.10, 0.02), STRAP));
    v.push(spec("harness keeper R", Torso, (-0.115, 1.13, -0.19), (0.09, 0.10, 0.02), STRAP));
    // Shoulders: the LEFT is the red pauldron (canon) — red cloth;
    // the right stays coat-dark.
    v.push(spec("pauldron L (red)", Torso, (0.315, 1.49, 0.0), (0.22, 0.12, 0.30), REDCLOTH));
    v.push(spec("shoulder pad R", Torso, (-0.315, 1.49, 0.0), (0.21, 0.11, 0.29), COAT));
    // ---- head: helmet, goggle band, blank mask ----
    v.push(spec("neck seal", Head, (0.0, 1.535, 0.0), (0.15, 0.09, 0.15), FABRIC));
    v.push(spec("helmet", Head, (0.0, 1.69, 0.0), (0.30, 0.33, 0.30), HELMET));
    v.push(spec("helmet brow band", Head, (0.0, 1.775, 0.02), (0.31, 0.05, 0.31), STRAP));
    v.push(spec("goggle band", Head, (0.0, 1.705, 0.135), (0.25, 0.055, 0.035), PLASTIC));
    v.push(spec("mask plate", Head, (0.0, 1.575, 0.118), (0.215, 0.135, 0.055), MASK));
    v.push(spec("mask strap L", Head, (0.145, 1.63, -0.02), (0.03, 0.04, 0.24), STRAP));
    v.push(spec("mask strap R", Head, (-0.145, 1.63, -0.02), (0.03, 0.04, 0.24), STRAP));
    // ---- arms ----
    for (sx, up, fore, hand) in [
        (1.0f32, Bone::UpperArmL, Bone::ForearmL, Bone::HandL),
        (-1.0f32, Bone::UpperArmR, Bone::ForearmR, Bone::HandR),
    ] {
        let tag = if sx > 0.0 { "L" } else { "R" };
        v.push(spec(
            if sx > 0.0 { "upper arm L" } else { "upper arm R" },
            up,
            (0.305 * sx, 1.315, 0.0),
            (0.175, 0.30, 0.18),
            COAT,
        ));
        v.push(spec(
            if sx > 0.0 { "elbow disc L" } else { "elbow disc R" },
            fore,
            (0.305 * sx, 1.155, -0.055),
            (0.15, 0.11, 0.07),
            PLASTIC,
        ));
        v.push(spec(
            if sx > 0.0 { "forearm L" } else { "forearm R" },
            fore,
            (0.305 * sx, 1.035, 0.005),
            (0.15, 0.28, 0.155),
            COAT,
        ));
        v.push(spec(
            if sx > 0.0 { "glove L" } else { "glove R" },
            hand,
            (0.305 * sx, 0.845, 0.02),
            (0.12, 0.15, 0.13),
            FABRIC,
        ));
        let _ = tag;
    }
    // THE ARMBAND — left upper arm only, a thin box wrapped on the
    // arm so the three bars read from every side at distance. It is
    // a separate part ON the left arm; there is no right-arm twin.
    v.push(spec("armband (LEFT)", Bone::UpperArmL, (0.305, 1.375, 0.0), (0.21, 0.16, 0.215), ARMBAND));
    // The palm plate: a pale disc on the left hand's front — the
    // HALT signal surface. At carry it is a glove detail; raised,
    // it is the one pale thing at the top of the silhouette, the
    // way the sheet's raised palm catches the light.
    v.push(spec("palm plate L", Bone::HandL, (0.305, 0.845, 0.089), (0.085, 0.10, 0.014), MASK));
    // ---- legs: trousers under the skirt, big knee pads, tall boots ----
    for (sx, thigh, shin, foot) in [
        (1.0f32, Bone::ThighL, Bone::ShinL, Bone::FootL),
        (-1.0f32, Bone::ThighR, Bone::ShinR, Bone::FootR),
    ] {
        v.push(spec(
            if sx > 0.0 { "thigh L" } else { "thigh R" },
            thigh,
            (0.12 * sx, 0.775, 0.0),
            (0.20, 0.20, 0.22),
            FABRIC,
        ));
        v.push(spec(
            if sx > 0.0 { "thigh lower L" } else { "thigh lower R" },
            thigh,
            (0.12 * sx, 0.605, 0.0),
            (0.185, 0.17, 0.205),
            FABRIC,
        ));
        v.push(spec(
            if sx > 0.0 { "knee pad L" } else { "knee pad R" },
            shin,
            (0.12 * sx, 0.53, 0.105),
            (0.19, 0.19, 0.07),
            PLASTIC,
        ));
        v.push(spec(
            if sx > 0.0 { "knee strap L" } else { "knee strap R" },
            shin,
            (0.12 * sx, 0.44, -0.02),
            (0.20, 0.045, 0.22),
            STRAP,
        ));
        v.push(spec(
            if sx > 0.0 { "boot shaft L" } else { "boot shaft R" },
            shin,
            (0.12 * sx, 0.225, 0.0),
            (0.165, 0.27, 0.185),
            LEATHER,
        ));
        // Boot straps: three buckled bands down the shaft (canon).
        for (i, y) in [0.315, 0.225, 0.135].iter().enumerate() {
            v.push(spec(
                if sx > 0.0 { "boot strap L" } else { "boot strap R" },
                shin,
                (0.12 * sx, *y, 0.0),
                (0.175, 0.035, 0.195),
                STRAP,
            ));
            let _ = i;
        }
        v.push(spec(
            if sx > 0.0 { "boot foot L" } else { "boot foot R" },
            foot,
            (0.12 * sx, 0.055, 0.05),
            (0.155, 0.11, 0.33),
            LEATHER,
        ));
        v.push(spec(
            if sx > 0.0 { "boot sole L" } else { "boot sole R" },
            foot,
            (0.12 * sx, 0.016, 0.035),
            (0.165, 0.032, 0.36),
            PLASTIC,
        ));
    }
    // ---- the rifle, on the right hand's axis (American-pattern,
    // suppressed, carried low) ----
    let h = Bone::HandR;
    let hx = -0.305f32;
    v.push(spec("rifle stock", h, (hx, 1.00, 0.01), (0.05, 0.16, 0.075), STRAP));
    v.push(spec("rifle receiver", h, (hx, 0.845, 0.03), (0.05, 0.17, 0.07), METAL));
    v.push(spec("rifle carry handle", h, (hx, 0.905, 0.075), (0.03, 0.035, 0.10), METAL));
    v.push(spec("rifle magazine", h, (hx, 0.815, 0.085), (0.032, 0.13, 0.05), METAL));
    v.push(spec("rifle handguard", h, (hx, 0.685, 0.03), (0.045, 0.16, 0.055), STRAP));
    v.push(spec("rifle barrel", h, (hx, 0.565, 0.03), (0.024, 0.10, 0.024), METAL));
    v.push(spec("rifle suppressor", h, (hx, 0.435, 0.03), (0.05, 0.19, 0.05), PLASTIC));
    v
}

/// Build the soldier's posed, placed mesh through the shared
/// figure builder — the same seam the operator uses.
pub fn soldier_mesh(frame: &PoseFrame, spot: Vec3, ground_y: f32, yaw_deg: f32) -> OperatorMesh {
    c35::operator::build_figure_frame(&soldier_parts(), &[], frame, spot, ground_y, yaw_deg)
}

// ---------------------------------------------------------------------
// The pose vocabulary (golden #15) — states as DATA on the rig.
//
// The patrol walks UNAWARE in Slice 1; these frames exist so the
// states the perception design names are poses the rig already
// holds, provable in stills, awaiting only the state machine.
// ---------------------------------------------------------------------

/// Low ready (#15 pose 1): the patrol's alert carry — the staged
/// stand-ready angles with the head up and the torso straight.
pub fn pose_low_ready() -> PoseFrame {
    let mut f = PoseFrame::from_pose(c35::OperatorPose::StandReady);
    f.set(Bone::Head, Vec3::new(0.0, 0.0, 0.0));
    f.set(Bone::Torso, Vec3::new(1.0, 0.0, 0.0));
    f
}

/// Firing (#15 pose 2): the staged aim — rifle up on the eye line.
pub fn pose_firing() -> PoseFrame {
    PoseFrame::from_pose(c35::OperatorPose::Aim)
}

/// Kneeling aim (#15 pose 3): the aim layer over the crouch — the
/// pose graph's own layering, expressed as data: crouch frame with
/// the AIM_MASK bones taken from the aim frame.
pub fn pose_kneel_aim() -> PoseFrame {
    let mut f = PoseFrame::from_pose(c35::OperatorPose::Crouch);
    let aim = PoseFrame::from_pose(c35::OperatorPose::Aim);
    for b in c35::locomotion::AIM_MASK {
        f.set(b, aim.get(b));
    }
    f
}

/// Reloading (#15 pose 5): head down to the weapon, support hand
/// up to the magazine well — the honest vulnerability window.
pub fn pose_reload() -> PoseFrame {
    let mut f = pose_low_ready();
    f.set(Bone::Head, Vec3::new(19.0, 0.0, 0.0));
    f.set(Bone::ForearmL, Vec3::new(-78.0, 0.0, 0.0));
    f.set(Bone::UpperArmL, Vec3::new(-44.0, 16.0, -18.0));
    f.set(Bone::HandL, Vec3::new(-12.0, 0.0, 0.0));
    f
}

/// Halt (#15 pose 6): the checkpoint verb — LEFT palm raised. The
/// armband is on the arm that rises: the mark is the message.
pub fn pose_halt() -> PoseFrame {
    let mut f = PoseFrame::bind();
    // Rifle arm stays down at his side, weapon low behind (#7).
    f.set(Bone::UpperArmR, Vec3::new(2.0, 0.0, -6.0));
    f.set(Bone::ForearmR, Vec3::new(-6.0, 0.0, 0.0));
    // Left arm raises forward-up and OUT, forearm vertical, palm
    // out. The roll is deliberate: a purely frontal raise folds
    // into the torso's silhouette at night and the verb vanishes
    // (first staging) — raised clear of the body's outline, the
    // palm reads from the front and the profile alike.
    f.set(Bone::UpperArmL, Vec3::new(-38.0, 0.0, 34.0));
    f.set(Bone::ForearmL, Vec3::new(-98.0, 0.0, 0.0));
    f.set(Bone::HandL, Vec3::new(-6.0, 0.0, 0.0));
    f.set(Bone::Head, Vec3::new(2.0, 0.0, 0.0));
    f.root_y = -0.01;
    f
}

/// The vocabulary, in the action sheet's order, for the pose proof.
pub fn pose_vocabulary() -> Vec<(&'static str, PoseFrame)> {
    vec![
        ("low-ready", pose_low_ready()),
        ("firing", pose_firing()),
        ("kneel-aim", pose_kneel_aim()),
        ("reload", pose_reload()),
        ("halt", pose_halt()),
    ]
}

