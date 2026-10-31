//! The editable mesh: the modeler's one data model.
//!
//! Index-based polygon soup with shared vertices — the shape a
//! human (or a build script standing in for one) actually edits.
//! Faces are n-gons wound counter-clockwise seen from outside;
//! every corner carries its own UV, so a shared vertex can sit in
//! the atlas differently per face. A face names a material by
//! index into the mesh's material table (names, resolved against
//! the engine's material registry only at bake time — a model file
//! never addresses atlas coordinates, exactly like a mod recipe).
//! A face may pin an explicit normal; `None` means "the geometric
//! normal", and the bake recomputes it from winding.
//!
//! Determinism is a hard law here, as everywhere in this engine:
//! storage is plain `Vec`s in insertion order, no hash iteration
//! ever reaches the output, and the same op sequence on the same
//! mesh produces a byte-identical mesh (the file format's
//! round-trip test leans on this).

/// One face corner: a vertex index plus this face's UV for it.
/// UVs are region-local: (0, 0) is the material region's top-left,
/// (1, 1) its bottom-right — the same convention the recipe
/// builder uses when it maps a box face onto the full region.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corner {
    pub v: u32,
    pub uv: [f32; 2],
}

/// A polygon face: corners wound CCW seen from outside.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub corners: Vec<Corner>,
    /// Index into [`Mesh::materials`].
    pub material: u16,
    /// Explicit normal override, or `None` for the geometric
    /// (Newell) normal. Authored control for the faces whose
    /// shading must not follow the triangulation — a lamp shade's
    /// inner bell, a coat panel that must catch light flat.
    pub normal: Option<[f32; 3]>,
}

/// The editable mesh. A mesh may hold several disjoint shells
/// (the lamp standard is a plinth shell, a pole-and-arm shell,
/// and collar shells — overlapping solids are the house building
/// style, exactly as recipe parts overlap).
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub name: String,
    /// Material names in declaration order. Names resolve through
    /// `modding::model::material_region` at bake time; an unknown
    /// name is a bake error, never a fallback.
    pub materials: Vec<String>,
    pub verts: Vec<[f32; 3]>,
    pub faces: Vec<Face>,
}

/// A principal axis, for mirror planes and projections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub fn index(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }
}

/// One thing wrong with a mesh, as [`Mesh::validate`] finds it.
/// Validation never mutates; ops that cannot preserve sanity
/// refuse with an `OpError` instead (see `ops`).
#[derive(Debug, Clone, PartialEq)]
pub enum MeshFault {
    /// A corner names a vertex that does not exist.
    IndexOutOfRange { face: usize, vert: u32 },
    /// A face with fewer than three corners.
    TooFewCorners { face: usize },
    /// A face whose corners span no area (collinear / repeated).
    DegenerateFace { face: usize },
    /// An undirected edge used by exactly one face: a hole rim.
    OpenEdge { a: u32, b: u32 },
    /// An undirected edge used by more than two faces.
    NonManifoldEdge { a: u32, b: u32, uses: usize },
    /// An edge used twice in the SAME direction: one of the faces
    /// is wound backwards against its neighbour.
    FlippedEdge { a: u32, b: u32 },
}

pub fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

pub fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = length(a);
    if l > 1e-12 {
        scale(a, 1.0 / l)
    } else {
        [0.0, 0.0, 0.0]
    }
}

pub fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    add(a, scale(sub(b, a), t))
}

impl Mesh {
    pub fn new(name: &str) -> Mesh {
        Mesh {
            name: name.to_string(),
            materials: Vec::new(),
            verts: Vec::new(),
            faces: Vec::new(),
        }
    }

    /// Declare a material (or find the existing declaration) and
    /// return its index. Declaration order is file order.
    pub fn add_material(&mut self, name: &str) -> u16 {
        if let Some(i) = self.materials.iter().position(|m| m == name) {
            return i as u16;
        }
        self.materials.push(name.to_string());
        (self.materials.len() - 1) as u16
    }

    pub fn push_vert(&mut self, p: [f32; 3]) -> u32 {
        self.verts.push(p);
        (self.verts.len() - 1) as u32
    }

    /// Append a face from bare vertex indices, UVs zeroed.
    /// Returns the new face's index. Face indices are stable
    /// across every op except `delete_face` / `dissolve_face`,
    /// which remove a face and shift the indices above it.
    pub fn push_face(&mut self, verts: &[u32], material: u16) -> usize {
        let corners = verts
            .iter()
            .map(|&v| Corner { v, uv: [0.0, 0.0] })
            .collect();
        self.push_face_corners(corners, material)
    }

    pub fn push_face_corners(&mut self, corners: Vec<Corner>, material: u16) -> usize {
        self.faces.push(Face {
            corners,
            material,
            normal: None,
        });
        self.faces.len() - 1
    }

    /// The face's corner positions, in winding order.
    pub fn face_positions(&self, face: usize) -> Vec<[f32; 3]> {
        self.faces[face]
            .corners
            .iter()
            .map(|c| self.verts[c.v as usize])
            .collect()
    }

    /// Newell's method: exact for planar faces, the honest
    /// average for near-planar ones. Normalized; a degenerate
    /// face yields the zero vector.
    pub fn face_normal(&self, face: usize) -> [f32; 3] {
        let p = self.face_positions(face);
        let mut n = [0.0f32; 3];
        for i in 0..p.len() {
            let a = p[i];
            let b = p[(i + 1) % p.len()];
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        normalize(n)
    }

    /// The normal a bake should use: the authored override when
    /// pinned, the geometric normal otherwise.
    pub fn shading_normal(&self, face: usize) -> [f32; 3] {
        self.faces[face]
            .normal
            .unwrap_or_else(|| self.face_normal(face))
    }

    pub fn face_centroid(&self, face: usize) -> [f32; 3] {
        let p = self.face_positions(face);
        let mut c = [0.0f32; 3];
        for v in &p {
            c = add(c, *v);
        }
        if p.is_empty() {
            c
        } else {
            scale(c, 1.0 / p.len() as f32)
        }
    }

    /// Twice the signed area's magnitude is folded into the
    /// length of the unnormalized Newell vector; this returns the
    /// plain area.
    pub fn face_area(&self, face: usize) -> f32 {
        let p = self.face_positions(face);
        let mut n = [0.0f32; 3];
        for i in 0..p.len() {
            let a = p[i];
            let b = p[(i + 1) % p.len()];
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        length(n) / 2.0
    }

    pub fn triangle_count(&self) -> u64 {
        self.faces
            .iter()
            .map(|f| f.corners.len().saturating_sub(2) as u64)
            .sum()
    }

    /// Every face incident to a vertex, in face order.
    pub fn faces_at_vert(&self, v: u32) -> Vec<usize> {
        self.faces
            .iter()
            .enumerate()
            .filter(|(_, f)| f.corners.iter().any(|c| c.v == v))
            .map(|(i, _)| i)
            .collect()
    }

    /// Manifold sanity, reported whole: index range, corner
    /// counts, degenerate faces, and the edge-pairing census
    /// (every undirected edge of a closed shell is used exactly
    /// twice, once per direction). Faults come back in a stable
    /// order — faces first, then edges by (min, max) — so callers
    /// can diff reports.
    pub fn validate(&self) -> Vec<MeshFault> {
        use std::collections::BTreeMap;
        let mut faults = Vec::new();
        // Directed edge census: (min, max) -> [uses a->b, uses b->a]
        // where a = min, b = max.
        let mut edges: BTreeMap<(u32, u32), [usize; 2]> = BTreeMap::new();
        for (fi, f) in self.faces.iter().enumerate() {
            if f.corners.len() < 3 {
                faults.push(MeshFault::TooFewCorners { face: fi });
                continue;
            }
            let mut bad_index = false;
            for c in &f.corners {
                if (c.v as usize) >= self.verts.len() {
                    faults.push(MeshFault::IndexOutOfRange {
                        face: fi,
                        vert: c.v,
                    });
                    bad_index = true;
                }
            }
            if bad_index {
                continue;
            }
            if self.face_area(fi) < 1e-10 {
                faults.push(MeshFault::DegenerateFace { face: fi });
            }
            for i in 0..f.corners.len() {
                let a = f.corners[i].v;
                let b = f.corners[(i + 1) % f.corners.len()].v;
                let key = if a < b { (a, b) } else { (b, a) };
                let e = edges.entry(key).or_insert([0, 0]);
                if a == key.0 {
                    e[0] += 1;
                } else {
                    e[1] += 1;
                }
            }
        }
        for ((a, b), uses) in &edges {
            let total = uses[0] + uses[1];
            if total > 2 {
                faults.push(MeshFault::NonManifoldEdge {
                    a: *a,
                    b: *b,
                    uses: total,
                });
            } else if total == 1 {
                faults.push(MeshFault::OpenEdge { a: *a, b: *b });
            } else if uses[0] == 2 || uses[1] == 2 {
                faults.push(MeshFault::FlippedEdge { a: *a, b: *b });
            }
        }
        faults
    }

    /// True when validation finds nothing: every shell closed,
    /// every edge paired, every winding consistent.
    pub fn is_watertight(&self) -> bool {
        self.validate().is_empty()
    }

    // ---------------------------------------------------------
    // Generators: the "add mesh" shelf. Each appends one shell.
    // ---------------------------------------------------------

    /// Add an axis-aligned box shell from its min/max corners.
    /// Returns the six face indices in [+X, -X, +Y, -Y, +Z, -Z]
    /// order. Every face maps the full material region (the
    /// recipe builder's convention).
    pub fn add_box(&mut self, min: [f32; 3], max: [f32; 3], material: u16) -> [usize; 6] {
        let (x0, y0, z0) = (min[0], min[1], min[2]);
        let (x1, y1, z1) = (max[0], max[1], max[2]);
        // Corner table, then faces wound CCW from outside (the
        // recipe builder's proven windings, shared-vertex form).
        let v = [
            [x0, y0, z0],
            [x1, y0, z0],
            [x1, y1, z0],
            [x0, y1, z0],
            [x0, y0, z1],
            [x1, y0, z1],
            [x1, y1, z1],
            [x0, y1, z1],
        ];
        let base = self.verts.len() as u32;
        for p in v {
            self.push_vert(p);
        }
        let uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        let defs: [[u32; 4]; 6] = [
            [1, 2, 6, 5], // +X
            [4, 7, 3, 0], // -X
            [3, 7, 6, 2], // +Y
            [4, 0, 1, 5], // -Y
            [4, 5, 6, 7], // +Z
            [1, 0, 3, 2], // -Z
        ];
        let mut out = [0usize; 6];
        for (i, d) in defs.iter().enumerate() {
            let corners: Vec<Corner> = d
                .iter()
                .enumerate()
                .map(|(k, &vi)| Corner {
                    v: base + vi,
                    uv: uv[k],
                })
                .collect();
            out[i] = self.push_face_corners(corners, material);
        }
        out
    }

    /// Add a vertical prism shell: an n-gon ring at `y0` and
    /// `y1`, capped by two n-gon faces. Side corners carry
    /// cylindrical UVs (u wraps the ring, v runs y0→y1); cap
    /// corners carry planar UVs from XZ. `phase` rotates the
    /// ring (radians) so a flat can face the street. Returns the
    /// face indices of the caps and the side band.
    #[allow(clippy::too_many_arguments)]
    pub fn add_prism(
        &mut self,
        center_xz: [f32; 2],
        radius: f32,
        y0: f32,
        y1: f32,
        sides: usize,
        phase: f32,
        material: u16,
    ) -> PrismFaces {
        assert!(sides >= 3, "a prism needs at least 3 sides");
        let (cx, cz) = (center_xz[0], center_xz[1]);
        let mut bot = Vec::with_capacity(sides);
        let mut top = Vec::with_capacity(sides);
        for i in 0..sides {
            let t = phase + (i as f32) / (sides as f32) * std::f32::consts::TAU;
            let (x, z) = (cx + radius * t.cos(), cz + radius * t.sin());
            bot.push(self.push_vert([x, y0, z]));
            top.push(self.push_vert([x, y1, z]));
        }
        let cap_uv = |p: [f32; 3]| {
            [
                (p[0] - cx) / (2.0 * radius) + 0.5,
                (p[2] - cz) / (2.0 * radius) + 0.5,
            ]
        };
        // Sides: bot_i → top_i → top_j → bot_j (the recipe
        // cylinder's outward winding).
        let mut side_ids = Vec::with_capacity(sides);
        for i in 0..sides {
            let j = (i + 1) % sides;
            let (u0, u1) = (i as f32 / sides as f32, (i + 1) as f32 / sides as f32);
            side_ids.push(self.push_face_corners(
                vec![
                    Corner {
                        v: bot[i],
                        uv: [u0, 1.0],
                    },
                    Corner {
                        v: top[i],
                        uv: [u0, 0.0],
                    },
                    Corner {
                        v: top[j],
                        uv: [u1, 0.0],
                    },
                    Corner {
                        v: bot[j],
                        uv: [u1, 1.0],
                    },
                ],
                material,
            ));
        }
        // Bottom cap: ring walked forward (faces -Y; the
        // top cap below walks it backward — the recipe
        // cylinder's fans make the same pair of choices).
        let bot_cap = self.push_face_corners(
            (0..sides)
                .map(|i| Corner {
                    v: bot[i],
                    uv: cap_uv(self.verts[bot[i] as usize]),
                })
                .collect(),
            material,
        );
        // Top cap: ring walked backwards as a polygon — with the
        // ring's angle increasing clockwise seen from above, the
        // reversed walk is CCW from above and faces +Y (the
        // recipe cylinder's cap fan walks the same way).
        let top_cap = self.push_face_corners(
            (0..sides)
                .rev()
                .map(|i| Corner {
                    v: top[i],
                    uv: cap_uv(self.verts[top[i] as usize]),
                })
                .collect(),
            material,
        );
        PrismFaces {
            top: top_cap,
            bottom: bot_cap,
            sides: side_ids,
        }
    }
}

/// Face indices returned by [`Mesh::add_prism`].
#[derive(Debug, Clone, PartialEq)]
pub struct PrismFaces {
    pub top: usize,
    pub bottom: usize,
    pub sides: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outward_check(m: &Mesh) {
        // For a convex shell around the origin-ish, every face
        // normal must point away from the shell centroid.
        let mut c = [0.0f32; 3];
        for v in &m.verts {
            c = add(c, *v);
        }
        c = scale(c, 1.0 / m.verts.len() as f32);
        for fi in 0..m.faces.len() {
            let n = m.face_normal(fi);
            let d = dot(n, sub(m.face_centroid(fi), c));
            assert!(d > 0.0, "face {fi} normal {n:?} points inward");
        }
    }

    #[test]
    fn box_is_watertight_and_outward() {
        let mut m = Mesh::new("t");
        let mat = m.add_material("painted-metal");
        m.add_box([-0.5, 0.0, -0.5], [0.5, 1.0, 0.5], mat);
        assert_eq!(m.verts.len(), 8);
        assert_eq!(m.faces.len(), 6);
        assert_eq!(m.triangle_count(), 12);
        assert!(m.is_watertight(), "faults: {:?}", m.validate());
        outward_check(&m);
    }

    #[test]
    fn prism_is_watertight_and_outward() {
        let mut m = Mesh::new("t");
        let mat = m.add_material("painted-metal");
        m.add_prism([0.0, 0.0], 0.5, 0.0, 2.0, 8, 0.0, mat);
        assert_eq!(m.verts.len(), 16);
        assert_eq!(m.faces.len(), 10);
        // 8 quads (16 tris) + two octagon fans (6 each).
        assert_eq!(m.triangle_count(), 28);
        assert!(m.is_watertight(), "faults: {:?}", m.validate());
        outward_check(&m);
    }

    #[test]
    fn open_shell_reports_open_edges() {
        let mut m = Mesh::new("t");
        let mat = m.add_material("x");
        let a = m.push_vert([0.0, 0.0, 0.0]);
        let b = m.push_vert([1.0, 0.0, 0.0]);
        let c = m.push_vert([0.0, 1.0, 0.0]);
        m.push_face(&[a, b, c], mat);
        let faults = m.validate();
        assert_eq!(faults.len(), 3);
        assert!(faults
            .iter()
            .all(|f| matches!(f, MeshFault::OpenEdge { .. })));
    }

    #[test]
    fn flipped_face_is_reported() {
        let mut m = Mesh::new("t");
        let mat = m.add_material("x");
        m.add_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], mat);
        m.faces[0].corners.reverse();
        let faults = m.validate();
        assert!(
            faults
                .iter()
                .any(|f| matches!(f, MeshFault::FlippedEdge { .. })),
            "faults: {faults:?}"
        );
    }
}
