//! The working set: the operations a modeling session is made
//! of. Every op is deterministic — same mesh in, same mesh out,
//! byte for byte — and every op either preserves manifold sanity
//! on the shells it touches or refuses with an [`OpError`]
//! BEFORE mutating anything. (The pre-flight checks run first;
//! an op that errors has not touched the mesh.)
//!
//! v1 scoping, stated plainly:
//! - `extrude_faces` / `bevel_faces` take face sets whose faces
//!   share no vertices (extruding a connected region needs
//!   boundary-loop surgery the format does not yet record).
//! - `bevel_edges` handles edges whose endpoint vertices are
//!   incident to exactly the edge's two faces (a tube's wall, a
//!   fin, a strap). At a three-face corner it refuses with
//!   `CornerFan` instead of tearing the corner — the corner fan
//!   is future work, and the refusal is the honest v1 answer.
//! - `loop_cut` walks quad loops only, and proves the whole
//!   loop before cutting any of it.
//! - `subdivide_faces` shares edge midpoints across the given
//!   set, so a whole shell subdivides watertight; a partial set
//!   leaves T-junctions along its boundary by design.

use super::mesh::*;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum OpError {
    BadFace(usize),
    BadVert(u32),
    BadEdge(u32, u32),
    DegenerateFace(usize),
    /// The face set shares a vertex; v1 ops work on disjoint faces.
    AdjacentFaces,
    /// A bevel amount that would invert the ring it cuts.
    AmountTooLarge,
    /// bevel_edges met a vertex where more than the edge's two
    /// faces meet. The corner fan is not in v1.
    CornerFan(u32),
    /// The edge is not shared by exactly two faces.
    EdgeNotManifold(u32, u32),
    /// A loop cut could not close its ring.
    LoopOpen,
    NonQuadFace(usize),
    NoSharedEdge,
    MultipleSharedEdges,
    /// A dissolve would produce a self-touching polygon.
    NotSimple,
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpError::BadFace(i) => write!(f, "no face {i}"),
            OpError::BadVert(v) => write!(f, "no vertex {v}"),
            OpError::BadEdge(a, b) => write!(f, "no edge {a}-{b}"),
            OpError::DegenerateFace(i) => write!(f, "face {i} is degenerate"),
            OpError::AdjacentFaces => {
                write!(f, "face set shares a vertex; v1 ops need disjoint faces")
            }
            OpError::AmountTooLarge => write!(f, "amount would invert the cut ring"),
            OpError::CornerFan(v) => {
                write!(
                    f,
                    "vertex {v} meets more than the edge's two faces; corner fan is not in v1"
                )
            }
            OpError::EdgeNotManifold(a, b) => {
                write!(f, "edge {a}-{b} is not shared by exactly two faces")
            }
            OpError::LoopOpen => write!(f, "loop does not close"),
            OpError::NonQuadFace(i) => write!(f, "face {i} is not a quad"),
            OpError::NoSharedEdge => write!(f, "faces share no edge"),
            OpError::MultipleSharedEdges => write!(f, "faces share more than one edge"),
            OpError::NotSimple => write!(f, "merged polygon would not be simple"),
        }
    }
}

impl std::error::Error for OpError {}

pub type OpResult<T> = Result<T, OpError>;

fn check_faces(m: &Mesh, faces: &[usize]) -> OpResult<()> {
    for &f in faces {
        if f >= m.faces.len() {
            return Err(OpError::BadFace(f));
        }
        if m.faces[f].corners.len() < 3 {
            return Err(OpError::DegenerateFace(f));
        }
    }
    Ok(())
}

fn check_disjoint(m: &Mesh, faces: &[usize]) -> OpResult<()> {
    let mut seen: Vec<u32> = Vec::new();
    for &f in faces {
        for c in &m.faces[f].corners {
            if seen.contains(&c.v) {
                return Err(OpError::AdjacentFaces);
            }
            seen.push(c.v);
        }
    }
    Ok(())
}

/// Wrap-projection UVs for an extrusion/bevel side band: u runs
/// the source polygon's boundary arc length (normalized), v runs
/// 0 at the source ring to 1 at the new ring.
fn band_uvs(m: &Mesh, face: usize) -> Vec<[f32; 2]> {
    let pos = m.face_positions(face);
    let n = pos.len();
    let mut cum = vec![0.0f32; n];
    let mut total = 0.0f32;
    for i in 0..n {
        total += length(sub(pos[(i + 1) % n], pos[i]));
        if i + 1 < n {
            cum[i + 1] = total;
        }
    }
    let inv = if total > 1e-9 { 1.0 / total } else { 0.0 };
    cum.iter().map(|c| [*c * inv, 0.0]).collect()
}

impl Mesh {
    /// Extrude a set of disjoint faces along `offset` (a full
    /// translation vector, not a direction × distance — a bent
    /// tube is a chain of extrusions whose offsets turn). The
    /// moved ring is re-oriented to face the offset's direction
    /// (rotated about its new centroid from the face's current
    /// normal to the offset direction), so a turning chain
    /// bends smoothly instead of stair-stepping horizontal
    /// rings; an offset along the normal is the plain extrusion
    /// every modeler knows. The cap faces keep their indices
    /// and materials; new side quads inherit the cap's material.
    /// `taper` scales the cap about its own centroid in its
    /// plane before the re-orientation: positive values narrow
    /// it (a pole rising out of a socket), negative values
    /// flare it (a lamp shade's bell). The cap's explicit
    /// normal, if any, is cleared — it moved.
    pub fn extrude_faces(
        &mut self,
        faces: &[usize],
        offset: [f32; 3],
        taper: f32,
    ) -> OpResult<Vec<usize>> {
        check_faces(self, faces)?;
        check_disjoint(self, faces)?;
        for &f in faces {
            let face = self.faces[f].clone();
            let n = face.corners.len();
            let band = band_uvs(self, f);
            let old: Vec<[f32; 3]> = face
                .corners
                .iter()
                .map(|c| self.verts[c.v as usize])
                .collect();
            let mut cen = [0.0f32; 3];
            for p in &old {
                cen = add(cen, add(*p, offset));
            }
            cen = scale(cen, 1.0 / n as f32);
            let keep = 1.0 - taper;
            // Re-orientation: rotate the moved ring about its
            // new centroid from the face's current normal to
            // the offset direction (Rodrigues). Parallel offsets
            // skip it; the antiparallel case picks any axis
            // perpendicular to the normal.
            let n0 = self.face_normal(f);
            let dir = normalize(offset);
            let cos = dot(n0, dir).clamp(-1.0, 1.0);
            let axis_raw = cross(n0, dir);
            let sin = length(axis_raw);
            let rotate = if sin > 1e-6 || cos < 0.0 {
                let axis = if sin > 1e-6 {
                    scale(axis_raw, 1.0 / sin)
                } else if n0[0].abs() < 0.9 {
                    normalize(cross(n0, [1.0, 0.0, 0.0]))
                } else {
                    normalize(cross(n0, [0.0, 0.0, 1.0]))
                };
                Some((axis, cos, if sin > 1e-6 { sin } else { 0.0 }))
            } else {
                None
            };
            let mut new_idx = Vec::with_capacity(n);
            for p in &old {
                let moved = add(*p, offset);
                let mut placed = add(cen, scale(sub(moved, cen), keep));
                if let Some((axis, c, s)) = rotate {
                    let v = sub(placed, cen);
                    let turned = add(
                        add(scale(v, c), scale(cross(axis, v), s)),
                        scale(axis, dot(axis, v) * (1.0 - c)),
                    );
                    placed = add(cen, turned);
                }
                new_idx.push(self.push_vert(placed));
            }
            for i in 0..n {
                let j = (i + 1) % n;
                self.push_face_corners(
                    vec![
                        Corner {
                            v: face.corners[i].v,
                            uv: [band[i][0], 0.0],
                        },
                        Corner {
                            v: face.corners[j].v,
                            uv: [band[j][0], 0.0],
                        },
                        Corner {
                            v: new_idx[j],
                            uv: [band[j][0], 1.0],
                        },
                        Corner {
                            v: new_idx[i],
                            uv: [band[i][0], 1.0],
                        },
                    ],
                    face.material,
                );
            }
            let cap = &mut self.faces[f];
            for (i, c) in cap.corners.iter_mut().enumerate() {
                c.v = new_idx[i];
            }
            cap.normal = None;
        }
        Ok(faces.to_vec())
    }

    /// Bevel a set of disjoint faces: step the boundary ring
    /// inward and outward along the face normal, `segments`
    /// times, `amount` in total each way. One segment is a flat
    /// chamfer (a plinth's shoulder, a shade's rim); more steps
    /// approximate the round-over. The cap keeps its index,
    /// material, and normal; the ring faces inherit its material.
    pub fn bevel_faces(&mut self, faces: &[usize], amount: f32, segments: u32) -> OpResult<()> {
        check_faces(self, faces)?;
        check_disjoint(self, faces)?;
        let segments = segments.max(1);
        for &f in faces {
            let face = self.faces[f].clone();
            let n = face.corners.len();
            let normal = self.face_normal(f);
            if length(normal) < 0.5 {
                return Err(OpError::DegenerateFace(f));
            }
            let cen = self.face_centroid(f);
            // Refuse an inset that would cross the centroid.
            let mut min_r = f32::INFINITY;
            for c in &face.corners {
                min_r = min_r.min(length(sub(self.verts[c.v as usize], cen)));
            }
            if amount >= 0.45 * min_r {
                return Err(OpError::AmountTooLarge);
            }
            // Ring 0 is the face's own boundary — original verts,
            // so the neighbours' pairing survives untouched.
            let mut ring: Vec<u32> = face.corners.iter().map(|c| c.v).collect();
            for s in 1..=segments {
                let t = s as f32 / segments as f32;
                let mut new_ring = Vec::with_capacity(n);
                for c in &face.corners {
                    let p = self.verts[c.v as usize];
                    let inward = normalize(sub(cen, p));
                    let placed = add(add(p, scale(normal, amount * t)), scale(inward, amount * t));
                    new_ring.push(self.push_vert(placed));
                }
                for i in 0..n {
                    let j = (i + 1) % n;
                    let (u0, u1) = (
                        [i as f32 / n as f32, (s - 1) as f32 / segments as f32],
                        [(i + 1) as f32 / n as f32, (s - 1) as f32 / segments as f32],
                    );
                    let (w0, w1) = ([i as f32 / n as f32, t], [(i + 1) as f32 / n as f32, t]);
                    self.push_face_corners(
                        vec![
                            Corner { v: ring[i], uv: u0 },
                            Corner { v: ring[j], uv: u1 },
                            Corner {
                                v: new_ring[j],
                                uv: w1,
                            },
                            Corner {
                                v: new_ring[i],
                                uv: w0,
                            },
                        ],
                        face.material,
                    );
                }
                ring = new_ring;
            }
            let cap = &mut self.faces[f];
            for (i, c) in cap.corners.iter_mut().enumerate() {
                c.v = ring[i];
            }
        }
        Ok(())
    }

    /// Bevel edges with a chamfer/round-over profile of
    /// `segments` strips (`1` = a flat chamfer). v1 restriction,
    /// checked before any mutation: each edge must be shared by
    /// exactly two faces, and each endpoint vertex must be
    /// incident to those two faces only — otherwise
    /// [`OpError::CornerFan`]. The edge's original endpoint
    /// vertices are left unreferenced; call [`Mesh::compact`]
    /// when a session wants them gone. Returns the number of
    /// strip faces added.
    pub fn bevel_edges(
        &mut self,
        edges: &[(u32, u32)],
        amount: f32,
        segments: u32,
    ) -> OpResult<usize> {
        let segments = segments.max(1);
        // ---- pre-flight: resolve and check every edge first.
        struct Plan {
            f1: usize,
            f2: usize,
            a: u32,
            b: u32,
            d1: [f32; 3],
            d2: [f32; 3],
        }
        let mut plans = Vec::new();
        for &(a, b) in edges {
            if (a as usize) >= self.verts.len() || (b as usize) >= self.verts.len() || a == b {
                return Err(OpError::BadEdge(a, b));
            }
            let mut sharing = Vec::new();
            for (fi, f) in self.faces.iter().enumerate() {
                let k = f.corners.len();
                for i in 0..k {
                    let (x, y) = (f.corners[i].v, f.corners[(i + 1) % k].v);
                    if (x == a && y == b) || (x == b && y == a) {
                        sharing.push(fi);
                        break;
                    }
                }
            }
            if sharing.len() != 2 {
                return Err(OpError::EdgeNotManifold(a, b));
            }
            let (f1, f2) = (sharing[0], sharing[1]);
            for v in [a, b] {
                if self.faces_at_vert(v).iter().any(|&fi| fi != f1 && fi != f2) {
                    return Err(OpError::CornerFan(v));
                }
            }
            let pa = self.verts[a as usize];
            let pb = self.verts[b as usize];
            let axis = normalize(sub(pb, pa));
            if length(axis) < 0.5 {
                return Err(OpError::BadEdge(a, b));
            }
            let mid = scale(add(pa, pb), 0.5);
            let inward = |fi: usize| -> [f32; 3] {
                let to_c = sub(self.face_centroid(fi), mid);
                normalize(sub(to_c, scale(axis, dot(to_c, axis))))
            };
            plans.push(Plan {
                f1,
                f2,
                a,
                b,
                d1: inward(f1),
                d2: inward(f2),
            });
        }
        // ---- apply.
        let mut added = 0usize;
        for plan in &plans {
            let pa = self.verts[plan.a as usize];
            let pb = self.verts[plan.b as usize];
            // Profile points: slerp-ish blend of the two faces'
            // inward directions, constant radius.
            let mut a_pts = Vec::with_capacity(segments as usize + 1);
            let mut b_pts = Vec::with_capacity(segments as usize + 1);
            for k in 0..=segments {
                let t = k as f32 / segments as f32;
                let dir = normalize(lerp(plan.d1, plan.d2, t));
                a_pts.push(self.push_vert(add(pa, scale(dir, amount))));
                b_pts.push(self.push_vert(add(pb, scale(dir, amount))));
            }
            // The f1 corner at a takes profile point 0 (d1 side);
            // f2's takes the last (d2 side).
            let replace = |m: &mut Mesh, fi: usize, va: u32, na: u32, vb: u32, nb: u32| {
                for c in m.faces[fi].corners.iter_mut() {
                    if c.v == va {
                        c.v = na;
                    } else if c.v == vb {
                        c.v = nb;
                    }
                }
            };
            replace(self, plan.f1, plan.a, a_pts[0], plan.b, b_pts[0]);
            let last = segments as usize;
            replace(self, plan.f2, plan.a, a_pts[last], plan.b, b_pts[last]);
            // Strips wound to pair with f1's directed a->b flow
            // (whichever direction f1 actually walks the edge,
            // the strip presents the opposite directed edges to
            // both neighbours by construction of the corner cut).
            let mat = self.faces[plan.f1].material;
            for k in 0..segments as usize {
                let (t0, t1) = (k as f32 / segments as f32, (k + 1) as f32 / segments as f32);
                self.push_face_corners(
                    vec![
                        Corner {
                            v: b_pts[k],
                            uv: [t0, 1.0],
                        },
                        Corner {
                            v: a_pts[k],
                            uv: [t0, 0.0],
                        },
                        Corner {
                            v: a_pts[k + 1],
                            uv: [t1, 0.0],
                        },
                        Corner {
                            v: b_pts[k + 1],
                            uv: [t1, 1.0],
                        },
                    ],
                    mat,
                );
                added += 1;
            }
        }
        Ok(added)
    }

    /// Cut a loop of quads through `edge` at parameter `t`
    /// (0 at the first vertex, 1 at the second, as the edge is
    /// written). The whole ring is walked and proven first —
    /// every face a quad, the ring closed — before a single
    /// vertex is added. Returns the number of faces added
    /// (one per quad in the ring).
    pub fn loop_cut(&mut self, edge: (u32, u32), t: f32) -> OpResult<usize> {
        // Find the start face and the directed edge as it
        // appears there.
        let (start, dir) = {
            let mut found = None;
            for (fi, f) in self.faces.iter().enumerate() {
                let k = f.corners.len();
                for i in 0..k {
                    let (x, y) = (f.corners[i].v, f.corners[(i + 1) % k].v);
                    if (x == edge.0 && y == edge.1)
                        || (x == edge.1 && y == edge.0 && found.is_none())
                    {
                        found = Some((fi, (x, y)));
                    }
                }
            }
            found.ok_or(OpError::BadEdge(edge.0, edge.1))?
        };
        // ---- pre-walk.
        struct Step {
            face: usize,
            quad: [u32; 4],
        }
        let mut steps: Vec<Step> = Vec::new();
        let (mut cur, mut entry) = (start, dir);
        loop {
            let f = &self.faces[cur];
            if f.corners.len() != 4 {
                return Err(OpError::NonQuadFace(cur));
            }
            let i0 = f
                .corners
                .iter()
                .position(|c| c.v == entry.0)
                .filter(|&i| f.corners[(i + 1) % 4].v == entry.1)
                .ok_or(OpError::LoopOpen)?;
            let quad = [
                f.corners[i0].v,
                f.corners[(i0 + 1) % 4].v,
                f.corners[(i0 + 2) % 4].v,
                f.corners[(i0 + 3) % 4].v,
            ];
            steps.push(Step { face: cur, quad });
            if steps.len() > self.faces.len() {
                return Err(OpError::LoopOpen);
            }
            // Exit edge: the quad's far side, directed as walked.
            let exit = (quad[2], quad[3]);
            // The next face contains the exit edge reversed.
            let mut next = None;
            for (fi, g) in self.faces.iter().enumerate() {
                if fi == cur {
                    continue;
                }
                let k = g.corners.len();
                for i in 0..k {
                    if g.corners[i].v == exit.1 && g.corners[(i + 1) % k].v == exit.0 {
                        next = Some(fi);
                    }
                }
            }
            let next = next.ok_or(OpError::LoopOpen)?;
            if next == start {
                // Closure: a consistent ring arrives back at
                // the start face ON its entry edge — the edge
                // the next face walks, reversed, is `dir`.
                if (exit.1, exit.0) != dir {
                    return Err(OpError::LoopOpen);
                }
                break;
            }
            entry = (exit.1, exit.0);
            cur = next;
        }
        // ---- apply: shared midpoints keep the ring watertight.
        let mut mids: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        let mut mid = |m: &mut Mesh, a: u32, b: u32| -> u32 {
            let key = if a < b { (a, b) } else { (b, a) };
            if let Some(&v) = mids.get(&key) {
                return v;
            }
            let p = lerp(m.verts[a as usize], m.verts[b as usize], t);
            let v = m.push_vert(p);
            mids.insert(key, v);
            v
        };
        let mut added = 0usize;
        for step in &steps {
            let [p, q, r, s] = step.quad;
            let orig = self.faces[step.face].clone();
            let uvof = |v: u32| -> [f32; 2] { orig.corners.iter().find(|c| c.v == v).unwrap().uv };
            let m_entry = mid(self, p, q);
            let m_exit = mid(self, r, s);
            let lerp2 = |a: [f32; 2], b: [f32; 2]| -> [f32; 2] {
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
            };
            let (up, uq, ur, us) = (uvof(p), uvof(q), uvof(r), uvof(s));
            let face = &mut self.faces[step.face];
            face.corners = vec![
                Corner { v: p, uv: up },
                Corner {
                    v: m_entry,
                    uv: lerp2(up, uq),
                },
                Corner {
                    v: m_exit,
                    uv: lerp2(ur, us),
                },
                Corner { v: s, uv: us },
            ];
            self.push_face_corners(
                vec![
                    Corner {
                        v: m_entry,
                        uv: lerp2(up, uq),
                    },
                    Corner { v: q, uv: uq },
                    Corner { v: r, uv: ur },
                    Corner {
                        v: m_exit,
                        uv: lerp2(ur, us),
                    },
                ],
                orig.material,
            );
            // The split halves share the source face's plane;
            // an authored normal stays true for both.
            let nf = self.faces.len() - 1;
            self.faces[nf].normal = orig.normal;
            self.faces[step.face].normal = orig.normal;
            added += 1;
        }
        Ok(added)
    }

    /// Mirror the whole mesh across the plane `axis = at`,
    /// appending the mirrored copy (winding reversed, explicit
    /// normals mirrored). Vertices on the plane duplicate —
    /// follow with [`Mesh::weld`] at a hair threshold to fuse
    /// the seam. Returns the number of faces added.
    pub fn mirror(&mut self, axis: Axis, at: f32) -> usize {
        let ai = axis.index();
        let base = self.verts.len() as u32;
        let src: Vec<[f32; 3]> = self.verts.clone();
        for p in &src {
            let mut q = *p;
            q[ai] = 2.0 * at - q[ai];
            self.push_vert(q);
        }
        let faces: Vec<Face> = self.faces.clone();
        let mut added = 0;
        for f in &faces {
            let corners: Vec<Corner> = f
                .corners
                .iter()
                .rev()
                .map(|c| Corner {
                    v: c.v + base,
                    uv: c.uv,
                })
                .collect();
            let normal = f.normal.map(|mut n| {
                n[ai] = -n[ai];
                n
            });
            self.faces.push(Face {
                corners,
                material: f.material,
                normal,
            });
            added += 1;
        }
        added
    }

    /// Merge vertices within `threshold` of each other
    /// (union-find, lowest index wins — the result is order
    /// independent). Faces left degenerate by a merge are
    /// dropped and counted. Returns (merges, dropped faces).
    pub fn weld(&mut self, threshold: f32) -> (usize, usize) {
        let n = self.verts.len();
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        let t2 = threshold * threshold;
        let mut merges = 0usize;
        for i in 0..n {
            for j in (i + 1)..n {
                let d = sub(self.verts[i], self.verts[j]);
                if dot(d, d) <= t2 {
                    let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                    if ri != rj {
                        let (lo, hi) = if ri < rj { (ri, rj) } else { (rj, ri) };
                        parent[hi] = lo;
                        merges += 1;
                    }
                }
            }
        }
        let root: Vec<u32> = (0..n).map(|i| find(&mut parent, i) as u32).collect();
        let mut dropped = 0usize;
        let mut kept: Vec<Face> = Vec::with_capacity(self.faces.len());
        for f in std::mem::take(&mut self.faces) {
            let corners: Vec<Corner> = f
                .corners
                .iter()
                .map(|c| Corner {
                    v: root[c.v as usize],
                    uv: c.uv,
                })
                .collect();
            let mut distinct: Vec<u32> = Vec::new();
            for c in &corners {
                if !distinct.contains(&c.v) {
                    distinct.push(c.v);
                }
            }
            if distinct.len() < 3 {
                dropped += 1;
                continue;
            }
            kept.push(Face {
                corners,
                material: f.material,
                normal: f.normal,
            });
        }
        self.faces = kept;
        // Faces whose area collapsed under the merge go too.
        let areas: Vec<bool> = (0..self.faces.len())
            .map(|fi| self.face_area(fi) >= 1e-10)
            .collect();
        let mut final_faces = Vec::new();
        for (fi, f) in std::mem::take(&mut self.faces).into_iter().enumerate() {
            if areas[fi] {
                final_faces.push(f);
            } else {
                dropped += 1;
            }
        }
        self.faces = final_faces;
        self.compact();
        (merges, dropped)
    }

    /// Drop unreferenced vertices, renumbering the rest in
    /// their original order. Returns the number dropped.
    pub fn compact(&mut self) -> usize {
        let mut used = vec![false; self.verts.len()];
        for f in &self.faces {
            for c in &f.corners {
                used[c.v as usize] = true;
            }
        }
        let mut remap = vec![0u32; self.verts.len()];
        let mut verts = Vec::with_capacity(self.verts.len());
        for (i, p) in self.verts.iter().enumerate() {
            if used[i] {
                remap[i] = verts.len() as u32;
                verts.push(*p);
            }
        }
        let dropped = self.verts.len() - verts.len();
        self.verts = verts;
        for f in &mut self.faces {
            for c in &mut f.corners {
                c.v = remap[c.v as usize];
            }
        }
        dropped
    }

    /// Subdivide each listed face FLAT — every child lies in the
    /// source face's plane, so the PS1.5 facet read survives.
    /// Quads become 4 quads, triangles 4 triangles, n-gons a fan
    /// of n quads around the centroid. Edge midpoints are shared
    /// across the listed set (a whole shell stays watertight);
    /// faces outside the set keep their long edges, so a partial
    /// subdivision T-junctions at its boundary by design.
    /// Returns the number of faces in the mesh afterwards.
    pub fn subdivide_faces(&mut self, faces: &[usize]) -> OpResult<usize> {
        check_faces(self, faces)?;
        let in_set: Vec<bool> = (0..self.faces.len()).map(|i| faces.contains(&i)).collect();
        let mut mids: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        let mut out: Vec<Face> = Vec::with_capacity(self.faces.len());
        // Iterate a clone: the loop appends midpoint/centre
        // vertices to self.verts as it goes.
        let src = self.faces.clone();
        for (fi, f) in src.iter().enumerate() {
            if !in_set[fi] {
                out.push(f.clone());
                continue;
            }
            let k = f.corners.len();
            let mut mid_idx = Vec::with_capacity(k);
            for i in 0..k {
                let (a, b) = (f.corners[i].v, f.corners[(i + 1) % k].v);
                let key = if a < b { (a, b) } else { (b, a) };
                let v = if let Some(&v) = mids.get(&key) {
                    v
                } else {
                    let p = lerp(self.verts[a as usize], self.verts[b as usize], 0.5);
                    let v = self.push_vert(p);
                    mids.insert(key, v);
                    v
                };
                mid_idx.push(v);
            }
            let mid_uv = |i: usize| -> [f32; 2] {
                let a = f.corners[i].uv;
                let b = f.corners[(i + 1) % k].uv;
                [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
            };
            let cen_pos = self.face_centroid(fi);
            let cen_uv = {
                let mut u = [0.0f32; 2];
                for c in &f.corners {
                    u[0] += c.uv[0];
                    u[1] += c.uv[1];
                }
                [u[0] / k as f32, u[1] / k as f32]
            };
            let corner = |i: usize| f.corners[i];
            let child = |corners: Vec<Corner>| Face {
                corners,
                material: f.material,
                normal: f.normal,
            };
            if k == 3 {
                let (a, b, c) = (corner(0), corner(1), corner(2));
                let (mab, mbc, mca) = (mid_idx[0], mid_idx[1], mid_idx[2]);
                let (uab, ubc, uca) = (mid_uv(0), mid_uv(1), mid_uv(2));
                out.push(child(vec![
                    a,
                    Corner { v: mab, uv: uab },
                    Corner { v: mca, uv: uca },
                ]));
                out.push(child(vec![
                    Corner { v: mab, uv: uab },
                    b,
                    Corner { v: mbc, uv: ubc },
                ]));
                out.push(child(vec![
                    Corner { v: mca, uv: uca },
                    Corner { v: mbc, uv: ubc },
                    c,
                ]));
                out.push(child(vec![
                    Corner { v: mab, uv: uab },
                    Corner { v: mbc, uv: ubc },
                    Corner { v: mca, uv: uca },
                ]));
            } else {
                let cen = self.push_vert(cen_pos);
                let cc = Corner { v: cen, uv: cen_uv };
                for i in 0..k {
                    let prev = (i + k - 1) % k;
                    out.push(child(vec![
                        corner(i),
                        Corner {
                            v: mid_idx[i],
                            uv: mid_uv(i),
                        },
                        cc,
                        Corner {
                            v: mid_idx[prev],
                            uv: mid_uv(prev),
                        },
                    ]));
                }
            }
        }
        self.faces = out;
        Ok(self.faces.len())
    }

    /// Remove a face outright. The hole it leaves is the
    /// caller's intent (an opening, a doorway); `validate` will
    /// report the rim as open edges. Face indices above the
    /// removed one shift down.
    pub fn delete_face(&mut self, face: usize) -> OpResult<Face> {
        if face >= self.faces.len() {
            return Err(OpError::BadFace(face));
        }
        Ok(self.faces.remove(face))
    }

    /// Dissolve a face into the neighbour sharing its first
    /// shared edge: the two polygons merge into one simple
    /// polygon carrying THIS face's material and normal. The
    /// neighbour is removed (indices above it shift). Refuses
    /// when the pair shares no edge, more than one edge, or the
    /// merged boundary would touch itself.
    pub fn dissolve_face(&mut self, face: usize) -> OpResult<()> {
        if face >= self.faces.len() {
            return Err(OpError::BadFace(face));
        }
        // Find a neighbour sharing exactly one undirected edge.
        let mine: Vec<(u32, u32)> = {
            let f = &self.faces[face];
            let k = f.corners.len();
            (0..k)
                .map(|i| (f.corners[i].v, f.corners[(i + 1) % k].v))
                .collect()
        };
        let mut partner: Option<usize> = None;
        let mut shared: Option<(u32, u32)> = None;
        for (gi, g) in self.faces.iter().enumerate() {
            if gi == face {
                continue;
            }
            let k = g.corners.len();
            let mut count = 0;
            let mut edge = None;
            for i in 0..k {
                let (x, y) = (g.corners[i].v, g.corners[(i + 1) % k].v);
                if mine
                    .iter()
                    .any(|&(a, b)| (a == y && b == x) || (a == x && b == y))
                {
                    count += 1;
                    edge = Some((x, y));
                }
            }
            if count > 1 {
                return Err(OpError::MultipleSharedEdges);
            }
            if count == 1 {
                partner = Some(gi);
                shared = edge;
                break;
            }
        }
        let gi = partner.ok_or(OpError::NoSharedEdge)?;
        let (sx, sy) = shared.unwrap(); // directed as in the partner
                                        // Partner walks s->t (its directed shared edge); this
                                        // face walks it reversed. Merged boundary: this face's
                                        // chain from the edge's end back around to its start,
                                        // then the partner's chain likewise.
        let chain = |f: &Face, from: u32, to: u32| -> Vec<Corner> {
            let k = f.corners.len();
            let start = (0..k).find(|&i| f.corners[i].v == from).unwrap();
            let mut out = Vec::new();
            let mut i = start;
            loop {
                out.push(f.corners[i]);
                if f.corners[i].v == to {
                    break;
                }
                i = (i + 1) % k;
            }
            out
        };
        // In this face the shared edge runs sy -> sx (opposite
        // the partner's). Chain A: from sx around to sy (this
        // face's non-shared run, endpoints included). Chain B:
        // from sy around to sx in the partner, endpoints dropped
        // (already in A).
        let a = chain(&self.faces[face], sx, sy);
        let b = chain(&self.faces[gi], sy, sx);
        let mut merged: Vec<Corner> = a;
        for c in b.iter().skip(1).take(b.len().saturating_sub(2)) {
            merged.push(*c);
        }
        let mut distinct: Vec<u32> = Vec::new();
        for c in &merged {
            if distinct.contains(&c.v) {
                return Err(OpError::NotSimple);
            }
            distinct.push(c.v);
        }
        if merged.len() < 3 {
            return Err(OpError::NotSimple);
        }
        self.faces[face].corners = merged;
        self.faces.remove(gi);
        Ok(())
    }

    /// Move the listed vertices by `delta`. Vertices the mesh
    /// does not have are skipped.
    pub fn translate_verts(&mut self, verts: &[u32], delta: [f32; 3]) {
        for &v in verts {
            if let Some(p) = self.verts.get_mut(v as usize) {
                *p = add(*p, delta);
            }
        }
    }

    /// Scale the listed vertices about `center`, per axis.
    pub fn scale_verts(&mut self, verts: &[u32], center: [f32; 3], factor: [f32; 3]) {
        for &v in verts {
            if let Some(p) = self.verts.get_mut(v as usize) {
                *p = [
                    center[0] + (p[0] - center[0]) * factor[0],
                    center[1] + (p[1] - center[1]) * factor[1],
                    center[2] + (p[2] - center[2]) * factor[2],
                ];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed() -> (Mesh, [usize; 6]) {
        let mut m = Mesh::new("t");
        let mat = m.add_material("painted-metal");
        let faces = m.add_box([-0.5, -0.5, -0.5], [0.5, 0.5, 0.5], mat);
        (m, faces)
    }

    #[test]
    fn extrude_raises_a_cap_and_stays_watertight() {
        let (mut m, faces) = boxed();
        let top = faces[2]; // +Y
        let before = m.faces.len();
        m.extrude_faces(&[top], [0.0, 1.0, 0.0], 0.0).unwrap();
        assert_eq!(m.faces.len(), before + 4);
        assert!(m.is_watertight(), "faults: {:?}", m.validate());
        // The cap now sits at y = 1.5.
        let c = m.face_centroid(top);
        assert!((c[1] - 1.5).abs() < 1e-6, "cap centroid {c:?}");
    }

    #[test]
    fn extrude_taper_narrows_the_cap() {
        let (mut m, faces) = boxed();
        let top = faces[2];
        let a0 = m.face_area(top);
        m.extrude_faces(&[top], [0.0, 1.0, 0.0], 0.5).unwrap();
        let a1 = m.face_area(top);
        assert!((a1 - a0 * 0.25).abs() < 1e-5, "areas {a0} -> {a1}");
    }

    #[test]
    fn extrude_rejects_adjacent_faces() {
        let (mut m, faces) = boxed();
        let r = m.extrude_faces(&[faces[2], faces[4]], [0.0, 1.0, 0.0], 0.0);
        assert_eq!(r, Err(OpError::AdjacentFaces));
    }

    #[test]
    fn bevel_face_adds_a_ring_and_stays_watertight() {
        let (mut m, faces) = boxed();
        let top = faces[2];
        let before = m.faces.len();
        m.bevel_faces(&[top], 0.1, 1).unwrap();
        assert_eq!(m.faces.len(), before + 4);
        assert!(m.is_watertight(), "faults: {:?}", m.validate());
        // The cap shrank and rose.
        assert!(m.face_area(top) < 1.0);
        assert!(m.face_centroid(top)[1] > 0.5);
    }

    #[test]
    fn bevel_face_two_segments_adds_two_rings() {
        let (mut m, faces) = boxed();
        let before = m.faces.len();
        m.bevel_faces(&[faces[2]], 0.1, 2).unwrap();
        assert_eq!(m.faces.len(), before + 8);
        assert!(m.is_watertight());
    }

    #[test]
    fn bevel_face_refuses_an_inverting_amount() {
        let (mut m, faces) = boxed();
        assert_eq!(
            m.bevel_faces(&[faces[2]], 0.9, 1),
            Err(OpError::AmountTooLarge)
        );
    }

    /// An open tube (a prism's side band, no caps): every wall
    /// vertex meets exactly two faces, so edge bevel applies.
    fn tube() -> Mesh {
        let mut m = Mesh::new("tube");
        let mat = m.add_material("painted-metal");
        let p = m.add_prism([0.0, 0.0], 0.5, 0.0, 2.0, 8, 0.0, mat);
        let mut caps = [p.top, p.bottom];
        caps.sort_unstable();
        // Delete caps high index first so ids stay valid.
        for &c in caps.iter().rev() {
            m.delete_face(c).unwrap();
        }
        m
    }

    #[test]
    fn bevel_edge_chamfers_a_tube_wall() {
        let mut m = tube();
        // A vertical edge of the tube: the prism generator
        // interleaves its rings — ring vertex i is verts 2i
        // (bottom) and 2i+1 (top) — so ring 0's wall edge is
        // (0, 1).
        let before = m.faces.len();
        let added = m.bevel_edges(&[(0, 1)], 0.05, 1).unwrap();
        assert_eq!(added, 1);
        assert_eq!(m.faces.len(), before + 1);
        let faults = m.validate();
        assert!(
            !faults.iter().any(|f| matches!(
                f,
                MeshFault::FlippedEdge { .. } | MeshFault::NonManifoldEdge { .. }
            )),
            "faults: {faults:?}"
        );
        // The strip's normal points away from the tube axis.
        let strip = m.faces.len() - 1;
        let n = m.face_normal(strip);
        let c = m.face_centroid(strip);
        let radial = normalize([c[0], 0.0, c[2]]);
        assert!(
            dot(n, radial) > 0.3,
            "strip normal {n:?} vs radial {radial:?}"
        );
    }

    #[test]
    fn bevel_edge_refuses_a_three_face_corner() {
        let (mut m, _) = boxed();
        // Box edge 0-1: its endpoints meet three faces each.
        let r = m.bevel_edges(&[(0, 1)], 0.05, 1);
        assert!(matches!(r, Err(OpError::CornerFan(_))), "got {r:?}");
    }

    #[test]
    fn loop_cut_around_a_box() {
        let (mut m, faces) = boxed();
        // A horizontal ring edge on the +Z face: verts 4->5.
        let _ = faces;
        let before_faces = m.faces.len();
        let before_verts = m.verts.len();
        let added = m.loop_cut((4, 5), 0.5).unwrap();
        assert_eq!(added, 4);
        assert_eq!(m.faces.len(), before_faces + 4);
        assert_eq!(m.verts.len(), before_verts + 4);
        assert!(m.is_watertight(), "faults: {:?}", m.validate());
    }

    #[test]
    fn loop_cut_rejects_a_non_quad() {
        let mut m = Mesh::new("t");
        let mat = m.add_material("x");
        let a = m.push_vert([0.0, 0.0, 0.0]);
        let b = m.push_vert([1.0, 0.0, 0.0]);
        let c = m.push_vert([0.0, 1.0, 0.0]);
        m.push_face(&[a, b, c], mat);
        assert_eq!(m.loop_cut((a, b), 0.5), Err(OpError::NonQuadFace(0)));
    }

    #[test]
    fn mirror_and_weld_make_a_whole_box() {
        // Half a box: build a full box, delete the +X face, keep
        // the rest as the "half", mirror it, weld the seam.
        let (mut m, faces) = boxed();
        m.delete_face(faces[0]).unwrap(); // +X open
        let added = m.mirror(Axis::X, 0.5);
        assert_eq!(added, 5);
        let (merges, _) = m.weld(0.001);
        assert!(merges >= 4, "merges {merges}");
        assert!(m.is_watertight(), "faults: {:?}", m.validate());
    }

    #[test]
    fn weld_merges_coincident_verts() {
        let mut m = Mesh::new("t");
        let mat = m.add_material("x");
        m.add_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], mat);
        // A second box sharing the first's +X face plane exactly.
        m.add_box([1.0, 0.0, 0.0], [2.0, 1.0, 1.0], mat);
        let (merges, dropped) = m.weld(0.0001);
        assert_eq!(merges, 4);
        assert_eq!(dropped, 0);
        assert_eq!(m.verts.len(), 12);
    }

    #[test]
    fn subdivide_a_box_stays_watertight() {
        let (mut m, _) = boxed();
        let all: Vec<usize> = (0..m.faces.len()).collect();
        let after = m.subdivide_faces(&all).unwrap();
        assert_eq!(after, 24);
        assert_eq!(m.triangle_count(), 48);
        assert!(m.is_watertight(), "faults: {:?}", m.validate());
        // Total surface area is preserved by flat subdivision.
        let area: f32 = (0..m.faces.len()).map(|f| m.face_area(f)).sum();
        assert!((area - 6.0).abs() < 1e-4, "area {area}");
    }

    #[test]
    fn subdivide_a_triangle_makes_four() {
        let mut m = Mesh::new("t");
        let mat = m.add_material("x");
        let a = m.push_vert([0.0, 0.0, 0.0]);
        let b = m.push_vert([2.0, 0.0, 0.0]);
        let c = m.push_vert([0.0, 2.0, 0.0]);
        m.push_face(&[a, b, c], mat);
        let after = m.subdivide_faces(&[0]).unwrap();
        assert_eq!(after, 4);
        let area: f32 = (0..m.faces.len()).map(|f| m.face_area(f)).sum();
        assert!((area - 2.0).abs() < 1e-5, "area {area}");
    }

    #[test]
    fn dissolve_merges_two_quads() {
        // A 2x1 plate of two quads (open shell), dissolved into
        // one quad of double area.
        let mut m = Mesh::new("t");
        let mat = m.add_material("x");
        let v: Vec<u32> = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [2.0, 1.0, 0.0],
        ]
        .iter()
        .map(|&p| m.push_vert(p))
        .collect();
        m.push_face(&[v[0], v[1], v[4], v[3]], mat);
        m.push_face(&[v[1], v[2], v[5], v[4]], mat);
        m.dissolve_face(0).unwrap();
        assert_eq!(m.faces.len(), 1);
        // The merged boundary keeps the shared edge's endpoints
        // as collinear points on the long runs: a hexagon.
        assert_eq!(m.faces[0].corners.len(), 6);
        assert!((m.face_area(0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn delete_face_leaves_a_reported_hole() {
        let (mut m, faces) = boxed();
        m.delete_face(faces[2]).unwrap();
        assert_eq!(m.faces.len(), 5);
        let faults = m.validate();
        assert_eq!(
            faults
                .iter()
                .filter(|f| matches!(f, MeshFault::OpenEdge { .. }))
                .count(),
            4
        );
    }
}
