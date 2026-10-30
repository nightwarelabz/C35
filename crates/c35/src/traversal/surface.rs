//! Shared surface record — the ONE field every reader prices from.
//!
//! The traversal brief's law: perception prices footsteps from these
//! fields, the wall-run prices wetness from these fields, and the
//! grapple reads biteability from these fields. There is exactly one
//! record shape, and no traversal-local override flag anywhere: a
//! wet wall is wet because the record says so, and the weather
//! engine (ABSENT, not built here) will one day write that value.
//! Until then, courses exercise dry and wet values through the
//! record itself.

/// What a surface is made of. Deliberately coarse: the record
/// prices behavior, not appearance — painters own appearance.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SurfaceKind {
    Brick,
    Concrete,
    Stone,
    Asphalt,
    Metal,
    Wood,
    /// Sky and other unbiteable volumes: the claw cannot hold here,
    /// and the strike fails visibly instead of latching. This is a
    /// physical property of the surface, never a permission bit.
    Sky,
}

/// One surface's behavior record. `wetness` is 0 (dry) to 1
/// (soaked); it is written by the weather field, read by everyone.
#[derive(Copy, Clone, Debug)]
pub struct SurfaceRecord {
    pub kind: SurfaceKind,
    pub wetness: f32,
}

impl SurfaceRecord {
    pub fn dry(kind: SurfaceKind) -> SurfaceRecord {
        SurfaceRecord { kind, wetness: 0.0 }
    }

    pub fn wet(kind: SurfaceKind, wetness: f32) -> SurfaceRecord {
        SurfaceRecord {
            kind,
            wetness: wetness.clamp(0.0, 1.0),
        }
    }

    /// Can the claw bite here? Sky never bites; everything else
    /// bites, because EVERYTHING grapples — the constraint is
    /// physical (line length, hold strength), never permission.
    pub fn biteable(&self) -> bool {
        self.kind != SurfaceKind::Sky
    }

    /// Footstep stimulus magnitude: surface base × speed. The
    /// perception layer reads this number; traversal only writes
    /// it. Base gains are tuned so metal rings loudest and
    /// asphalt quietest, at the same speed.
    pub fn footstep_gain(&self, speed: f32) -> f32 {
        let base = match self.kind {
            SurfaceKind::Brick => 1.0,
            SurfaceKind::Concrete => 0.9,
            SurfaceKind::Stone => 1.0,
            SurfaceKind::Asphalt => 0.8,
            SurfaceKind::Metal => 1.25,
            SurfaceKind::Wood => 1.1,
            SurfaceKind::Sky => 0.0,
        };
        let speed_factor = 0.4 + 0.6 * (speed / 5.8).clamp(0.0, 1.2);
        base * speed_factor * (1.0 - 0.1 * self.wetness)
    }

    /// Wall-run grip: how much of the entry budget the wall
    /// returns. Dry brick holds (1.0); wet concrete slickens
    /// (0.95 dry × (1 − 0.55 wet) ≈ 0.43 soaked). Metal and wood
    /// are poorer walls by nature, not by permission.
    pub fn wall_grip(&self) -> f32 {
        let dry = match self.kind {
            SurfaceKind::Brick => 1.0,
            SurfaceKind::Concrete => 0.95,
            SurfaceKind::Stone => 0.9,
            SurfaceKind::Asphalt => 0.7,
            SurfaceKind::Metal => 0.7,
            SurfaceKind::Wood => 0.8,
            SurfaceKind::Sky => 0.0,
        };
        dry * (1.0 - 0.55 * self.wetness)
    }

    pub fn name(&self) -> &'static str {
        match self.kind {
            SurfaceKind::Brick => "brick",
            SurfaceKind::Concrete => "concrete",
            SurfaceKind::Stone => "stone",
            SurfaceKind::Asphalt => "asphalt",
            SurfaceKind::Metal => "metal",
            SurfaceKind::Wood => "wood",
            SurfaceKind::Sky => "sky",
        }
    }
}
