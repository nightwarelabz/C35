//! Part recipes into [`crate::props::PropMesh`].
//!
//! This is the only geometry path mod props ever take: a recipe's
//! parts are emitted as vertices/indices in exactly the `PropMesh`
//! shape the renderer already draws (and the host already uploads
//! through `set_props`). No renderer change, no shader change, no
//! new vertex format — a mod prop is indistinguishable, at draw
//! time, from a shipped prop.
//!
//! Conventions, all frozen:
//! - Parts are centred on their `position`, sized by `size`.
//! - Rotation is Euler degrees, applied as Rz * Ry * Rx.
//! - Face normals are computed from winding (cross product), never
//!   hand-written, so a rotated part's lighting stays correct.
//! - UVs are planar projections of the part's local coordinates
//!   into its material region: a box face maps the full region
//!   (the same convention the figure builder uses); a cylinder
//!   side wraps continuously around the region.
//! - The baked `light` term is 0.0. The host's light field bakes
//!   lamp terms into vertices when it builds the world, exactly as
//!   it does for shipped geometry; the mesh builder does not invent
//!   light.

use crate::props::{uv_rect, PropMesh, PropVertex};
use crate::modding::model::{Part, Primitive};

/// Emit a prop's mesh from its resolved parts.
pub fn build_mesh(parts: &[Part]) -> PropMesh {
    let mut mesh = PropMesh::new();
    for part in parts {
        emit_part(&mut mesh, part);
    }
    mesh
}

type Poly = Vec<[f32; 3]>;

fn local_polys(part: &Part) -> Vec<Poly> {
    let (sx, sy, sz) = (part.size[0], part.size[1], part.size[2]);
    let (hx, hy, hz) = (sx / 2.0, sy / 2.0, sz / 2.0);
    match part.primitive {
        Primitive::Box => vec![
            // +X, -X, +Y, -Y, +Z, -Z (each wound counter-clockwise
            // seen from outside).
            vec![
                [hx, -hy, -hz],
                [hx, hy, -hz],
                [hx, hy, hz],
                [hx, -hy, hz],
            ],
            vec![
                [-hx, -hy, hz],
                [-hx, hy, hz],
                [-hx, hy, -hz],
                [-hx, -hy, -hz],
            ],
            vec![
                [-hx, hy, -hz],
                [-hx, hy, hz],
                [hx, hy, hz],
                [hx, hy, -hz],
            ],
            vec![
                [-hx, -hy, hz],
                [-hx, -hy, -hz],
                [hx, -hy, -hz],
                [hx, -hy, hz],
            ],
            vec![
                [-hx, -hy, hz],
                [-hx, hy, hz],
                [hx, hy, hz],
                [hx, -hy, hz],
            ],
            vec![
                [hx, -hy, -hz],
                [hx, hy, -hz],
                [-hx, hy, -hz],
                [-hx, -hy, -hz],
            ],
        ],
        Primitive::Wedge => {
            // Right-triangle cross-section in XY (right angle at
            // (-hx,-hy), hypotenuse from (hx,-hy) to (-hx,hy)),
            // extruded along Z.
            let a = [-hx, -hy];
            let b = [hx, -hy];
            let c = [-hx, hy];
            vec![
                // Front triangle (z = +hz).
                vec![
                    [a[0], a[1], hz],
                    [b[0], b[1], hz],
                    [c[0], c[1], hz],
                ],
                // Back triangle (z = -hz), reversed winding.
                vec![
                    [a[0], a[1], -hz],
                    [c[0], c[1], -hz],
                    [b[0], b[1], -hz],
                ],
                // Bottom (y = -hy).
                vec![
                    [a[0], a[1], -hz],
                    [b[0], b[1], -hz],
                    [b[0], b[1], hz],
                    [a[0], a[1], hz],
                ],
                // Back (x = -hx).
                vec![
                    [a[0], a[1], hz],
                    [c[0], c[1], hz],
                    [c[0], c[1], -hz],
                    [a[0], a[1], -hz],
                ],
                // Slope (hypotenuse).
                vec![
                    [b[0], b[1], -hz],
                    [c[0], c[1], -hz],
                    [c[0], c[1], hz],
                    [b[0], b[1], hz],
                ],
            ]
        }
        Primitive::Cylinder => {
            // 8-sided prism along Y. Radius from size X/Z.
            let n = 8usize;
            let mut rim_top: Vec<[f32; 3]> = Vec::new();
            let mut rim_bot: Vec<[f32; 3]> = Vec::new();
            for i in 0..n {
                let t = (i as f32) / (n as f32) * std::f32::consts::TAU;
                rim_top.push([t.cos() * hx, hy, t.sin() * hz]);
                rim_bot.push([t.cos() * hx, -hy, t.sin() * hz]);
            }
            let mut polys: Vec<Poly> = Vec::new();
            for i in 0..n {
                let j = (i + 1) % n;
                polys.push(vec![
                    rim_bot[i],
                    rim_bot[j],
                    rim_top[j],
                    rim_top[i],
                ]);
            }
            // Caps as fans.
            let mut top: Poly = vec![[0.0, hy, 0.0]];
            for i in 0..n {
                top.push(rim_top[i]);
            }
            let mut bot: Poly = vec![[0.0, -hy, 0.0]];
            for i in (0..n).rev() {
                bot.push(rim_bot[i]);
            }
            polys.push(top);
            polys.push(bot);
            polys
        }
        Primitive::Quad => {
            // Flat panel in local XY, facing +Z. Emitted
            // double-sided by the caller (both windings).
            let q = vec![
                [-hx, -hy, 0.0],
                [hx, -hy, 0.0],
                [hx, hy, 0.0],
                [-hx, hy, 0.0],
            ];
            let mut rev = q.clone();
            rev.reverse();
            vec![q, rev]
        }
    }
}

fn rot_matrix(deg: [f32; 3]) -> [[f32; 3]; 3] {
    let (rx, ry, rz) = (
        deg[0].to_radians(),
        deg[1].to_radians(),
        deg[2].to_radians(),
    );
    let (sx, cx) = (rx.sin(), rx.cos());
    let (sy, cy) = (ry.sin(), ry.cos());
    let (sz, cz) = (rz.sin(), rz.cos());
    // Rx, Ry, Rz, then R = Rz * Ry * Rx.
    let rxm = [
        [1.0, 0.0, 0.0],
        [0.0, cx, -sx],
        [0.0, sx, cx],
    ];
    let rym = [
        [cy, 0.0, sy],
        [0.0, 1.0, 0.0],
        [-sy, 0.0, cy],
    ];
    let rzm = [
        [cz, -sz, 0.0],
        [sz, cz, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let mul = |a: [[f32; 3]; 3], b: [[f32; 3]; 3]| -> [[f32; 3]; 3] {
        let mut o = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                o[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
            }
        }
        o
    };
    mul(mul(rzm, rym), rxm)
}

fn mat_vec(m: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-9 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn emit_part(mesh: &mut PropMesh, part: &Part) {
    let rot = rot_matrix(part.rotation);
    let uv = uv_rect(part.region);
    for poly in local_polys(part) {
        if poly.len() < 3 {
            continue;
        }
        // Face normal from winding (local space).
        let n_local = normalize(cross(sub(poly[1], poly[0]), sub(poly[2], poly[0])));
        let n_world = normalize(mat_vec(rot, n_local));
        // Projection axes: drop the dominant normal axis.
        let ax = {
            let a = n_local[0].abs();
            let b = n_local[1].abs();
            let c = n_local[2].abs();
            if a >= b && a >= c {
                0
            } else if b >= c {
                1
            } else {
                2
            }
        };
        let (pa, pb) = match ax {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        let base = mesh.vertices.len() as u32;
        for v in &poly {
            let world = mat_vec(rot, *v);
            let world = [
                world[0] + part.position[0],
                world[1] + part.position[1],
                world[2] + part.position[2],
            ];
            // Planar projection into the region: local coords
            // normalised by the part's size on each axis, v growing
            // downward (the authored atlas convention).
            let ua = (v[pa] / part.size[pa]).clamp(-0.5, 0.5) + 0.5;
            let vb = 0.5 - (v[pb] / part.size[pb]).clamp(-0.5, 0.5);
            let u = uv[0] + ua * (uv[2] - uv[0]);
            let vv = uv[1] + vb * (uv[3] - uv[1]);
            mesh.vertices.push(PropVertex {
                position: world,
                normal: n_world,
                uv: [u, vv],
                mat: part.mat_flag,
                light: 0.0,
            });
        }
        // Fan triangulation.
        for i in 1..poly.len() - 1 {
            mesh.indices
                .extend_from_slice(&[base, base + i as u32, base + i as u32 + 1]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modding::model::*;
    use crate::props::region;

    fn mkpart(prim: Primitive) -> Part {
        Part {
            primitive: prim,
            position: [0.0, 0.0, 0.0],
            size: [1.0, 1.0, 1.0],
            rotation: [0.0, 0.0, 0.0],
            region: region::BRICK,
            mat_flag: 0.0,
        }
    }

    #[test]
    fn box_emits_12_tris() {
        let m = build_mesh(&[mkpart(Primitive::Box)]);
        assert_eq!(m.indices.len(), 36);
        assert_eq!(m.vertices.len(), 24);
    }

    #[test]
    fn wedge_emits_8_tris() {
        let m = build_mesh(&[mkpart(Primitive::Wedge)]);
        assert_eq!(m.indices.len(), 24);
    }

    #[test]
    fn cylinder_emits_30_tris() {
        let m = build_mesh(&[mkpart(Primitive::Cylinder)]);
        assert_eq!(m.indices.len(), 90);
    }

    #[test]
    fn quad_is_double_sided_4_tris() {
        let m = build_mesh(&[mkpart(Primitive::Quad)]);
        assert_eq!(m.indices.len(), 12);
    }

    #[test]
    fn rotated_part_keeps_unit_normals() {
        let mut p = mkpart(Primitive::Box);
        p.rotation = [30.0, 45.0, 0.0];
        let m = build_mesh(&[p]);
        for v in &m.vertices {
            let len = (v.normal[0] * v.normal[0]
                + v.normal[1] * v.normal[1]
                + v.normal[2] * v.normal[2])
                .sqrt();
            assert!((len - 1.0).abs() < 1e-4, "normal len {len}");
        }
    }
}
