//! The block — one Greyport street, procedural, zero image assets.
//!
//! Layout (metres; the street runs along Z, X across it, ground at
//! y = 0): a warehouse row on the EAST (facades at x = +10.4) and
//! tenements on the WEST (facades at x = −10.4), a 14.4 m street
//! between raised sidewalks, dockyard silhouettes closing the north
//! end, a cross-block the south. A continuous rooftop line runs
//! both rows — parapets, flat roofs — because the grapple survey
//! (PREP §3) reads the block as a volume, not a backdrop.
//!
//! Lamps (PREP §2): a WEST row only, heads reaching over the street
//! from the tenement side, so the lit spine runs down the street's
//! west-centre as a chain of pools and the warehouse wall holds
//! honest shadow — plus TWO east lamps as the exceptions the shadow
//! route must cross. Lamp placement IS the light-read design: the
//! field in light.rs bakes these pools into the street's vertices
//! and answers the read from the same numbers.
//!
//! Canon anchors: the street (#9) — wet slabs, sodium reflection,
//! brick/stone pixel surfaces, roller door, sagging wires, the
//! banner as street furniture, crane + gantry in fog; the patrol
//! frame (#11) — banner on the warehouse wall, lamp pools, dark
//! corners.

use crate::light::{Lamp, LightField, Occluder};
use c35::glam::Vec3;
use c35::props::{region as city, uv_rect, PropMesh, PropVertex, MAT_EMISSIVE, MAT_STANDARD};

pub const BLOCK_HALF_Z: f32 = 48.0;
pub const LAMP_HEAD_Y: f32 = 6.2;

fn vx(p: Vec3, n: Vec3, uv: [f32; 2], mat: f32, light: f32) -> PropVertex {
    PropVertex { position: p.to_array(), normal: n.to_array(), uv, mat, light }
}

/// A mesh under construction: vertices + indices + the field that
/// bakes every vertex's practical light as it lands.
struct B<'a> {
    v: Vec<PropVertex>,
    i: Vec<u32>,
    field: &'a LightField,
}

impl<'a> B<'a> {
    fn new(field: &'a LightField) -> B<'a> {
        B { v: Vec::new(), i: Vec::new(), field }
    }

    fn bake(&self, p: Vec3) -> f32 {
        self.field.exposure(p)
    }

    /// One quad, corners counter-clockwise seen from `n`'s side.
    /// UVs run the full region, corner order matching the corners.
    fn quad(&mut self, c: [Vec3; 4], n: Vec3, r: (u32, u32, u32, u32), mat: f32) {
        let [u0, v0, u1, v1] = uv_rect(r);
        let uvs = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        let base = self.v.len() as u32;
        for k in 0..4 {
            let l = self.bake(c[k]);
            self.v.push(vx(c[k], n, uvs[k], mat, l));
        }
        self.i.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A quad whose light is GIVEN, not baked (emissive glass, wet
    /// streak profiles, wire silhouettes).
    fn quad_lit(&mut self, c: [Vec3; 4], n: Vec3, r: (u32, u32, u32, u32), mat: f32, lights: [f32; 4]) {
        let [u0, v0, u1, v1] = uv_rect(r);
        let uvs = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        let base = self.v.len() as u32;
        for k in 0..4 {
            self.v.push(vx(c[k], n, uvs[k], mat, lights[k]));
        }
        self.i.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Axis-aligned box (all six faces; the caller culls by not
    /// building buried boxes). Full region on every face, like the
    /// character part boxes.
    fn box3(&mut self, mn: Vec3, mx: Vec3, r: (u32, u32, u32, u32), mat: f32) {
        let (x0, y0, z0) = (mn.x, mn.y, mn.z);
        let (x1, y1, z1) = (mx.x, mx.y, mx.z);
        // +Y top
        self.quad(
            [Vec3::new(x0, y1, z1), Vec3::new(x1, y1, z1), Vec3::new(x1, y1, z0), Vec3::new(x0, y1, z0)],
            Vec3::Y, r, mat,
        );
        // −Y bottom
        self.quad(
            [Vec3::new(x0, y0, z0), Vec3::new(x1, y0, z0), Vec3::new(x1, y0, z1), Vec3::new(x0, y0, z1)],
            Vec3::NEG_Y, r, mat,
        );
        // +Z front
        self.quad(
            [Vec3::new(x0, y0, z1), Vec3::new(x1, y0, z1), Vec3::new(x1, y1, z1), Vec3::new(x0, y1, z1)],
            Vec3::Z, r, mat,
        );
        // −Z back
        self.quad(
            [Vec3::new(x1, y0, z0), Vec3::new(x0, y0, z0), Vec3::new(x0, y1, z0), Vec3::new(x1, y1, z0)],
            Vec3::NEG_Z, r, mat,
        );
        // +X right
        self.quad(
            [Vec3::new(x1, y0, z1), Vec3::new(x1, y0, z0), Vec3::new(x1, y1, z0), Vec3::new(x1, y1, z1)],
            Vec3::X, r, mat,
        );
        // −X left
        self.quad(
            [Vec3::new(x0, y0, z0), Vec3::new(x0, y0, z1), Vec3::new(x0, y1, z1), Vec3::new(x0, y1, z0)],
            Vec3::NEG_X, r, mat,
        );
    }

    /// A subdivided vertical facade facing `facing` (−1: faces −X,
    /// the warehouse; +1: faces +X, the tenements) at plane x,
    /// spanning z0..z1 and y 0..h. Grid cells ~3 m so the lamp bake
    /// resolves across the wall.
    fn facade(&mut self, x: f32, facing: f32, z0: f32, z1: f32, h: f32, r: (u32, u32, u32, u32)) {
        let n = Vec3::new(facing, 0.0, 0.0);
        let rows = [0.0, 2.8, 5.6, 8.4, 11.2];
        let mut ys: Vec<f32> = rows.iter().copied().filter(|&y| y < h).collect();
        ys.push(h);
        let cols = ((z1 - z0) / 3.0).ceil() as usize;
        for ci in 0..cols {
            let za = z0 + (z1 - z0) * (ci as f32 / cols as f32);
            let zb = z0 + (z1 - z0) * ((ci + 1) as f32 / cols as f32);
            for wi in 0..ys.len() - 1 {
                let (ya, yb) = (ys[wi], ys[wi + 1]);
                let c = if facing < 0.0 {
                    [
                        Vec3::new(x, ya, za),
                        Vec3::new(x, ya, zb),
                        Vec3::new(x, yb, zb),
                        Vec3::new(x, yb, za),
                    ]
                } else {
                    [
                        Vec3::new(x, ya, zb),
                        Vec3::new(x, ya, za),
                        Vec3::new(x, yb, za),
                        Vec3::new(x, yb, zb),
                    ]
                };
                self.quad(c, n, r, MAT_STANDARD);
            }
        }
    }

    /// A window quad standing slightly proud of a facade.
    fn window(&mut self, x: f32, facing: f32, zc: f32, yc: f32, w: f32, h: f32, lit: bool) {
        let px = x + facing * 0.03; // proud toward the street
        let n = Vec3::new(facing, 0.0, 0.0);
        let (za, zb) = (zc - w / 2.0, zc + w / 2.0);
        let (ya, yb) = (yc - h / 2.0, yc + h / 2.0);
        let c = if facing < 0.0 {
            [
                Vec3::new(px, ya, za),
                Vec3::new(px, ya, zb),
                Vec3::new(px, yb, zb),
                Vec3::new(px, yb, za),
            ]
        } else {
            [
                Vec3::new(px, ya, zb),
                Vec3::new(px, ya, za),
                Vec3::new(px, yb, za),
                Vec3::new(px, yb, zb),
            ]
        };
        let r = if lit { city::WINDOW_LIT } else { city::WINDOW_DARK };
        let m = if lit { MAT_EMISSIVE } else { MAT_STANDARD };
        self.quad(c, n, r, m);
    }

    /// A subdivided horizontal plane at height y (street, sidewalk,
    /// roofs): xs × zs grid, every vertex baked.
    fn plane(&mut self, xs: &[f32], zs: &[f32], y: f32, r: (u32, u32, u32, u32), mat: f32) {
        for ci in 0..xs.len() - 1 {
            for wi in 0..zs.len() - 1 {
                let (xa, xb) = (xs[ci], xs[ci + 1]);
                let (za, zb) = (zs[wi], zs[wi + 1]);
                self.quad(
                    [
                        Vec3::new(xa, y, za),
                        Vec3::new(xb, y, za),
                        Vec3::new(xb, y, zb),
                        Vec3::new(xa, y, zb),
                    ],
                    Vec3::Y,
                    r,
                    mat,
                );
            }
        }
    }

    /// A sagging wire: `segs` ribbon quads along a→b with a catenary
    /// dip. Dark silhouette — baked light near zero by construction
    /// of the field at height; given explicitly so a wire passing a
    /// lamp head does not glow like a filament.
    fn wire(&mut self, a: Vec3, b: Vec3, sag: f32) {
        let segs = 6;
        let mut prev = a;
        for s in 1..=segs {
            let t = s as f32 / segs as f32;
            let mut p = a.lerp(b, t);
            p.y -= sag * 4.0 * t * (1.0 - t);
            let up = Vec3::new(0.0, 0.018, 0.0);
            self.quad_lit(
                [prev + up, p + up, p - up, prev - up],
                Vec3::Z,
                city::METAL_DARK,
                MAT_STANDARD,
                [0.0; 4],
            );
            prev = p;
        }
    }
}

/// Deterministic lit-window pick (a hash, not an RNG — the block
/// builds identically every run).
fn lit_pick(a: u32, b: u32, c: u32) -> bool {
    let mut h = a.wrapping_mul(0x9E3779B1) ^ b.wrapping_mul(0x85EBCA6B) ^ c.wrapping_mul(0xC2B2AE35);
    h ^= h >> 13;
    h = h.wrapping_mul(0x27D4EB2F);
    h ^= h >> 15;
    (h % 100) < 26
}

/// One grapple anchor candidate (PREP §3): recorded, never used —
/// traversal is a later milestone. The swing volume is described by
/// the anchor's height (pendulum room below it) and the horizontal
/// clearance to the nearest building face, plus a note naming what
/// the anchor opens.
#[derive(Clone, Debug)]
pub struct Anchor {
    pub pos: Vec3,
    pub kind: &'static str,
    pub height: f32,
    pub clearance: f32,
    pub opens: &'static str,
}

pub struct Block {
    pub mesh: PropMesh,
    pub field: LightField,
    pub anchors: Vec<Anchor>,
    pub census: String,
}

/// Lamp plan (PREP §2): west row of ten + two east exceptions.
fn lamp_plan() -> (Vec<Lamp>, Vec<(Vec3, Vec3)>) {
    // (pole base position, head position)
    let mut plan: Vec<(Vec3, Vec3)> = Vec::new();
    let mut zi = -40.5f32;
    while zi <= 40.5 {
        plan.push((Vec3::new(-7.55, 0.0, zi), Vec3::new(-4.4, LAMP_HEAD_Y, zi)));
        zi += 9.0;
    }
    plan.push((Vec3::new(7.55, 0.0, -20.0), Vec3::new(4.4, LAMP_HEAD_Y, -20.0)));
    plan.push((Vec3::new(7.55, 0.0, 20.0), Vec3::new(4.4, LAMP_HEAD_Y, 20.0)));
    let lamps = plan.iter().map(|&(_, head)| Lamp { head }).collect();
    (lamps, plan)
}

/// Building volumes for the field's occlusion (and the survey's
/// clearance math): the two rows + end caps + the kiosk.
fn occluders() -> Vec<Occluder> {
    let mut v = Vec::new();
    // Warehouse segments (facade x 10.4, depth to 16.4).
    for (z0, z1, h) in [(-48.0, -24.0, 12.0), (-24.0, 0.0, 14.0), (0.0, 24.0, 11.0), (24.0, 48.0, 13.0)] {
        v.push(Occluder::new(Vec3::new(10.4, 0.0, z0), Vec3::new(16.4, h, z1)));
    }
    // Tenement segments (facade x −10.4, depth to −16.4).
    for (z0, z1, h) in [(-48.0, -16.0, 15.0), (-16.0, 16.0, 17.0), (16.0, 48.0, 13.5)] {
        v.push(Occluder::new(Vec3::new(-16.4, 0.0, z0), Vec3::new(-10.4, h, z1)));
    }
    // End caps + kiosk.
    v.push(Occluder::new(Vec3::new(-16.0, 0.0, -56.0), Vec3::new(16.0, 18.0, -48.0)));
    v.push(Occluder::new(Vec3::new(-16.0, 0.0, 48.0), Vec3::new(16.0, 9.0, 54.0)));
    v.push(Occluder::new(Vec3::new(7.5, 0.0, 28.9), Vec3::new(9.7, 2.6, 31.1)));
    v
}

/// Build the whole block: field first (it bakes the geometry),
/// geometry second, survey last.
pub fn build_block() -> Block {
    let (lamps, plan) = lamp_plan();
    let field = LightField { lamps, occluders: occluders() };
    let mut b = B::new(&field);

    // ---------- ground ----------
    // The street: columns gather at the lamp spine (x −4.4) and the
    // wall shadow lines so the bake's gradient lands on vertices.
    let street_xs = [-7.2, -5.6, -4.4, -3.2, -1.0, 1.2, 3.2, 4.4, 5.6, 7.2];
    let mut street_zs: Vec<f32> = Vec::new();
    let mut z = -BLOCK_HALF_Z;
    while z < BLOCK_HALF_Z {
        street_zs.push(z);
        z += 1.2;
    }
    street_zs.push(BLOCK_HALF_Z);
    b.plane(&street_xs, &street_zs, 0.0, city::ASPHALT, c35::props::MAT_WET);
    // Sidewalks (raised slabs).
    let walk_xs_e = [7.2, 8.8, 10.4];
    let walk_xs_w = [-10.4, -8.8, -7.2];
    b.plane(&walk_xs_e, &street_zs, 0.12, city::CONCRETE, c35::props::MAT_WET);
    b.plane(&walk_xs_w, &street_zs, 0.12, city::CONCRETE, c35::props::MAT_WET);
    // Curb faces.
    for (x, facing) in [(7.2f32, -1.0f32), (-7.2f32, 1.0f32)] {
        let n = Vec3::new(facing, 0.0, 0.0);
        for wi in 0..street_zs.len() - 1 {
            let (za, zb) = (street_zs[wi], street_zs[wi + 1]);
            let c = if facing < 0.0 {
                [Vec3::new(x, 0.0, za), Vec3::new(x, 0.0, zb), Vec3::new(x, 0.12, zb), Vec3::new(x, 0.12, za)]
            } else {
                [Vec3::new(x, 0.0, zb), Vec3::new(x, 0.0, za), Vec3::new(x, 0.12, za), Vec3::new(x, 0.12, zb)]
            };
            b.quad(c, n, city::CONCRETE, c35::props::MAT_WET);
        }
    }
    // (No painted streak geometry: two builds of it read as slabs
    // and panels — hardware, not water. The wet pass in the props
    // shader mirrors the lamps per-fragment on the MAT_WET ground
    // instead, and the streak emerges where a reflection should.)

    // ---------- warehouse row (east) ----------
    let wh = [
        (-48.0f32, -24.0f32, 12.0f32),
        (-24.0, 0.0, 14.0),
        (0.0, 24.0, 11.0),
        (24.0, 48.0, 13.0),
    ];
    for (si, &(z0, z1, h)) in wh.iter().enumerate() {
        b.facade(10.4, -1.0, z0, z1, h, city::BRICK);
        // Roof + parapet (the continuous rooftop line).
        b.plane(&[10.4, 13.4, 16.4], &[z0, z1], h, city::ROOF, MAT_STANDARD);
        b.box3(Vec3::new(10.4, h, z0), Vec3::new(10.95, h + 0.5, z1), city::BRICK, MAT_STANDARD);
        // Windows: two floors, sparse; a few lit.
        for (fi, &yc) in [4.1f32, 7.3, 10.2].iter().enumerate() {
            if yc > h - 1.2 {
                continue;
            }
            let mut zc = z0 + 3.4;
            let mut ci = 0u32;
            while zc < z1 - 2.0 {
                // The banner owns its wall (segment 3, z 15..23).
                let banner_wall = si == 2 && (15.0..23.5).contains(&zc) && yc > 3.5;
                if !banner_wall {
                    let lit = lit_pick(si as u32, fi as u32, ci);
                    b.window(10.4, -1.0, zc, yc, 1.5, 2.0, lit);
                }
                zc += 6.4;
                ci += 1;
            }
        }
    }
    // Roller doors on segments 1 and 3 + personnel doors.
    for &zc in &[-36.0f32, 8.0] {
        b.quad(
            [
                Vec3::new(10.36, 0.12, zc - 2.1),
                Vec3::new(10.36, 0.12, zc + 2.1),
                Vec3::new(10.36, 4.7, zc + 2.1),
                Vec3::new(10.36, 4.7, zc - 2.1),
            ],
            Vec3::NEG_X,
            city::ROLLER,
            MAT_STANDARD,
        );
        b.box3(Vec3::new(10.3, 4.7, zc - 2.3), Vec3::new(10.45, 5.05, zc + 2.3), city::METAL_DARK, MAT_STANDARD);
    }
    for &zc in &[-6.0f32, 30.0] {
        b.quad(
            [
                Vec3::new(10.36, 0.12, zc - 0.55),
                Vec3::new(10.36, 0.12, zc + 0.55),
                Vec3::new(10.36, 2.5, zc + 0.55),
                Vec3::new(10.36, 2.5, zc - 0.55),
            ],
            Vec3::NEG_X,
            city::DOOR,
            MAT_STANDARD,
        );
    }
    // THE BANNER — three bars on the warehouse wall (segment 3),
    // the street's furniture and the patrol frame's backdrop.
    b.quad(
        [
            Vec3::new(10.34, 4.4, 16.6),
            Vec3::new(10.34, 4.4, 21.4),
            Vec3::new(10.34, 10.8, 21.4),
            Vec3::new(10.34, 10.8, 16.6),
        ],
        Vec3::NEG_X,
        city::BANNER,
        MAT_STANDARD,
    );
    // Warehouse loading gantries over the roller doors: posts + beam.
    for &zc in &[-36.0f32, 8.0] {
        b.box3(Vec3::new(9.3, 0.0, zc - 2.5), Vec3::new(9.55, 5.5, zc - 2.25), city::METAL_DARK, MAT_STANDARD);
        b.box3(Vec3::new(9.3, 0.0, zc + 2.25), Vec3::new(9.55, 5.5, zc + 2.5), city::METAL_DARK, MAT_STANDARD);
        b.box3(Vec3::new(9.3, 5.3, zc - 2.5), Vec3::new(9.55, 5.6, zc + 2.5), city::METAL_DARK, MAT_STANDARD);
    }

    // ---------- tenements (west) ----------
    let ten = [
        (-48.0f32, -16.0f32, 15.0f32),
        (-16.0, 16.0, 17.0),
        (16.0, 48.0, 13.5),
    ];
    for (si, &(z0, z1, h)) in ten.iter().enumerate() {
        b.facade(-10.4, 1.0, z0, z1, h, city::BRICK_DARK);
        b.plane(&[-16.4, -13.4, -10.4], &[z0, z1], h, city::ROOF, MAT_STANDARD);
        b.box3(Vec3::new(-10.95, h, z0), Vec3::new(-10.4, h + 0.5, z1), city::BRICK_DARK, MAT_STANDARD);
        // Stone base + cornice: the tenement's civic dress.
        b.box3(Vec3::new(-10.55, 0.0, z0), Vec3::new(-10.4, 1.25, z1), city::STONE, MAT_STANDARD);
        b.box3(Vec3::new(-10.62, h - 0.55, z0), Vec3::new(-10.4, h - 0.15, z1), city::STONE, MAT_STANDARD);
        // Windows: three floors, denser, tenement rhythm.
        for (fi, &yc) in [3.4f32, 6.6, 9.8, 13.0].iter().enumerate() {
            if yc > h - 1.4 {
                continue;
            }
            let mut zc = z0 + 2.6;
            let mut ci = 0u32;
            while zc < z1 - 1.6 {
                let lit = lit_pick(40 + si as u32, fi as u32, ci);
                b.window(-10.4, 1.0, zc, yc, 1.25, 1.9, lit);
                zc += 4.6;
                ci += 1;
            }
        }
    }
    // A water tower on the middle tenement's roof: skyline beat.
    b.box3(Vec3::new(-13.6, 17.0, -2.6), Vec3::new(-13.3, 19.2, -2.3), city::METAL_DARK, MAT_STANDARD);
    b.box3(Vec3::new(-11.4, 17.0, -2.6), Vec3::new(-11.1, 19.2, -2.3), city::METAL_DARK, MAT_STANDARD);
    b.box3(Vec3::new(-13.9, 19.2, -3.1), Vec3::new(-10.8, 21.6, -1.8), city::TIMBER, MAT_STANDARD);
    b.box3(Vec3::new(-13.5, 21.6, -2.85), Vec3::new(-11.2, 22.3, -2.05), city::METAL_DARK, MAT_STANDARD);

    // ---------- end caps + dockyard (north) ----------
    // North cross block face (z = −48, facing +Z).
    {
        let n = Vec3::Z;
        for (xa, xb) in [(-16.0f32, -5.4f32), (-5.4, 5.4), (5.4, 16.0)] {
            for (ya, yb) in [(0.0f32, 6.0f32), (6.0, 12.0), (12.0, 18.0)] {
                b.quad(
                    [Vec3::new(xa, ya, -48.0), Vec3::new(xb, ya, -48.0), Vec3::new(xb, yb, -48.0), Vec3::new(xa, yb, -48.0)],
                    n, city::BRICK_DARK, MAT_STANDARD,
                );
            }
        }
    }
    // South cross block face (z = +48, facing −Z).
    {
        let n = Vec3::NEG_Z;
        for (xa, xb) in [(-16.0f32, -5.4f32), (-5.4, 5.4), (5.4, 16.0)] {
            for (ya, yb) in [(0.0f32, 4.5f32), (4.5, 9.0)] {
                b.quad(
                    [Vec3::new(xb, ya, 48.0), Vec3::new(xa, ya, 48.0), Vec3::new(xa, yb, 48.0), Vec3::new(xb, yb, 48.0)],
                    n, city::BRICK_DARK, MAT_STANDARD,
                );
            }
        }
    }
    // The crane (north, in fog): mast, jib, counter-jib, hook line.
    b.box3(Vec3::new(7.6, 0.0, -54.4), Vec3::new(8.4, 21.0, -53.6), city::METAL_DARK, MAT_STANDARD);
    b.box3(Vec3::new(-6.0, 20.3, -54.3), Vec3::new(8.4, 21.0, -53.7), city::METAL_DARK, MAT_STANDARD);
    b.box3(Vec3::new(8.4, 19.2, -54.3), Vec3::new(10.6, 21.0, -53.7), city::METAL_DARK, MAT_STANDARD);
    b.box3(Vec3::new(-0.35, 8.4, -54.15), Vec3::new(0.35, 9.1, -53.85), city::METAL_DARK, MAT_STANDARD);
    b.wire(Vec3::new(0.0, 20.3, -54.0), Vec3::new(0.0, 9.1, -54.0), 0.0);
    // A hull in the dock beyond.
    b.box3(Vec3::new(-3.0, 0.0, -63.0), Vec3::new(15.0, 6.5, -56.0), city::METAL_DARK, MAT_STANDARD);
    // The gantry at the street's north mouth.
    b.box3(Vec3::new(-9.3, 0.0, -50.3), Vec3::new(-8.8, 9.0, -49.8), city::METAL_DARK, MAT_STANDARD);
    b.box3(Vec3::new(8.8, 0.0, -50.3), Vec3::new(9.3, 9.0, -49.8), city::METAL_DARK, MAT_STANDARD);
    b.box3(Vec3::new(-9.3, 8.7, -50.3), Vec3::new(9.3, 9.35, -49.8), city::METAL_DARK, MAT_STANDARD);
    // Second-rank silhouettes behind both rows.
    b.box3(Vec3::new(19.0, 0.0, -30.0), Vec3::new(27.0, 19.0, -6.0), city::BRICK_DARK, MAT_STANDARD);
    b.box3(Vec3::new(19.0, 0.0, 6.0), Vec3::new(27.0, 16.0, 34.0), city::METAL_DARK, MAT_STANDARD);
    b.box3(Vec3::new(-27.0, 0.0, -20.0), Vec3::new(-19.0, 21.0, 4.0), city::BRICK_DARK, MAT_STANDARD);
    b.box3(Vec3::new(-27.0, 0.0, 14.0), Vec3::new(-19.0, 15.0, 40.0), city::METAL_DARK, MAT_STANDARD);

    // ---------- dockyard + street dressing ----------
    // Crates and a container stack by the north roller door.
    b.box3(Vec3::new(8.6, 0.12, -41.5), Vec3::new(10.1, 1.35, -40.0), city::TIMBER, MAT_STANDARD);
    b.box3(Vec3::new(8.7, 1.35, -41.4), Vec3::new(9.9, 2.4, -40.2), city::TIMBER, MAT_STANDARD);
    b.box3(Vec3::new(8.4, 0.12, -39.6), Vec3::new(10.2, 1.1, -38.4), city::TIMBER, MAT_STANDARD);
    b.box3(Vec3::new(11.2, 0.0, -44.5), Vec3::new(13.6, 2.5, -38.5), city::METAL_DARK, MAT_STANDARD);
    // Checkpoint-lite (set dressing, south-east): kiosk, barrier,
    // drums. The HALT is staged here later; in Slice 1 it is a
    // dressed corner the drives pass.
    b.box3(Vec3::new(7.5, 0.0, 28.9), Vec3::new(9.7, 2.6, 31.1), city::TIMBER, MAT_STANDARD);
    b.box3(Vec3::new(7.35, 2.6, 28.75), Vec3::new(9.85, 2.85, 31.25), city::METAL_DARK, MAT_STANDARD);
    b.quad(
        [
            Vec3::new(7.44, 1.35, 29.3),
            Vec3::new(7.44, 1.35, 30.7),
            Vec3::new(7.44, 2.15, 30.7),
            Vec3::new(7.44, 2.15, 29.3),
        ],
        Vec3::NEG_X,
        city::WINDOW_LIT,
        MAT_EMISSIVE,
    );
    // Barrier: pivot post + the arm across the street's east lane.
    b.box3(Vec3::new(7.3, 0.0, 28.35), Vec3::new(7.75, 1.1, 28.8), city::METAL_DARK, MAT_STANDARD);
    b.box3(Vec3::new(2.1, 0.98, 28.5), Vec3::new(7.5, 1.14, 28.66), city::TIMBER, MAT_STANDARD);
    b.box3(Vec3::new(1.95, 0.0, 28.42), Vec3::new(2.25, 1.0, 28.74), city::METAL_DARK, MAT_STANDARD);
    // Drums by the kiosk.
    for &(dx, dz) in &[(6.6f32, 31.6f32), (7.15, 32.15), (6.7, 32.3)] {
        b.box3(Vec3::new(dx, 0.0, dz), Vec3::new(dx + 0.58, 0.92, dz + 0.58), city::METAL_DARK, MAT_STANDARD);
    }

    // ---------- lamps (geometry) ----------
    let mut lamp_heads: Vec<Vec3> = Vec::new();
    for &(pole, head) in &plan {
        lamp_heads.push(head);
        let px = pole.x;
        let pz = pole.z;
        // Base, pole, arm, head housing, glass.
        b.box3(Vec3::new(px - 0.16, 0.12, pz - 0.16), Vec3::new(px + 0.16, 0.5, pz + 0.16), city::METAL_DARK, MAT_STANDARD);
        b.box3(Vec3::new(px - 0.055, 0.12, pz - 0.055), Vec3::new(px + 0.055, 6.05, pz + 0.055), city::METAL_DARK, MAT_STANDARD);
        let (ax0, ax1) = if head.x < px { (head.x - 0.3, px) } else { (px, head.x + 0.3) };
        b.box3(Vec3::new(ax0, 5.98, pz - 0.05), Vec3::new(ax1, 6.08, pz + 0.05), city::METAL_DARK, MAT_STANDARD);
        b.box3(Vec3::new(head.x - 0.34, 6.14, pz - 0.15), Vec3::new(head.x + 0.34, 6.3, pz + 0.15), city::METAL_DARK, MAT_STANDARD);
        // A glowing core band around the housing, so the fixture
        // reads as a LIT lamp from street level (the down-glass
        // alone is invisible at grazing angles).
        b.box3(
            Vec3::new(head.x - 0.26, 6.17, pz - 0.165),
            Vec3::new(head.x + 0.26, 6.27, pz + 0.165),
            city::LAMP_GLASS,
            MAT_EMISSIVE,
        );
        // The glass: emissive, facing down — the pool's source.
        b.quad_lit(
            [
                Vec3::new(head.x - 0.28, 6.13, pz - 0.11),
                Vec3::new(head.x + 0.28, 6.13, pz - 0.11),
                Vec3::new(head.x + 0.28, 6.13, pz + 0.11),
                Vec3::new(head.x - 0.28, 6.13, pz + 0.11),
            ],
            Vec3::NEG_Y,
            city::LAMP_GLASS,
            MAT_EMISSIVE,
            [0.0; 4],
        );
    }
    // ---------- wires ----------
    // Along the west row, pole to pole (sagging), and four cross
    // spans to the warehouse face; one span carries a street banner.
    let west_z: Vec<f32> = plan.iter().filter(|(p, _)| p.x < 0.0).map(|(p, _)| p.z).collect();
    for w in west_z.windows(2) {
        b.wire(Vec3::new(-7.55, 5.92, w[0]), Vec3::new(-7.55, 5.92, w[1]), 0.55);
    }
    for &zc in &[-31.5f32, -4.5, 13.5, 31.5] {
        b.wire(Vec3::new(-7.55, 5.95, zc), Vec3::new(10.4, 7.6, zc), 0.85);
    }
    // The street banner hung from the z = -4.5 span, its top edge
    // following the wire's sag (wire y at the banner's ends ~5.83
    // and ~6.04; short hanger straps close the gap).
    {
        b.quad(
            [
                Vec3::new(0.3, 2.8, -4.5),
                Vec3::new(2.5, 3.0, -4.5),
                Vec3::new(2.5, 5.7, -4.5),
                Vec3::new(0.3, 5.5, -4.5),
            ],
            Vec3::Z,
            city::BANNER,
            MAT_STANDARD,
        );
        b.wire(Vec3::new(0.3, 5.83, -4.5), Vec3::new(0.3, 5.5, -4.5), 0.0);
        b.wire(Vec3::new(2.5, 6.04, -4.5), Vec3::new(2.5, 5.7, -4.5), 0.0);
    }

    let tris = (b.i.len() / 3) as u32;
    let census = format!(
        "block: {} vertices, {} triangles, {} lamps, {} occluders",
        b.v.len(),
        tris,
        lamp_heads.len(),
        field.occluders.len()
    );
    let mesh = PropMesh { vertices: b.v, indices: b.i };
    let anchors = anchor_survey(&field, &lamp_heads);
    Block { mesh, field, anchors, census }
}

/// The grapple anchor survey (PREP §3): candidates recorded as
/// data — parapet edges and rooftop lines, wire runs, the crane
/// jib, warehouse gantries, lamp standards (short hops only). Each
/// carries the swing volume it would open. NO traversal is built
/// on this in Slice 1; the traversal milestone reads this table.
fn anchor_survey(field: &LightField, lamp_heads: &[Vec3]) -> Vec<Anchor> {
    let mut v: Vec<Anchor> = Vec::new();
    let parapets: [(f32, f32, f32, f32); 7] = [
        // (x, z0, z1, parapet-top y) per row segment
        (10.65, -48.0, -24.0, 12.5),
        (10.65, -24.0, 0.0, 14.5),
        (10.65, 0.0, 24.0, 11.5),
        (10.65, 24.0, 48.0, 13.5),
        (-10.65, -48.0, -16.0, 15.5),
        (-10.65, -16.0, 16.0, 17.5),
        (-10.65, 16.0, 48.0, 14.0),
    ];
    for &(x, z0, z1, y) in &parapets {
        let mut z = z0 + 4.0;
        while z < z1 {
            v.push(Anchor {
                pos: Vec3::new(x, y, z),
                kind: if x > 0.0 { "parapet edge (warehouse)" } else { "parapet edge (tenement)" },
                height: y,
                clearance: x.abs() + 10.4, // to the opposite facade
                opens: if x > 0.0 { "the east rooftop line; descent to the warehouse wall shadow" } else { "the west rooftop line; the lit spine from above" },
            });
            z += 8.0;
        }
    }
    // Wire runs: the four cross spans (midpoints) + along-street runs.
    for &zc in &[-31.5f32, -4.5, 13.5, 31.5] {
        v.push(Anchor {
            pos: Vec3::new(1.4, 6.6, zc),
            kind: "wire run (cross span)",
            height: 6.6,
            clearance: 17.8,
            opens: "a street crossing above patrol eye line; drops to either sidewalk",
        });
    }
    for &zc in &[-36.0f32, -18.0, 0.0, 18.0, 36.0] {
        v.push(Anchor {
            pos: Vec3::new(-7.55, 5.65, zc),
            kind: "wire run (along street)",
            height: 5.65,
            clearance: 3.1,
            opens: "a short slide along the tenement wall; lamp-to-lamp hops",
        });
    }
    // Crane jib + street gantry + warehouse gantries.
    for &x in &[-4.0f32, 0.0, 4.0] {
        v.push(Anchor {
            pos: Vec3::new(x, 20.6, -54.0),
            kind: "crane jib",
            height: 20.6,
            clearance: 12.0,
            opens: "the dockyard mouth; the longest swing on the block, over the north end",
        });
    }
    v.push(Anchor {
        pos: Vec3::new(0.0, 9.0, -50.0),
        kind: "street gantry beam",
        height: 9.0,
        clearance: 17.8,
        opens: "the north mouth crossing; a perch above the street's end",
    });
    for &zc in &[-36.0f32, 8.0] {
        v.push(Anchor {
            pos: Vec3::new(9.42, 5.45, zc),
            kind: "warehouse gantry",
            height: 5.45,
            clearance: 9.4,
            opens: "the roller-door face; a swing into the wall shadow or up to the parapet line",
        });
    }
    // Lamp standards: every head — short hops only (the pole is a
    // line, not a perch).
    for (i, &head) in lamp_heads.iter().enumerate() {
        let _ = i;
        v.push(Anchor {
            pos: head,
            kind: "lamp standard",
            height: head.y,
            clearance: 3.3,
            opens: "a short hop only — across the pool, never along it",
        });
    }
    let _ = field;
    v
}

/// Serialize the survey as CSV for the deliverable.
pub fn anchors_csv(anchors: &[Anchor]) -> String {
    let mut s = String::from("kind,x,y,z,height_m,clearance_m,opens\n");
    for a in anchors {
        s.push_str(&format!(
            "{},{:.2},{:.2},{:.2},{:.2},{:.2},\"{}\"\n",
            a.kind, a.pos.x, a.pos.y, a.pos.z, a.height, a.clearance, a.opens
        ));
    }
    s
}
