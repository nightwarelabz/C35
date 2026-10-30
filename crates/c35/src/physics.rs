//! Fixed-step point-mass dynamics and a unilateral grapple-rope constraint.
//! The rope allows slack, removes outward radial velocity when taut, and
//! preserves tangential release momentum. Static world queries live in
//! [`crate::spatial`]. Capsule collision and rigid-body solving are separate
//! systems; this module does not claim to provide either.

use glam::Vec3;

#[derive(Copy, Clone, Debug)]
pub struct Particle {
    pub position: Vec3,
    pub velocity: Vec3,
}

#[derive(Copy, Clone, Debug)]
pub struct Rope {
    pub anchor: Vec3,
    pub length: f32,
}

#[derive(Copy, Clone, Default, Debug)]
pub struct ConstraintReport {
    pub taut: bool,
    pub correction: f32,
    pub removed_outward_speed: f32,
}

impl Rope {
    pub fn new(anchor: Vec3, length: f32) -> Option<Self> {
        (anchor.is_finite() && length.is_finite() && length > 0.0)
            .then_some(Self { anchor, length })
    }

    pub fn constrain(&self, body: &mut Particle) -> ConstraintReport {
        if !self.anchor.is_finite()
            || !self.length.is_finite()
            || self.length <= 0.0
            || !body.position.is_finite()
            || !body.velocity.is_finite()
        {
            return ConstraintReport::default();
        }
        let offset = body.position - self.anchor;
        let distance = offset.length();
        if distance < self.length || distance <= f32::EPSILON {
            return ConstraintReport::default();
        }
        let radial = offset / distance;
        let outward = body.velocity.dot(radial).max(0.0);
        body.position = self.anchor + radial * self.length;
        body.velocity -= radial * outward;
        ConstraintReport {
            taut: true,
            correction: (distance - self.length).max(0.0),
            removed_outward_speed: outward,
        }
    }
}

impl Particle {
    /// Semi-implicit Euler. Supply the fixed clock's step, then optionally
    /// solve a rope. Passing `None` releases it without changing velocity.
    pub fn step(&mut self, acceleration: Vec3, dt: f32, rope: Option<&Rope>) -> ConstraintReport {
        if !dt.is_finite()
            || dt <= 0.0
            || !acceleration.is_finite()
            || !self.position.is_finite()
            || !self.velocity.is_finite()
        {
            return ConstraintReport::default();
        }
        self.velocity += acceleration * dt;
        self.position += self.velocity * dt;
        rope.map_or_else(ConstraintReport::default, |rope| rope.constrain(self))
    }
}
