//! The operator — DeBrief's protagonist as an engine citizen.
//!
//! M6: the character pipeline. The operator is built **procedurally**
//! — box-part geometry over a small skeleton, skinned rigidly per
//! bone on the CPU, surfaced from a **generated** texture atlas (no
//! image assets anywhere, as everywhere in this engine). He is the
//! banner's man, modelled from the references the operator supplied:
//! the bust portrait (`c35-stage/debrief-operator-reference.jpg`)
//! governs the face and visor, the banner (`debrief-banner.jpg`) the
//! pose and the night grade, and the full-body model sheet
//! (`c35-stage/debrief-operator-model-sheet.jpg`) governs the
//! proportions and everything below the chest — long legs, tall
//! lace-up boots, capped knee pads, the drop-leg holster rig, the
//! belt pouch row, the tall radio pack. Sealed hood with headset, a
//! bare skin band, the red visor band, balaclava, carrier vest with
//! pouches, the NSA patch on his right shoulder, a radio with a tall
//! antenna, and the HK USP in his hand — chunky low-poly in the
//! 1–3k triangle class the PS1.5 doctrine names for characters.
//!
//! Posing is rigid skinning: every part is bound to one bone, and a
//! pose is a set of joint angles over the skeleton, evaluated once
//! when the host asks for it (proof poses are stills). The seams are
//! where animation will need them — shoulders and elbows articulate
//! for aim and for a future grapple-reach; the per-frame animation
//! system itself is a later milestone (see the M6 report's forward
//! section).
//!
//! Local space: origin at the ground between the feet, +Y up, the
//! operator faces +Z, his left is +X. Placement (a world position and
//! a yaw) is applied on top by the pose evaluation, so the geometry
//! the renderer uploads is already world-space — the shader is dumb
//! on purpose.

use bytemuck::{Pod, Zeroable};
use glam::{EulerRot, Mat4, Vec3};

/// The generated atlas edge (pixels). One 256² RGBA texture carries
/// every surface the operator wears.
pub const ATLAS_SIZE: u32 = 256;

/// Material ids (the `mat` vertex attribute; the shader branches on
/// it). 0 = standard lit surface. 1 = the visor: emissive red.
pub const MAT_STANDARD: f32 = 0.0;
pub const MAT_VISOR: f32 = 1.0;

/// Where the operator stands for the proof scene: the island's
/// southern tip — the one level patch of dry shore the south-east
/// apron offers (the spot scan in the M6 working notes measured the
/// whole apron: slope everywhere else), facing out over the bay with
/// the island rising behind him when seen from the front
/// three-quarter. The feet plant at `scene::height_at` there —
/// `operator_probe` prints the height so the record can be checked.
pub const OPERATOR_SPOT: (f32, f32) = (-8.0, 64.0);
/// Yaw in degrees: rotation about +Y from +Z (the rotation maps +Z
/// to (sin θ, 0, cos θ)). 38° turns his face toward the south-east —
/// out over the bay, toward the proof cameras.
pub const OPERATOR_YAW_DEG: f32 = 38.0;

/// Camera A: the banner's angle — front three-quarter from his right
/// (the patch side), close, at his chest height, the island and the
/// headwall behind him across the bay inlet. Heights assume the
/// proof spot's terrain (0.7 at the spot — the probe prints it; the
/// cameras were framed against it).
pub const OP_CAM_A: crate::scene::CameraPose = crate::scene::CameraPose {
    eye: Vec3::new(-6.6, 2.25, 67.6),
    target: Vec3::new(-8.0, 2.05, 64.0),
    fov_y_deg: 50.0,
};
/// Camera B: his right side full-on — the patch side — profile with
/// the USP silhouette and the shore running away behind him.
pub const OP_CAM_B: crate::scene::CameraPose = crate::scene::CameraPose {
    eye: Vec3::new(-10.7, 2.1, 66.9),
    target: Vec3::new(-8.0, 1.9, 64.0),
    fov_y_deg: 50.0,
};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct OperatorVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub mat: f32,
}

// ---------------------------------------------------------------------
// The generated atlas
// ---------------------------------------------------------------------

/// Atlas regions as pixel rects (x, y, w, h) in the 256² atlas.
pub mod region {
    pub const CAMO: (u32, u32, u32, u32) = (0, 0, 128, 128);
    pub const VEST: (u32, u32, u32, u32) = (128, 0, 64, 64);
    pub const FABRIC: (u32, u32, u32, u32) = (192, 0, 64, 48);
    pub const SKIN: (u32, u32, u32, u32) = (192, 48, 64, 32);
    pub const VISOR: (u32, u32, u32, u32) = (128, 64, 128, 32);
    pub const HELMET: (u32, u32, u32, u32) = (0, 128, 64, 64);
    pub const PATCH: (u32, u32, u32, u32) = (64, 128, 64, 64);
    pub const METAL: (u32, u32, u32, u32) = (128, 128, 64, 32);
    pub const METAL_LIGHT: (u32, u32, u32, u32) = (192, 128, 64, 32);
    pub const LEATHER: (u32, u32, u32, u32) = (128, 160, 64, 32);
    pub const PLASTIC: (u32, u32, u32, u32) = (192, 160, 64, 32);
    pub const STRAP: (u32, u32, u32, u32) = (0, 192, 64, 32);
    // Slice 1 extension (DeBrief): regions painted into texels the
    // operator never used, for the second figure on this skeleton —
    // the Regime soldier. The operator's own regions are untouched
    // (his pixels are proven unchanged by frame comparison; only the
    // whole-atlas checksum moves).
    pub const COAT: (u32, u32, u32, u32) = (64, 192, 96, 64);
    pub const ARMBAND: (u32, u32, u32, u32) = (128, 96, 64, 32);
    pub const MASK: (u32, u32, u32, u32) = (160, 192, 48, 48);
    pub const REDCLOTH: (u32, u32, u32, u32) = (208, 192, 48, 32);
}

pub fn uv_rect(r: (u32, u32, u32, u32)) -> [f32; 4] {
    let s = ATLAS_SIZE as f32;
    [
        r.0 as f32 / s,
        r.1 as f32 / s,
        (r.0 + r.2) as f32 / s,
        (r.1 + r.3) as f32 / s,
    ]
}

/// Deterministic xorshift — the atlas is generated, and generation is
/// seeded, like everything else in this engine.
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Rng {
        Rng(seed | 1)
    }
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    fn range(&mut self, n: u32) -> u32 {
        self.next() % n
    }
    fn jitter(&mut self, amt: i32) -> i32 {
        (self.next() % (2 * amt as u32 + 1)) as i32 - amt
    }
}

struct Atlas {
    px: Vec<u8>,
}

impl Atlas {
    fn new() -> Atlas {
        Atlas {
            px: vec![0; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize],
        }
    }
    fn set(&mut self, x: u32, y: u32, c: [u8; 3]) {
        if x < ATLAS_SIZE && y < ATLAS_SIZE {
            let i = ((y * ATLAS_SIZE + x) * 4) as usize;
            self.px[i] = c[0];
            self.px[i + 1] = c[1];
            self.px[i + 2] = c[2];
            self.px[i + 3] = 255;
        }
    }
    fn rect(&mut self, r: (u32, u32, u32, u32), c: [u8; 3]) {
        for y in r.1..r.1 + r.3 {
            for x in r.0..r.0 + r.2 {
                self.set(x, y, c);
            }
        }
    }
    /// Fill a region with a base colour, per-pixel noise, and optional
    /// blocky blotches from a palette (the digital-camo treatment).
    fn cloth(
        &mut self,
        r: (u32, u32, u32, u32),
        base: [u8; 3],
        noise: i32,
        blotch: Option<(&[[u8; 3]], u32, u32)>,
        seed: u32,
    ) {
        let mut rng = Rng::new(seed);
        self.rect(r, base);
        if let Some((palette, count, max_sz)) = blotch {
            for _ in 0..count {
                let bx = r.0 + rng.range(r.2 - 4);
                let by = r.1 + rng.range(r.3 - 4);
                let bw = 2 + rng.range(max_sz);
                let bh = 2 + rng.range(max_sz);
                let c = palette[(rng.next() as usize) % palette.len()];
                for y in by..(by + bh).min(r.1 + r.3) {
                    for x in bx..(bx + bw).min(r.0 + r.2) {
                        self.set(x, y, c);
                    }
                }
            }
        }
        for y in r.1..r.1 + r.3 {
            for x in r.0..r.0 + r.2 {
                let i = ((y * ATLAS_SIZE + x) * 4) as usize;
                for k in 0..3 {
                    let v = self.px[i + k] as i32 + rng.jitter(noise);
                    self.px[i + k] = v.clamp(0, 255) as u8;
                }
            }
        }
    }
}

/// A 5×7 bitmap glyph, rows top to bottom, bit 4 = leftmost pixel.
fn glyph(ch: char) -> [u8; 7] {
    match ch {
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'S' => [0b01110, 0b10001, 0b10000, 0b01110, 0b00001, 0b10001, 0b01110],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        _ => [0; 7],
    }
}

/// The NSA shoulder patch, drawn as low-res surface art: tan border,
/// deep-blue field, a blocky eagle over a striped shield, "NSA" under
/// it in a bitmap hand. It is meant to read as a patch at proof
/// distance — a rectangle of allegiance, not a photograph.
fn paint_patch(a: &mut Atlas) {
    let r = region::PATCH;
    let (x0, y0) = (r.0, r.1);
    a.rect(r, [168, 148, 100]); // tan border
    a.rect((x0 + 3, y0 + 3, 58, 58), [24, 38, 78]); // field
    // Eagle: white head wedge, grey body and wing bars.
    a.rect((x0 + 28, y0 + 7, 8, 6), [226, 228, 230]); // head
    a.rect((x0 + 30, y0 + 12, 4, 10), [198, 200, 204]); // neck/body
    a.rect((x0 + 12, y0 + 10, 18, 5), [150, 152, 158]); // left wing
    a.rect((x0 + 34, y0 + 10, 18, 5), [150, 152, 158]); // right wing
    a.rect((x0 + 14, y0 + 16, 14, 4), [128, 130, 138]); // wing shade
    a.rect((x0 + 36, y0 + 16, 14, 4), [128, 130, 138]);
    // Shield on the breast: blue chief, red/white stripes.
    a.rect((x0 + 25, y0 + 20, 14, 5), [32, 52, 110]);
    for i in 0..5 {
        let c = if i % 2 == 0 { [178, 34, 40] } else { [228, 226, 220] };
        a.rect((x0 + 25 + i * 3, y0 + 25, 2, 11), c);
    }
    a.rect((x0 + 25, y0 + 36, 14, 2), [168, 148, 100]); // shield base trim
    // "NSA" in white, 5×7 glyphs at 2× scale, centred.
    let word = ['N', 'S', 'A'];
    let gw = 5 * 2 + 3;
    let start_x = x0 + 32 - (gw * 3 - 3) / 2;
    for (ci, ch) in word.iter().enumerate() {
        let g = glyph(*ch);
        for row in 0..7 {
            for col in 0..5 {
                if (g[row] >> (4 - col)) & 1 == 1 {
                    let gx = start_x + (ci as u32) * gw + col as u32 * 2;
                    let gy = y0 + 42 + row as u32 * 2;
                    a.rect((gx, gy, 2, 2), [232, 232, 228]);
                }
            }
        }
    }
}

/// The soldier's surfaces (Slice 1), painted into the atlas's
/// previously-unused texels — the greatcoat's woven checker, the
/// armband's three bars, the mask's vent slits, plain red cloth.
/// Same doctrine as the patch: surface art meant to read at patrol
/// distance.
fn paint_soldier_regions(a: &mut Atlas) {
    // COAT: near-black cloth with a fine woven checker (2px weave,
    // two close values) and noise — the greatcoat's weight is in
    // the weave, per the canon's close-up.
    let r = region::COAT;
    a.rect(r, [16, 16, 18]);
    for y in r.1..r.1 + r.3 {
        for x in r.0..r.0 + r.2 {
            if (x / 2 + y / 2) % 2 == 0 {
                a.set(x, y, [21, 21, 24]);
            }
        }
    }
    let mut rng = Rng::new(0xC0A7_0001);
    for y in r.1..r.1 + r.3 {
        for x in r.0..r.0 + r.2 {
            let i = ((y * ATLAS_SIZE + x) * 4) as usize;
            for k in 0..3 {
                let v = a.px[i + k] as i32 + rng.jitter(3);
                a.px[i + k] = v.clamp(0, 255) as u8;
            }
        }
    }
    // ARMBAND: deep-red field, three white vertical bars (Order,
    // Unity, Silence), dark edge stitching. Painted like the patch:
    // the mark must read before the man does.
    let r = region::ARMBAND;
    a.rect(r, [152, 30, 34]);
    a.rect((r.0, r.1, r.2, 2), [84, 17, 21]);
    a.rect((r.0, r.1 + r.3 - 2, r.2, 2), [84, 17, 21]);
    for i in 0..3 {
        a.rect((r.0 + 14 + i * 15, r.1 + 5, 8, r.3 - 10), [226, 224, 218]);
    }
    let mut rng = Rng::new(0xA12B_0002);
    for y in r.1..r.1 + r.3 {
        for x in r.0..r.0 + r.2 {
            let i = ((y * ATLAS_SIZE + x) * 4) as usize;
            for k in 0..3 {
                let v = a.px[i + k] as i32 + rng.jitter(4);
                a.px[i + k] = v.clamp(0, 255) as u8;
            }
        }
    }
    // MASK: pale warm grey, fine speckle, two dark vertical vent
    // slits low in the plate — the only "features" the face carries.
    let r = region::MASK;
    a.cloth(r, [168, 162, 150], 5, None, 0xAA5C_0003);
    a.rect((r.0 + 17, r.1 + 28, 4, 13), [38, 36, 33]);
    a.rect((r.0 + 27, r.1 + 28, 4, 13), [38, 36, 33]);
    // REDCLOTH: the pauldron's plain red, slightly sun-faded noise.
    a.cloth(region::REDCLOTH, [154, 34, 38], 6, None, 0xBED0_0004);
}

/// Paint the whole atlas. Deterministic: same pixels on every host,
/// every run (a proof in the operator probe compares a checksum).
pub fn generate_atlas() -> Vec<u8> {
    let mut a = Atlas::new();
    // The uniform: dark digital camo — near-black blocks with brown
    // and grey flecks, the reference's night palette.
    a.cloth(
        region::CAMO,
        [38, 39, 40],
        5,
        Some((
            &[
                [26, 27, 28],
                [52, 49, 44],
                [33, 31, 29],
                [62, 56, 46],
                [22, 23, 25],
                [44, 44, 46],
            ],
            420,
            6,
        )),
        0xCA35_0001,
    );
    // Vest: near-black fabric with webbing rows.
    a.cloth(region::VEST, [20, 21, 23], 4, None, 0xCA35_0002);
    {
        // MOLLE webbing: slightly lighter horizontal straps.
        for row in 0..8 {
            let y = region::VEST.1 + 4 + row * 8;
            a.rect((region::VEST.0, y, region::VEST.2, 3), [29, 30, 33]);
        }
    }
    a.cloth(region::FABRIC, [15, 16, 18], 4, None, 0xCA35_0003);
    // Skin band: warm mid-tone with blotches.
    a.cloth(
        region::SKIN,
        [198, 152, 116],
        6,
        Some((&[[178, 130, 94], [214, 172, 134], [188, 140, 102]], 26, 5)),
        0xCA35_0004,
    );
    // Visor: dark red gradient, hot core left of centre (the glow
    // point the banner shows), fine scanline shading.
    {
        let r = region::VISOR;
        for y in 0..r.3 {
            for x in 0..r.2 {
                let v = 1.0 - ((y as f32 / r.3 as f32) - 0.5).abs() * 2.0; // 1 mid, 0 edges
                let dx = (x as f32 - 44.0) / 26.0;
                let dy = (y as f32 - 15.0) / 9.0;
                let hot = (-(dx * dx + dy * dy)).exp();
                let scan = if y % 3 == 2 { 0.82 } else { 1.0 };
                let red = (26.0 + 96.0 * v + 150.0 * hot) * scan;
                let grn = (4.0 + 14.0 * v + 26.0 * hot) * scan;
                let blu = (5.0 + 10.0 * v + 16.0 * hot) * scan;
                a.set(
                    r.0 + x,
                    r.1 + y,
                    [
                        red.clamp(0.0, 255.0) as u8,
                        grn.clamp(0.0, 255.0) as u8,
                        blu.clamp(0.0, 255.0) as u8,
                    ],
                );
            }
        }
    }
    // Helmet/hood: matte charcoal with panel variation.
    a.cloth(
        region::HELMET,
        [30, 32, 36],
        4,
        Some((&[[38, 41, 46], [24, 26, 30]], 22, 10)),
        0xCA35_0005,
    );
    paint_patch(&mut a);
    a.cloth(region::METAL, [44, 47, 51], 4, None, 0xCA35_0006);
    a.cloth(region::METAL_LIGHT, [72, 77, 84], 4, None, 0xCA35_0007);
    a.cloth(region::LEATHER, [22, 19, 15], 5, None, 0xCA35_0008);
    a.cloth(region::PLASTIC, [12, 13, 15], 3, None, 0xCA35_0009);
    a.cloth(region::STRAP, [33, 34, 39], 4, None, 0xCA35_000A);
    paint_soldier_regions(&mut a);
    a.px
}

// ---------------------------------------------------------------------
// Skeleton and parts
// ---------------------------------------------------------------------

/// Bones, parents listed before children (the evaluation order).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Bone {
    Pelvis,
    Torso,
    Head,
    UpperArmL,
    ForearmL,
    HandL,
    UpperArmR,
    ForearmR,
    HandR,
    ThighL,
    ShinL,
    FootL,
    ThighR,
    ShinR,
    FootR,
}

impl Bone {
    pub fn idx(self) -> usize {
        self as usize
    }
    fn parent(self) -> Option<Bone> {
        use Bone::*;
        Some(match self {
            Pelvis => return None,
            Torso => Pelvis,
            Head => Torso,
            UpperArmL | UpperArmR => Torso,
            ForearmL => UpperArmL,
            ForearmR => UpperArmR,
            HandL => ForearmL,
            HandR => ForearmR,
            ThighL | ThighR => Pelvis,
            ShinL => ThighL,
            ShinR => ThighR,
            FootL => ShinL,
            FootR => ShinR,
        })
    }
    /// Joint pivot in bind-pose local space. Proportions follow the
    /// full-body model sheet (`debrief-operator-model-sheet.jpg`),
    /// which is long-legged: the hip sits at 0.88 of a ~1.87 frame,
    /// where the first build (from the bust portrait alone) had it
    /// at 0.80 of 1.79.
    pub fn pivot(self) -> Vec3 {
        use Bone::*;
        match self {
            Pelvis => Vec3::new(0.0, 1.00, 0.0),
            Torso => Vec3::new(0.0, 1.08, 0.0),
            Head => Vec3::new(0.0, 1.52, 0.0),
            UpperArmL => Vec3::new(0.27, 1.45, 0.0),
            ForearmL => Vec3::new(0.30, 1.17, 0.0),
            HandL => Vec3::new(0.30, 0.91, 0.01),
            UpperArmR => Vec3::new(-0.27, 1.45, 0.0),
            ForearmR => Vec3::new(-0.30, 1.17, 0.0),
            HandR => Vec3::new(-0.30, 0.91, 0.01),
            ThighL => Vec3::new(0.11, 0.88, 0.0),
            ShinL => Vec3::new(0.11, 0.53, 0.0),
            FootL => Vec3::new(0.11, 0.15, 0.0),
            ThighR => Vec3::new(-0.11, 0.88, 0.0),
            ShinR => Vec3::new(-0.11, 0.53, 0.0),
            FootR => Vec3::new(-0.11, 0.15, 0.0),
        }
    }
}

pub const BONE_COUNT: usize = 15;

/// One rigid part: a box (or the patch quad) in bind-pose local
/// space, bound to one bone, surfaced from one atlas region.
struct Part {
    name: &'static str,
    bone: Bone,
    center: Vec3,
    size: Vec3,
    region: (u32, u32, u32, u32),
    mat: f32,
}

fn part(
    name: &'static str,
    bone: Bone,
    c: (f32, f32, f32),
    s: (f32, f32, f32),
    region: (u32, u32, u32, u32),
    mat: f32,
) -> Part {
    Part {
        name,
        bone,
        center: Vec3::new(c.0, c.1, c.2),
        size: Vec3::new(s.0, s.1, s.2),
        region,
        mat,
    }
}

/// The part list — the operator, as geometry. Mirrored pairs are
/// written out in full so the inventory the probe prints is the
/// whole truth. Sizes are chunky on purpose (the class reads at
/// silhouette distance).
fn parts() -> Vec<Part> {
    use region::*;
    use Bone::*;
    let std = MAT_STANDARD;
    let mut v: Vec<Part> = Vec::new();
    // ---- pelvis and torso ----
    v.push(part("pelvis", Pelvis, (0.0, 0.96, 0.0), (0.36, 0.22, 0.24), CAMO, std));
    v.push(part("belt", Pelvis, (0.0, 1.065, 0.0), (0.38, 0.07, 0.26), STRAP, std));
    v.push(part("belt pouch L", Pelvis, (0.13, 1.02, -0.14), (0.10, 0.12, 0.05), VEST, std));
    v.push(part("belt pouch R", Pelvis, (-0.13, 1.02, -0.14), (0.10, 0.12, 0.05), VEST, std));
    v.push(part("canteen", Pelvis, (0.0, 1.00, -0.155), (0.11, 0.15, 0.06), VEST, std));
    v.push(part("groin flap", Pelvis, (0.0, 0.84, 0.10), (0.17, 0.14, 0.04), VEST, std));
    // The model sheet's waist: a full front row of flap pouches
    // hanging from the belt below the vest's bottom edge.
    v.push(part("waist pouch 1", Pelvis, (-0.140, 1.005, 0.130), (0.085, 0.125, 0.055), VEST, std));
    v.push(part("waist pouch 2", Pelvis, (-0.047, 1.005, 0.138), (0.085, 0.125, 0.055), VEST, std));
    v.push(part("waist pouch 3", Pelvis, (0.047, 1.005, 0.138), (0.085, 0.125, 0.055), VEST, std));
    v.push(part("waist pouch 4", Pelvis, (0.140, 1.005, 0.130), (0.085, 0.125, 0.055), VEST, std));
    v.push(part("vest torso", Torso, (0.0, 1.28, 0.0), (0.46, 0.44, 0.30), VEST, std));
    v.push(part("chest plate", Torso, (0.0, 1.32, 0.145), (0.34, 0.30, 0.03), VEST, std));
    v.push(part("back plate", Torso, (0.0, 1.32, -0.145), (0.34, 0.30, 0.03), VEST, std));
    v.push(part("collar", Torso, (0.0, 1.515, 0.01), (0.24, 0.07, 0.22), FABRIC, std));
    v.push(part("chest pouch 1", Torso, (-0.135, 1.24, 0.175), (0.11, 0.14, 0.05), VEST, std));
    v.push(part("chest pouch 2", Torso, (0.0, 1.24, 0.175), (0.11, 0.14, 0.05), VEST, std));
    v.push(part("chest pouch 3", Torso, (0.135, 1.24, 0.175), (0.11, 0.14, 0.05), VEST, std));
    v.push(part("chest pouch 4", Torso, (-0.07, 1.385, 0.172), (0.12, 0.10, 0.045), VEST, std));
    v.push(part("chest pouch 5", Torso, (0.07, 1.385, 0.172), (0.12, 0.10, 0.045), VEST, std));
    v.push(part("abdomen pouch", Torso, (0.0, 1.095, 0.165), (0.17, 0.09, 0.05), VEST, std));
    v.push(part("chest strap", Torso, (0.0, 1.415, 0.158), (0.40, 0.045, 0.028), STRAP, std));
    v.push(part("side pouch L", Torso, (0.25, 1.18, 0.02), (0.05, 0.14, 0.17), VEST, std));
    v.push(part("side pouch R", Torso, (-0.25, 1.18, 0.02), (0.05, 0.14, 0.17), VEST, std));
    // The sheet's radio pack is tall — its top clears the shoulders
    // and the mast climbs well past the head. Two whips: the tall
    // mast and a shorter one beside it, both behind his left
    // shoulder, as in both references.
    v.push(part("radio pack", Torso, (0.0, 1.30, -0.205), (0.32, 0.44, 0.11), VEST, std));
    v.push(part("radio pack pouch L", Torso, (0.11, 1.18, -0.225), (0.10, 0.16, 0.06), VEST, std));
    v.push(part("radio pack pouch R", Torso, (-0.11, 1.18, -0.225), (0.10, 0.16, 0.06), VEST, std));
    v.push(part("radio unit", Torso, (0.10, 1.44, -0.21), (0.13, 0.16, 0.13), PLASTIC, std));
    v.push(part("antenna base", Torso, (0.14, 1.555, -0.22), (0.035, 0.05, 0.035), PLASTIC, std));
    v.push(part("antenna lower", Torso, (0.14, 1.86, -0.22), (0.016, 0.56, 0.016), PLASTIC, std));
    v.push(part("antenna upper", Torso, (0.145, 2.20, -0.225), (0.011, 0.14, 0.011), PLASTIC, std));
    v.push(part("whip base", Torso, (0.05, 1.545, -0.22), (0.028, 0.04, 0.028), PLASTIC, std));
    v.push(part("whip antenna", Torso, (0.05, 1.79, -0.22), (0.010, 0.44, 0.010), PLASTIC, std));
    v.push(part("shoulder pad L", Torso, (0.285, 1.485, 0.0), (0.18, 0.09, 0.26), CAMO, std));
    v.push(part("shoulder pad R", Torso, (-0.285, 1.485, 0.0), (0.18, 0.09, 0.26), CAMO, std));
    v.push(part("shoulder strap L", Torso, (0.155, 1.53, 0.0), (0.085, 0.05, 0.34), STRAP, std));
    v.push(part("shoulder strap R", Torso, (-0.155, 1.53, 0.0), (0.085, 0.05, 0.34), STRAP, std));
    // ---- head ----
    v.push(part("neck", Head, (0.0, 1.535, 0.0), (0.13, 0.09, 0.13), FABRIC, std));
    v.push(part("hood", Head, (0.0, 1.68, 0.0), (0.27, 0.31, 0.28), HELMET, std));
    v.push(part("hood rear plate", Head, (0.0, 1.68, -0.135), (0.22, 0.24, 0.03), HELMET, std));
    v.push(part("crown plate", Head, (0.0, 1.848, 0.0), (0.20, 0.03, 0.20), HELMET, std));
    v.push(part("brow bar", Head, (0.0, 1.765, 0.125), (0.22, 0.035, 0.04), HELMET, std));
    v.push(part("skin band", Head, (0.0, 1.715, 0.130), (0.205, 0.058, 0.024), SKIN, std));
    v.push(part("visor band", Head, (0.0, 1.652, 0.134), (0.220, 0.056, 0.024), VISOR, MAT_VISOR));
    v.push(part("balaclava panel", Head, (0.0, 1.579, 0.117), (0.20, 0.095, 0.05), FABRIC, std));
    v.push(part("ear cup L", Head, (0.150, 1.68, 0.0), (0.05, 0.14, 0.15), PLASTIC, std));
    v.push(part("ear cup R", Head, (-0.150, 1.68, 0.0), (0.05, 0.14, 0.15), PLASTIC, std));
    v.push(part("headset band", Head, (0.0, 1.822, 0.0), (0.315, 0.030, 0.07), PLASTIC, std));
    v.push(part("mic boom", Head, (-0.135, 1.625, 0.075), (0.018, 0.018, 0.13), PLASTIC, std));
    v.push(part("mic tip", Head, (-0.135, 1.615, 0.135), (0.035, 0.03, 0.03), PLASTIC, std));
    // ---- arms ----
    for (sx, up, fore, hand) in [
        (1.0f32, UpperArmL, ForearmL, HandL),
        (-1.0f32, UpperArmR, ForearmR, HandR),
    ] {
        v.push(part(if sx > 0.0 { "upper arm L" } else { "upper arm R" }, up,
            (0.30 * sx, 1.315, 0.0), (0.15, 0.29, 0.155), CAMO, std));
        v.push(part(if sx > 0.0 { "elbow pad L" } else { "elbow pad R" }, fore,
            (0.30 * sx, 1.155, -0.05), (0.12, 0.09, 0.055), STRAP, std));
        v.push(part(if sx > 0.0 { "forearm L" } else { "forearm R" }, fore,
            (0.30 * sx, 1.035, 0.005), (0.13, 0.27, 0.135), CAMO, std));
        v.push(part(if sx > 0.0 { "cuff L" } else { "cuff R" }, fore,
            (0.30 * sx, 0.925, 0.008), (0.12, 0.05, 0.13), FABRIC, std));
        v.push(part(if sx > 0.0 { "glove L" } else { "glove R" }, hand,
            (0.30 * sx, 0.845, 0.02), (0.11, 0.145, 0.12), FABRIC, std));
        v.push(part(if sx > 0.0 { "knuckle plate L" } else { "knuckle plate R" }, hand,
            (0.30 * sx, 0.875, 0.058), (0.10, 0.045, 0.05), STRAP, std));
        v.push(part(if sx > 0.0 { "thumb L" } else { "thumb R" }, hand,
            (0.248 * sx, 0.825, 0.05), (0.04, 0.09, 0.045), FABRIC, std));
    }
    // ---- legs ----
    // The model sheet's legs: long, with big capped knee pads and
    // TALL lace-up boots — the shaft climbs to mid-shin, the laced
    // front panel runs its full height, the trouser blouses over the
    // boot top, and a thick sole grounds the whole figure. (The
    // first build's boots were ankle shoes; the sheet corrected
    // that, along with the proportions.)
    for (sx, thigh, shin, foot) in [
        (1.0f32, ThighL, ShinL, FootL),
        (-1.0f32, ThighR, ShinR, FootR),
    ] {
        v.push(part(if sx > 0.0 { "thigh L" } else { "thigh R" }, thigh,
            (0.115 * sx, 0.775, 0.0), (0.18, 0.19, 0.20), CAMO, std));
        v.push(part(if sx > 0.0 { "thigh lower L" } else { "thigh lower R" }, thigh,
            (0.115 * sx, 0.605, 0.0), (0.165, 0.16, 0.185), CAMO, std));
        v.push(part(if sx > 0.0 { "thigh pocket L" } else { "thigh pocket R" }, thigh,
            (0.115 * sx, 0.72, 0.108), (0.11, 0.13, 0.03), CAMO, std));
        v.push(part(if sx > 0.0 { "knee pad L" } else { "knee pad R" }, shin,
            (0.115 * sx, 0.53, 0.10), (0.165, 0.17, 0.06), PLASTIC, std));
        v.push(part(if sx > 0.0 { "knee pad cap L" } else { "knee pad cap R" }, shin,
            (0.115 * sx, 0.53, 0.128), (0.11, 0.11, 0.03), STRAP, std));
        v.push(part(if sx > 0.0 { "shin upper L" } else { "shin upper R" }, shin,
            (0.115 * sx, 0.435, 0.0), (0.16, 0.15, 0.175), CAMO, std));
        v.push(part(if sx > 0.0 { "boot top strap L" } else { "boot top strap R" }, shin,
            (0.115 * sx, 0.35, 0.0), (0.16, 0.04, 0.18), STRAP, std));
        v.push(part(if sx > 0.0 { "boot shaft L" } else { "boot shaft R" }, shin,
            (0.115 * sx, 0.225, 0.0), (0.145, 0.26, 0.16), LEATHER, std));
        v.push(part(if sx > 0.0 { "boot lace panel L" } else { "boot lace panel R" }, shin,
            (0.115 * sx, 0.24, 0.082), (0.095, 0.24, 0.025), STRAP, std));
        v.push(part(if sx > 0.0 { "boot foot L" } else { "boot foot R" }, foot,
            (0.115 * sx, 0.055, 0.045), (0.14, 0.11, 0.30), LEATHER, std));
        v.push(part(if sx > 0.0 { "boot instep L" } else { "boot instep R" }, foot,
            (0.115 * sx, 0.115, 0.09), (0.095, 0.025, 0.13), STRAP, std));
        v.push(part(if sx > 0.0 { "boot toe L" } else { "boot toe R" }, foot,
            (0.115 * sx, 0.05, 0.185), (0.13, 0.085, 0.05), LEATHER, std));
        v.push(part(if sx > 0.0 { "boot sole L" } else { "boot sole R" }, foot,
            (0.115 * sx, 0.016, 0.03), (0.15, 0.032, 0.33), PLASTIC, std));
        v.push(part(if sx > 0.0 { "boot heel L" } else { "boot heel R" }, foot,
            (0.115 * sx, 0.05, -0.105), (0.135, 0.10, 0.06), LEATHER, std));
    }
    // The sheet's drop-leg rig (his right thigh): a big holster body
    // on two straps that wrap the thigh, flap on its outer face. His
    // left thigh carries the smaller pouch on one strap.
    v.push(part("holster strap upper R", ThighR, (-0.115, 0.79, 0.0), (0.19, 0.035, 0.215), STRAP, std));
    v.push(part("holster strap lower R", ThighR, (-0.115, 0.635, 0.0), (0.19, 0.035, 0.215), STRAP, std));
    v.push(part("thigh holster R", ThighR, (-0.245, 0.70, 0.01), (0.075, 0.26, 0.18), STRAP, std));
    v.push(part("holster flap R", ThighR, (-0.29, 0.74, 0.01), (0.02, 0.16, 0.15), VEST, std));
    v.push(part("thigh strap L", ThighL, (0.115, 0.79, 0.0), (0.19, 0.035, 0.215), STRAP, std));
    v.push(part("thigh pouch L", ThighL, (0.24, 0.72, 0.01), (0.07, 0.19, 0.15), VEST, std));
    // ---- finishing mass: rails, straps, pouches — the small boxes
    // that keep silhouettes honest at silhouette distance.
    v.push(part("helmet rail L", Head, (0.145, 1.745, 0.02), (0.03, 0.035, 0.17), PLASTIC, std));
    v.push(part("helmet rail R", Head, (-0.145, 1.745, 0.02), (0.03, 0.035, 0.17), PLASTIC, std));
    v.push(part("chest radio mic", Torso, (0.155, 1.415, 0.155), (0.055, 0.09, 0.035), PLASTIC, std));
    v.push(part("forearm pocket L", ForearmL, (0.30, 1.07, 0.075), (0.10, 0.12, 0.03), CAMO, std));
    v.push(part("dump pouch", Pelvis, (-0.10, 0.94, -0.15), (0.13, 0.13, 0.06), VEST, std));
    v.push(part("drop pouch", Pelvis, (0.145, 0.88, 0.10), (0.10, 0.12, 0.05), VEST, std));
    // ---- the HK USP, in the right hand ----
    // Modelled along the hand's -Y axis (muzzle down at bind, like a
    // pistol held at rest); the arm chain's pitch raises it to the
    // ready and the aim (see the pose tables).
    let h = HandR;
    let hx = -0.30f32;
    v.push(part("USP grip", h, (hx, 0.835, 0.03), (0.042, 0.115, 0.055), STRAP, std));
    v.push(part("USP mag base", h, (hx, 0.775, 0.032), (0.046, 0.02, 0.058), METAL, std));
    v.push(part("USP frame", h, (hx, 0.895, 0.045), (0.040, 0.05, 0.16), METAL, std));
    v.push(part("USP slide", h, (hx, 0.925, 0.045), (0.036, 0.028, 0.185), METAL_LIGHT, std));
    v.push(part("USP barrel tip", h, (hx, 0.918, 0.135), (0.020, 0.020, 0.03), METAL, std));
    v.push(part("USP trigger guard", h, (hx, 0.855, 0.085), (0.012, 0.045, 0.07), METAL, std));
    v.push(part("USP rear sight", h, (hx, 0.948, -0.028), (0.026, 0.016, 0.02), METAL, std));
    v.push(part("USP front sight", h, (hx, 0.946, 0.118), (0.010, 0.014, 0.012), METAL, std));
    v.push(part("USP hammer", h, (hx, 0.925, -0.048), (0.014, 0.03, 0.018), METAL, std));
    v
}

/// The patch is a quad, not a box: its art has a reading direction,
/// and only a placed quad controls which way is up and which way the
/// eagle faces. It rides the right upper arm's outer face.
fn patch_quad() -> ([Vec3; 4], Bone) {
    // On the right upper arm's outer face (arm centre −0.30, half
    // width 0.075). The sheet's patch is big — it covers most of the
    // arm's width — so the quad is 0.125 × 0.135.
    let x = -0.377;
    (
        [
            Vec3::new(x, 1.3975, 0.0625),  // top, toward his front
            Vec3::new(x, 1.3975, -0.0625), // top, toward his back
            Vec3::new(x, 1.2625, -0.0625),
            Vec3::new(x, 1.2625, 0.0625),
        ],
        Bone::UpperArmR,
    )
}

// ---------------------------------------------------------------------
// Poses
// ---------------------------------------------------------------------

/// The three staged poses the brief names.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OperatorPose {
    StandReady,
    Crouch,
    Aim,
}

impl OperatorPose {
    pub fn describe(&self) -> &'static str {
        match self {
            OperatorPose::StandReady => "stand-ready (USP low, two hands)",
            OperatorPose::Crouch => "crouch (kneeling, USP low)",
            OperatorPose::Aim => "aim (USP up, eye line)",
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            OperatorPose::StandReady => "stand",
            OperatorPose::Crouch => "crouch",
            OperatorPose::Aim => "aim",
        }
    }
}

/// A pose: root drop (world units) plus per-bone Euler angles in
/// degrees (YXZ order; +X pitches the limb's top forward, and for a
/// hanging limb -X swings it forward — the pose tables are written
/// against the renders, and the M6 report says so).
struct PoseDef {
    root_drop: f32,
    angles: [(Bone, Vec3); 12],
}

fn pose_def(pose: OperatorPose) -> PoseDef {
    use Bone::*;
    let d = Vec3::new;
    match pose {
        OperatorPose::StandReady => PoseDef {
            root_drop: 0.01,
            angles: [
                (Pelvis, d(3.0, 0.0, 0.0)),
                (Torso, d(4.0, 0.0, 0.0)),
                (Head, d(5.0, 0.0, 0.0)),
                (UpperArmR, d(-38.0, -6.0, 14.0)),
                (ForearmR, d(-44.0, 0.0, 0.0)),
                (HandR, d(-10.0, 0.0, 0.0)),
                (UpperArmL, d(-30.0, 10.0, -22.0)),
                (ForearmL, d(-48.0, 0.0, 0.0)),
                (HandL, d(-4.0, 0.0, 0.0)),
                (ThighL, d(-7.0, 0.0, 0.0)),
                (ShinL, d(9.0, 0.0, 0.0)),
                (ThighR, d(4.0, 0.0, 0.0)),
            ],
        },
        OperatorPose::Aim => PoseDef {
            root_drop: 0.03,
            angles: [
                (Pelvis, d(3.0, 0.0, 0.0)),
                (Torso, d(4.0, -7.0, 0.0)),
                (Head, d(2.0, 4.0, 0.0)),
                (UpperArmR, d(-63.0, -4.0, 10.0)),
                (ForearmR, d(-29.0, 0.0, 0.0)),
                (HandR, d(2.0, 0.0, 0.0)),
                (UpperArmL, d(-50.0, 14.0, -30.0)),
                (ForearmL, d(-42.0, 0.0, 0.0)),
                (HandL, d(0.0, 0.0, 0.0)),
                (ThighL, d(-13.0, 0.0, 0.0)),
                (ShinL, d(11.0, 0.0, 0.0)),
                (ThighR, d(8.0, 0.0, 0.0)),
            ],
        },
        OperatorPose::Crouch => PoseDef {
            root_drop: 0.50,
            angles: [
                (Pelvis, d(9.0, 0.0, 0.0)),
                (Torso, d(15.0, 0.0, 0.0)),
                (Head, d(4.0, 0.0, 0.0)),
                (UpperArmR, d(-30.0, -6.0, 13.0)),
                (ForearmR, d(-30.0, 0.0, 0.0)),
                (HandR, d(-6.0, 0.0, 0.0)),
                (UpperArmL, d(-24.0, 10.0, -19.0)),
                (ForearmL, d(-36.0, 0.0, 0.0)),
                (HandL, d(-4.0, 0.0, 0.0)),
                (ThighL, d(-56.0, 0.0, 0.0)),
                (ShinL, d(52.0, 0.0, 0.0)),
                (ThighR, d(6.0, 0.0, 0.0)),
            ],
        },
    }
}

/// Extra shin fold for the crouch's kneeling leg, applied outside the
/// table (it is the pose's defining fold, kept visible in code).
fn extra_fold(pose: OperatorPose) -> Option<(Bone, Vec3)> {
    match pose {
        OperatorPose::Crouch => Some((Bone::ShinR, Vec3::new(92.0, 0.0, 0.0))),
        _ => None,
    }
}

// ---------------------------------------------------------------------
// The pose interface (M7)
// ---------------------------------------------------------------------

/// A pose as pure skeleton data: per-bone Euler angles in degrees
/// (YXZ order, the pose tables' convention) plus the root's vertical
/// offset from the standing origin (negative = dropped: crouch, gait
/// bob, slide). **This is the interface between posing and geometry.**
/// Staged poses, the locomotion system ([`crate::locomotion`]), and
/// any future mesh built on this skeleton all meet here: joint
/// transforms in, posed geometry out. Nothing downstream of this
/// type may know which mesh consumes it, and nothing upstream may
/// know the mesh's vertices or parts.
#[derive(Copy, Clone, Debug)]
pub struct PoseFrame {
    pub angles: [Vec3; BONE_COUNT],
    pub root_y: f32,
}

impl PoseFrame {
    /// The zero pose: bind angles, standing root.
    pub fn bind() -> PoseFrame {
        PoseFrame {
            angles: [Vec3::ZERO; BONE_COUNT],
            root_y: 0.0,
        }
    }

    /// A staged pose ([`OperatorPose`]) expressed through the
    /// interface — the same angle set and root drop the M6 build
    /// applied, so geometry built from it is unchanged.
    pub fn from_pose(pose: OperatorPose) -> PoseFrame {
        let def = pose_def(pose);
        let mut angles = [Vec3::ZERO; BONE_COUNT];
        for (b, a) in def.angles {
            angles[b.idx()] = a;
        }
        if let Some((b, a)) = extra_fold(pose) {
            angles[b.idx()] += a;
        }
        PoseFrame {
            angles,
            root_y: -def.root_drop,
        }
    }

    /// Angle-wise blend toward another frame (shortest-path is not
    /// needed: every angle in play stays well inside ±180°).
    pub fn lerp(&self, other: &PoseFrame, t: f32) -> PoseFrame {
        let mut angles = [Vec3::ZERO; BONE_COUNT];
        for i in 0..BONE_COUNT {
            angles[i] = self.angles[i].lerp(other.angles[i], t);
        }
        PoseFrame {
            angles,
            root_y: self.root_y + (other.root_y - self.root_y) * t,
        }
    }

    pub fn set(&mut self, bone: Bone, angles: Vec3) {
        self.angles[bone.idx()] = angles;
    }

    pub fn add(&mut self, bone: Bone, delta: Vec3) {
        self.angles[bone.idx()] += delta;
    }

    pub fn get(&self, bone: Bone) -> Vec3 {
        self.angles[bone.idx()]
    }
}

/// The root placement matrix shared by the geometry build and the
/// kinematics helpers, so a measured foot and a drawn foot can never
/// disagree about where the figure stands: origin at the spot, the
/// terrain height plus the frame's root offset, yawed to facing.
pub fn root_matrix(spot: Vec3, ground_y: f32, frame: &PoseFrame, yaw_deg: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(spot.x, ground_y + frame.root_y, spot.z))
        * Mat4::from_rotation_y(yaw_deg.to_radians())
}

/// Every bone's pivot in operator-local space for a frame — the
/// forward kinematics of the skeleton itself, no mesh involved.
/// Measurement (foot speed, joint ranges) reads this.
pub fn joint_positions(frame: &PoseFrame) -> [Vec3; BONE_COUNT] {
    let mats = bone_matrices(frame);
    let mut out = [Vec3::ZERO; BONE_COUNT];
    for (i, slot) in out.iter_mut().enumerate() {
        let bone = bone_from_idx(i);
        *slot = mats[i].transform_point3(bone.pivot());
    }
    out
}

/// A point rigidly attached to a bone (given in bind-pose local
/// coordinates), evaluated in operator-local space for a frame.
pub fn bone_point(frame: &PoseFrame, bone: Bone, bind_point: Vec3) -> Vec3 {
    bone_matrices(frame)[bone.idx()].transform_point3(bind_point)
}

/// The bind-pose centre of the boot sole, in skeleton coordinates:
/// directly under the ankle pivot (which stands 0.15 above the sole
/// plane) and a touch forward — where the boot meets the ground.
/// Skeleton data: any mesh skinned to this skeleton plants here.
pub fn sole_bind_point(left: bool) -> Vec3 {
    let x = if left { 0.11 } else { -0.11 };
    Vec3::new(x, 0.0, 0.03)
}

/// One foot's kinematics in operator-local space: the ankle pivot
/// and the sole centre, both from the skeleton alone.
#[derive(Copy, Clone, Debug)]
pub struct FootKinematics {
    pub ankle: Vec3,
    pub sole: Vec3,
}

pub fn foot_kinematics(frame: &PoseFrame, left: bool) -> FootKinematics {
    let mats = bone_matrices(frame);
    let (foot, ankle_pivot) = if left {
        (Bone::FootL, Bone::FootL.pivot())
    } else {
        (Bone::FootR, Bone::FootR.pivot())
    };
    FootKinematics {
        ankle: mats[foot.idx()].transform_point3(ankle_pivot),
        sole: mats[foot.idx()].transform_point3(sole_bind_point(left)),
    }
}

// ---------------------------------------------------------------------
// Geometry build
// ---------------------------------------------------------------------

/// Cube faces: (outward normal, four corners as signs of the half
/// extents, ordered bottom-left → bottom-right → top-right → top-left
/// as seen from outside, so (0,1,2),(0,2,3) winds CCW outward).
const FACES: [([f32; 3], [[f32; 3]; 4]); 6] = [
    // +Z (front)
    (
        [0.0, 0.0, 1.0],
        [
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
        ],
    ),
    // -Z (back)
    (
        [0.0, 0.0, -1.0],
        [
            [1.0, -1.0, -1.0],
            [-1.0, -1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [1.0, 1.0, -1.0],
        ],
    ),
    // +X (his left)
    (
        [1.0, 0.0, 0.0],
        [
            [1.0, -1.0, 1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [1.0, 1.0, 1.0],
        ],
    ),
    // -X (his right)
    (
        [-1.0, 0.0, 0.0],
        [
            [-1.0, -1.0, -1.0],
            [-1.0, -1.0, 1.0],
            [-1.0, 1.0, 1.0],
            [-1.0, 1.0, -1.0],
        ],
    ),
    // +Y (top): screen-up is -Z when viewed from above
    (
        [0.0, 1.0, 0.0],
        [
            [-1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
        ],
    ),
    // -Y (bottom)
    (
        [0.0, -1.0, 0.0],
        [
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, -1.0, 1.0],
            [-1.0, -1.0, 1.0],
        ],
    ),
];

/// One part's inventory line, for the probe and the report.
pub struct PartInfo {
    pub name: &'static str,
    pub bone: Bone,
    pub triangles: u32,
}

pub struct OperatorMesh {
    pub vertices: Vec<OperatorVertex>,
    pub indices: Vec<u32>,
    pub parts: Vec<PartInfo>,
}

impl OperatorMesh {
    pub fn triangle_count(&self) -> u64 {
        (self.indices.len() / 3) as u64
    }
}

/// Evaluate the skeleton for a frame: each bone's world matrix (in
/// operator-local space) from the chain of pivot rotations. Public
/// because solvers above the skeleton (the locomotion IK) work in
/// parent-bone space and need the chain's own transforms — the
/// same matrices the measurement helpers read.
pub fn bone_matrices(frame: &PoseFrame) -> [Mat4; BONE_COUNT] {
    let angles = frame.angles;
    let mut mats = [Mat4::IDENTITY; BONE_COUNT];
    // Bones are declared parents-first; a single pass in declaration
    // order evaluates the whole chain.
    for i in 0..BONE_COUNT {
        let bone = bone_from_idx(i);
        let a = angles[i];
        let rot = Mat4::from_euler(
            EulerRot::YXZ,
            a.y.to_radians(),
            a.x.to_radians(),
            a.z.to_radians(),
        );
        let pivot = bone.pivot();
        let local = Mat4::from_translation(pivot) * rot * Mat4::from_translation(-pivot);
        mats[i] = match bone.parent() {
            Some(p) => mats[p.idx()] * local,
            None => local,
        };
    }
    mats
}

fn bone_from_idx(i: usize) -> Bone {
    use Bone::*;
    match i {
        0 => Pelvis,
        1 => Torso,
        2 => Head,
        3 => UpperArmL,
        4 => ForearmL,
        5 => HandL,
        6 => UpperArmR,
        7 => ForearmR,
        8 => HandR,
        9 => ThighL,
        10 => ShinL,
        11 => FootL,
        12 => ThighR,
        13 => ShinR,
        _ => FootR,
    }
}

/// Build the posed, placed operator mesh from a staged pose: the
/// pose is expressed through the [`PoseFrame`] interface and built
/// by [`build_operator_frame`] — one path for staged and animated
/// geometry alike.
pub fn build_operator(
    pose: OperatorPose,
    spot: Vec3,
    ground_y: f32,
    yaw_deg: f32,
) -> OperatorMesh {
    build_operator_frame(&PoseFrame::from_pose(pose), spot, ground_y, yaw_deg)
}

/// Build the posed, placed operator mesh from a raw pose frame:
/// skinning (rigid, per bone) then placement (position + yaw + the
/// frame's root offset). The feet plant at `ground_y` — the caller
/// reads the terrain height.
///
/// The operator is one figure on the part system; this builds him
/// through [`build_figure_frame`], the shared figure builder every
/// character on this skeleton uses.
pub fn build_operator_frame(
    frame: &PoseFrame,
    spot: Vec3,
    ground_y: f32,
    yaw_deg: f32,
) -> OperatorMesh {
    let specs: Vec<PartSpec> = parts()
        .iter()
        .map(|p| PartSpec {
            name: p.name,
            bone: p.bone,
            center: p.center,
            size: p.size,
            uv: uv_rect(p.region),
            mat: p.mat,
        })
        .collect();
    let (corners, bone) = patch_quad();
    let uv = uv_rect(region::PATCH);
    let patch = QuadSpec {
        name: "NSA shoulder patch (quad)",
        bone,
        corners,
        normal: Vec3::new(-1.0, 0.0, 0.0),
        // Corners are listed TL(front), TL(back), BL(back), BL(front);
        // art-left is toward his front (+Z local).
        uvs: [[uv[0], uv[1]], [uv[2], uv[1]], [uv[2], uv[3]], [uv[0], uv[3]]],
        mat: MAT_STANDARD,
    };
    build_figure_frame(&specs, &[patch], frame, spot, ground_y, yaw_deg)
}

/// A figure part, as data: one box in bind-pose local space, bound
/// to one bone, surfaced from one precomputed atlas rect. This is
/// the public form of the operator's private part list — the seam
/// through which OTHER figures are built on the same skeleton (the
/// DeBrief soldier is the first): same bones, same PoseFrame in,
/// same OperatorMesh out.
pub struct PartSpec {
    pub name: &'static str,
    pub bone: Bone,
    pub center: Vec3,
    pub size: Vec3,
    pub uv: [f32; 4],
    pub mat: f32,
}

/// A figure quad, as data: four bind-pose corners on one bone with
/// per-corner UVs and a bind-pose normal (the shoulder patch is the
/// operator's instance; insignia and marks are the general case).
pub struct QuadSpec {
    pub name: &'static str,
    pub bone: Bone,
    pub corners: [Vec3; 4],
    pub normal: Vec3,
    pub uvs: [[f32; 2]; 4],
    pub mat: f32,
}

/// Build any figure on the shared skeleton from part/quad data and
/// a pose frame: rigid per-bone skinning, then placement — the
/// exact emission path the operator has always used (his staged
/// frames hash-compare against M6/M7 to prove it).
pub fn build_figure_frame(
    parts: &[PartSpec],
    quads: &[QuadSpec],
    frame: &PoseFrame,
    spot: Vec3,
    ground_y: f32,
    yaw_deg: f32,
) -> OperatorMesh {
    let mats = bone_matrices(frame);
    let root = root_matrix(spot, ground_y, frame, yaw_deg);

    let mut vertices: Vec<OperatorVertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut infos: Vec<PartInfo> = Vec::new();

    for p in parts {
        let m = root * mats[p.bone.idx()];
        let uv = p.uv;
        let half = p.size * 0.5;
        let base = vertices.len() as u32;
        for (n, corners) in FACES {
            let nrm = m
                .transform_vector3(Vec3::from(n))
                .normalize()
                .to_array();
            // UV corners follow the face's view order: BL, BR, TR, TL
            // with v growing downward in the authored atlas.
            let face_uv = [
                [uv[0], uv[3]],
                [uv[2], uv[3]],
                [uv[2], uv[1]],
                [uv[0], uv[1]],
            ];
            for (ci, c) in corners.iter().enumerate() {
                let local = Vec3::new(
                    p.center.x + c[0] * half.x,
                    p.center.y + c[1] * half.y,
                    p.center.z + c[2] * half.z,
                );
                let world = m.transform_point3(local);
                vertices.push(OperatorVertex {
                    position: world.to_array(),
                    normal: nrm,
                    uv: face_uv[ci],
                    mat: p.mat,
                });
            }
            let f = base + (vertices.len() as u32 - base - 4);
            indices.extend_from_slice(&[f, f + 1, f + 2, f, f + 2, f + 3]);
        }
        infos.push(PartInfo {
            name: p.name,
            bone: p.bone,
            triangles: 12,
        });
    }

    for q in quads {
        let m = root * mats[q.bone.idx()];
        let nrm = m
            .transform_vector3(q.normal)
            .normalize()
            .to_array();
        let base = vertices.len() as u32;
        for (ci, c) in q.corners.iter().enumerate() {
            let world = m.transform_point3(*c);
            vertices.push(OperatorVertex {
                position: world.to_array(),
                normal: nrm,
                uv: q.uvs[ci],
                mat: q.mat,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        infos.push(PartInfo {
            name: q.name,
            bone: q.bone,
            triangles: 2,
        });
    }

    OperatorMesh {
        vertices,
        indices,
        parts: infos,
    }
}
