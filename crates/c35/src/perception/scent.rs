//! Scent — the Hound's channel, and the only one the
//! operator cannot watch himself spend.
//!
//! No reference engine in the catalog has this channel, so
//! the design docs are the reference and the shape is
//! original: a ground-trail field of segments, each with a
//! position, a strength, and an age. The trail persists for
//! minutes, decays, freshens when the Hound crosses it (his
//! lock sharpens on a crossed segment — the trail itself
//! is not re-laid by the dog), washes in rain, drifts on
//! wind, and breaks at water and at the Wanderer's raked
//! gravel. The visor never shows the player's own scent
//! picture — the debrief grades it after the fact. That
//! last rule is structural: the sense view (visor.rs)
//! takes no scent input at all, so it cannot leak it.
//!
//! Strength is a 0.0–1.0 number. Fresh footfalls lay 1.0;
//! a segment below 0.05 is forgotten (pruned on tick).

use glam::Vec3;

/// One laid segment of trail: where, how strong, how old.
/// Age is in fixed-step ticks (240 Hz); the decay law is
/// stated in minutes, the tick math follows from it.
#[derive(Copy, Clone, Debug)]
pub struct ScentSegment {
    pub pos: Vec3,
    pub strength: f32,
    pub age_ticks: u64,
}

/// The trail field: segments in lay order, wind, rain.
/// Wind is a horizontal vector in m/s; rain is 0.0 (dry)
/// to 1.0 (downpour), written by the weather field when
/// it exists, read here until then.
#[derive(Debug, Default)]
pub struct ScentField {
    pub segments: Vec<ScentSegment>,
    pub wind: Vec3,
    /// 0.0 dry to 1.0 downpour.
    pub rain: f32,
}

impl ScentField {
    /// Half-life of a segment in still, dry air: three
    /// minutes. A trail laid minutes ago is faint, not
    /// gone — the Hound's nose, not the clock, decides
    /// when it is spent.
    pub const HALF_LIFE_SECS: f32 = 180.0;
    /// Ticks per second of the fixed step.
    pub const TICKS_PER_SEC: f32 = 240.0;
    /// Hard cap: the field forgets its oldest segments
    /// past this count. A trail is minutes long, not a
    /// career long — the cap is a ledger bound, not a
    /// gameplay promise.
    pub const MAX_SEGMENTS: usize = 4096;

    pub fn new() -> ScentField {
        ScentField {
            segments: Vec::new(),
            wind: Vec3::ZERO,
            rain: 0.0,
        }
    }

    /// Lay scent at a position. A footfall within half a
    /// metre of an existing segment refreshes that segment
    /// to full strength instead of stacking a second one —
    /// the trail is a line, not a pile.
    pub fn deposit(&mut self, pos: Vec3, strength: f32) {
        for s in self.segments.iter_mut().rev().take(8) {
            if (s.pos - pos).length() < 0.5 {
                if strength > s.strength {
                    s.strength = strength;
                }
                s.age_ticks = 0;
                return;
            }
        }
        self.segments.push(ScentSegment {
            pos,
            strength: strength.clamp(0.0, 1.0),
            age_ticks: 0,
        });
        if self.segments.len() > Self::MAX_SEGMENTS {
            let excess = self.segments.len() - Self::MAX_SEGMENTS;
            self.segments.drain(..excess);
        }
    }

    /// One fixed-step tick: age everything, decay by the
    /// half-life law, wash extra in rain, drift downwind.
    /// Rain wash is an additional decay, not a deletion:
    /// a downpour halves the trail in well under a minute,
    /// a drizzle barely touches it. Wind drift is slow
    /// (2% of wind speed) — scent leans downwind, it does
    /// not blow away.
    pub fn tick(&mut self) {
        let dt = 1.0 / Self::TICKS_PER_SEC;
        let half_life_decay =
            0.5f32.powf(dt / Self::HALF_LIFE_SECS);
        let wash = 1.0 - 0.55 * self.rain * dt * 4.0;
        // Drift is horizontal only: wind leans the trail
        // downwind, it never lifts it off the ground plane
        // it was laid on.
        let drift = Vec3::new(self.wind.x, 0.0, self.wind.z) * dt * 0.02;
        for s in self.segments.iter_mut() {
            s.age_ticks += 1;
            s.strength *= half_life_decay * wash;
            s.pos += drift;
        }
        self.segments.retain(|s| s.strength >= 0.05);
    }

    /// Break the trail in a radius: water crossings and
    /// the Wanderer's raked gravel. A break is a deletion,
    /// not a decay — the Hound reaches the break and the
    /// line simply stops. Returns how many segments broke.
    pub fn break_radius(&mut self, center: Vec3, radius: f32) -> usize {
        let before = self.segments.len();
        self.segments
            .retain(|s| (s.pos - center).length() >= radius);
        before - self.segments.len()
    }

    /// The nearest segment within radius of a nose.
    /// This is ACQUISITION, not following: the nose
    /// turns toward the closest scent when it has no
    /// heading lock. For following a line, use
    /// [`ScentField::sample_heading`]: nearest-only
    /// following oscillates across each bead (just
    /// past a segment, that segment is nearest again,
    /// and the nose walks back onto it forever).
    /// Returns (direction to it, strength).
    pub fn sample(&self, nose: Vec3, radius: f32) -> Option<(Vec3, f32)> {
        let mut best: Option<(f32, Vec3, f32)> = None;
        for s in &self.segments {
            let d = (s.pos - nose).length();
            if d > radius {
                continue;
            }
            let dir = if d > 1e-6 {
                (s.pos - nose) / d
            } else {
                Vec3::X
            };
            let better = match best {
                None => true,
                Some((bd, _, _)) => d < bd,
            };
            if better {
                best = Some((d, dir, s.strength));
            }
        }
        best.map(|(_, dir, strength)| (dir, strength))
    }

    /// The next bead on the line in a heading's forward
    /// hemisphere: the nearest segment within radius
    /// whose direction from the nose has a positive
    /// dot with the heading. Segments behind the nose
    /// are invisible to this sample — that is what
    /// keeps a follower walking the line instead of
    /// oscillating across each bead it passes. When
    /// no forward segment exists, falls back to
    /// [`ScentField::sample`] (re-acquisition: the
    /// nose turns toward the closest scent and takes
    /// a new heading from it).
    pub fn sample_heading(
        &self,
        nose: Vec3,
        radius: f32,
        heading: Vec3,
    ) -> Option<(Vec3, f32)> {
        let mut best: Option<(f32, Vec3, f32)> = None;
        for s in &self.segments {
            let d = (s.pos - nose).length();
            if d > radius || d < 1e-6 {
                continue;
            }
            let dir = (s.pos - nose) / d;
            if dir.dot(heading) <= 0.0 {
                continue;
            }
            let better = match best {
                None => true,
                Some((bd, _, _)) => d < bd,
            };
            if better {
                best = Some((d, dir, s.strength));
            }
        }
        match best {
            Some((_, dir, strength)) => Some((dir, strength)),
            None => self.sample(nose, radius),
        }
    }

    /// The freshest segment near a nose (lowest age), for
    /// the Hound's lock: crossing a fresh segment sharpens
    /// the lock even when the trail is faint overall.
    pub fn freshest_age_near(&self, nose: Vec3, radius: f32) -> Option<u64> {
        self.segments
            .iter()
            .filter(|s| (s.pos - nose).length() <= radius)
            .map(|s| s.age_ticks)
            .min()
    }
}
