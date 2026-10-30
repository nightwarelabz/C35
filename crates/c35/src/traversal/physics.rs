//! Physics slice 1 — capsule, gravity, mantle, the seam.
//!
//! Deliberately NOT a general solver (Rapier stays PARKED): a
//! capsule against static box geometry, gravity with honest fall
//! reporting, a narrow mantle, and an explicit step-ring seam rule.
//! Everything ticks at the fixed 240 Hz step; no wall-clock, no
//! randomness anywhere in this file, so same inputs give same
//! trajectories.
//!
//! The capsule: radius 0.35 m, half-height 0.55 m — 1.80 m tall,
//! the operator's height, eye at 1.62 m. The same capsule shape
//! fits the soldier later; the mesh is never simulated directly.

use crate::traversal::events::{EventLog, StimulusKind};
use crate::traversal::surface::{SurfaceKind, SurfaceRecord};
use glam::Vec3;

pub const GRAVITY: f32 = 9.81;
/// Fixed sim step, matching the animator's 240 Hz.
pub const FIXED_DT: f32 = 1.0 / 240.0;
pub const CAPSULE_RADIUS: f32 = 0.35;
pub const CAPSULE_HALF: f32 = 0.55;
/// A rise at or below this is stepped onto; above it is a wall.
pub const STEP_HEIGHT: f32 = 0.5;
/// Vertical slew rate across a height-field seam — the animator's
/// own rate, so body and feet never disagree about the ledge.
pub const SEAM_SLEW: f32 = 2.2;
/// Ground accel toward the wish speed; air control is weaker.
pub const GROUND_ACCEL: f32 = 10.0;
pub const AIR_ACCEL: f32 = 2.5;
pub const JUMP_SPEED: f32 = 4.6;
/// The step-ring contour: |h| = 2.2 in the island height function
/// (the M1 shore shelf rescales heights inside it by 0.62 — a
/// ~0.84 m discontinuity in the FUNCTION, bridged by the terrain
/// mesh as a steep ramp). Never smoothed over silently: see
/// [`SeamDetector`].
pub const SEAM_CONTOUR: f32 = 2.2;

/// One static box: axis-aligned, with its surface record. Walls,
/// rooftops, parapets, crates, ledges — the whole static world is
/// boxes, and the capsule resolves against them only.
#[derive(Copy, Clone, Debug)]
pub struct BoxCollider {
    pub min: Vec3,
    pub max: Vec3,
    pub surface: SurfaceRecord,
}

impl BoxCollider {
    pub fn new(min: Vec3, max: Vec3, surface: SurfaceRecord) -> BoxCollider {
        BoxCollider { min, max, surface }
    }

    /// Closest point on (or in) the box to `p`.
    pub fn closest(&self, p: Vec3) -> Vec3 {
        Vec3::new(
            p.x.clamp(self.min.x, self.max.x),
            p.y.clamp(self.min.y, self.max.y),
            p.z.clamp(self.min.z, self.max.z),
        )
    }

    pub fn top(&self) -> f32 {
        self.max.y
    }
}

/// What the ground query found under a point.
#[derive(Copy, Clone, Debug)]
pub struct GroundSample {
    pub height: f32,
    pub surface: SurfaceRecord,
}

/// A raycast hit against static geometry.
#[derive(Copy, Clone, Debug)]
pub struct Hit {
    pub point: Vec3,
    pub normal: Vec3,
    pub surface: SurfaceRecord,
    pub dist: f32,
}

/// The static world: a flat list of boxes, iterated in order —
/// deterministic on every target. No spatial index in this slice:
/// the course world is tens of boxes, and a hash or tree would buy
/// nothing the ledger could see.
#[derive(Debug, Default)]
pub struct StaticWorld {
    pub boxes: Vec<BoxCollider>,
}

impl StaticWorld {
    pub fn new() -> StaticWorld {
        StaticWorld { boxes: Vec::new() }
    }

    pub fn add(&mut self, b: BoxCollider) {
        self.boxes.push(b);
    }

    /// Highest top face whose XZ footprint contains (x, z).
    pub fn ground_at(&self, x: f32, z: f32) -> Option<GroundSample> {
        let mut best: Option<GroundSample> = None;
        for b in &self.boxes {
            if x >= b.min.x && x <= b.max.x && z >= b.min.z && z <= b.max.z {
                let h = b.top();
                if best.map_or(true, |g| h > g.height) {
                    best = Some(GroundSample {
                        height: h,
                        surface: b.surface,
                    });
                }
            }
        }
        best
    }

    /// Slab-method raycast; nearest hit wins, ties broken by box
    /// order (deterministic).
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        for b in &self.boxes {
            let inv = Vec3::new(1.0 / dir.x, 1.0 / dir.y, 1.0 / dir.z);
            let mut tmin = 0.0f32;
            let mut tmax = max_dist;
            let mut normal = Vec3::ZERO;
            let mut ok = true;
            // X slab.
            let (t1, t2, n) = slab(origin.x, dir.x, inv.x, b.min.x, b.max.x, Vec3::X);
            if t1 > tmax || t2 < tmin {
                ok = false;
            } else {
                if t1 > tmin {
                    tmin = t1;
                    normal = n;
                }
                tmax = tmax.min(t2);
            }
            if ok {
                let (t1, t2, n) = slab(origin.y, dir.y, inv.y, b.min.y, b.max.y, Vec3::Y);
                if t1 > tmax || t2 < tmin {
                    ok = false;
                } else {
                    if t1 > tmin {
                        tmin = t1;
                        normal = n;
                    }
                    tmax = tmax.min(t2);
                }
            }
            if ok {
                let (t1, t2, n) = slab(origin.z, dir.z, inv.z, b.min.z, b.max.z, Vec3::Z);
                if t1 > tmax || t2 < tmin {
                    ok = false;
                } else if t1 > tmin {
                    tmin = t1;
                    normal = n;
                }
            }
            if ok && tmin <= max_dist {
                let hit = Hit {
                    point: origin + dir * tmin,
                    normal,
                    surface: b.surface,
                    dist: tmin,
                };
                if best.map_or(true, |h| hit.dist < h.dist) {
                    best = Some(hit);
                }
            }
        }
        best
    }

    /// Segment (raycast with a thickness) against boxes: steps
    /// along the segment at capsule-radius intervals so a fast
    /// claw cannot tunnel through a thin wall in one substep.
    pub fn segment_hit(&self, from: Vec3, to: Vec3) -> Option<Hit> {
        let d = to - from;
        let len = d.length();
        if len < 1e-6 {
            return None;
        }
        let dir = d / len;
        let steps = (len / CAPSULE_RADIUS).ceil() as i32 + 1;
        let mut travelled = 0.0f32;
        for _ in 0..steps {
            let at = from + dir * travelled;
            if let Some(h) = self.raycast(at, dir, (len - travelled).min(CAPSULE_RADIUS * 2.0)) {
                return Some(Hit {
                    dist: travelled + h.dist,
                    ..h
                });
            }
            travelled += CAPSULE_RADIUS;
            if travelled >= len {
                break;
            }
        }
        None
    }

    /// Capsule-vs-box penetration resolution. Returns the corrected
    /// feet position. The capsule segment runs from feet + radius
    /// to feet + (2 * half + radius) on the Y axis. For each box,
    /// the closest point on the box to the segment's nearest
    /// approach decides the push; two passes settle corners.
    /// Approximation, stated plainly: this is a slice solver for
    /// axis-aligned boxes, not a general convex solver.
    pub fn resolve_capsule(&self, feet: Vec3) -> Vec3 {
        let mut pos = feet;
        for _ in 0..2 {
            for b in &self.boxes {
                let bot = Vec3::new(pos.x, pos.y + CAPSULE_RADIUS, pos.z);
                let top = Vec3::new(pos.x, pos.y + 2.0 * CAPSULE_HALF + CAPSULE_RADIUS, pos.z);
                // Nearest point on the segment to the box centre,
                // then nearest point on the box to that.
                let c = (b.min + b.max) * 0.5;
                let ab = top - bot;
                let t = ((c - bot).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0);
                let seg_pt = bot + ab * t;
                let box_pt = b.closest(seg_pt);
                let delta = seg_pt - box_pt;
                let d = delta.length();
                if d < CAPSULE_RADIUS - 1e-4 {
                    if d > 1e-6 {
                        pos += delta * ((CAPSULE_RADIUS - d) / d);
                    } else {
                        // Centre inside the box: push along the
                        // smallest-penetration axis.
                        let dx = (b.max.x - pos.x).min(pos.x - b.min.x);
                        let dy = (b.max.y - pos.y).min(pos.y - b.min.y);
                        let dz = (b.max.z - pos.z).min(pos.z - b.min.z);
                        if dx <= dy && dx <= dz {
                            pos.x += if pos.x < c.x { -dx } else { dx };
                        } else if dy <= dz {
                            pos.y += if pos.y < c.y { -dy } else { dy };
                        } else {
                            pos.z += if pos.z < c.z { -dz } else { dz };
                        }
                    }
                }
            }
        }
        pos
    }

    /// Is a capsule at `feet` free of penetration (touching counts
    /// as free — standing exactly on a top face is contact, not a
    /// hit)? Used by the mantle target check.
    pub fn capsule_free(&self, feet: Vec3, ignore_top_of: Option<usize>) -> bool {
        for (i, b) in self.boxes.iter().enumerate() {
            if Some(i) == ignore_top_of {
                continue;
            }
            let bot = Vec3::new(feet.x, feet.y + CAPSULE_RADIUS, feet.z);
            let top = Vec3::new(feet.x, feet.y + 2.0 * CAPSULE_HALF + CAPSULE_RADIUS, feet.z);
            let c = (b.min + b.max) * 0.5;
            let ab = top - bot;
            let t = ((c - bot).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0);
            let seg_pt = bot + ab * t;
            let box_pt = b.closest(seg_pt);
            if (seg_pt - box_pt).length() < CAPSULE_RADIUS - 1e-4 {
                return false;
            }
        }
        true
    }
}

fn slab(o: f32, d: f32, inv: f32, lo: f32, hi: f32, axis: Vec3) -> (f32, f32, Vec3) {
    if d.abs() < 1e-9 {
        if o < lo || o > hi {
            (f32::INFINITY, f32::NEG_INFINITY, Vec3::ZERO)
        } else {
            (f32::NEG_INFINITY, f32::INFINITY, Vec3::ZERO)
        }
    } else {
        let mut t1 = (lo - o) * inv;
        let mut t2 = (hi - o) * inv;
        let mut n = -axis;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
            n = axis;
        }
        // Entering through the lo face hits -axis; through hi, +axis.
        if inv < 0.0 {
            n = axis;
        }
        (t1, t2, n)
    }
}

/// The simulated body. `feet` is the sole position; velocity is
/// full 3D. Physics reports falls; it never judges them — what an
/// impact MEANS is doctrine's decision, and no threshold lives
/// here.
#[derive(Copy, Clone, Debug)]
pub struct Body {
    pub feet: Vec3,
    pub vel: Vec3,
    pub grounded: bool,
    /// Impact speed of the last landing (0 when none yet).
    pub last_impact: f32,
}

impl Body {
    pub fn new(start: Vec3) -> Body {
        Body {
            feet: start,
            vel: Vec3::ZERO,
            grounded: false,
            last_impact: 0.0,
        }
    }

    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    pub fn horizontal_speed(&self) -> f32 {
        Vec3::new(self.vel.x, 0.0, self.vel.z).length()
    }
}

/// One fixed-step ground/air move. `wish` is a horizontal unit
/// direction (or zero), `speed` the target ground speed, `jump`
/// the jump intent this tick. Landing events carry impact speed.
pub fn move_body(
    body: &mut Body,
    wish: Vec3,
    speed: f32,
    jump: bool,
    dt: f32,
    world: &StaticWorld,
    events: &mut EventLog,
    tick: u64,
) {
    let accel = if body.grounded {
        GROUND_ACCEL
    } else {
        AIR_ACCEL
    };
    let target = Vec3::new(wish.x * speed, 0.0, wish.z * speed);
    let mut hv = Vec3::new(body.vel.x, 0.0, body.vel.z);
    let diff = target - hv;
    let step = accel * dt;
    if diff.length() <= step {
        hv = target;
    } else {
        hv += diff.normalize() * step;
    }
    body.vel.x = hv.x;
    body.vel.z = hv.z;

    if body.grounded && jump {
        body.vel.y = JUMP_SPEED;
        body.grounded = false;
    }
    if !body.grounded {
        body.vel.y -= GRAVITY * dt;
    }

    body.feet += body.vel * dt;
    body.feet = world.resolve_capsule(body.feet);

    // Ground settle: snap to the ground underfoot when close and
    // not rising; step up rises at or below STEP_HEIGHT; a higher
    // rise is a wall, and the penetration pass already blocked it.
    let was_grounded = body.grounded;
    let was_falling = !was_grounded && body.vel.y < 0.0;
    if let Some(g) = world.ground_at(body.feet.x, body.feet.z) {
        let over = body.feet.y - g.height;
        if body.vel.y <= 0.0 && over <= 0.02 && over >= -STEP_HEIGHT {
            if was_falling {
                let impact = -body.vel.y;
                body.last_impact = impact;
                if impact > 0.5 {
                    events.push(tick, StimulusKind::Landing, body.feet, g.surface, impact);
                }
            }
            body.feet.y = g.height;
            body.vel.y = 0.0;
            body.grounded = true;
        } else if body.grounded && over > 0.25 {
            // Walked off a ledge: airborne, keep the velocity.
            body.grounded = false;
        } else if body.grounded && over < -0.02 {
            // Ground rose underfoot beyond the snap band but the
            // capsule pass did not block it: ride up to it only
            // within the step rule, else stay (wall).
            if -over <= STEP_HEIGHT {
                body.feet.y = g.height;
            }
        }
    } else if body.grounded {
        body.grounded = false;
    }
}

/// A mantle in progress: a committed camera/body move, eased at
/// both ends, never a teleport of state.
#[derive(Copy, Clone, Debug)]
pub struct Mantle {
    pub t: f32,
    pub dur: f32,
    pub from: Vec3,
    pub to: Vec3,
}

impl Mantle {
    /// Ledge within reach at capsule top, forward intent, climb
    /// to stand. The probe: a ray at waist height finds the face;
    /// the face's top must sit 0.4–1.5 m above the feet (reachable,
    /// not a wall, not a kerb); the landing spot must be free.
    /// Returns None when any condition fails — a failed mantle
    /// is a failed mantle, not a partial climb.
    pub fn try_start(feet: Vec3, facing: Vec3, world: &StaticWorld) -> Option<Mantle> {
        let probe = Vec3::new(feet.x, feet.y + 1.2, feet.z);
        let hit = world.raycast(probe, facing, 1.3)?;
        let ledge_top = hit.point.y;
        // The hit box is the one whose face we struck: recover it
        // as the box whose top is nearest the hit height and whose
        // footprint contains the hit point's XZ on its face.
        let mut ledge: Option<(usize, &BoxCollider)> = None;
        for (i, b) in world.boxes.iter().enumerate() {
            let on_face = (hit.point.x - b.min.x).abs() < 0.05
                || (hit.point.x - b.max.x).abs() < 0.05
                || (hit.point.z - b.min.z).abs() < 0.05
                || (hit.point.z - b.max.z).abs() < 0.05;
            if on_face && (b.top() - ledge_top).abs() < 0.1 {
                ledge = Some((i, b));
                break;
            }
        }
        let (idx, b) = ledge?;
        let rise = b.top() - feet.y;
        if rise < 0.4 || rise > 1.5 {
            return None;
        }
        let to = Vec3::new(
            hit.point.x + facing.x * 0.45,
            b.top(),
            hit.point.z + facing.z * 0.45,
        );
        if !world.capsule_free(to, Some(idx)) {
            return None;
        }
        Some(Mantle {
            t: 0.0,
            dur: 0.45,
            from: feet,
            to,
        })
    }

    /// Advance; returns the eased position, or None when done
    /// (the caller plants the body at `to` and grounds it).
    pub fn step(&mut self, dt: f32) -> Option<Vec3> {
        self.t += dt;
        let u = (self.t / self.dur).clamp(0.0, 1.0);
        // Smoothstep ease at both ends; a raised midpoint arcs
        // the hands over the ledge instead of through it.
        let e = u * u * (3.0 - 2.0 * u);
        let pos = self.from.lerp(self.to, e);
        let arc = (e * (1.0 - e) * 4.0) * 0.3;
        let done = self.t >= self.dur;
        if done {
            None
        } else {
            Some(Vec3::new(pos.x, pos.y + arc, pos.z))
        }
    }
}

/// The step-ring seam rule, as pure functions over height
/// samples. Two detectors, for two observables:
/// - `crossed`: the contour test — endpoints on opposite sides
///   of |h| = SEAM_CONTOUR. Right when the caller samples the
///   PRE-shelf height (the value before the island's shore
///   rescale); on rescaled output it misfires on smooth steep
///   crossings, so the island demo does not use it.
/// - `jump_crossed`: the discontinuity signature — consecutive
///   samples differing by more than `threshold`. The shelf
///   jump is ~0.84 m; smooth terrain moves <0.01 m per
///   sub-metre step, so a 0.5 m threshold separates them
///   cleanly WITHOUT touching the terrain function (the brief
///   forbids smuggling a terrain change into this milestone).
///   The seam is REPORTED, never smoothed over silently, and
///   the caller slews the ground reference across it at
///   SEAM_SLEW instead of snapping.
pub struct SeamDetector;

impl SeamDetector {
    pub fn crossed(h0: f32, h1: f32) -> bool {
        (h0.abs() < SEAM_CONTOUR) != (h1.abs() < SEAM_CONTOUR)
    }

    pub fn jump_crossed(h0: f32, h1: f32, threshold: f32) -> bool {
        (h1 - h0).abs() > threshold
    }

    /// Crossing along a segment, with a midpoint sample so a
    /// touch-and-return inside one step is not missed.
    pub fn crossed_segment(
        height: fn(f32, f32) -> f32,
        x0: f32,
        z0: f32,
        x1: f32,
        z1: f32,
    ) -> bool {
        let h0 = height(x0, z0);
        let h1 = height(x1, z1);
        if Self::crossed(h0, h1) {
            return true;
        }
        let hm = height((x0 + x1) * 0.5, (z0 + z1) * 0.5);
        Self::crossed(h0, hm) || Self::crossed(hm, h1)
    }

    /// Jump-signature crossing along a segment (same midpoint
    /// refinement, on output differences).
    pub fn jump_segment(
        height: fn(f32, f32) -> f32,
        x0: f32,
        z0: f32,
        x1: f32,
        z1: f32,
        threshold: f32,
    ) -> bool {
        let h0 = height(x0, z0);
        let h1 = height(x1, z1);
        if Self::jump_crossed(h0, h1, threshold) {
            return true;
        }
        let hm = height((x0 + x1) * 0.5, (z0 + z1) * 0.5);
        Self::jump_crossed(h0, hm, threshold) || Self::jump_crossed(hm, h1, threshold)
    }
}

/// A height-field world adapter: ground height + normal from a
/// height function, for the island terrain path. Surface is
/// stone by construction here; the course world (boxes) carries
/// the real surface records.
pub struct HeightField {
    pub height: fn(f32, f32) -> f32,
}

impl HeightField {
    pub fn sample(&self, x: f32, z: f32) -> GroundSample {
        GroundSample {
            height: (self.height)(x, z),
            surface: SurfaceRecord::dry(SurfaceKind::Stone),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traversal::surface::{SurfaceKind, SurfaceRecord};

    fn unit_box() -> StaticWorld {
        let mut world = StaticWorld::new();
        world.add(BoxCollider::new(
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
            SurfaceRecord::dry(SurfaceKind::Stone),
        ));
        world
    }

    #[test]
    fn raycast_hits_each_face_and_misses_a_clear_ray() {
        let world = unit_box();
        let cases = [
            (Vec3::new(3.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0), Vec3::X),
            (Vec3::new(-3.0, 0.0, 0.0), Vec3::X, -Vec3::X),
            (Vec3::new(0.0, 3.0, 0.0), Vec3::new(0.0, -1.0, 0.0), Vec3::Y),
            (Vec3::new(0.0, -3.0, 0.0), Vec3::Y, -Vec3::Y),
            (Vec3::new(0.0, 0.0, 3.0), Vec3::new(0.0, 0.0, -1.0), Vec3::Z),
            (Vec3::new(0.0, 0.0, -3.0), Vec3::Z, -Vec3::Z),
        ];
        for (origin, dir, expect_n) in cases {
            let hit = world.raycast(origin, dir, 10.0).expect("hit");
            assert!(
                (hit.normal - expect_n).length() < 1e-4,
                "from {origin:?} normal {:?} want {expect_n:?}",
                hit.normal
            );
            assert!(hit.dist > 0.0 && hit.dist < 3.0, "dist {}", hit.dist);
        }
        assert!(world
            .raycast(Vec3::new(0.0, 3.0, 3.0), Vec3::new(0.0, 0.0, 1.0), 10.0)
            .is_none());
    }
}
