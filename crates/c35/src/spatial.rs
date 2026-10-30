//! Static triangle BVH for picking, line of sight, and sound occlusion.
//! Built once from the same positions/indices the renderer consumes.
//! Queries allocate nothing; tags retain the original triangle identity.

use glam::Vec3;

#[derive(Copy, Clone, Debug)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(a: Vec3, b: Vec3) -> Self {
        Self {
            min: a.min(b),
            max: a.max(b),
        }
    }
    fn union(self, other: Self) -> Self {
        Self::new(self.min.min(other.min), self.max.max(other.max))
    }
    pub fn overlaps(self, other: Self) -> bool {
        self.min.cmple(other.max).all() && other.min.cmple(self.max).all()
    }
    fn entry(self, ray: &Ray, limit: f32) -> Option<f32> {
        let mut near: f32 = 0.0;
        let mut far = limit;
        for axis in 0..3 {
            let d = ray.direction[axis];
            let o = ray.origin[axis];
            if d == 0.0 {
                if o < self.min[axis] || o > self.max[axis] {
                    return None;
                }
            } else {
                let a = (self.min[axis] - o) / d;
                let b = (self.max[axis] - o) / d;
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if near > far {
                    return None;
                }
            }
        }
        Some(near)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
    pub max_distance: f32,
}

impl Ray {
    /// Normalize direction so hit distances are always in world units.
    pub fn new(origin: Vec3, direction: Vec3, max_distance: f32) -> Option<Self> {
        if !origin.is_finite()
            || !direction.is_finite()
            || !max_distance.is_finite()
            || max_distance <= 0.0
        {
            return None;
        }
        let direction = direction.try_normalize()?;
        Some(Self {
            origin,
            direction,
            max_distance,
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Triangle {
    pub vertices: [Vec3; 3],
    pub tag: u32,
}

impl Triangle {
    fn bounds(&self) -> Aabb {
        Aabb::new(self.vertices[0], self.vertices[1])
            .union(Aabb::new(self.vertices[2], self.vertices[2]))
    }
    fn hit(&self, ray: &Ray, limit: f32) -> Option<RayHit> {
        let [a, b, c] = self.vertices;
        let e1 = b - a;
        let e2 = c - a;
        let p = ray.direction.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-8 {
            return None;
        }
        let inv = det.recip();
        let tvec = ray.origin - a;
        let u = tvec.dot(p) * inv;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = tvec.cross(e1);
        let v = ray.direction.dot(q) * inv;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let distance = e2.dot(q) * inv;
        if distance < 0.0001 || distance > limit {
            return None;
        }
        let normal = e1.cross(e2).normalize_or_zero();
        Some(RayHit {
            distance,
            position: ray.origin + ray.direction * distance,
            normal,
            barycentric: Vec3::new(1.0 - u - v, u, v),
            tag: self.tag,
            front_face: normal.dot(ray.direction) < 0.0,
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub struct RayHit {
    pub distance: f32,
    pub position: Vec3,
    /// Geometric winding normal, even for a back-face hit.
    pub normal: Vec3,
    pub barycentric: Vec3,
    pub tag: u32,
    pub front_face: bool,
}

#[derive(Copy, Clone, Debug)]
struct Node {
    bounds: Aabb,
    start: usize,
    count: usize,
    children: Option<[usize; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshQueryError(pub &'static str);
impl std::fmt::Display for MeshQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for MeshQueryError {}

#[derive(Default)]
pub struct TriangleBvh {
    triangles: Vec<Triangle>,
    order: Vec<usize>,
    nodes: Vec<Node>,
}

impl TriangleBvh {
    pub fn new(triangles: Vec<Triangle>) -> Result<Self, MeshQueryError> {
        if triangles
            .iter()
            .any(|t| t.vertices.iter().any(|v| !v.is_finite()))
        {
            return Err(MeshQueryError("triangle positions must be finite"));
        }
        let mut bvh = Self {
            order: (0..triangles.len()).collect(),
            triangles,
            nodes: Vec::new(),
        };
        if !bvh.order.is_empty() {
            bvh.build(0, bvh.order.len());
        }
        Ok(bvh)
    }

    pub fn from_indexed(positions: &[Vec3], indices: &[u32]) -> Result<Self, MeshQueryError> {
        if indices.len() % 3 != 0 {
            return Err(MeshQueryError(
                "triangle indices must be a multiple of three",
            ));
        }
        let mut triangles = Vec::with_capacity(indices.len() / 3);
        for (tag, face) in indices.chunks_exact(3).enumerate() {
            let mut vertices = [Vec3::ZERO; 3];
            for i in 0..3 {
                vertices[i] = *positions
                    .get(face[i] as usize)
                    .ok_or(MeshQueryError("triangle index out of range"))?;
            }
            triangles.push(Triangle {
                vertices,
                tag: tag as u32,
            });
        }
        Self::new(triangles)
    }

    pub fn from_props(mesh: &crate::props::PropMesh) -> Result<Self, MeshQueryError> {
        let positions: Vec<_> = mesh
            .vertices
            .iter()
            .map(|v| Vec3::from_array(v.position))
            .collect();
        Self::from_indexed(&positions, &mesh.indices)
    }

    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    fn build(&mut self, start: usize, count: usize) -> usize {
        let mut bounds = self.triangles[self.order[start]].bounds();
        for &i in &self.order[start + 1..start + count] {
            bounds = bounds.union(self.triangles[i].bounds());
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            start,
            count,
            children: None,
        });
        if count > 4 {
            let extent = bounds.max - bounds.min;
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            let triangles = &self.triangles;
            let mid = count / 2;
            self.order[start..start + count].select_nth_unstable_by(mid, |a, b| {
                let center = |i: usize| {
                    let b = triangles[i].bounds();
                    b.min[axis] * 0.5 + b.max[axis] * 0.5
                };
                center(*a).total_cmp(&center(*b)).then_with(|| a.cmp(b))
            });
            let left = self.build(start, mid);
            let right = self.build(start + mid, count - mid);
            self.nodes[index].children = Some([left, right]);
        }
        index
    }

    pub fn raycast(&self, ray: &Ray) -> Option<RayHit> {
        // Public Ray fields may have been edited since construction.
        let ray = Ray::new(ray.origin, ray.direction, ray.max_distance)?;
        let mut hit = None;
        if !self.nodes.is_empty() {
            self.cast_node(0, &ray, &mut hit);
        }
        hit
    }

    fn cast_node(&self, index: usize, ray: &Ray, hit: &mut Option<RayHit>) {
        let node = self.nodes[index];
        let limit = hit.as_ref().map_or(ray.max_distance, |h| h.distance);
        if node.bounds.entry(ray, limit).is_none() {
            return;
        }
        if let Some([a, b]) = node.children {
            let da = self.nodes[a]
                .bounds
                .entry(ray, limit)
                .unwrap_or(f32::INFINITY);
            let db = self.nodes[b]
                .bounds
                .entry(ray, limit)
                .unwrap_or(f32::INFINITY);
            let order = if da <= db { [a, b] } else { [b, a] };
            self.cast_node(order[0], ray, hit);
            self.cast_node(order[1], ray, hit);
        } else {
            for &i in &self.order[node.start..node.start + node.count] {
                let limit = hit.as_ref().map_or(ray.max_distance, |h| h.distance);
                if let Some(candidate) = self.triangles[i].hit(ray, limit) {
                    if hit.as_ref().is_none_or(|h| {
                        candidate.distance < h.distance
                            || (candidate.distance == h.distance && candidate.tag < h.tag)
                    }) {
                        *hit = Some(candidate);
                    }
                }
            }
        }
    }

    pub fn occluded(&self, from: Vec3, to: Vec3) -> bool {
        let delta = to - from;
        Ray::new(from, delta, (delta.length() - 0.001).max(0.0))
            .is_some_and(|ray| self.raycast(&ray).is_some())
    }

    /// Broad-phase candidates, emitted without allocating an intermediate list.
    pub fn query_aabb(&self, bounds: Aabb, mut visit: impl FnMut(&Triangle)) {
        if !self.nodes.is_empty() {
            self.query_node(0, bounds, &mut visit);
        }
    }

    fn query_node(&self, index: usize, bounds: Aabb, visit: &mut impl FnMut(&Triangle)) {
        let node = self.nodes[index];
        if !node.bounds.overlaps(bounds) {
            return;
        }
        if let Some(children) = node.children {
            for child in children {
                self.query_node(child, bounds, visit);
            }
        } else {
            for &i in &self.order[node.start..node.start + node.count] {
                if self.triangles[i].bounds().overlaps(bounds) {
                    visit(&self.triangles[i]);
                }
            }
        }
    }
}
