//! Static textured geometry — the props apparatus (Slice 1).
//!
//! The operator pipeline proved the pattern for characters: box-part
//! geometry over a **generated** texture atlas, lit by the reality
//! layer, shadowed like terrain. Props are the same pattern for the
//! built world: a host builds one static mesh (a street, a room, a
//! ruin), hands it to [`crate::Renderer::set_props`], and it draws
//! in the scene pass and casts into the shadow map alongside the
//! operator. No image assets anywhere — the city atlas below is
//! generated texel by texel, seeded, like the operator's.
//!
//! One deliberate extension over the character shading: each vertex
//! carries a **baked lamp term** ([`PropVertex::light`]). The era
//! this engine renders in lit its streets exactly this way — the
//! practical lights are baked into the surface at build time by the
//! host's light field, and the same field answers gameplay's
//! questions (DeBrief's light read). The shader adds the baked term
//! in sodium colour on top of the reality layer's sun/moon/ambient,
//! so a prop stands in the same night as everything else *and*
//! under its lamps. Material flag 1 is emissive surface art (lit
//! windows, lamp heads): the texture holds its own light.

use bytemuck::{Pod, Zeroable};

/// The generated city atlas edge (pixels). One 512² RGBA texture
/// carries every surface the built world wears.
pub const CITY_ATLAS_SIZE: u32 = 512;

/// Material ids (the `mat` vertex attribute; the shader branches).
/// 0 = standard lit surface. 1 = emissive (lit windows, lamp heads,
/// wet-streak glints): the atlas texels are the light.
pub const MAT_STANDARD: f32 = 0.0;
pub const MAT_EMISSIVE: f32 = 1.0;
/// Wet surface (street, sidewalks): standard diffuse + the
/// view-dependent lamp specular the wet pass computes — pools and
/// streaks that answer lamp positions AND the camera, instead of
/// painted streak geometry (which read as hardware).
pub const MAT_WET: f32 = 2.0;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct PropVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub mat: f32,
    /// Baked practical-light term in [0, ~1.2]: the host light
    /// field's exposure at this vertex (lamp falloff × cone ×
    /// occlusion), evaluated when the mesh was built. The shader
    /// adds it in sodium colour.
    pub light: f32,
}

pub struct PropMesh {
    pub vertices: Vec<PropVertex>,
    pub indices: Vec<u32>,
}

impl PropMesh {
    pub fn new() -> PropMesh {
        PropMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }
    pub fn triangle_count(&self) -> u64 {
        (self.indices.len() / 3) as u64
    }
}

impl Default for PropMesh {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------
// The generated city atlas
// ---------------------------------------------------------------------

/// Atlas regions as pixel rects (x, y, w, h) in the 512² atlas.
pub mod region {
    /// Red-brown warehouse brick, mortar courses, grime at the foot.
    pub const BRICK: (u32, u32, u32, u32) = (0, 0, 128, 128);
    /// Darker weathered brick — the tenement rank.
    pub const BRICK_DARK: (u32, u32, u32, u32) = (128, 0, 128, 128);
    /// Ashlar stone blocks — bases, banding, cornices, curbs.
    pub const STONE: (u32, u32, u32, u32) = (256, 0, 128, 128);
    /// Sidewalk concrete slabs with joints and speckle.
    pub const CONCRETE: (u32, u32, u32, u32) = (384, 0, 128, 128);
    /// Wet asphalt: near-black, speckled, faint slab seams.
    pub const ASPHALT: (u32, u32, u32, u32) = (0, 128, 128, 128);
    /// Roller shutter: horizontal slats, rust weeping from the joints.
    pub const ROLLER: (u32, u32, u32, u32) = (128, 128, 128, 128);
    /// The three-bars banner: deep red cloth, three white bars.
    pub const BANNER: (u32, u32, u32, u32) = (256, 128, 96, 128);
    /// A panelled personnel door with a wired-glass slit.
    pub const DOOR: (u32, u32, u32, u32) = (352, 128, 80, 128);
    /// A lit window: warm glass behind a muntin grid (emissive).
    pub const WINDOW_LIT: (u32, u32, u32, u32) = (432, 128, 64, 64);
    /// A dark window: dead glass, one faint reflection streak.
    pub const WINDOW_DARK: (u32, u32, u32, u32) = (432, 192, 64, 64);
    /// A sodium streak for wet ground: warm vertical gradient
    /// (emissive; the reflection a lamp drags down wet asphalt).
    pub const STREAK: (u32, u32, u32, u32) = (256, 256, 64, 128);
    /// Painted metal — crane, kiosk, lamp standards: panel lines, rust.
    pub const METAL_DARK: (u32, u32, u32, u32) = (0, 256, 128, 128);
    /// Tar roof with felt seams.
    pub const ROOF: (u32, u32, u32, u32) = (128, 256, 128, 128);
    /// Checkpoint timber: planks with worn white/red paint bands.
    pub const TIMBER: (u32, u32, u32, u32) = (384, 256, 128, 128);
    /// Lamp head glass: the sodium source itself (emissive).
    pub const LAMP_GLASS: (u32, u32, u32, u32) = (320, 256, 64, 64);
}

/// UV rect for an atlas region, as [u_min, v_min, u_max, v_max].
pub fn uv_rect(r: (u32, u32, u32, u32)) -> [f32; 4] {
    let s = CITY_ATLAS_SIZE as f32;
    [
        r.0 as f32 / s,
        r.1 as f32 / s,
        (r.0 + r.2) as f32 / s,
        (r.1 + r.3) as f32 / s,
    ]
}

/// Deterministic xorshift — the atlas is generated, and generation
/// is seeded, like everything else in this engine. (The operator
/// atlas keeps its own copy; the two generators are independent by
/// design — either can change without re-minting the other.)
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
            px: vec![0; (CITY_ATLAS_SIZE * CITY_ATLAS_SIZE * 4) as usize],
        }
    }
    fn set(&mut self, x: u32, y: u32, c: [u8; 3]) {
        if x < CITY_ATLAS_SIZE && y < CITY_ATLAS_SIZE {
            let i = ((y * CITY_ATLAS_SIZE + x) * 4) as usize;
            self.px[i] = c[0];
            self.px[i + 1] = c[1];
            self.px[i + 2] = c[2];
            self.px[i + 3] = 255;
        }
    }
    /// Write ONLY the alpha channel: the atlas alpha carries
    /// per-texel ROUGHNESS for the wet pass (graphics push) —
    /// 0 = glass-smooth, 255 = rough stone. It must be painted
    /// LAST in generate_city_atlas: set()/rect()/vshade() all
    /// reset alpha to 255, while noise() and soften_city() leave
    /// it alone.
    fn set_alpha(&mut self, x: u32, y: u32, v: u8) {
        if x < CITY_ATLAS_SIZE && y < CITY_ATLAS_SIZE {
            let i = ((y * CITY_ATLAS_SIZE + x) * 4) as usize;
            self.px[i + 3] = v;
        }
    }
    /// Fill a region's roughness channel: base ± variation noise.
    fn rough(&mut self, r: (u32, u32, u32, u32), base: i32, var: i32, seed: u32) {
        let mut rng = Rng::new(seed);
        for y in r.1..r.1 + r.3 {
            for x in r.0..r.0 + r.2 {
                let v = (base + rng.jitter(var)).clamp(0, 255) as u8;
                self.set_alpha(x, y, v);
            }
        }
    }
    fn get(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * CITY_ATLAS_SIZE + x) * 4) as usize;
        [self.px[i], self.px[i + 1], self.px[i + 2]]
    }
    fn rect(&mut self, r: (u32, u32, u32, u32), c: [u8; 3]) {
        for y in r.1..r.1 + r.3 {
            for x in r.0..r.0 + r.2 {
                self.set(x, y, c);
            }
        }
    }
    /// Base colour + per-pixel noise + optional blocky blotches.
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
                let bx = r.0 + rng.range(r.2.saturating_sub(4).max(1));
                let by = r.1 + rng.range(r.3.saturating_sub(4).max(1));
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
        self.noise(r, noise, &mut rng);
    }
    fn noise(&mut self, r: (u32, u32, u32, u32), amt: i32, rng: &mut Rng) {
        for y in r.1..r.1 + r.3 {
            for x in r.0..r.0 + r.2 {
                let i = ((y * CITY_ATLAS_SIZE + x) * 4) as usize;
                for k in 0..3 {
                    let v = self.px[i + k] as i32 + rng.jitter(amt);
                    self.px[i + k] = v.clamp(0, 255) as u8;
                }
            }
        }
    }
    /// Multiply a vertical gradient over a region (grime at the
    /// foot of a wall, wear at the head): `top` at the region's
    /// first row, `bot` at its last, per-channel multipliers.
    fn vshade(&mut self, r: (u32, u32, u32, u32), top: f32, bot: f32) {
        for y in r.1..r.1 + r.3 {
            let t = (y - r.1) as f32 / (r.3 - 1).max(1) as f32;
            let m = top + (bot - top) * t;
            for x in r.0..r.0 + r.2 {
                let c = self.get(x, y);
                self.set(
                    x,
                    y,
                    [
                        (c[0] as f32 * m).clamp(0.0, 255.0) as u8,
                        (c[1] as f32 * m).clamp(0.0, 255.0) as u8,
                        (c[2] as f32 * m).clamp(0.0, 255.0) as u8,
                    ],
                );
            }
        }
    }
}

/// Brickwork: courses of offset stretchers over a mortar field.
/// Brick ≈ 8×3 px at region scale (the atlas paints at roughly
/// 16 px per metre on a 8 m UV span, so a brick reads ~0.5 m —
/// chunky, PS1.5-scaled, matching the reference's pixel size).
fn paint_brick(a: &mut Atlas, r: (u32, u32, u32, u32), base: [u8; 3], dark: [u8; 3], seed: u32) {
    a.rect(r, [96, 88, 80]); // mortar
    let mut rng = Rng::new(seed);
    let (bw, bh) = (16u32, 6u32);
    let rows = r.3 / bh;
    for row in 0..rows {
        let off = if row % 2 == 0 { 0 } else { bw / 2 };
        let mut x = r.0 as i32 - off as i32;
        while x < (r.0 + r.2) as i32 {
            let tone = 0.82 + (rng.next() % 36) as f32 / 100.0;
            let c = if rng.next() % 7 == 0 { dark } else { base };
            let c = [
                (c[0] as f32 * tone).clamp(0.0, 255.0) as u8,
                (c[1] as f32 * tone).clamp(0.0, 255.0) as u8,
                (c[2] as f32 * tone).clamp(0.0, 255.0) as u8,
            ];
            for y in 0..bh - 2 {
                for xx in 0..bw - 2 {
                    let px = x + xx as i32;
                    if px >= r.0 as i32 && px < (r.0 + r.2) as i32 {
                        // Graphics push: a bevel read per brick —
                        // the top row catches the light, the bottom
                        // row falls into its own shadow. At street
                        // distance this is what separates brick
                        // courses from a flat tint.
                        let m = if y == 0 {
                            1.15
                        } else if y == bh - 3 {
                            0.85
                        } else {
                            1.0
                        };
                        a.set(
                            px as u32,
                            r.1 + row * bh + y,
                            [
                                (c[0] as f32 * m).clamp(0.0, 255.0) as u8,
                                (c[1] as f32 * m).clamp(0.0, 255.0) as u8,
                                (c[2] as f32 * m).clamp(0.0, 255.0) as u8,
                            ],
                        );
                    }
                }
            }
            x += bw as i32;
        }
    }
    a.noise(r, 3, &mut rng);
}

/// Ashlar stone: big blocks, wide joints, per-block tone drift.
fn paint_stone(a: &mut Atlas, r: (u32, u32, u32, u32), seed: u32) {
    a.rect(r, [62, 60, 58]);
    let mut rng = Rng::new(seed);
    let (bw, bh) = (42u32, 21u32);
    let rows = r.3 / bh + 1;
    for row in 0..rows {
        let off = if row % 2 == 0 { 0 } else { bw / 2 };
        let mut x = r.0 as i32 - off as i32;
        while x < (r.0 + r.2) as i32 {
            let tone = 0.90 + (rng.next() % 18) as f32 / 100.0;
            let base = [86, 82, 76];
            let c = [
                (base[0] as f32 * tone) as u8,
                (base[1] as f32 * tone) as u8,
                (base[2] as f32 * tone) as u8,
            ];
            for y in 0..bh - 2 {
                for xx in 0..bw - 2 {
                    let px = x + xx as i32;
                    if px >= r.0 as i32 && px < (r.0 + r.2) as i32 {
                        // Graphics push: the same bevel read as
                        // brick, at ashlar scale — a lit top edge
                        // and a shadowed foot per block, so walls
                        // keep their courses under flat night light.
                        let m = if y == 0 {
                            1.13
                        } else if y == bh - 3 {
                            0.86
                        } else {
                            1.0
                        };
                        a.set(
                            px as u32,
                            r.1 + row * bh + y,
                            [
                                (c[0] as f32 * m).clamp(0.0, 255.0) as u8,
                                (c[1] as f32 * m).clamp(0.0, 255.0) as u8,
                                (c[2] as f32 * m).clamp(0.0, 255.0) as u8,
                            ],
                        );
                    }
                }
            }
            x += bw as i32;
        }
    }
    // Solidity: stone's per-pixel speckle halved — ashlar reads as
    // blocks with tone drift, not as a wire mesh of noise.
    a.noise(r, 3, &mut rng);
}

/// Solidity pass (city): cross-blur a region's interior, 2 px
/// inset. Same doctrine as the operator's soften — per-pixel
/// speckle becomes broad tone, while 2 px-plus joints, courses,
/// and blocks survive as softer masses. Applied only to the big
/// flat surfaces (brick, stone, concrete, asphalt); the furniture
/// regions keep their drawn lines (muntins, banner bars, seams).
fn soften_city(a: &mut Atlas, r: (u32, u32, u32, u32)) {
    let snap = a.px.clone();
    let at = |x: u32, y: u32, k: usize| -> i32 {
        snap[((y * CITY_ATLAS_SIZE + x) * 4) as usize + k] as i32
    };
    for y in (r.1 + 2)..(r.1 + r.3 - 2) {
        for x in (r.0 + 2)..(r.0 + r.2 - 2) {
            let i = ((y * CITY_ATLAS_SIZE + x) * 4) as usize;
            for k in 0..3 {
                let v = (at(x, y, k) * 2
                    + at(x - 1, y, k)
                    + at(x + 1, y, k)
                    + at(x, y - 1, k)
                    + at(x, y + 1, k))
                    / 6;
                a.px[i + k] = v.clamp(0, 255) as u8;
            }
        }
    }
}

/// Paint the whole city atlas. Deterministic: same pixels on every
/// host, every run.
pub fn generate_city_atlas() -> Vec<u8> {
    let mut a = Atlas::new();
    // ---- masonry ----
    paint_brick(&mut a, region::BRICK, [128, 74, 58], [96, 52, 44], 0xC17E_0001);
    a.vshade(region::BRICK, 1.04, 0.78); // grime settles downward
    paint_brick(&mut a, region::BRICK_DARK, [84, 62, 56], [58, 42, 40], 0xC17E_0002);
    a.vshade(region::BRICK_DARK, 1.02, 0.74);
    paint_stone(&mut a, region::STONE, 0xC17E_0003);
    // ---- ground ----
    a.cloth(
        region::CONCRETE,
        [63, 62, 59],
        3,
        Some((&[[54, 53, 51], [74, 72, 68], [48, 48, 47]], 130, 7)),
        0xC17E_0004,
    );
    {
        // Slab joints: 2 px shadowed gaps on a 32 px grid, not 1 px
        // lines — a 1 px joint aliases into a wire under the 2.2x
        // nearest upscale; a 2 px gap survives it as a joint.
        let r = region::CONCRETE;
        for k in 0..=4 {
            let x = r.0 + k * 32;
            for y in r.1..r.1 + r.3 {
                a.set(x.min(r.0 + r.2 - 1), y, [40, 39, 37]);
                a.set((x + 1).min(r.0 + r.2 - 1), y, [40, 39, 37]);
            }
            let y = r.1 + k * 32;
            for x in r.0..r.0 + r.2 {
                a.set(x, y.min(r.1 + r.3 - 1), [40, 39, 37]);
                a.set(x, (y + 1).min(r.1 + r.3 - 1), [40, 39, 37]);
            }
        }
    }
    a.cloth(
        region::ASPHALT,
        [38, 39, 44],
        3,
        Some((&[[30, 31, 36], [48, 49, 55], [26, 27, 31]], 220, 5)),
        0xC17E_0005,
    );
    {
        // Faint slab seams across the asphalt (the street is poured
        // in slabs, per the reference's expansion joints) — 2 px,
        // for the same upscale reason as the concrete joints.
        let r = region::ASPHALT;
        for k in 0..=2 {
            let y = r.1 + k * 64;
            for x in r.0..r.0 + r.2 {
                a.set(x, y.min(r.1 + r.3 - 1), [24, 25, 29]);
                a.set(x, (y + 1).min(r.1 + r.3 - 1), [24, 25, 29]);
            }
        }
    }
    // Solidity: settle the big flat surfaces into masses before
    // the furniture is drawn over them.
    for r in [
        region::BRICK,
        region::BRICK_DARK,
        region::STONE,
        region::CONCRETE,
        region::ASPHALT,
    ] {
        soften_city(&mut a, r);
    }
    // ---- warehouse furniture ----
    {
        // Roller shutter: slats with shadow lines and rust weep.
        let r = region::ROLLER;
        a.rect(r, [88, 90, 94]);
        let mut rng = Rng::new(0xC17E_0006);
        for y in 0..r.3 {
            let phase = y % 8;
            let m = match phase {
                0 => 0.55,
                1 => 0.78,
                7 => 1.10,
                _ => 0.95,
            };
            for x in r.0..r.0 + r.2 {
                let c = a.get(x, r.1 + y);
                a.set(
                    x,
                    r.1 + y,
                    [
                        (c[0] as f32 * m) as u8,
                        (c[1] as f32 * m) as u8,
                        (c[2] as f32 * m) as u8,
                    ],
                );
            }
        }
        for _ in 0..26 {
            // Rust streaks run down from random slats.
            let x = r.0 + rng.range(r.2 - 2);
            let y0 = r.1 + rng.range(r.3 - 24);
            let len = 6 + rng.range(20);
            for k in 0..len {
                let fade = 1.0 - k as f32 / len as f32;
                let c = a.get(x, y0 + k);
                a.set(
                    x,
                    y0 + k,
                    [
                        (c[0] as f32 + 44.0 * fade).min(255.0) as u8,
                        (c[1] as f32 + 12.0 * fade).min(255.0) as u8,
                        c[2],
                    ],
                );
            }
        }
        a.noise(r, 5, &mut rng);
    }
    {
        // The banner: deep red cloth, three white bars, worn.
        let r = region::BANNER;
        a.cloth(r, [110, 30, 30], 6, Some((&[[96, 24, 26], [124, 38, 34]], 60, 8)), 0xC17E_0007);
        // Bars occupy the middle band; three verticals, evenly read.
        for i in 0..3 {
            let bx = r.0 + 22 + i * 22;
            for y in r.1 + 26..r.1 + 102 {
                for x in bx..bx + 11 {
                    let worn = (x + y) % 9 == 0;
                    a.set(x, y, if worn { [168, 150, 138] } else { [216, 208, 196] });
                }
            }
        }
        a.vshade(r, 1.05, 0.82);
    }
    {
        // Personnel door: dark painted steel, panels, a wired slit.
        let r = region::DOOR;
        a.cloth(r, [44, 46, 44], 4, None, 0xC17E_0008);
        a.rect((r.0 + 10, r.1 + 12, r.2 - 20, 44), [38, 40, 38]);
        a.rect((r.0 + 10, r.1 + 66, r.2 - 20, 48), [38, 40, 38]);
        a.rect((r.0 + 30, r.1 + 18, 20, 32), [16, 18, 20]); // glass slit
        a.rect((r.0 + 30, r.1 + 18, 20, 3), [52, 54, 52]);
        a.rect((r.0 + r.2 - 14, r.1 + 62, 5, 10), [150, 148, 140]); // handle
    }
    {
        // Lit window: warm gradient + muntin grid + one curtain edge.
        let r = region::WINDOW_LIT;
        for y in 0..r.3 {
            for x in 0..r.2 {
                let v = 1.0 - ((y as f32 / r.3 as f32) - 0.42).abs() * 1.5;
                let warm = [
                    (196.0 + 42.0 * v) as u8,
                    (128.0 + 30.0 * v) as u8,
                    (52.0 + 14.0 * v) as u8,
                ];
                a.set(r.0 + x, r.1 + y, warm);
            }
        }
        a.rect((r.0, r.1, 4, r.3), [30, 28, 26]);
        a.rect((r.0 + r.2 - 4, r.1, 4, r.3), [30, 28, 26]);
        a.rect((r.0, r.1, r.2, 4), [30, 28, 26]);
        a.rect((r.0, r.1 + r.3 - 4, r.2, 4), [30, 28, 26]);
        a.rect((r.0 + r.2 / 2 - 2, r.1, 4, r.3), [30, 28, 26]);
        a.rect((r.0, r.1 + r.3 / 2 - 2, r.2, 4), [30, 28, 26]);
        a.rect((r.0 + r.2 - 15, r.1 + 4, 11, r.3 - 8), [150, 92, 40]); // curtain
        let mut rng = Rng::new(0xC17E_0009);
        a.noise(r, 5, &mut rng);
    }
    {
        // Dark window: dead blue-grey glass, a diagonal sheen.
        let r = region::WINDOW_DARK;
        a.cloth(r, [26, 32, 40], 4, Some((&[[20, 25, 32], [34, 42, 52]], 24, 9)), 0xC17E_000A);
        for k in 0..40 {
            let x = r.0 + 6 + k;
            let y = r.1 + 8 + k / 2;
            if y < r.1 + r.3 {
                a.set(x, y, [58, 70, 84]);
                a.set(x, y + 1, [46, 56, 68]);
            }
        }
        a.rect((r.0, r.1, 3, r.3), [18, 19, 20]);
        a.rect((r.0 + r.2 - 3, r.1, 3, r.3), [18, 19, 20]);
        a.rect((r.0, r.1, r.2, 3), [18, 19, 20]);
        a.rect((r.0, r.1 + r.3 - 3, r.2, 3), [18, 19, 20]);
        a.rect((r.0 + r.2 / 2 - 1, r.1, 3, r.3), [18, 19, 20]);
    }
    {
        // The wet streak's sheen: deliberately FLAT (a soft warm
        // grey + noise). The streak's shape lives entirely in its
        // vertices' baked light (a subdivided grid samples the
        // field's profile); any gradient painted here would restart
        // per grid cell and read as panels — as the first version,
        // hot-cored per cell, plainly did.
        let r = region::STREAK;
        a.cloth(r, [62, 56, 48], 4, None, 0xC17E_000B);
    }
    {
        // Painted metal: dark dockyard green-grey, panel seams, rust.
        let r = region::METAL_DARK;
        a.cloth(r, [52, 56, 54], 5, Some((&[[44, 48, 46], [62, 66, 62], [70, 58, 44]], 90, 8)), 0xC17E_000C);
        for k in 0..=2 {
            let x = r.0 + k * 64;
            for y in r.1..r.1 + r.3 {
                a.set(x.min(r.0 + r.2 - 1), y, [36, 39, 37]);
            }
            let y = r.1 + k * 64;
            for x in r.0..r.0 + r.2 {
                a.set(x, y.min(r.1 + r.3 - 1), [36, 39, 37]);
            }
        }
    }
    a.cloth(
        region::ROOF,
        [34, 33, 36],
        5,
        Some((&[[28, 27, 30], [42, 41, 45]], 130, 8)),
        0xC17E_000D,
    );
    {
        // Checkpoint timber: planks + worn paint bands.
        let r = region::TIMBER;
        a.cloth(r, [96, 78, 56], 6, Some((&[[82, 64, 44], [110, 92, 68]], 120, 7)), 0xC17E_000E);
        for band in 0..3 {
            let x0 = r.0 + 8 + band * 42;
            a.rect((x0, r.1 + 10, 20, r.3 - 20), [196, 190, 178]);
            a.rect((x0 + 20, r.1 + 10, 10, r.3 - 20), [148, 34, 32]);
        }
        a.vshade(r, 1.03, 0.85);
    }
    {
        // Lamp glass: a hot sodium core in a dark housing surround.
        let r = region::LAMP_GLASS;
        a.rect(r, [24, 22, 20]);
        for y in 8..r.3 - 8 {
            for x in 8..r.2 - 8 {
                let dx = (x as f32 - r.2 as f32 / 2.0) / (r.2 as f32 / 2.0);
                let dy = (y as f32 - r.3 as f32 / 2.0) / (r.3 as f32 / 2.0);
                let m = (1.0 - (dx * dx + dy * dy)).clamp(0.0, 1.0).powf(0.7);
                a.set(
                    r.0 + x,
                    r.1 + y,
                    [
                        (90.0 + 165.0 * m) as u8,
                        (48.0 + 108.0 * m) as u8,
                        (12.0 + 34.0 * m) as u8,
                    ],
                );
            }
        }
    }
    // Graphics push: ground wear, then roughness — both LAST, after
    // every set()/rect()/vshade() in the file, because those reset
    // alpha to 255 and roughness lives in alpha.
    {
        // Worn patches on the concrete slabs and repair patches on
        // the asphalt: large soft tone shifts, not speckle, so the
        // street reads as maintained ground rather than noise.
        let mut rng = Rng::new(0xC17E_0010);
        for _ in 0..24 {
            let r = region::CONCRETE;
            let bx = r.0 + rng.range(r.2.saturating_sub(16).max(1));
            let by = r.1 + rng.range(r.3.saturating_sub(16).max(1));
            let bw = 8 + rng.range(16);
            let bh = 8 + rng.range(16);
            let tone = if rng.next() % 2 == 0 { 0.88 } else { 1.10 };
            for y in by..(by + bh).min(r.1 + r.3) {
                for x in bx..(bx + bw).min(r.0 + r.2) {
                    let c = a.get(x, y);
                    a.set(
                        x,
                        y,
                        [
                            (c[0] as f32 * tone).clamp(0.0, 255.0) as u8,
                            (c[1] as f32 * tone).clamp(0.0, 255.0) as u8,
                            (c[2] as f32 * tone).clamp(0.0, 255.0) as u8,
                        ],
                    );
                }
            }
        }
        for _ in 0..18 {
            let r = region::ASPHALT;
            let bx = r.0 + rng.range(r.2.saturating_sub(14).max(1));
            let by = r.1 + rng.range(r.3.saturating_sub(14).max(1));
            let bw = 8 + rng.range(14);
            let bh = 6 + rng.range(12);
            let tone = if rng.next() % 2 == 0 { 0.82 } else { 1.14 };
            for y in by..(by + bh).min(r.1 + r.3) {
                for x in bx..(bx + bw).min(r.0 + r.2) {
                    let c = a.get(x, y);
                    a.set(
                        x,
                        y,
                        [
                            (c[0] as f32 * tone).clamp(0.0, 255.0) as u8,
                            (c[1] as f32 * tone).clamp(0.0, 255.0) as u8,
                            (c[2] as f32 * tone).clamp(0.0, 255.0) as u8,
                        ],
                    );
                }
            }
        }
    }
    // Roughness channel (alpha): smooth ground mirrors lamps
    // tight and hot; rough masonry spreads them broad and dim.
    // Emissive regions (WINDOW_LIT, LAMP_GLASS) keep alpha 255 —
    // the emissive branch never reads it.
    a.rough(region::ASPHALT, 130, 30, 0xC17E_0020);
    a.rough(region::CONCRETE, 185, 25, 0xC17E_0021);
    a.rough(region::STREAK, 100, 25, 0xC17E_0022);
    a.rough(region::BRICK, 225, 20, 0xC17E_0023);
    a.rough(region::BRICK_DARK, 225, 20, 0xC17E_0024);
    a.rough(region::STONE, 230, 20, 0xC17E_0025);
    a.rough(region::METAL_DARK, 150, 25, 0xC17E_0026);
    a.rough(region::ROLLER, 160, 25, 0xC17E_0027);
    a.rough(region::DOOR, 150, 20, 0xC17E_0028);
    a.rough(region::ROOF, 220, 25, 0xC17E_0029);
    a.rough(region::TIMBER, 215, 25, 0xC17E_002A);
    a.rough(region::BANNER, 235, 15, 0xC17E_002B);
    a.rough(region::WINDOW_DARK, 80, 20, 0xC17E_002C);
    a.px
}
