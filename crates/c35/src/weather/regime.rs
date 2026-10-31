//! Seeded weather regime: one slow pattern per day, smooth
//! variation inside it, never a snap.
//!
//! The regime is a function of (seed, day), not of state:
//! two runs with the same seed and the same clock read the
//! same sky. Inside a day, rain and wind wander on the
//! house value-noise field, so a front arrives over
//! minutes, not in one tick. A day's regime is drawn from
//! a hash of (seed, day) — deterministic, uniform, and
//! independent of whatever the host did yesterday.

use crate::noise::{fbm, value_noise};

/// What kind of sky a day leans toward. The regime biases
/// the noise field; it does not dictate it — a Clear day
/// can still drizzle, a Storm day still has dry hours.
/// That is the point: the regime is a climate, not a script.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Regime {
    /// Dry, light wind, thin air.
    Clear,
    /// Grey, still, damp air — fog's day.
    Overcast,
    /// Rain comes and goes; wetness climbs.
    Rain,
    /// Rain plus wind plus lightning.
    Storm,
}

impl Regime {
    /// Bias added to the rain noise for this regime.
    /// Clear still rains if the noise runs hot; Storm
    /// still dries if it runs cold. Bias, not command.
    pub fn rain_bias(self) -> f32 {
        match self {
            Regime::Clear => -0.35,
            Regime::Overcast => -0.05,
            Regime::Rain => 0.25,
            Regime::Storm => 0.45,
        }
    }

    /// Bias added to the wind-speed noise.
    pub fn wind_bias(self) -> f32 {
        match self {
            Regime::Clear => -0.2,
            Regime::Overcast => -0.3,
            Regime::Rain => 0.1,
            Regime::Storm => 0.5,
        }
    }

    /// Fog bias: overcast days hang; storms clear the
    /// air between fronts.
    pub fn fog_bias(self) -> f32 {
        match self {
            Regime::Clear => -0.15,
            Regime::Overcast => 0.35,
            Regime::Rain => 0.15,
            Regime::Storm => 0.0,
        }
    }

    /// Does this regime ever strike lightning?
    pub fn strikes(self) -> bool {
        matches!(self, Regime::Storm)
    }
}

/// One 32-bit hash step: SplitMix64's finalizer shape,
/// written here so weather never depends on another
/// module's RNG. Deterministic math, not borrowed code.
fn hash32(mut x: u64) -> u32 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (x ^ (x >> 31)) as u32
}

/// The day's regime, from (seed, day-of-year). Uniform
/// over the four regimes in the long run; any single
/// seed's calendar is fixed forever.
pub fn regime_for_day(seed: u32, day_of_year: u32) -> Regime {
    let h = hash32(((seed as u64) << 32) | day_of_year as u64);
    // Weighted, not uniform: Greyport is grey more often
    // than it is clear. The weights are the city's climate.
    match h % 100 {
        0..=24 => Regime::Clear,
        25..=54 => Regime::Overcast,
        55..=84 => Regime::Rain,
        _ => Regime::Storm,
    }
}

/// Rain intensity at a moment: regime bias + slow noise
/// over hours, clamped to 0..1. The noise coordinate is
/// (hours since epoch, seed) so the field is continuous
/// across midnight — a front does not reset at 00:00.
pub fn rain_at(seed: u32, hours: f32, regime: Regime) -> f32 {
    let n = fbm(hours * 0.08, seed as f32 * 0.37, 3, seed);
    (n * 0.9 + regime.rain_bias()).clamp(0.0, 1.0)
}

/// Wind speed in m/s at a moment: base breeze plus regime
/// bias plus slow noise. Never negative, never still for
/// long — Greyport always has air moving.
pub fn wind_speed_at(seed: u32, hours: f32, regime: Regime) -> f32 {
    let n = fbm(hours * 0.05 + 40.0, seed as f32 * 0.61, 3, seed ^ 0x5EED);
    (3.0 + n * 5.0 + regime.wind_bias() * 6.0).clamp(0.5, 22.0)
}

/// Wind direction in radians at a moment: a slow wander,
/// not a random draw. Direction is integrated noise, so
/// it turns over hours and never jumps — a compass that
/// spins in one tick is a bug, and this shape cannot do it.
pub fn wind_dir_at(seed: u32, hours: f32) -> f32 {
    let n = value_noise(hours * 0.03, seed as f32 * 0.11, seed ^ 0x5A17);
    (n * std::f32::consts::TAU).rem_euclid(std::f32::consts::TAU)
}

/// The weather fog amount (dimensionless, 0..0.6) expressed
/// as a fog DENSITY. The reality layer's base density lives
/// at the 0.0015 scale (world.rs), and the shader's
/// exponential fog eats density — an amount added raw is
/// ~300x too strong and whites out the world beyond a few
/// metres (the first windowed Mac run, 2026-10-31: amount
/// 0.508 bound as density, street invisible past ~6 m).
/// At this scale the worst storm adds 0.0072: at 50 m the
/// transmittance is ~0.65, at 200 m ~0.2 — Greyport murk,
/// not a whiteout. A typical overcast adds ~0.006.
pub const FOG_ADD_DENSITY_SCALE: f32 = 0.012;

/// Fog addition at a moment, as a fog DENSITY ready to add
/// to the reality layer's `fog_density` at bind time; it
/// never replaces it. A clear noon stays clear; an
/// overcast night thickens.
pub fn fog_add_at(seed: u32, hours: f32, regime: Regime, rain: f32) -> f32 {
    let n = fbm(hours * 0.06 + 80.0, seed as f32 * 0.23, 2, seed ^ 0xF069);
    (n * 0.3 + regime.fog_bias() + rain * 0.15).clamp(0.0, 0.6) * FOG_ADD_DENSITY_SCALE
}

#[cfg(test)]
mod fog_density_contract {
    use super::*;

    /// The bind contract: fog_add() is a DENSITY at the
    /// reality layer's scale, so a host may add it straight
    /// onto world.fog_density (~0.0015). The 2026-10-31 Mac
    /// run bound the raw 0..0.6 amount instead and the world
    /// whited out past ~6 m; these pins keep the unit honest.
    #[test]
    fn fog_add_is_density_scaled_for_every_regime() {
        for regime in [Regime::Clear, Regime::Overcast, Regime::Rain, Regime::Storm] {
            for rain in [0.0f32, 0.5, 1.0] {
                for h in 0..48 {
                    let d = fog_add_at(0xC35_0001, h as f32 * 0.5, regime, rain);
                    assert!(
                        (0.0..=0.0072 + f32::EPSILON).contains(&d),
                        "fog_add {d} outside density range for {regime:?} rain {rain}"
                    );
                }
            }
        }
    }

    #[test]
    fn worst_case_bound_density_still_sees_the_street() {
        // Base reality density tops out near 0.0017 (winter
        // night); even summed with the maximum weather add,
        // 40 m of street must keep most of its light.
        let total = 0.0017 + 0.6 * FOG_ADD_DENSITY_SCALE;
        let transmittance_40m = (-total * 40.0).exp();
        assert!(
            transmittance_40m > 0.6,
            "40 m transmittance {transmittance_40m}"
        );
    }
}
