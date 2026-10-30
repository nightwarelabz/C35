//! Lightning: a visual flash plus a delayed thunder, both
//! derived from the same strike. Thunder is deliberately
//! NOT a DistrictStimulus: that enum is closed
//! (Scream / Report / Tremor) and adding a variant means
//! editing perception/sound.rs, which is not this module's
//! to touch. So weather reports strikes and the host
//! routes them — through the sound playback path once a
//! device backend exists, or through a future
//! DistrictKind::Thunder. The strike carries everything
//! the router needs: position, magnitude, and the
//! sound-travel delay from any listener.

use glam::Vec3;

/// Speed of sound in m/s. Thunder delay is distance over
/// this — a strike 3.4 km away rumbles 10 s later, which
/// is why thunder always arrives after the flash.
pub const SOUND_SPEED_M_S: f32 = 343.0;

/// One strike: a flash the renderer can show and a
/// thunder the host can sound. Position is world
/// coordinates; the host picks a strike point in the
/// district (a rooftop, a mast) — weather does not
/// invent geography.
#[derive(Copy, Clone, Debug)]
pub struct LightningStrike {
    /// Where the bolt hit, world coordinates.
    pub pos: Vec3,
    /// Flash peak, 0..1. The renderer scales its sky
    /// flash by this; 1.0 is a storm-splitter.
    pub flash: f32,
    /// Thunder magnitude at the strike, 0..1. Falls with
    /// distance at the router, never here.
    pub magnitude: f32,
}

impl LightningStrike {
    /// Seconds between flash and thunder for a listener
    /// at `ear`. Distance over the speed of sound — the
    /// only honest delay model, and it costs one length.
    pub fn thunder_delay(&self, ear: Vec3) -> f32 {
        (self.pos - ear).length() / SOUND_SPEED_M_S
    }
}

/// Strike scheduler: deterministic strikes inside Storm
/// regimes only. Strike times come from a hash of
/// (seed, strike index), so the same seed storms the
/// same minutes — and a host that replays a day hears
/// the same thunder. Between strikes the scheduler is
/// silent; it never invents weather to fill the gap.
#[derive(Debug)]
pub struct StrikeClock {
    seed: u32,
    next_strike_hours: f32,
    strike_index: u32,
}

impl StrikeClock {
    pub fn new(seed: u32, start_hours: f32) -> StrikeClock {
        let mut clock = StrikeClock {
            seed,
            next_strike_hours: start_hours,
            strike_index: 0,
        };
        clock.schedule_next();
        clock
    }

    fn schedule_next(&mut self) {
        // 2–9 minutes between strikes in a storm: hash
        // the index, scale into the gap. Deterministic.
        let mut x = (self.seed as u64) ^ ((self.strike_index as u64) << 32);
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let frac = ((x ^ (x >> 31)) as u32 % 1000) as f32 / 1000.0;
        self.next_strike_hours += (2.0 + frac * 7.0) / 60.0;
        self.strike_index += 1;
    }

    /// If `hours` has reached the next strike and the
    /// regime allows it, yield a strike at `pos` and
    /// schedule the following one. Otherwise None.
    /// A non-storm regime never yields, however long
    /// the clock runs — the clock is armed by the sky.
    pub fn poll(&mut self, hours: f32, strikes: bool, pos: Vec3) -> Option<LightningStrike> {
        if !strikes || hours < self.next_strike_hours {
            return None;
        }
        // Flash and magnitude from the same index hash
        // family, so a replayed storm flashes identically.
        let mut x = (self.seed as u64) ^ ((self.strike_index as u64) << 16);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        let frac = ((x ^ (x >> 31)) as u32 % 1000) as f32 / 1000.0;
        let strike = LightningStrike {
            pos,
            flash: 0.6 + frac * 0.4,
            magnitude: 0.7 + frac * 0.3,
        };
        self.schedule_next();
        Some(strike)
    }
}
