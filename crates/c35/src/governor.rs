//! The quality governor — the thesis's signature system.
//!
//! The engine profiles itself continuously and **governs** its own cost:
//! an exponential moving average of frame time is held against a target
//! (16.7 ms by default), and when the average runs over budget the
//! governor steps DOWN a ranked ladder of costs — render scale first,
//! then shadow resolution, then terrain detail reach, then water wave
//! octaves — and steps back UP, cautiously, when headroom persists. The
//! ladder order is the design document's: shed the pixels nobody can
//! count before touching the light, the light before the land, the land
//! before the sea's motion.
//!
//! Anti-oscillation law (Simulacrum's Iris is the house precedent):
//! changes are evaluated on a cadence, never per frame; a step down
//! buys a cooldown; a step up demands a long, sustained headroom and a
//! longer cooldown. The governor cannot hunt.
//!
//! **A pin is a pin.** An operator may pin a ladder level (windowed host
//! keys, or the proof rig's flags). While pinned, the governor observes
//! and *reports* overruns — it never acts, not even when the frame is
//! catastrophically over budget. If the pinned level cannot hold the
//! target, the ledger says so; the operator's word stands.

/// The ladder: each level is a complete quality state. Level 0 is the
/// full picture; level 10 is the floor. The decomposition is staged —
/// scale exhausts its steps before shadows are touched, and so on.
/// Graphics push (2026-10-30): the shadow stage grew a fourth step
/// (4096 at level 0) — the G1 holds 60 fps with headroom, so the top
/// of the ladder spends it on shadow texels first.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct QualityState {
    /// Internal render scale (temporal reconstruction lifts it back to
    /// the output resolution where the backend carries history).
    pub render_scale: f32,
    /// Sun shadow map edge in texels; 0 = shadow pass off.
    pub shadow_size: u32,
    /// Multiplier on the terrain detail fade distance.
    pub detail_fade_mul: f32,
    /// Water wave octaves (highest-frequency first to be shed).
    pub water_octaves: u32,
}

pub const MAX_LEVEL: usize = 10;

const SCALE_STEPS: [f32; 4] = [1.00, 0.85, 0.70, 0.55];
const SHADOW_STEPS: [u32; 4] = [4096, 2048, 1024, 0];
const DETAIL_STEPS: [f32; 3] = [1.00, 0.70, 0.45];
const WATER_STEPS: [u32; 3] = [4, 3, 2];

/// The quality state at a ladder level.
pub fn quality_at(level: usize) -> QualityState {
    let level = level.min(MAX_LEVEL);
    // Staged decomposition: level 0..=3 spends scale, 3..=6 shadows,
    // 6..=8 detail reach, 8..=10 water octaves (each stage starts when
    // the previous one is exhausted).
    let scale_idx = level.min(3);
    let shadow_idx = level.saturating_sub(3).min(3);
    let detail_idx = level.saturating_sub(6).min(2);
    let water_idx = level.saturating_sub(8).min(2);
    QualityState {
        render_scale: SCALE_STEPS[scale_idx],
        shadow_size: SHADOW_STEPS[shadow_idx],
        detail_fade_mul: DETAIL_STEPS[detail_idx],
        water_octaves: WATER_STEPS[water_idx],
    }
}

/// One governor decision, recorded at the frame it happened.
#[derive(Copy, Clone, Debug)]
pub struct Adjustment {
    pub frame: u64,
    pub from_level: usize,
    pub to_level: usize,
    pub ema_ms: f64,
}

pub struct Governor {
    target_ms: f64,
    level: usize,
    pin: Option<usize>,
    ema_ms: f64,
    frames: u64,
    /// Frames since the last level change (cooldown clock).
    since_change: u64,
    /// Consecutive evaluations of sustained headroom (for step-ups).
    headroom_evals: u32,
    adjustments: Vec<Adjustment>,
    /// Frames whose EMA sat over target while pinned (reported, never
    /// acted on).
    pinned_overruns: u64,
    state: QualityState,
}

const EVAL_EVERY: u64 = 30;
const DOWN_COOLDOWN: u64 = 60;
const UP_COOLDOWN: u64 = 300;
const UP_EVALS_NEEDED: u32 = 8; // ~4 s of sustained headroom at cadence

impl Governor {
    pub fn new(target_ms: f64) -> Governor {
        Governor {
            target_ms,
            level: 0,
            pin: None,
            ema_ms: 0.0,
            frames: 0,
            since_change: 0,
            headroom_evals: 0,
            adjustments: Vec::new(),
            pinned_overruns: 0,
            state: quality_at(0),
        }
    }

    /// Retarget the governor (rigs do this before the first frame; the
    /// ladder level and EMA are untouched).
    pub fn set_target(&mut self, target_ms: f64) {
        self.target_ms = target_ms;
    }

    /// Place the ladder at a level WITHOUT pinning and WITHOUT logging
    /// an adjustment — a rig/setup operation (the proof host's
    /// `--start-level`, used to prove the step-up law from partway
    /// down the ladder). The governor then governs live from there.
    pub fn force_level(&mut self, level: usize) {
        let level = level.min(MAX_LEVEL);
        self.level = level;
        self.state = quality_at(level);
        self.since_change = 0;
        self.headroom_evals = 0;
    }

    /// Pin the governor at a level (clamped). From this call on it only
    /// watches and reports.
    pub fn pin_to(&mut self, level: usize) {
        let level = level.min(MAX_LEVEL);
        self.pin = Some(level);
        if level != self.level {
            self.adjustments.push(Adjustment {
                frame: self.frames,
                from_level: self.level,
                to_level: level,
                ema_ms: self.ema_ms,
            });
            self.level = level;
            self.state = quality_at(level);
            self.since_change = 0;
        }
    }

    pub fn unpin(&mut self) {
        self.pin = None;
    }

    pub fn is_pinned(&self) -> bool {
        self.pin.is_some()
    }

    pub fn state(&self) -> QualityState {
        self.state
    }

    pub fn level(&self) -> usize {
        self.level
    }

    pub fn ema_ms(&self) -> f64 {
        self.ema_ms
    }

    pub fn target_ms(&self) -> f64 {
        self.target_ms
    }

    pub fn adjustments(&self) -> &[Adjustment] {
        &self.adjustments
    }

    pub fn pinned_overruns(&self) -> u64 {
        self.pinned_overruns
    }

    /// Feed one frame's cost. `ms` should be GPU frame cost where the
    /// backend exposes it, frame wall time otherwise — the caller says
    /// which in its own ledger. Returns true when the quality state
    /// changed and the renderer must re-apply it.
    pub fn observe(&mut self, ms: f64) -> bool {
        self.frames += 1;
        self.since_change += 1;
        if self.frames == 1 || self.ema_ms == 0.0 {
            self.ema_ms = ms;
        } else {
            self.ema_ms = self.ema_ms * 0.92 + ms * 0.08;
        }

        if self.pin.is_some() {
            if self.ema_ms > self.target_ms {
                self.pinned_overruns += 1;
            }
            return false;
        }

        if self.frames % EVAL_EVERY != 0 {
            return false;
        }

        // Step DOWN: over budget and cooldown served.
        if self.ema_ms > self.target_ms * 1.05
            && self.level < MAX_LEVEL
            && self.since_change >= DOWN_COOLDOWN
        {
            self.step(self.level + 1);
            return true;
        }
        // Step UP: deep, sustained headroom only.
        if self.ema_ms < self.target_ms * 0.60 && self.level > 0 {
            self.headroom_evals += 1;
            if self.headroom_evals >= UP_EVALS_NEEDED && self.since_change >= UP_COOLDOWN {
                self.step(self.level - 1);
                return true;
            }
        } else {
            self.headroom_evals = 0;
        }
        false
    }

    fn step(&mut self, to: usize) {
        self.adjustments.push(Adjustment {
            frame: self.frames,
            from_level: self.level,
            to_level: to,
            ema_ms: self.ema_ms,
        });
        self.level = to;
        self.state = quality_at(to);
        self.since_change = 0;
        self.headroom_evals = 0;
    }
}
