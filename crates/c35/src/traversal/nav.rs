//! Navigation — a Recast-DESIGN navmesh, not a Recast port.
//!
//! From scratch, by design (the from-scratch rule forbids
//! porting): rasterize → walkable test → region growing →
//! greedy polygon merge → typed off-mesh links → A*. The bake
//! takes a district of blocks without redesign (bounds in,
//! mesh out), but only the one Greyport block is owed here.
//!
//! Walkable means: ground exists, headroom is free, and the
//! capsule can stand there. Connectivity joins only neighbors
//! whose height differs by at most STEP_HEIGHT — so a ledge
//! SPLITS regions instead of being walked off: crossing it
//! needs a link, and the link's verb is typed. A link the
//! physics cannot traverse is a bake error, not a runtime
//! surprise — the course checks exactly that.

use crate::traversal::physics::{StaticWorld, STEP_HEIGHT};
use glam::Vec3;
use std::collections::{BinaryHeap, HashMap, VecDeque};

/// Bake grid cell (m). 0.5 m resolves a capsule (0.35 m radius)
/// without drowning the bake in cells.
pub const NAV_CELL: f32 = 0.5;

/// Link verbs — typed, never generic. A router may use a link
/// only by naming its verb, and the physics must be able to
/// traverse it by that verb.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum LinkKind {
    /// Walkable adjacency (poly neighbours, not a link record).
    Walk,
    /// Mantle up: height diff 0.4–1.5 m, within mantle reach.
    Mantle,
    /// Drop down: one-way, priced by the fall report.
    Drop,
    /// Grapple swing across a gap: unobstructed rope line.
    Grapple,
    /// Wall-run along a wall face.
    WallRun,
}

/// One off-mesh link. `price` is the route cost (distance +
/// verb price); `stimulus` is the exposure/noise price the
/// perception layer reads later. Navigation computes routes;
/// it does not decide whether a route is WISE.
#[derive(Clone, Debug)]
pub struct OffMeshLink {
    pub from: Vec3,
    pub to: Vec3,
    pub kind: LinkKind,
    pub price: f32,
    pub stimulus: f32,
}

/// One merged polygon (an axis-aligned rectangle of walkable
/// cells). Neighbours are (poly id, cost) pairs.
#[derive(Clone, Debug)]
pub struct NavPoly {
    pub id: usize,
    pub min: Vec3,
    pub max: Vec3,
    pub center: Vec3,
    pub region: usize,
    pub neighbors: Vec<(usize, f32)>,
}

#[derive(Debug, Default)]
pub struct BakeStats {
    pub cells: usize,
    pub walkable: usize,
    pub regions: usize,
    pub polys: usize,
    pub links_by_kind: Vec<(LinkKind, usize)>,
}

/// The baked mesh: polygons, typed links, and the bake stats.
#[derive(Debug)]
pub struct NavMesh {
    pub polys: Vec<NavPoly>,
    pub links: Vec<OffMeshLink>,
    pub stats: BakeStats,
}

struct Cell {
    ground: f32,
    region: Option<usize>,
}

/// Bake a mesh from the static world over `bounds` (min/max
/// XZ corners; Y is read from the geometry).
pub fn bake(world: &StaticWorld, min: Vec3, max: Vec3) -> NavMesh {
    let nx = ((max.x - min.x) / NAV_CELL).ceil() as usize;
    let nz = ((max.z - min.z) / NAV_CELL).ceil() as usize;
    let mut grid: Vec<Option<Cell>> = Vec::with_capacity(nx * nz);
    let mut walkable = 0usize;
    for j in 0..nz {
        for i in 0..nx {
            let x = min.x + (i as f32 + 0.5) * NAV_CELL;
            let z = min.z + (j as f32 + 0.5) * NAV_CELL;
            let cell = match world.ground_at(x, z) {
                Some(g) => {
                    let feet = Vec3::new(x, g.height, z);
                    if world.capsule_free(feet, None) {
                        walkable += 1;
                        Some(Cell {
                            ground: g.height,
                            region: None,
                        })
                    } else {
                        None
                    }
                }
                None => None,
            };
            grid.push(cell);
        }
    }
    // Region growing: 4-connectivity, joined only where the
    // height step is walkable — ledges split regions.
    let mut regions = 0usize;
    let mut grid = grid;
    for j in 0..nz {
        for i in 0..nx {
            let idx = j * nx + i;
            if grid[idx].is_none() || grid[idx].as_ref().unwrap().region.is_some() {
                continue;
            }
            let region = regions;
            regions += 1;
            let mut queue = VecDeque::new();
            queue.push_back((i, j));
            grid[idx].as_mut().unwrap().region = Some(region);
            while let Some((ci, cj)) = queue.pop_front() {
                let h = grid[cj * nx + ci].as_ref().unwrap().ground;
                for (ni, nj) in [
                    (ci.wrapping_sub(1), cj),
                    (ci + 1, cj),
                    (ci, cj.wrapping_sub(1)),
                    (ci, cj + 1),
                ] {
                    if ni >= nx || nj >= nz {
                        continue;
                    }
                    let nidx = nj * nx + ni;
                    let Some(nc) = grid[nidx].as_ref() else {
                        continue;
                    };
                    if nc.region.is_some() {
                        continue;
                    }
                    if (nc.ground - h).abs() <= STEP_HEIGHT {
                        grid[nidx].as_mut().unwrap().region = Some(region);
                        queue.push_back((ni, nj));
                    }
                }
            }
        }
    }
    // Greedy row-merge: consecutive same-region cells in a row
    // become runs; runs merge vertically when the x-span and
    // region match. Each merged rectangle is one polygon.
    let mut polys: Vec<NavPoly> = Vec::new();
    let mut consumed = vec![false; nx * nz];
    for j in 0..nz {
        for i in 0..nx {
            let idx = j * nx + i;
            if consumed[idx] {
                continue;
            }
            let Some(cell) = grid[idx].as_ref() else {
                continue;
            };
            let region = cell.region.unwrap();
            // Run along the row.
            let mut i1 = i;
            while i1 + 1 < nx {
                let nidx = j * nx + i1 + 1;
                match grid[nidx].as_ref() {
                    Some(nc) if nc.region == Some(region) && !consumed[nidx] => i1 += 1,
                    _ => break,
                }
            }
            // Extend down while every column matches.
            let mut j1 = j;
            'outer: while j1 + 1 < nz {
                for ci in i..=i1 {
                    let nidx = (j1 + 1) * nx + ci;
                    match grid[nidx].as_ref() {
                        Some(nc)
                            if nc.region == Some(region) && !consumed[nidx] => {}
                        _ => break 'outer,
                    }
                }
                j1 += 1;
            }
            for cj in j..=j1 {
                for ci in i..=i1 {
                    consumed[cj * nx + ci] = true;
                }
            }
            let pmin = Vec3::new(
                min.x + i as f32 * NAV_CELL,
                0.0,
                min.z + j as f32 * NAV_CELL,
            );
            let pmax = Vec3::new(
                min.x + (i1 + 1) as f32 * NAV_CELL,
                0.0,
                min.z + (j1 + 1) as f32 * NAV_CELL,
            );
            let mut hsum = 0.0f32;
            let mut hcount = 0usize;
            for cj in j..=j1 {
                for ci in i..=i1 {
                    if let Some(c) = grid[cj * nx + ci].as_ref() {
                        hsum += c.ground;
                        hcount += 1;
                    }
                }
            }
            let havg = if hcount > 0 {
                hsum / hcount as f32
            } else {
                0.0
            };
            polys.push(NavPoly {
                id: polys.len(),
                min: Vec3::new(pmin.x, havg, pmin.z),
                max: Vec3::new(pmax.x, havg, pmax.z),
                center: Vec3::new((pmin.x + pmax.x) * 0.5, havg, (pmin.z + pmax.z) * 0.5),
                region,
                neighbors: Vec::new(),
            });
        }
    }
    // Poly adjacency: shared edges (touching bounds, overlapping
    // span) within a region. Bounds are copied out first so the
    // neighbour push does not borrow against itself.
    for a in 0..polys.len() {
        for b in 0..polys.len() {
            if a == b || polys[a].region != polys[b].region {
                continue;
            }
            let (amin_x, amax_x, amin_z, amax_z) = (
                polys[a].min.x,
                polys[a].max.x,
                polys[a].min.z,
                polys[a].max.z,
            );
            let (bmin_x, bmax_x, bmin_z, bmax_z) = (
                polys[b].min.x,
                polys[b].max.x,
                polys[b].min.z,
                polys[b].max.z,
            );
            let x_touch = (amax_x - bmin_x).abs() < 0.01
                || (bmax_x - amin_x).abs() < 0.01;
            let z_overlap = amin_z < bmax_z && bmin_z < amax_z;
            let z_touch = (amax_z - bmin_z).abs() < 0.01
                || (bmax_z - amin_z).abs() < 0.01;
            let x_overlap = amin_x < bmax_x && bmin_x < amax_x;
            if (x_touch && z_overlap) || (z_touch && x_overlap) {
                // Adjacency inside a region is FREE passage:
                // the polys are contiguous, so walking across
                // a shared edge costs nothing at this
                // granularity. Prices live on the typed links
                // (the interesting decisions). Center-to-center
                // distances were tried here and distorted link
                // choice — a 37 m center distance made a
                // grapple beat a mantle onto a 1.2 m crate.
                // A production mesh would price portals/edge
                // midpoints; this milestone prices links.
                // The loop is full-matrix, so one push covers
                // each direction exactly once.
                polys[a].neighbors.push((b, 0.0));
            }
        }
    }
    // Typed links.
    let mut links: Vec<OffMeshLink> = Vec::new();
    // Mantle / drop links across region boundaries: adjacent
    // cells (4-neighbourhood) in different regions. The
    // capsule clearance erodes a one-cell moat around every
    // obstacle (cells within radius of a face are unwalkable),
    // so ledge pairs are TWO cells apart (1.0 m), not
    // adjacent: the loop checks neighbours at distance 1 AND
    // 2 in +x/+z, within mantle reach (1.3 m) and drop range.
    // Checking only adjacent cells produced zero mantle/drop
    // links — the moat ate them.
    for j in 0..nz {
        for i in 0..nx {
            let Some(cell) = grid[j * nx + i].as_ref() else {
                continue;
            };
            let r0 = cell.region.unwrap();
            let h0 = cell.ground;
            for (di, dj) in [(1, 0), (2, 0), (0, 1), (0, 2)] {
                let (ni, nj) = (i + di, j + dj);
                if ni >= nx || nj >= nz {
                    continue;
                }
                let Some(nc) = grid[nj * nx + ni].as_ref() else {
                    continue;
                };
                let r1 = nc.region.unwrap();
                if r1 == r0 {
                    continue;
                }
                let dh = nc.ground - h0;
                let p0 = Vec3::new(
                    min.x + (i as f32 + 0.5) * NAV_CELL,
                    h0,
                    min.z + (j as f32 + 0.5) * NAV_CELL,
                );
                let p1 = Vec3::new(
                    min.x + (ni as f32 + 0.5) * NAV_CELL,
                    nc.ground,
                    min.z + (nj as f32 + 0.5) * NAV_CELL,
                );
                if dh >= 0.4 && dh <= 1.5 {
                    // Mantle up (one-way), plus the reverse drop.
                    links.push(OffMeshLink {
                        from: p0,
                        to: p1,
                        kind: LinkKind::Mantle,
                        price: 2.0 + dh,
                        stimulus: 0.8,
                    });
                    links.push(OffMeshLink {
                        from: p1,
                        to: p0,
                        kind: LinkKind::Drop,
                        price: 1.0 + dh,
                        stimulus: 0.6,
                    });
                } else if dh < -1.5 {
                    links.push(OffMeshLink {
                        from: p0,
                        to: p1,
                        kind: LinkKind::Drop,
                        price: 1.0 + (-dh),
                        stimulus: 0.6,
                    });
                } else if dh > 1.5 {
                    links.push(OffMeshLink {
                        from: p1,
                        to: p0,
                        kind: LinkKind::Drop,
                        price: 1.0 + dh,
                        stimulus: 0.6,
                    });
                }
            }
        }
    }
    // Grapple links: poly pairs in different regions whose
    // rope line (at +2 m) is unobstructed and within line
    // length. Only the shortest link per ordered pair of
    // regions is kept, so the link set stays a course, not a
    // complete graph.
    let mut best_grapple: HashMap<(usize, usize), OffMeshLink> = HashMap::new();
    for a in 0..polys.len() {
        for b in 0..polys.len() {
            if a == b || polys[a].region == polys[b].region {
                continue;
            }
            let pa = polys[a].center + Vec3::new(0.0, 2.0, 0.0);
            let pb = polys[b].center + Vec3::new(0.0, 2.0, 0.0);
            let dist = (pa - pb).length();
            if dist > 30.0 {
                continue;
            }
            if world.segment_hit(pa, pb).is_some() {
                continue;
            }
            let link = OffMeshLink {
                from: polys[a].center,
                to: polys[b].center,
                kind: LinkKind::Grapple,
                price: 3.0 + dist * 0.1,
                stimulus: 1.2,
            };
            let key = (polys[a].region, polys[b].region);
            let better = best_grapple
                .get(&key)
                .map_or(true, |old| link.price < old.price);
            if better {
                best_grapple.insert(key, link);
            }
        }
    }
    links.extend(best_grapple.into_values());
    // Wall-run links: runs of wall-adjacent cells sharing a
    // wall normal along an axis. A cell is wall-adjacent when a
    // 1 m probe sideways hits a wall within reach.
    {
        let mut wall_at = vec![None; nx * nz];
        for j in 0..nz {
            for i in 0..nx {
                let Some(cell) = grid[j * nx + i].as_ref() else {
                    continue;
                };
                let x = min.x + (i as f32 + 0.5) * NAV_CELL;
                let z = min.z + (j as f32 + 0.5) * NAV_CELL;
                let probe = Vec3::new(x, cell.ground + 1.0, z);
                for dir in [
                    Vec3::X,
                    -Vec3::X,
                    Vec3::Z,
                    -Vec3::Z,
                ] {
                    if let Some(hit) = world.raycast(probe, dir, 0.95) {
                        // Wall normals are horizontal here.
                        if hit.normal.y.abs() < 0.42 {
                            wall_at[j * nx + i] =
                                Some((dir, hit.normal, cell.ground));
                            break;
                        }
                    }
                }
            }
        }
        // Runs along rows and columns with a matching normal.
        for j in 0..nz {
            let mut run_start: Option<usize> = None;
            let mut run_normal: Option<Vec3> = None;
            let flush = |run_start: &mut Option<usize>,
                             run_normal: &mut Option<Vec3>,
                             end: usize,
                             links: &mut Vec<OffMeshLink>| {
                if let (Some(s), Some(n)) = (*run_start, *run_normal) {
                    if end - s >= 3 {
                        let p0 = Vec3::new(
                            min.x + (s as f32 + 0.5) * NAV_CELL,
                            grid[j * nx + s].as_ref().unwrap().ground,
                            min.z + (j as f32 + 0.5) * NAV_CELL,
                        );
                        let p1 = Vec3::new(
                            min.x + ((end - 1) as f32 + 0.5) * NAV_CELL,
                            grid[j * nx + end - 1].as_ref().unwrap().ground,
                            min.z + (j as f32 + 0.5) * NAV_CELL,
                        );
                        let len = (p1 - p0).length();
                        let _ = n;
                        links.push(OffMeshLink {
                            from: p0,
                            to: p1,
                            kind: LinkKind::WallRun,
                            price: len * 0.1,
                            stimulus: 0.9,
                        });
                    }
                }
                *run_start = None;
                *run_normal = None;
            };
            for i in 0..nx {
                match wall_at[j * nx + i] {
                    Some((_, n, _)) => {
                        if run_normal.map_or(true, |rn| (rn - n).length() < 0.1) {
                            if run_start.is_none() {
                                run_start = Some(i);
                                run_normal = Some(n);
                            }
                        } else {
                            flush(&mut run_start, &mut run_normal, i, &mut links);
                            run_start = Some(i);
                            run_normal = Some(n);
                        }
                    }
                    None => flush(&mut run_start, &mut run_normal, i, &mut links),
                }
            }
            flush(&mut run_start, &mut run_normal, nx, &mut links);
        }
    }
    let mut counts: HashMap<LinkKind, usize> = HashMap::new();
    for l in &links {
        *counts.entry(l.kind).or_insert(0) += 1;
    }
    let mut links_by_kind: Vec<(LinkKind, usize)> = counts.into_iter().collect();
    links_by_kind.sort_by_key(|(k, _)| *k as usize);
    NavMesh {
        stats: BakeStats {
            cells: nx * nz,
            walkable,
            regions,
            polys: polys.len(),
            links_by_kind,
        },
        polys,
        links,
    }
}

/// Nearest poly centre to a point (the route's anchor).
/// Nearest polygon to a point. Containment first: the poly
/// whose bounds hold the point wins, because center distance
/// mis-assigns endpoints near small polys — a mantle link
/// leaving a crate's west face attached to the small rect
/// EAST of the crate (nearest center) instead of the rect
/// containing its foot, and A* then routed the long way round
/// via wall B to reach that rect. Only when no poly contains
/// the point (off-mesh queries, link endpoints on seams) does
/// it fall back to nearest center.
pub fn nearest_poly(mesh: &NavMesh, p: Vec3) -> Option<usize> {
    for (i, poly) in mesh.polys.iter().enumerate() {
        if p.x >= poly.min.x - 0.01
            && p.x <= poly.max.x + 0.01
            && p.z >= poly.min.z - 0.01
            && p.z <= poly.max.z + 0.01
        {
            return Some(i);
        }
    }
    mesh.polys
        .iter()
        .enumerate()
        .map(|(i, poly)| (i, (poly.center - p).length()))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Dijkstra over polygons plus typed links (see the
/// inconsistency note at the search loop: free walk edges
/// forbid a Euclidean heuristic). Waypoints are poly
/// centres joined by link endpoints; the returned steps name
/// the link kind used (Walk for poly adjacency). Cost is
/// distance + link price; stimulus sums alongside for the
/// perception layer to read later.
pub struct Route {
    pub waypoints: Vec<Vec3>,
    pub kinds: Vec<LinkKind>,
    pub cost: f32,
    pub stimulus: f32,
}

pub fn route(mesh: &NavMesh, from: Vec3, to: Vec3) -> Option<Route> {
    let start = nearest_poly(mesh, from)?;
    let goal = nearest_poly(mesh, to)?;
    // Edges: poly adjacency + links attached to their nearest polys.
    let mut edges: HashMap<usize, Vec<(usize, f32, LinkKind, Vec3, Vec3, f32)>> =
        HashMap::new();
    for poly in &mesh.polys {
        for (n, cost) in &poly.neighbors {
            edges
                .entry(poly.id)
                .or_default()
                .push((*n, *cost, LinkKind::Walk, poly.center, mesh.polys[*n].center, 0.0));
        }
    }
    for l in &mesh.links {
        let a = nearest_poly(mesh, l.from)?;
        let b = nearest_poly(mesh, l.to)?;
        if a == b {
            continue;
        }
        edges
            .entry(a)
            .or_default()
            .push((b, l.price, l.kind, l.from, l.to, l.stimulus));
    }
    // Dijkstra, not A*: walk edges cost 0, so a Euclidean
    // heuristic is INCONSISTENT here (f can fall along a free
    // edge — moving 30 m closer costs nothing), and A* would
    // stop at the first goal pop, which need not be optimal
    // (it returned a 6.2 wall-run+grapple path while a 3.2
    // walk+mantle path existed). Meshes here are tens of
    // polys; Dijkstra is cheap. When portal costs exist, a
    // consistent heuristic can return.
    // A* with a binary heap (min-heap via reversed ordering).
    #[derive(PartialEq)]
    struct Node {
        f: f32,
        id: usize,
    }
    impl Eq for Node {}
    impl Ord for Node {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            // BinaryHeap pops the greatest Ord. Reverse on
            // total_cmp so NaN cannot panic the search.
            other.f.total_cmp(&self.f)
        }
    }
    impl PartialOrd for Node {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    let heuristic = |_id: usize| 0.0f32;
    let mut open = BinaryHeap::new();
    let mut gscore: HashMap<usize, f32> = HashMap::new();
    let mut came: HashMap<usize, (usize, LinkKind, Vec3, Vec3, f32)> = HashMap::new();
    gscore.insert(start, 0.0);
    open.push(Node {
        f: heuristic(start),
        id: start,
    });
    while let Some(Node { id, .. }) = open.pop() {
        if id == goal {
            break;
        }
        let g = gscore[&id];
        if let Some(es) = edges.get(&id) {
            for (n, cost, kind, wp0, wp1, stim) in es {
                let ng = g + cost;
                if ng < *gscore.get(n).unwrap_or(&f32::INFINITY) {
                    gscore.insert(*n, ng);
                    came.insert(*n, (id, *kind, *wp0, *wp1, *stim));
                    open.push(Node {
                        f: ng + heuristic(*n),
                        id: *n,
                    });
                }
            }
        }
    }
    if !came.contains_key(&goal) && start != goal {
        return None;
    }
    // Walk back collecting segments, then reverse into forward
    // order: waypoints run start → goal, never goal → start.
    let mut segments: Vec<(LinkKind, Vec3, Vec3, f32)> = Vec::new();
    let mut cur = goal;
    while cur != start {
        let (prev, kind, wp0, wp1, stim) = came[&cur];
        segments.push((kind, wp0, wp1, stim));
        cur = prev;
    }
    segments.reverse();
    let mut waypoints = vec![from];
    let mut kinds = Vec::new();
    let mut stimulus = 0.0f32;
    for (kind, wp0, wp1, stim) in segments {
        kinds.push(kind);
        stimulus += stim;
        waypoints.push(wp0);
        waypoints.push(wp1);
    }
    waypoints.push(to);
    waypoints.dedup();
    Some(Route {
        waypoints,
        kinds,
        cost: gscore[&goal],
        stimulus,
    })
}
