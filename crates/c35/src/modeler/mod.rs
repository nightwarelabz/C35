//! The modeler: C35's in-engine 3D modeling system (v1).
//!
//! Until now every mesh in this engine was code-assembled
//! primitives — recipe parts (box / wedge / cylinder / quad) in
//! the mod engine, ~114 placed boxes in the operator builder —
//! or a procedural painter's output. Primitives approximate;
//! they cannot author. A coat skirt that splits and flares, a
//! lamp arm that curves: those are modeling sessions, not part
//! lists. This module is where they happen.
//!
//! - [`mesh`]: the editable data model — shared-vertex polygon
//!   faces, per-corner UVs, per-face materials and authored
//!   normals, manifold validation.
//! - [`ops`]: the working set — extrude, bevel (face + edge),
//!   loop cut, mirror, weld, flat subdivide, delete, dissolve,
//!   transforms. Deterministic, and each op refuses rather
//!   than tear a shell.
//! - [`uv`]: planar / box / cylindrical projection into
//!   region-local UV space.
//! - [`format`]: the `.c35m` text format — diff-able,
//!   versioned, byte-identical round-trips.
//! - [`assets`]: shipped assets authored as op scripts (the
//!   session transcript IS the source file).
//!
//! The integration seam is one function: [`to_prop_mesh`] bakes
//! a modeled mesh into the same `PropMesh` the renderer already
//! draws and the host already uploads through `set_props` — a
//! modeled asset instances exactly like a procedural prop, with
//! materials resolved through the mod engine's name registry
//! (a model file names materials; it never addresses atlas
//! coordinates). The mod recipe vocabulary and the operator
//! builder are untouched: the modeler is additive, a second
//! road into the same renderer.

pub mod assets;
pub mod format;
pub mod mesh;
pub mod ops;
pub mod uv;

pub use format::{load, load_from, save, save_to, ModelError};
pub use mesh::{Axis, Corner, Face, Mesh, MeshFault};
pub use ops::OpError;

use crate::props::{uv_rect, PropMesh, PropVertex};

/// Bake a modeled mesh into renderer geometry.
///
/// Faces fan-triangulate; normals come from the face's authored
/// override when pinned, else its winding (the recipe builder's
/// rule: normals from geometry, never hand-waved); corner UVs
/// map from region-local space into the material's atlas
/// region; the baked lamp term is 0.0 — the host's light field
/// bakes practical light into placed geometry exactly as it
/// does for recipe props, and the bake does not invent light.
pub fn to_prop_mesh(mesh: &Mesh) -> Result<PropMesh, ModelError> {
    let mut out = PropMesh::new();
    for (fi, f) in mesh.faces.iter().enumerate() {
        if f.corners.len() < 3 {
            return Err(ModelError {
                line: 0,
                message: format!("face {fi} has fewer than 3 corners"),
            });
        }
        let mat_name = mesh
            .materials
            .get(f.material as usize)
            .ok_or_else(|| ModelError {
                line: 0,
                message: format!("face {fi} names undeclared material index {}", f.material),
            })?;
        let (region, mat_flag) =
            crate::modding::model::material_region(mat_name).ok_or_else(|| ModelError {
                line: 0,
                message: format!("face {fi}: unknown material '{mat_name}'"),
            })?;
        let rect = uv_rect(region);
        let n = mesh.shading_normal(fi);
        let base = out.vertices.len() as u32;
        for c in &f.corners {
            let pos = mesh.verts.get(c.v as usize).ok_or_else(|| ModelError {
                line: 0,
                message: format!("face {fi} names missing vertex {}", c.v),
            })?;
            out.vertices.push(PropVertex {
                position: *pos,
                normal: n,
                uv: [
                    rect[0] + c.uv[0] * (rect[2] - rect[0]),
                    rect[1] + c.uv[1] * (rect[3] - rect[1]),
                ],
                mat: mat_flag,
                light: 0.0,
            });
        }
        for i in 1..f.corners.len() - 1 {
            out.indices
                .extend_from_slice(&[base, base + i as u32, base + i as u32 + 1]);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::MAT_EMISSIVE;

    #[test]
    fn lamp_bakes_to_the_same_triangle_count() {
        let m = assets::lamp_standard();
        let baked = to_prop_mesh(&m).unwrap();
        assert_eq!(baked.triangle_count(), m.triangle_count());
        assert_eq!(
            baked.vertices.len(),
            m.faces.iter().map(|f| f.corners.len()).sum::<usize>()
        );
    }

    #[test]
    fn lamp_glass_bakes_emissive_into_the_lamp_glass_region() {
        let m = assets::lamp_standard();
        let baked = to_prop_mesh(&m).unwrap();
        let rect = uv_rect(crate::props::region::LAMP_GLASS);
        let glass_verts: Vec<&PropVertex> = baked
            .vertices
            .iter()
            .filter(|v| v.mat == MAT_EMISSIVE)
            .collect();
        assert!(!glass_verts.is_empty(), "no emissive verts baked");
        for v in glass_verts {
            assert!(
                v.uv[0] >= rect[0]
                    && v.uv[0] <= rect[2]
                    && v.uv[1] >= rect[1]
                    && v.uv[1] <= rect[3],
                "glass uv {:?} outside the lamp-glass region {rect:?}",
                v.uv
            );
        }
    }

    #[test]
    fn unknown_material_is_a_bake_error_not_a_fallback() {
        let mut m = Mesh::new("x");
        let mat = m.add_material("unobtanium");
        let a = m.push_vert([0.0, 0.0, 0.0]);
        let b = m.push_vert([1.0, 0.0, 0.0]);
        let c = m.push_vert([0.0, 1.0, 0.0]);
        m.push_face(&[a, b, c], mat);
        let e = match to_prop_mesh(&m) {
            Ok(_) => panic!("an unknown material must not bake"),
            Err(e) => e,
        };
        assert!(e.message.contains("unobtanium"), "{}", e.message);
    }
}
