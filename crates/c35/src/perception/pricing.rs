//! Pricing — light × motion × posture, attention, memory.
//!
//! Perception never reads a raw position and calls it seen.
//! It prices: light at the target, motion of the target,
//! posture of the target — multiplied, against a perceiver's
//! thresholds, accumulated over ticks into attention, and
//! remembered as a last known position that decays.
//!
//! The conscript terms are deliberate: a soldier hesitates
//! (attention must be sustained, not spiked), can be wrong
//! (a deterministic wobble, never a hidden die roll), and
//! degrades in rain, cold, and fear (attention gain scaled
//! down, never up). A perceiver that cannot be wrong is a
//! camera, not a conscript.

/// What the target's body is doing. Posture is the cheapest
/// stealth the player owns: crouched and still in shadow is
/// nearly invisible; upright and running in a lamp pool is
/// seen street-length.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Posture {
    /// Standing, walking, running — full silhouette.
    Upright,
    /// Crouched — silhouette halved, kit tucked.
    Crouch,
    /// Prone — a ground line, not a figure.
    Prone,
}

impl Posture {
    /// The posture factor in the visibility product.
    pub fn factor(&self) -> f32 {
        match self {
            Posture::Upright => 1.0,
            Posture::Crouch => 0.55,
            Posture::Prone => 0.35,
        }
    }
}

/// Price a sighting: light × motion × posture. Light is the
/// field sample at the target (0.0–1.5). Motion is speed in
/// m/s — a still figure is 0.35 of a moving one, never zero,
/// because a still figure in a lamp pool is still a figure.
/// The product is clamped at 1.5, like the field itself.
pub fn visibility(light_level: f32, speed: f32, posture: Posture) -> f32 {
    let motion = 0.35 + 0.65 * (speed / 5.8).clamp(0.0, 1.2);
    (light_level * motion * posture.factor()).clamp(0.0, 1.5)
}

/// A perceiver's attention: a slow integrator, not a tripwire.
/// Attention rises while the priced score stays above the
/// notice threshold, falls while it stays below. Suspicion
/// needs sustained attention (hesitation); alarm needs
/// sustained suspicion or one confirming event. Thresholds
/// are perceiver properties — a Hound's nose is not a
/// conscript's eyes — so they live here as parameters, not
/// as globals.
#[derive(Copy, Clone, Debug)]
pub struct Attention {
    /// Current level, 0.0 (unaware) to 1.0 (certain).
    pub level: f32,
    /// Score above this starts the rise.
    pub notice: f32,
    /// Level above this reads suspicious.
    pub suspicious_at: f32,
    /// Level above this reads alarmed.
    pub alarmed_at: f32,
    /// Rise per tick at full score.
    pub rise: f32,
    /// Fall per tick at zero score.
    pub fall: f32,
}

impl Attention {
    pub fn new(notice: f32, suspicious_at: f32, alarmed_at: f32) -> Attention {
        Attention {
            level: 0.0,
            notice,
            suspicious_at,
            alarmed_at,
            rise: 0.02,
            fall: 0.01,
        }
    }

    /// Soldier-default attention: slow to suspect, slower
    /// to be sure. A conscript hesitates; the machine does
    /// not (see mecha.rs, where thresholds are tables).
    pub fn soldier() -> Attention {
        Attention {
            level: 0.0,
            notice: 0.25,
            suspicious_at: 0.45,
            alarmed_at: 0.85,
            rise: 0.02,
            fall: 0.012,
        }
    }

    /// One tick of observation. `score` is the priced
    /// visibility (or heard magnitude, same shape). `gain`
    /// scales the rise — degradation, grace, and fear all
    /// arrive here as a single multiplier, never as special
    /// cases. Returns true while the level is rising.
    pub fn observe(&mut self, score: f32, gain: f32) -> bool {
        if score > self.notice {
            let step = self.rise * gain * (score / self.notice).min(2.0);
            self.level = (self.level + step).min(1.0);
            true
        } else {
            self.level = (self.level - self.fall).max(0.0);
            false
        }
    }

    /// A confirming event (body found, report received,
    /// target seen at point-blank) jumps attention straight
    /// to alarmed. Confirmation is not accumulation.
    pub fn confirm(&mut self) {
        self.level = 1.0;
    }

    pub fn is_suspicious(&self) -> bool {
        self.level >= self.suspicious_at
    }

    pub fn is_alarmed(&self) -> bool {
        self.level >= self.alarmed_at
    }
}

/// Last known position: what a perceiver remembers after
/// the target leaves sight. Confidence decays per tick; a
/// search runs while confidence stays above the floor, then
/// the perceiver gives up and returns to patrol. Memory is
/// a position and a fading number — never a live tracker.
#[derive(Copy, Clone, Debug, Default)]
pub struct Memory {
    pub lkp: Option<glam::Vec3>,
    /// 1.0 (just seen) decaying toward 0.0 (forgotten).
    pub confidence: f32,
    /// Ticks since the target was last actually seen.
    pub ticks_since_seen: u64,
}

impl Memory {
    /// Decay per tick: a memory halves roughly every 20
    /// seconds at the 240 Hz fixed step. Slow enough to
    /// search on, fast enough to give up.
    pub const DECAY: f32 = 0.99986;

    pub fn new() -> Memory {
        Memory {
            lkp: None,
            confidence: 0.0,
            ticks_since_seen: 0,
        }
    }

    /// The target was seen here, now. Memory refreshes
    /// fully — a fresh sighting is not a faded one.
    pub fn see(&mut self, pos: glam::Vec3) {
        self.lkp = Some(pos);
        self.confidence = 1.0;
        self.ticks_since_seen = 0;
    }

    /// One tick without sight: confidence fades, the
    /// clock runs. Returns false once the memory is spent.
    pub fn decay_tick(&mut self) -> bool {
        self.ticks_since_seen += 1;
        self.confidence *= Self::DECAY;
        if self.confidence < 0.15 {
            self.lkp = None;
            self.confidence = 0.0;
            return false;
        }
        true
    }

    /// A search is live while a position is remembered
    /// with usable confidence.
    pub fn searching(&self) -> bool {
        self.lkp.is_some() && self.confidence >= 0.15
    }
}

/// The conscript's fallibility, deterministic: a hash of
/// tick and perceiver id, mapped to ±10%. Same tick, same
/// perceiver, same error — a run reproduces, but no
/// perceiver is ever exactly right. Multiply a priced
/// score by (1.0 + wobble) before thresholding.
pub fn wobble(tick: u64, perceiver_id: u64) -> f32 {
    let mut h = tick.wrapping_add(perceiver_id.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    let unit = (h % 1000) as f32 / 1000.0;
    (unit - 0.5) * 0.2
}
