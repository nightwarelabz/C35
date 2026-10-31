//! The weather state: one struct the host ticks, four
//! numbers the host reads. Everything else in this module
//! is the machinery behind those numbers.
//!
//! HOST CONTRACT — four calls, in order, every frame:
//! 1. `tick(dt, hours, day, daylight, cold)` — advance.
//! 2. `scent.wind = weather.wind_vec(); scent.rain =
//!    weather.rain();` — scent.rs already reads these
//!    fields; weather is the writer it was waiting for.
//! 3. Feed `weather.wetness()` into each surface's
//!    `SurfaceRecord::wet(kind, wetness)` — traversal
//!    prices wall-run slip from it, sound prices footstep
//!    splash from it. Weather never writes those records
//!    itself; the host owns the surfaces.
//! 4. Bind `world.fog_density + weather.fog_add()` —
//!    weather adds to the reality layer's fog, never
//!    replaces it. The renderer's ground-haze term then
//!    pools the result in streets and hollows as before.
//!
//! DETERMINISM: same seed, same dt sequence, same
//! weather — all variation is the noise field over time,
//! and the strike clock is a hash chain. A host that
//! replays a day replays its sky exactly.

use glam::Vec3;

use crate::weather::lightning::{LightningStrike, StrikeClock};
use crate::weather::regime::{
    fog_add_at, rain_at, regime_for_day, wind_dir_at, wind_speed_at, Regime,
};

/// What is falling, if anything. Snow is cold rain with
/// a different visual and a slower wetting rate — the
/// host picks the particle, weather picks the kind.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Precip {
    /// Nothing falling.
    None,
    /// Liquid rain. Wets at full rate.
    Rain,
    /// Snow. Wets at half rate (it melts, it does not
    /// soak), and only when it is cold enough to fall.
    Snow,
}

/// The whole sky in one struct. All fields are 0..1
/// except wind, which is honest m/s and radians.
#[derive(Debug)]
pub struct WeatherState {
    seed: u32,
    /// Simulated hours since an arbitrary epoch. The
    /// host owns the clock; weather only reads it.
    hours: f32,
    day_of_year: u32,
    regime: Regime,
    rain: f32,
    wind_speed: f32,
    wind_dir: f32,
    fog_add: f32,
    /// Surface wetness 0 (dry) to 1 (saturated). Rises
    /// under rain, falls in dry air. This is the number
    /// surfaces, wall-runs, and footsteps all price from.
    wetness: f32,
    strikes: StrikeClock,
    /// Last strike yielded, if any — the host drains
    /// this after each tick to flash and rumble.
    pub last_strike: Option<LightningStrike>,
}

impl WeatherState {
    /// Soak time constant: at full rain, wetness covers
    /// 63% of the remaining dryness per SOAK_TAU seconds
    /// (~15 min to 63% saturated, ~45 min to 95%). The
    /// approach is exponential, not linear, so the result
    /// does not depend on the host's tick size — a host
    /// ticking hourly and a host ticking per-second
    /// converge on the same wetness, where a linear soak
    /// would saturate in one hourly tick and crawl in
    /// per-second ones.
    const SOAK_TAU_SEC: f32 = 900.0;
    /// Dry time constant in still night air: wetness
    /// decays to 37% per DRY_TAU seconds. Daylight halves
    /// the constant (dries twice as fast); cold stretches
    /// it — a winter night street stays wet longest, a
    /// summer noon dries fastest.
    const DRY_TAU_SEC: f32 = 3600.0;

    pub fn new(seed: u32, start_hours: f32, day_of_year: u32) -> WeatherState {
        let mut state = WeatherState {
            seed,
            hours: start_hours,
            day_of_year,
            regime: regime_for_day(seed, day_of_year),
            rain: 0.0,
            wind_speed: 2.0,
            wind_dir: 0.0,
            fog_add: 0.0,
            wetness: 0.0,
            strikes: StrikeClock::new(seed, start_hours),
            last_strike: None,
        };
        state.sample();
        state
    }

    /// Advance the sky by `dt` seconds. `hours` is the
    /// host's simulated clock (same epoch as `new`'s
    /// start); `daylight` is 0 (night) to 1 (noon) from
    /// the reality layer; `cold` is 0 (warm) to 1
    /// (freezing) from the same. Day rollover re-draws
    /// the regime from (seed, day) — a new day may bring
    /// a new sky, but never mid-day.
    pub fn tick(&mut self, dt: f32, hours: f32, day_of_year: u32, daylight: f32, cold: f32) {
        if day_of_year != self.day_of_year {
            self.day_of_year = day_of_year;
            self.regime = regime_for_day(self.seed, day_of_year);
        }
        self.hours = hours;
        self.sample();
        // Wetness: exponential approach under precip,
        // exponential decay in dry air. Snow wets at half
        // rate; daylight halves the dry constant and cold
        // stretches it. Tick-size independent by shape.
        let precip = self.precip(cold);
        match precip {
            Precip::Rain => {
                let k = 1.0 - (-dt * self.rain / Self::SOAK_TAU_SEC).exp();
                self.wetness += (1.0 - self.wetness) * k;
            }
            Precip::Snow => {
                let k = 1.0 - (-dt * self.rain * 0.5 / Self::SOAK_TAU_SEC).exp();
                self.wetness += (1.0 - self.wetness) * k;
            }
            Precip::None => {
                let tau = Self::DRY_TAU_SEC / ((1.0 + daylight) * (1.0 - 0.5 * cold));
                self.wetness *= (-dt / tau).exp();
            }
        }
        self.wetness = self.wetness.clamp(0.0, 1.0);
        // Strikes arm only in Storm; the host supplies
        // the strike point (a mast, a rooftop) when it
        // drains last_strike. A fixed district-centre
        // default keeps the demo honest without geography.
        self.last_strike =
            self.strikes
                .poll(self.hours, self.regime.strikes(), Vec3::new(0.0, 60.0, 0.0));
    }

    fn sample(&mut self) {
        self.rain = rain_at(self.seed, self.hours, self.regime);
        self.wind_speed = wind_speed_at(self.seed, self.hours, self.regime);
        self.wind_dir = wind_dir_at(self.seed, self.hours);
        self.fog_add = fog_add_at(self.seed, self.hours, self.regime, self.rain);
    }

    /// Today's regime.
    pub fn regime(&self) -> Regime {
        self.regime
    }

    /// Rain intensity 0 (dry) to 1 (downpour). Copy this
    /// into `ScentField::rain` — rain wash is already
    /// priced there.
    pub fn rain(&self) -> f32 {
        self.rain
    }

    /// What is falling, given `cold` from the reality
    /// layer. Rain below 0.05 is nothing — drizzle that
    /// thin does not count as weather, it counts as damp.
    pub fn precip(&self, cold: f32) -> Precip {
        if self.rain < 0.05 {
            Precip::None
        } else if cold > 0.7 {
            Precip::Snow
        } else {
            Precip::Rain
        }
    }

    /// Wind as a horizontal vector in m/s. Copy this
    /// into `ScentField::wind` — drift is already priced
    /// there at 2% of wind speed, horizontal only.
    pub fn wind_vec(&self) -> Vec3 {
        Vec3::new(
            self.wind_dir.cos() * self.wind_speed,
            0.0,
            self.wind_dir.sin() * self.wind_speed,
        )
    }

    /// Wind speed in m/s, for HUD or audio.
    pub fn wind_speed(&self) -> f32 {
        self.wind_speed
    }

    /// Wind direction in radians (0 = +X, turning toward
    /// +Z). Wanders smoothly; never jumps.
    pub fn wind_dir(&self) -> f32 {
        self.wind_dir
    }

    /// Fog density to ADD to the reality layer's
    /// `fog_density` at bind time. A density at the reality
    /// layer's scale: 0..0.0072 (see regime::FOG_ADD_DENSITY_SCALE).
    pub fn fog_add(&self) -> f32 {
        self.fog_add
    }

    /// Surface wetness 0 (dry) to 1 (saturated). Feed
    /// this into `SurfaceRecord::wet` for every surface
    /// the host builds.
    pub fn wetness(&self) -> f32 {
        self.wetness
    }
}
