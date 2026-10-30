//! The operator — DeBrief's protagonist as an engine citizen.
//!
//! M6: the character pipeline. The operator is built **procedurally**
//! — box-part geometry over a small skeleton, skinned rigidly per
//! bone on the CPU, surfaced from a **generated** texture atlas (no
//! image assets anywhere, as everywhere in this engine).
//!
//! Operator perfection, phase 1 (the STREET configuration): the
//! figure is modelled from the reboot references the operator
//! supplied — `c35-stage/debrief-operator-reboot-reference.jpg`
//! (#31, front) and `c35-stage/debrief-operator-reboot-rear-reference.jpg`
//! (#32, rear). **The rear is the primary angle** — it is the
//! most-seen view in third person, and the build is judged
//! rear-first. The kit map is exact: waxed-canvas coat worn pale at
//! the strap lines; radio pack riding HIGH on the LEFT of the back
//! with TWO antenna whips; plain back panel; THREE rear belt
//! pouches with brass buckles; one re-sewn pack strap; the long gun
//! slung under the pack; the USP in a worn leather holster on the
//! right hip (and in his hand in the staged poses, as always); face
//! weathered, nose once broken, grey at the temples; the watch worn
//! FACE-INWARD on the left wrist. No insignia, no labels, nothing
//! that emits light. The M6 build's assault modelling (sealed hood,
//! red visor, carrier, NSA patch) is superseded for this figure —
//! the assault configuration returns as its own build in phase 2.
//! The skeleton, the pose interface, and the proportions' frame
//! (long legs, tall lace-up boots) are unchanged from M6/M7 —
//! chunky low-poly in the 1–3k triangle class the PS1.5 doctrine
//! names for characters.
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
/// it). 0 = standard lit surface. 1 = emissive (the M6 visor's id;
/// no part of the street figure uses it — nothing on the man emits).
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
/// (the gun side), close, at his chest height, the island and the
/// headwall behind him across the bay inlet. Heights assume the
/// proof spot's terrain (0.7 at the spot — the probe prints it; the
/// cameras were framed against it).
pub const OP_CAM_A: crate::scene::CameraPose = crate::scene::CameraPose {
    eye: Vec3::new(-6.6, 2.25, 67.6),
    target: Vec3::new(-8.0, 2.05, 64.0),
    fov_y_deg: 50.0,
};
/// Camera B: his right side full-on — the gun side — profile with
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
///
/// Ownership law, proven by the soldier's solo frames: the regions
/// the Regime soldier samples (VEST, FABRIC, HELMET, METAL, LEATHER,
/// PLASTIC, STRAP, COAT, ARMBAND, MASK, REDCLOTH) are FROZEN — their
/// rects and their paint never change in an operator pass, or his
/// pixels move. The street pass works only in operator-only regions
/// (WAXED, SKIN, TROUSER, FACE, METAL_LIGHT) and in texels no figure
/// had ever painted (WEBBING, POUCH, RADIO, HAIR).
pub mod region {
    /// The street coat's waxed canvas (the M6 build's CAMO rect,
    /// repainted — the soldier never sampled it).
    pub const WAXED: (u32, u32, u32, u32) = (0, 0, 128, 128);
    pub const VEST: (u32, u32, u32, u32) = (128, 0, 64, 64);
    pub const FABRIC: (u32, u32, u32, u32) = (192, 0, 64, 48);
    pub const SKIN: (u32, u32, u32, u32) = (192, 48, 64, 32);
    /// Trousers / the dark knit under the coat (the M6 build's VISOR
    /// rect, repainted — the soldier never sampled it).
    pub const TROUSER: (u32, u32, u32, u32) = (128, 64, 128, 32);
    pub const HELMET: (u32, u32, u32, u32) = (0, 128, 64, 64);
    /// The face, painted like the M6 patch was — surface art meant
    /// to read at proof distance (the M6 build's PATCH rect,
    /// repainted; the patch itself is deleted — no labels on him).
    pub const FACE: (u32, u32, u32, u32) = (64, 128, 64, 64);
    pub const METAL: (u32, u32, u32, u32) = (128, 128, 64, 32);
    /// Brass — buckles and clasps (the M6 build's light-metal rect,
    /// repainted warm; the soldier never sampled it).
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
    // Operator-perfection extension: the street figure's own
    // materials, painted into texels that were zero in every prior
    // build (the soldier's regions and pixels are untouched).
    /// Olive webbing — straps, belt, flaps.
    pub const WEBBING: (u32, u32, u32, u32) = (192, 96, 64, 32);
    /// Olive canvas — the belt pouches.
    pub const POUCH: (u32, u32, u32, u32) = (0, 224, 64, 32);
    /// The radio's dark case — panel seams, a speaker grille.
    pub const RADIO: (u32, u32, u32, u32) = (208, 224, 48, 32);
    /// Hair — dark brown gone grey at the temples.
    pub const HAIR: (u32, u32, u32, u32) = (160, 240, 48, 16);
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
    /// Return a rect to the atlas's unpainted state (all zero) —
    /// the border guards use it where a soldier region's filtered
    /// samples used to meet empty texels.
    fn clear_rect(&mut self, r: (u32, u32, u32, u32)) {
        for y in r.1..(r.1 + r.3).min(ATLAS_SIZE) {
            for x in r.0..(r.0 + r.2).min(ATLAS_SIZE) {
                let i = ((y * ATLAS_SIZE + x) * 4) as usize;
                self.px[i] = 0;
                self.px[i + 1] = 0;
                self.px[i + 2] = 0;
                self.px[i + 3] = 0;
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

/// The face, drawn as low-res surface art in the deleted patch's
/// rect — the same doctrine the patch was painted under: meant to
/// read at proof distance, not to be a photograph. Weathered skin,
/// a brow shadow, eyes, the once-broken nose carried in shading
/// (the box nose gives it silhouette), a mouth line, stubble on the
/// jaw, and the forehead scar from #31. Region-left is his RIGHT
/// (the face quad's corner order puts the region's left edge on his
/// right cheek), so the scar sits at region-left, where #31 wears
/// it. Painted top = hairline, bottom = chin.
fn paint_face(a: &mut Atlas) {
    let r = region::FACE;
    let (x0, y0) = (r.0, r.1);
    a.cloth(
        r,
        [198, 152, 116],
        5,
        Some((&[[178, 130, 94], [214, 172, 134], [188, 140, 102]], 30, 5)),
        0xCA35_0013,
    );
    let put = |a: &mut Atlas, x: u32, y: u32, w: u32, h: u32, c: [u8; 3]| {
        a.rect((x0 + x, y0 + y, w, h), c);
    };
    // Brow ridge shadow, then the brows themselves.
    put(a, 6, 13, 52, 4, [176, 132, 96]);
    put(a, 10, 17, 15, 3, [74, 56, 40]);
    put(a, 39, 17, 15, 3, [74, 56, 40]);
    // Eyes: dark slots, with the tired shade under them.
    put(a, 12, 22, 11, 4, [40, 30, 25]);
    put(a, 41, 22, 11, 4, [40, 30, 25]);
    put(a, 11, 27, 13, 2, [168, 124, 88]);
    put(a, 40, 27, 13, 2, [168, 124, 88]);
    // The nose in shading: bridge highlight, side shadows, the
    // nostril shadow at its base.
    put(a, 30, 28, 4, 10, [208, 164, 124]);
    put(a, 28, 28, 2, 12, [172, 126, 90]);
    put(a, 35, 28, 2, 12, [172, 126, 90]);
    put(a, 27, 41, 11, 3, [146, 104, 72]);
    // Weathered cheeks.
    put(a, 8, 34, 8, 6, [170, 124, 88]);
    put(a, 48, 34, 8, 6, [170, 124, 88]);
    // Mouth: a line, and the lower lip's hint.
    put(a, 24, 50, 17, 3, [104, 70, 54]);
    put(a, 27, 54, 11, 2, [172, 122, 88]);
    // The forehead scar (#31): a pale diagonal, region-left.
    for i in 0..6u32 {
        put(a, 11 + i, 4 + i, 3, 2, [230, 200, 172]);
    }
    // Stubble: speckle over the jaw, denser at the chin band.
    let mut rng = Rng::new(0xCA35_0014);
    for y in 42..64u32 {
        for x in 4..60u32 {
            let density = if y >= 58 { 3 } else { 2 };
            if rng.range(density + 2) < density {
                a.set(x0 + x, y0 + y, [98, 76, 58]);
            }
        }
    }
    // The border frame: the deleted patch's tan edge, kept — the
    // soldier's regions border this rect on three sides, and their
    // filtered samples have met this tan since M6 (see
    // paint_border_guards). On the face it reads as edge shading.
    put(a, 0, 0, 64, 3, [168, 148, 100]);
    put(a, 0, 61, 64, 3, [168, 148, 100]);
    put(a, 0, 0, 3, 64, [168, 148, 100]);
    put(a, 61, 0, 3, 64, [168, 148, 100]);
}

/// The street figure's own materials (operator perfection, phase
/// 1), painted into operator-only regions and previously-zero
/// texels. The value ladder is the point — it is what survives the
/// night grade and the PS1.5 palette: coat lightest of the cloths,
/// pouches and webbing a green step down, trousers dark, the radio
/// darkest, brass the one bright accent, skin above all.
fn paint_street_regions(a: &mut Atlas) {
    // WAXED: the coat's canvas — mottled browns, a pale wear band
    // where the straps and belt rub (every coat face carries it at
    // the same height, the way real wear repeats), grime at the hem.
    let r = region::WAXED;
    a.cloth(
        r,
        [104, 86, 57],
        6,
        Some((
            &[[86, 68, 42], [120, 100, 66], [94, 76, 48], [132, 110, 74], [76, 60, 38]],
            520,
            7,
        )),
        0xCA35_0011,
    );
    {
        let mut rng = Rng::new(0xCA35_0015);
        // Pale wear band, broken like rubbed cloth.
        for y in (r.1 + 34)..(r.1 + 40) {
            for x in r.0..(r.0 + r.2) {
                if rng.range(4) < 3 {
                    a.set(x, y, [122, 104, 72]);
                }
            }
        }
        // Hem grime, two deepening bands.
        for x in r.0..(r.0 + r.2) {
            for y in (r.1 + 112)..(r.1 + 120) {
                if rng.range(5) < 4 {
                    a.set(x, y, [84, 66, 42]);
                }
            }
            for y in (r.1 + 120)..(r.1 + 128) {
                if rng.range(5) < 4 {
                    a.set(x, y, [72, 56, 36]);
                }
            }
        }
    }
    // TROUSER: charcoal-brown cloth, quiet.
    a.cloth(
        region::TROUSER,
        [45, 40, 36],
        5,
        Some((&[[37, 33, 29], [54, 48, 42], [41, 36, 31]], 260, 6)),
        0xCA35_0012,
    );
    // WEBBING: olive drab with woven rows.
    a.cloth(region::WEBBING, [76, 74, 50], 4, None, 0xCA35_0016);
    {
        let r = region::WEBBING;
        for row in 0..8 {
            let y = r.1 + row * 4;
            a.rect((r.0, y, r.2, 2), [84, 82, 57]);
        }
    }
    // POUCH: olive canvas, a step lighter than the webbing.
    a.cloth(
        region::POUCH,
        [93, 89, 61],
        5,
        Some((&[[83, 79, 53], [104, 100, 70]], 60, 5)),
        0xCA35_0017,
    );
    // RADIO: near-black case, a speaker grille's dot field, panel
    // seams, one lighter dial plate.
    {
        let r = region::RADIO;
        a.cloth(r, [30, 32, 30], 3, None, 0xCA35_0018);
        for y in (r.1 + 5)..(r.1 + 19) {
            for x in (r.0 + 5)..(r.0 + 25) {
                if (x + y) % 3 == 0 {
                    a.set(x, y, [17, 18, 17]);
                }
            }
        }
        a.rect((r.0 + 33, r.1 + 6, 11, 9), [52, 54, 50]);
        a.rect((r.0, r.1 + 22, r.2, 2), [21, 22, 21]);
        a.rect((r.0, r.1 + 27, r.2, 2), [21, 22, 21]);
    }
    // HAIR: dark brown, salt-and-pepper throughout and greyer at
    // the region's edges — the temples, when the cap wraps.
    {
        let r = region::HAIR;
        a.cloth(
            r,
            [54, 42, 32],
            5,
            Some((&[[44, 34, 25], [66, 52, 40]], 40, 4)),
            0xCA35_0019,
        );
        let mut rng = Rng::new(0xCA35_001A);
        for y in r.1..(r.1 + r.3) {
            for x in r.0..(r.0 + r.2) {
                let lx = x - r.0;
                let temple = lx < 9 || lx >= r.2 - 9;
                let p = if temple { 3 } else { 9 };
                if rng.range(p) == 0 {
                    a.set(x, y, [148, 144, 136]);
                }
            }
        }
    }
    // BRASS (the METAL_LIGHT rect): warm metal, a bright top band —
    // the buckle highlight.
    a.cloth(region::METAL_LIGHT, [150, 118, 60], 6, None, 0xCA35_001B);
    {
        let r = region::METAL_LIGHT;
        a.rect((r.0, r.1, r.2, 5), [172, 140, 76]);
    }
}

/// Border guards, painted LAST in the atlas. The two figures share
/// one texture, and filtered sampling blends across region borders
/// — at glancing angles whole mip levels join in. The soldier's
/// renders were proven byte-identical against neighbours that were
/// dark cloth, a dark-red gradient edge, a tan patch frame, or
/// outright zero; this pass restores those exact border conditions
/// around the street figure's brighter paints, so his pixels cannot
/// move. Each guard is a few texels of the street regions' own
/// edges, where they read as seams, rims, and shadow lines.
fn paint_border_guards(a: &mut Atlas) {
    let dark: [u8; 3] = [38, 39, 40]; // the old camo's value
    // WAXED's bottom and right edges met HELMET / VEST / ARMBAND
    // when they were dark camo; keep the meeting dark.
    let r = region::WAXED;
    a.rect((r.0, r.1 + r.3 - 3, r.2, 3), dark);
    a.rect((r.0 + r.2 - 3, r.1, 3, r.3), dark);
    // TROUSER's top and bottom edges met VEST / ARMBAND when they
    // were the visor gradient's dark edge.
    let r = region::TROUSER;
    a.rect((r.0, r.1, r.2, 3), [30, 8, 8]);
    a.rect((r.0, r.1 + r.3 - 3, r.2, 3), [30, 8, 8]);
    // BRASS's left and bottom edges met METAL / PLASTIC when the
    // rect was mid-grey metal; keep a steel rim there.
    let r = region::METAL_LIGHT;
    a.rect((r.0, r.1, 3, r.3), [72, 77, 84]);
    a.rect((r.0, r.1 + r.3 - 3, r.2, 3), [72, 77, 84]);
    // The new regions border soldier regions across texels that
    // were ZERO in every prior build (MASK above HAIR, REDCLOTH
    // above RADIO, STRAP above POUCH, COAT beside POUCH and HAIR,
    // ARMBAND beside WEBBING): return those meeting edges to zero.
    let r = region::WEBBING;
    a.clear_rect((r.0, r.1, 3, r.3));
    let r = region::POUCH;
    a.clear_rect((r.0, r.1, r.2, 3));
    a.clear_rect((r.0 + r.2 - 3, r.1, 3, r.3));
    let r = region::RADIO;
    a.clear_rect((r.0, r.1, r.2, 3));
    a.clear_rect((r.0, r.1, 3, r.3));
    let r = region::HAIR;
    a.clear_rect((r.0, r.1, r.2, 3));
    a.clear_rect((r.0, r.1, 3, r.3));
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
    // Helmet/hood: matte charcoal with panel variation.
    a.cloth(
        region::HELMET,
        [30, 32, 36],
        4,
        Some((&[[38, 41, 46], [24, 26, 30]], 22, 10)),
        0xCA35_0005,
    );
    paint_face(&mut a);
    a.cloth(region::METAL, [44, 47, 51], 4, None, 0xCA35_0006);
    a.cloth(region::LEATHER, [22, 19, 15], 5, None, 0xCA35_0008);
    a.cloth(region::PLASTIC, [12, 13, 15], 3, None, 0xCA35_0009);
    a.cloth(region::STRAP, [33, 34, 39], 4, None, 0xCA35_000A);
    paint_street_regions(&mut a);
    paint_soldier_regions(&mut a);
    paint_border_guards(&mut a);
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
    // ---- pelvis: trousers, the webbing belt, the pouch row ----
    v.push(part("pelvis", Pelvis, (0.0, 0.955, 0.0), (0.355, 0.21, 0.235), TROUSER, std));
    v.push(part("belt", Pelvis, (0.0, 1.062, 0.0), (0.39, 0.072, 0.27), WEBBING, std));
    v.push(part("belt buckle", Pelvis, (0.0, 1.062, 0.138), (0.075, 0.05, 0.012), METAL_LIGHT, std));
    // THE REAR POUCH ROW (#32): three pouches across the back of
    // the belt, each with its flap and its brass clasp. This row is
    // the rear view's signature furniture — it gets its own parts,
    // not a texture's promise.
    for (tag, x) in [("L", 0.128f32), ("C", 0.0), ("R", -0.128)] {
        v.push(part(match tag { "L" => "rear pouch L", "C" => "rear pouch C", _ => "rear pouch R" },
            Pelvis, (x, 1.028, -0.163), (0.112, 0.148, 0.058), POUCH, std));
        v.push(part(match tag { "L" => "rear pouch flap L", "C" => "rear pouch flap C", _ => "rear pouch flap R" },
            Pelvis, (x, 1.072, -0.196), (0.104, 0.068, 0.012), WEBBING, std));
        v.push(part(match tag { "L" => "rear pouch clasp L", "C" => "rear pouch clasp C", _ => "rear pouch clasp R" },
            Pelvis, (x, 1.048, -0.203), (0.02, 0.032, 0.008), METAL_LIGHT, std));
    }
    // One pouch at the front-left hip (#31); the rest of the front
    // belt stays plain, the way his does.
    v.push(part("front pouch", Pelvis, (0.145, 1.022, 0.148), (0.105, 0.135, 0.05), POUCH, std));
    v.push(part("front pouch flap", Pelvis, (0.145, 1.062, 0.176), (0.097, 0.06, 0.012), WEBBING, std));
    v.push(part("front pouch clasp", Pelvis, (0.145, 1.04, 0.183), (0.018, 0.03, 0.008), METAL_LIGHT, std));
    // ---- the coat's skirt: band, rear and side panels on the
    // pelvis; the front quarters ride the thighs, the way a coat's
    // front drapes over each leg (and the only way a rigid skirt
    // survives the crouch without tearing through the thigh).
    v.push(part("coat skirt band", Pelvis, (0.0, 0.93, 0.0), (0.435, 0.13, 0.275), WAXED, std));
    v.push(part("coat skirt rear", Pelvis, (0.0, 0.755, -0.1225), (0.42, 0.30, 0.04), WAXED, std));
    v.push(part("coat vent shadow", Pelvis, (0.0, 0.68, -0.1465), (0.03, 0.17, 0.008), PLASTIC, std));
    v.push(part("coat skirt side L", Pelvis, (0.2025, 0.755, 0.0), (0.035, 0.30, 0.245), WAXED, std));
    v.push(part("coat skirt side R", Pelvis, (-0.2025, 0.755, 0.0), (0.035, 0.30, 0.245), WAXED, std));
    v.push(part("coat front quarter L", ThighL, (0.112, 0.815, 0.122), (0.205, 0.31, 0.035), WAXED, std));
    v.push(part("coat front quarter R", ThighR, (-0.112, 0.815, 0.122), (0.205, 0.31, 0.035), WAXED, std));
    // ---- torso: the coat itself ----
    v.push(part("coat torso", Torso, (0.0, 1.295, 0.0), (0.47, 0.45, 0.30), WAXED, std));
    v.push(part("coat yoke", Torso, (0.0, 1.435, -0.1535), (0.40, 0.105, 0.013), WAXED, std));
    // The shirt opening, kept narrow: a dark column between the
    // coat's fronts — at panel width it read as a bib, not a shirt.
    v.push(part("shirt chest", Torso, (0.0, 1.35, 0.152), (0.09, 0.27, 0.014), TROUSER, std));
    v.push(part("chest pocket L", Torso, (0.125, 1.375, 0.156), (0.125, 0.105, 0.014), WAXED, std));
    v.push(part("chest pocket R", Torso, (-0.125, 1.375, 0.156), (0.125, 0.105, 0.014), WAXED, std));
    v.push(part("chest pocket flap L", Torso, (0.125, 1.415, 0.158), (0.125, 0.045, 0.016), WAXED, std));
    v.push(part("chest pocket flap R", Torso, (-0.125, 1.415, 0.158), (0.125, 0.045, 0.016), WAXED, std));
    // Coat buttons down his right front panel, beside the shirt V.
    for (i, y) in [1.42f32, 1.31, 1.20].iter().enumerate() {
        v.push(part(match i { 0 => "coat button 1", 1 => "coat button 2", _ => "coat button 3" },
            Torso, (-0.095, *y, 0.153), (0.02, 0.02, 0.012), PLASTIC, std));
    }
    v.push(part("collar", Torso, (0.0, 1.525, -0.015), (0.27, 0.09, 0.25), WAXED, std));
    v.push(part("shoulder L", Torso, (0.29, 1.478, 0.0), (0.185, 0.095, 0.27), WAXED, std));
    v.push(part("shoulder R", Torso, (-0.29, 1.478, 0.0), (0.185, 0.095, 0.27), WAXED, std));
    // The radio's shoulder strap: front, over the shoulder, and
    // down the back on his left, brass-buckled at the chest (#31).
    v.push(part("shoulder strap front", Torso, (0.155, 1.315, 0.160), (0.068, 0.42, 0.016), WEBBING, std));
    v.push(part("shoulder strap top", Torso, (0.155, 1.532, 0.0), (0.068, 0.018, 0.33), WEBBING, std));
    v.push(part("shoulder strap back", Torso, (0.155, 1.315, -0.160), (0.068, 0.42, 0.016), WEBBING, std));
    v.push(part("shoulder strap buckle", Torso, (0.155, 1.44, 0.170), (0.052, 0.062, 0.010), METAL_LIGHT, std));
    // The pack strap down the back, medial of the radio (#32): two
    // brass buckles, and the ONE RE-SEWN PATCH — a paler canvas
    // repair riding the strap where every rear view will find it.
    v.push(part("pack strap back", Torso, (0.005, 1.29, -0.160), (0.075, 0.50, 0.016), WEBBING, std));
    v.push(part("pack strap buckle 1", Torso, (0.005, 1.42, -0.170), (0.055, 0.06, 0.010), METAL_LIGHT, std));
    v.push(part("pack strap buckle 2", Torso, (0.005, 1.20, -0.170), (0.055, 0.06, 0.010), METAL_LIGHT, std));
    v.push(part("re-sewn strap patch", Torso, (0.005, 1.33, -0.169), (0.08, 0.10, 0.006), POUCH, std));
    // The diagonal strap (#32), stepped across the pouch row's top
    // from under the radio toward the right hip, brass-buckled.
    v.push(part("diagonal strap 1", Torso, (0.07, 1.13, -0.196), (0.17, 0.055, 0.012), WEBBING, std));
    v.push(part("diagonal strap 2", Torso, (-0.04, 1.06, -0.196), (0.17, 0.055, 0.012), WEBBING, std));
    v.push(part("diagonal strap buckle", Torso, (-0.115, 1.02, -0.198), (0.055, 0.06, 0.012), METAL_LIGHT, std));
    // THE RADIO (#32): high on the LEFT of the back, its top at the
    // shoulder line — a dark case with its cap and knobs, and TWO
    // WHIPS: the tall one straight, the shorter one bent at the tip
    // the way the reference's second whip bends.
    v.push(part("radio pack", Torso, (0.135, 1.35, -0.20), (0.18, 0.36, 0.095), RADIO, std));
    v.push(part("radio cap", Torso, (0.135, 1.545, -0.20), (0.16, 0.05, 0.085), RADIO, std));
    v.push(part("radio knob 1", Torso, (0.09, 1.585, -0.20), (0.03, 0.03, 0.03), PLASTIC, std));
    v.push(part("radio knob 2", Torso, (0.17, 1.585, -0.20), (0.025, 0.03, 0.025), PLASTIC, std));
    v.push(part("whip tall lower", Torso, (0.20, 1.87, -0.205), (0.013, 0.58, 0.013), PLASTIC, std));
    v.push(part("whip tall upper", Torso, (0.20, 2.28, -0.205), (0.008, 0.26, 0.008), PLASTIC, std));
    v.push(part("whip short lower", Torso, (0.115, 1.80, -0.205), (0.010, 0.44, 0.010), PLASTIC, std));
    v.push(part("whip short tip", Torso, (0.128, 2.10, -0.205), (0.008, 0.18, 0.008), PLASTIC, std));
    // The long gun, slung under the pack down his left side (#32):
    // stock at the pack's hem, receiver, magazine, barrel to the knee.
    v.push(part("slung gun stock", Torso, (0.23, 1.24, -0.09), (0.045, 0.13, 0.065), LEATHER, std));
    v.push(part("slung gun receiver", Torso, (0.235, 1.02, -0.09), (0.05, 0.34, 0.085), METAL, std));
    v.push(part("slung gun magazine", Torso, (0.235, 0.86, -0.05), (0.03, 0.15, 0.035), METAL, std));
    v.push(part("slung gun barrel", Torso, (0.238, 0.66, -0.09), (0.02, 0.40, 0.02), METAL, std));
    // The holster (#31/#32): worn leather on the RIGHT hip, the
    // USP's grip above it, a webbing flap strap on its outer face.
    v.push(part("holster", Pelvis, (-0.245, 1.00, 0.03), (0.07, 0.23, 0.125), LEATHER, std));
    v.push(part("holster grip", Pelvis, (-0.245, 1.145, 0.03), (0.038, 0.075, 0.05), STRAP, std));
    v.push(part("holster flap strap", Pelvis, (-0.283, 1.06, 0.03), (0.012, 0.10, 0.06), WEBBING, std));
    // ---- head: bare, weathered — hair, ears, the box nose, and
    // the painted face on its quad (see face_quad).
    v.push(part("neck", Head, (0.0, 1.54, 0.0), (0.125, 0.10, 0.125), SKIN, std));
    v.push(part("head", Head, (0.0, 1.70, 0.0), (0.235, 0.30, 0.25), SKIN, std));
    v.push(part("hair cap", Head, (0.0, 1.80, 0.0), (0.25, 0.13, 0.27), HAIR, std));
    v.push(part("hair back", Head, (0.0, 1.685, -0.131), (0.24, 0.25, 0.012), HAIR, std));
    v.push(part("ear L", Head, (0.124, 1.68, 0.0), (0.024, 0.06, 0.05), SKIN, std));
    v.push(part("ear R", Head, (-0.124, 1.68, 0.0), (0.024, 0.06, 0.05), SKIN, std));
    // The nose — offset a whisper to his left: once broken, set
    // imperfectly, the way the reference's sits.
    v.push(part("nose", Head, (0.004, 1.662, 0.133), (0.046, 0.068, 0.032), SKIN, std));
    // ---- arms: coat sleeves, leather gloves ----
    for (sx, up, fore, hand) in [
        (1.0f32, UpperArmL, ForearmL, HandL),
        (-1.0f32, UpperArmR, ForearmR, HandR),
    ] {
        v.push(part(if sx > 0.0 { "upper arm L" } else { "upper arm R" }, up,
            (0.30 * sx, 1.315, 0.0), (0.155, 0.29, 0.16), WAXED, std));
        v.push(part(if sx > 0.0 { "forearm L" } else { "forearm R" }, fore,
            (0.30 * sx, 1.035, 0.005), (0.135, 0.27, 0.14), WAXED, std));
        v.push(part(if sx > 0.0 { "cuff L" } else { "cuff R" }, fore,
            (0.30 * sx, 0.928, 0.008), (0.128, 0.05, 0.135), WEBBING, std));
        v.push(part(if sx > 0.0 { "glove L" } else { "glove R" }, hand,
            (0.30 * sx, 0.845, 0.02), (0.11, 0.145, 0.12), LEATHER, std));
        v.push(part(if sx > 0.0 { "thumb L" } else { "thumb R" }, hand,
            (0.248 * sx, 0.825, 0.05), (0.04, 0.09, 0.045), LEATHER, std));
    }
    // THE WATCH (left wrist): a dark strap around the forearm, and
    // the case on the wrist's INNER face — face-inward, the way he
    // wears it (#31/#32). It reads at close range or not at all,
    // which is what a watch is.
    v.push(part("watch strap", ForearmL, (0.30, 0.975, 0.005), (0.14, 0.032, 0.15), STRAP, std));
    v.push(part("watch case (face-inward)", ForearmL, (0.228, 0.975, 0.005), (0.014, 0.05, 0.05), METAL, std));
    // ---- legs: plain trousers, tall lace-up boots — no pads, no
    // rigs; the street figure's legs are a working man's.
    for (sx, thigh, shin, foot) in [
        (1.0f32, ThighL, ShinL, FootL),
        (-1.0f32, ThighR, ShinR, FootR),
    ] {
        v.push(part(if sx > 0.0 { "thigh L" } else { "thigh R" }, thigh,
            (0.115 * sx, 0.775, 0.0), (0.185, 0.20, 0.205), TROUSER, std));
        v.push(part(if sx > 0.0 { "thigh lower L" } else { "thigh lower R" }, thigh,
            (0.115 * sx, 0.60, 0.0), (0.17, 0.17, 0.19), TROUSER, std));
        v.push(part(if sx > 0.0 { "shin upper L" } else { "shin upper R" }, shin,
            (0.115 * sx, 0.435, 0.0), (0.165, 0.16, 0.18), TROUSER, std));
        v.push(part(if sx > 0.0 { "trouser cuff L" } else { "trouser cuff R" }, shin,
            (0.115 * sx, 0.345, 0.0), (0.172, 0.055, 0.19), TROUSER, std));
        v.push(part(if sx > 0.0 { "boot shaft L" } else { "boot shaft R" }, shin,
            (0.115 * sx, 0.225, 0.0), (0.15, 0.26, 0.165), LEATHER, std));
        v.push(part(if sx > 0.0 { "boot lace panel L" } else { "boot lace panel R" }, shin,
            (0.115 * sx, 0.24, 0.086), (0.095, 0.235, 0.024), STRAP, std));
        v.push(part(if sx > 0.0 { "boot foot L" } else { "boot foot R" }, foot,
            (0.115 * sx, 0.055, 0.045), (0.145, 0.11, 0.30), LEATHER, std));
        v.push(part(if sx > 0.0 { "boot toe L" } else { "boot toe R" }, foot,
            (0.115 * sx, 0.05, 0.185), (0.135, 0.085, 0.05), LEATHER, std));
        v.push(part(if sx > 0.0 { "boot sole L" } else { "boot sole R" }, foot,
            (0.115 * sx, 0.016, 0.03), (0.155, 0.032, 0.33), PLASTIC, std));
        v.push(part(if sx > 0.0 { "boot heel L" } else { "boot heel R" }, foot,
            (0.115 * sx, 0.05, -0.105), (0.14, 0.10, 0.06), LEATHER, std));
    }
    // ---- the HK USP, in the right hand ----
    // Modelled along the hand's -Y axis (muzzle down at bind, like a
    // pistol held at rest); the arm chain's pitch raises it to the
    // ready and the aim (see the pose tables). Unchanged from M6 —
    // it was always his gun; only its slide's metal is re-sourced
    // (the light-metal rect is brass now, and a brass slide is a
    // different man's gun).
    let h = HandR;
    let hx = -0.30f32;
    v.push(part("USP grip", h, (hx, 0.835, 0.03), (0.042, 0.115, 0.055), STRAP, std));
    v.push(part("USP mag base", h, (hx, 0.775, 0.032), (0.046, 0.02, 0.058), METAL, std));
    v.push(part("USP frame", h, (hx, 0.895, 0.045), (0.040, 0.05, 0.16), METAL, std));
    v.push(part("USP slide", h, (hx, 0.925, 0.045), (0.036, 0.028, 0.185), METAL, std));
    v.push(part("USP barrel tip", h, (hx, 0.918, 0.135), (0.020, 0.020, 0.03), METAL, std));
    v.push(part("USP trigger guard", h, (hx, 0.855, 0.085), (0.012, 0.045, 0.07), METAL, std));
    v.push(part("USP rear sight", h, (hx, 0.948, -0.028), (0.026, 0.016, 0.02), METAL, std));
    v.push(part("USP front sight", h, (hx, 0.946, 0.118), (0.010, 0.014, 0.012), METAL, std));
    v.push(part("USP hammer", h, (hx, 0.925, -0.048), (0.014, 0.03, 0.018), METAL, std));
    v
}

/// The face is a quad, not a box: its art has a reading direction,
/// and only a placed quad controls which way is up. It rides the
/// head box's front face, a whisper proud of it — chin at the
/// collar's top edge, hairline where the hair cap takes over. (The
/// M6 build's quad was the NSA shoulder patch; the patch is deleted
/// — no labels on him — and the quad's craft moved to his face.)
fn face_quad() -> ([Vec3; 4], Bone) {
    // Corners in the box faces' own order — bottom-left, bottom-
    // right, top-right, top-left as seen from outside (the front) —
    // with the UVs the box code assigns those corners, so the
    // painted face stands upright: region-top at the hairline,
    // region-left on his right cheek.
    let z = 0.1265;
    (
        [
            Vec3::new(-0.104, 1.570, z), // bottom, his right
            Vec3::new(0.104, 1.570, z),  // bottom, his left
            Vec3::new(0.104, 1.740, z),  // top, his left
            Vec3::new(-0.104, 1.740, z), // top, his right
        ],
        Bone::Head,
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
    let (corners, bone) = face_quad();
    let uv = uv_rect(region::FACE);
    let face = QuadSpec {
        name: "face (quad)",
        bone,
        corners,
        normal: Vec3::new(0.0, 0.0, 1.0),
        // Corners are listed BL, BR, TR, TL as seen from the front;
        // the box faces' UV order gives BL the rect's bottom-left,
        // so the painted face stands upright.
        uvs: [[uv[0], uv[3]], [uv[2], uv[3]], [uv[2], uv[1]], [uv[0], uv[1]]],
        mat: MAT_STANDARD,
    };
    build_figure_frame(&specs, &[face], frame, spot, ground_y, yaw_deg)
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
/// per-corner UVs and a bind-pose normal (the operator's face is
/// the instance at hand; insignia and marks are the general case).
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
