//! UV projection: the three ways a modeled face gets its
//! region-local UVs. All three write corner UVs in the mesh's
//! 0..1-per-material-region space; `scale` is in UV units per
//! world metre, so a caller who wants a lamp pole's painted
//! metal to show one panel seam per metre projects at a scale
//! that puts a metre at the seam spacing. Projections never
//! clamp — a face can deliberately span the region more than
//! once — but the generators and the proof asset stay inside
//! 0..1, where the atlas region reads as authored.

use super::mesh::*;
use super::ops::{OpError, OpResult};

/// The two axes a plane keeps when `drop` is its dominant axis:
/// the recipe builder's projection choice, shared so modeled
/// and recipe geometry agree about which way is across.
fn plane_axes(drop: usize) -> (usize, usize) {
    match drop {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

fn dominant(n: [f32; 3]) -> usize {
    let (a, b, c) = (n[0].abs(), n[1].abs(), n[2].abs());
    if a >= b && a >= c {
        0
    } else if b >= c {
        1
    } else {
        2
    }
}

impl Mesh {
    /// Planar projection: every listed face is projected onto
    /// the plane perpendicular to `normal` (its dominant axis is
    /// dropped). One projection direction for the whole set —
    /// the right tool for a wall run, a roof, a glass pane.
    pub fn project_planar(
        &mut self,
        faces: &[usize],
        normal: [f32; 3],
        scale: f32,
        offset: [f32; 2],
    ) -> OpResult<()> {
        let (pa, pb) = plane_axes(dominant(normal));
        for &fi in faces {
            if fi >= self.faces.len() {
                return Err(OpError::BadFace(fi));
            }
            for ci in 0..self.faces[fi].corners.len() {
                let p = self.verts[self.faces[fi].corners[ci].v as usize];
                self.faces[fi].corners[ci].uv =
                    [p[pa] * scale + offset[0], p[pb] * scale + offset[1]];
            }
        }
        Ok(())
    }

    /// Box projection: each listed face drops ITS OWN dominant
    /// normal axis — the projection a crate, a plinth, or a
    /// housing wants, where every side reads upright in its
    /// region.
    pub fn project_box(&mut self, faces: &[usize], scale: f32, offset: [f32; 2]) -> OpResult<()> {
        for &fi in faces {
            if fi >= self.faces.len() {
                return Err(OpError::BadFace(fi));
            }
            let n = self.shading_normal(fi);
            let (pa, pb) = plane_axes(dominant(n));
            for ci in 0..self.faces[fi].corners.len() {
                let p = self.verts[self.faces[fi].corners[ci].v as usize];
                self.faces[fi].corners[ci].uv =
                    [p[pa] * scale + offset[0], p[pb] * scale + offset[1]];
            }
        }
        Ok(())
    }

    /// Cylindrical projection about the vertical axis through
    /// `center_xz`: u wraps the full turn (0 at +X, growing
    /// counter-clockwise seen from above), v is height ×
    /// `scale_v` plus `offset_v`. The seam lives at the back of
    /// the turn, where u jumps 1 → 0 between neighbouring
    /// corners — per-corner UVs carry the jump honestly.
    pub fn project_cylindrical(
        &mut self,
        faces: &[usize],
        center_xz: [f32; 2],
        scale_v: f32,
        offset_v: f32,
    ) -> OpResult<()> {
        for &fi in faces {
            if fi >= self.faces.len() {
                return Err(OpError::BadFace(fi));
            }
            for ci in 0..self.faces[fi].corners.len() {
                let p = self.verts[self.faces[fi].corners[ci].v as usize];
                let ang = (p[2] - center_xz[1]).atan2(p[0] - center_xz[0]);
                let u = ang / std::f32::consts::TAU + 0.5;
                self.faces[fi].corners[ci].uv = [u, p[1] * scale_v + offset_v];
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed() -> (Mesh, [usize; 6]) {
        let mut m = Mesh::new("t");
        let mat = m.add_material("painted-metal");
        let f = m.add_box([0.0, 0.0, 0.0], [2.0, 1.0, 1.0], mat);
        (m, f)
    }

    #[test]
    fn planar_projects_the_top_face_from_xz() {
        let (mut m, faces) = boxed();
        m.project_planar(&[faces[2]], [0.0, 1.0, 0.0], 0.5, [0.0, 0.0])
            .unwrap();
        for c in &m.faces[faces[2]].corners {
            assert!(c.uv[0] >= 0.0 && c.uv[0] <= 1.0, "u {}", c.uv[0]);
            assert!(c.uv[1] >= 0.0 && c.uv[1] <= 0.5, "v {}", c.uv[1]);
        }
    }

    #[test]
    fn box_projection_stays_in_region() {
        let (mut m, faces) = boxed();
        let all: Vec<usize> = faces.to_vec();
        // Scale by the box's largest extent so 0..1 holds.
        m.project_box(&all, 0.5, [0.0, 0.0]).unwrap();
        for f in &m.faces {
            for c in &f.corners {
                assert!(c.uv[0] >= 0.0 && c.uv[0] <= 1.0, "u {}", c.uv[0]);
                assert!(c.uv[1] >= 0.0 && c.uv[1] <= 1.0, "v {}", c.uv[1]);
            }
        }
    }

    #[test]
    fn cylindrical_wraps_the_prism_band() {
        let mut m = Mesh::new("t");
        let mat = m.add_material("painted-metal");
        let p = m.add_prism([0.0, 0.0], 1.0, 0.0, 2.0, 8, 0.0, mat);
        m.project_cylindrical(&p.sides, [0.0, 0.0], 0.5, 0.0)
            .unwrap();
        let mut us: Vec<f32> = Vec::new();
        for &fi in &p.sides {
            for c in &m.faces[fi].corners {
                assert!(c.uv[0] >= 0.0 && c.uv[0] <= 1.0, "u {}", c.uv[0]);
                assert!(c.uv[1] >= 0.0 && c.uv[1] <= 1.0, "v {}", c.uv[1]);
                us.push(c.uv[0]);
            }
        }
        // The band really wraps: an 8-sided ring puts corners
        // on eighths of the turn — one sits on the seam (~0)
        // and the last before it reaches 0.875.
        assert!(us.iter().any(|&u| u < 0.05), "us {us:?}");
        assert!(us.iter().any(|&u| u > 0.85), "us {us:?}");
    }
}
