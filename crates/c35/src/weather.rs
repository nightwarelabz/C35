//! Clock-seeded weather — Greyport's rain, wetness, wind, fog and cloud.
//!
//! The beauty pass names this the weather phase: rain, fog and wet
//! streets as one atmosphere, not particles sprinkled on a dry city.
//! The same UTC instant always yields the same reading. There is no
//! network fetch and no slider. A host that wants the storm calls
//! [`Atmosphere::at`] with the reality layer's unix seconds and
//! [`crate::Renderer::set_atmosphere`].
//!
//! The default is [`Atmosphere::clear`]. Clear weather is exactly
//! dry: fog multiplier 1, no rain sheet, no cloud veil, so a run
//! that never opts in spends nothing and draws nothing new.
//!
//! Cost, when a host does opt in and the reading is active:
//!
//! - **Rain** — one camera-facing sheet, **2 triangles**, one draw.
//!   Streaks are a fragment function (a column hash and a falling
//!   window). The sheet sits [`RAIN_SHEET_DISTANCE`] metres in front
//!   of the eye and depth-tests, so a wall closer than the sheet
//!   occludes it. Skipped when `rain` is at or below
//!   [`RAIN_DRAW_THRESHOLD`].
//! - **Cloud** — one fullscreen triangle, **1 triangle**, one draw,
//!   blended over the sky before terrain covers the ground. Skipped
//!   when `cloud` is at or below [`CLOUD_DRAW_THRESHOLD`].
//!
//! No textures, no extra render targets, no image assets. The
//! ledger line is [`Atmosphere::ledger_line`].
//!
//! Coupling, for the systems that already priced weather and had
//! nothing to write it:
//!
//! - [`Atmosphere::paint_surface`] writes ground wetness onto the
//!   shared [`crate::traversal::SurfaceRecord`] (wall grip, footstep
//!   gain). A wetter record already in hand is kept — a puddle is
//!   not dried by a lighter hour.
//! - [`Atmosphere::vision_gain`] is the soldier's rain degradation
//!   (attention gain × this). Clear weather returns exactly 1.
//! - [`Atmosphere::apply_scent`] writes `rain` and horizontal wind
//!   onto the Hound's trail field, which already washes and drifts.
//! - Hosts that tick a soldier copy `rain` onto his input. This
//!   module does not own the brain.

use glam::{Vec2, Vec3};

use crate::perception::scent::ScentField;
use crate::traversal::surface::SurfaceRecord;

/// Metres in front of the eye the rain sheet is drawn.
pub const RAIN_SHEET_DISTANCE: f32 = 8.0;
/// Triangles in the rain sheet (one quad).
pub const RAIN_TRIANGLES: u64 = 2;
/// Triangles in the cloud veil (one fullscreen triangle).
pub const CLOUD_TRIANGLES: u64 = 1;
/// Rain at or below this draws nothing.
pub const RAIN_DRAW_THRESHOLD: f32 = 0.02;
/// Cloud at or below this draws nothing.
pub const CLOUD_DRAW_THRESHOLD: f32 = 0.02;

/// How far back wetness remembers rain, in seconds (six hours).
pub const WETNESS_LOOKBACK_SECS: f64 = 6.0 * 3600.0;
/// Fixed steps across the lookback. The integral is this many
/// samples, every time — no allocation, no early-out that would
/// change the result.
pub const WETNESS_STEPS: u32 = 64;

/// Fraction of remaining dryness a full hour of rain fills.
const FILL_PER_HOUR: f32 = 0.85;
/// Fraction of current wetness a full dry hour drains.
const DRAIN_PER_HOUR: f32 = 0.45;

/// One instant of Greyport weather. Every field is deterministic
/// in `unix_secs` except [`Atmosphere::clear`], which is the
/// explicit off switch.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Atmosphere {
    pub unix_secs: f64,
    /// 0 dry air, 1 downpour. The rain sheet's column gate.
    pub rain: f32,
    /// 0 dry ground, 1 soaked. Integrated over the lookback,
    /// so the street stays wet after the rain has passed.
    pub wetness: f32,
    /// Horizontal wind, metres per second. `x` is east, `y` is
    /// south (world −Z is north; this vector is world XZ with
    /// Z stored in `y`).
    pub wind: Vec2,
    /// Multiplier on the reality layer's fog density. Exactly
    /// 1 when rain and wetness are both 0.
    pub fog_mul: f32,
    /// 0 clear sky, 1 overcast. The cloud veil's cover.
    pub cloud: f32,
}

impl Atmosphere {
    /// The off switch. Fog multiplier is exactly 1 and both
    /// draws are inactive, so the renderer spends nothing.
    pub fn clear() -> Atmosphere {
        Atmosphere {
            unix_secs: 0.0,
            rain: 0.0,
            wetness: 0.0,
            wind: Vec2::ZERO,
            fog_mul: 1.0,
            cloud: 0.0,
        }
    }

    /// Weather for a UTC instant. Pure: the same `unix_secs`
    /// always returns the same value in one process.
    pub fn at(unix_secs: f64) -> Atmosphere {
        let rain = rain_rate(unix_secs);
        let wetness = wetness_at(unix_secs);
        let wind = wind_at(unix_secs, rain);
        let cloud = cloud_cover(unix_secs, rain);
        let fog_mul = fog_multiplier(rain, wetness);
        Atmosphere {
            unix_secs,
            rain,
            wetness,
            wind,
            fog_mul,
            cloud,
        }
    }

    /// Write this hour's wetness onto a surface record. A record
    /// that is already wetter (a standing puddle, a course that
    /// set the value) keeps its wetness.
    pub fn paint_surface(self, mut rec: SurfaceRecord) -> SurfaceRecord {
        rec.wetness = self.wetness.max(rec.wetness).clamp(0.0, 1.0);
        rec
    }

    /// Attention-gain multiplier for a conscript's eyes. Matches
    /// the soldier brain's rain term (`1 - 0.30 * rain`) and is
    /// floored so a downpour degrades him without blinding him.
    /// Clear weather returns exactly 1.
    pub fn vision_gain(self) -> f32 {
        if self.rain <= 0.0 {
            return 1.0;
        }
        (1.0 - 0.30 * self.rain).clamp(0.70, 1.0)
    }

    /// Wind as a world-space vector. Y is always 0: scent leans
    /// downwind, it does not lift off the ground.
    pub fn wind3(self) -> Vec3 {
        Vec3::new(self.wind.x, 0.0, self.wind.y)
    }

    /// Write rain and wind onto the Hound's trail field. The
    /// field already washes in `rain` and drifts on `wind`; this
    /// is the writer those comments named.
    pub fn apply_scent(self, field: &mut ScentField) {
        field.rain = self.rain.clamp(0.0, 1.0);
        field.wind = self.wind3();
    }

    pub fn rain_active(self) -> bool {
        self.rain > RAIN_DRAW_THRESHOLD
    }

    pub fn cloud_active(self) -> bool {
        self.cloud > CLOUD_DRAW_THRESHOLD
    }

    /// Draws the renderer issues for this reading. 0, 1 or 2.
    pub fn draw_count(self) -> u32 {
        u32::from(self.rain_active()) + u32::from(self.cloud_active())
    }

    /// Triangles those draws submit. 0, 1, 2 or 3.
    pub fn triangle_count(self) -> u64 {
        u64::from(self.rain_active()) * RAIN_TRIANGLES
            + u64::from(self.cloud_active()) * CLOUD_TRIANGLES
    }

    /// One ledger line. Hosts print it when weather is opted in.
    pub fn ledger_line(self) -> String {
        format!(
            "weather          : rain {:.2}  wetness {:.2}  wind {:.1} m/s  fog x{:.2}  cloud {:.2}  draws {}  tris {} (sheet {:.0} m, skipped when clear)",
            self.rain,
            self.wetness,
            self.wind.length(),
            self.fog_mul,
            self.cloud,
            self.draw_count(),
            self.triangle_count(),
            RAIN_SHEET_DISTANCE,
        )
    }
}

/// Instantaneous rain rate in [0, 1] for a UTC instant.
///
/// Two slow lobes (about 17 h and 57 h) plus an hour-bucket
/// hash, then a threshold so most hours are dry. Inside a
/// storm the rate is squared, so drizzle is the body and a
/// downpour is the peak. Integer hash, then `f64` sine — the
/// same class of math the solar model already uses.
pub fn rain_rate(unix_secs: f64) -> f32 {
    let h = unix_secs / 3600.0;
    let a = (h * 0.37).sin() as f32 * 0.5 + 0.5;
    let b = (h * 0.11 + 1.7).sin() as f32 * 0.5 + 0.5;
    let hour_bucket = h.floor() as i64;
    let n = hash01(hour_bucket);
    let raw = a * 0.62 + b * 0.38;
    let t = ((raw - 0.58) / 0.42).clamp(0.0, 1.0);
    (t * t * (0.45 + 0.55 * n)).clamp(0.0, 1.0)
}

/// Integrate a sequence of rain rates into ground wetness.
///
/// Starts dry. Each sample is `dt_hours` long. Rain fills the
/// remaining dryness; a dry sample drains the current wetness.
/// The result stays in [0, 1]. This is the function
/// [`wetness_at`] runs over its fixed lookback, exposed so the
/// fill and drain can be tested without the clock.
pub fn integrate_wetness(rates: &[f32], dt_hours: f32) -> f32 {
    let mut w = 0.0f32;
    let dt = dt_hours.max(0.0);
    for &rain in rates {
        let rain = rain.clamp(0.0, 1.0);
        let fill = FILL_PER_HOUR * rain * (1.0 - w) * dt;
        let drain = DRAIN_PER_HOUR * (1.0 - rain) * w * dt;
        w = (w + fill - drain).clamp(0.0, 1.0);
    }
    w
}

/// Ground wetness at a UTC instant: [`WETNESS_STEPS`] samples
/// across the preceding [`WETNESS_LOOKBACK_SECS`], integrated
/// from dry. A pure function of the instant.
pub fn wetness_at(unix_secs: f64) -> f32 {
    let dt = WETNESS_LOOKBACK_SECS / f64::from(WETNESS_STEPS);
    let dt_hours = (dt / 3600.0) as f32;
    let mut rates = [0.0f32; WETNESS_STEPS as usize];
    for i in 0..WETNESS_STEPS {
        let t = unix_secs - WETNESS_LOOKBACK_SECS + (f64::from(i) + 1.0) * dt;
        rates[i as usize] = rain_rate(t);
    }
    integrate_wetness(&rates, dt_hours)
}

/// Wind for an instant. Storms are windier. Direction turns
/// over roughly a day; speed is a few metres per second in
/// the dry and higher in rain.
pub fn wind_at(unix_secs: f64, rain: f32) -> Vec2 {
    let ang = (unix_secs / 3600.0 * 0.23) as f32;
    let speed = 1.2 + 7.5 * rain.clamp(0.0, 1.0);
    Vec2::new(ang.cos() * speed, ang.sin() * speed)
}

/// Cloud cover in [0, 1]. A slow lobe can overcast ahead of
/// the rain; rain itself forces cover.
pub fn cloud_cover(unix_secs: f64, rain: f32) -> f32 {
    let h = unix_secs / 3600.0;
    let lobe = (h * 0.19 + 0.4).sin() as f32 * 0.5 + 0.5;
    (lobe * 0.35 + rain.clamp(0.0, 1.0) * 0.75).clamp(0.0, 1.0)
}

/// Fog density multiplier. Exactly 1 when both inputs are 0,
/// otherwise up to about 2.15 in a soaked downpour.
pub fn fog_multiplier(rain: f32, wetness: f32) -> f32 {
    let rain = rain.clamp(0.0, 1.0);
    let wetness = wetness.clamp(0.0, 1.0);
    if rain == 0.0 && wetness == 0.0 {
        return 1.0;
    }
    1.0 + 0.90 * rain + 0.25 * wetness * rain
}

/// Puddle-ripple offset in the ground plane, in metres of
/// normal tilt (not a displacement of the vertex).
///
/// Zero when `rain` is 0, bounded when it is not, and
/// deterministic in `(x, z, time_s, rain)`. The rain sheet
/// does not sample this — it is the normal offset a wet
/// surface paints with when a host wants ripples, kept here
/// so the weather reading and the ripple share one clock.
pub fn ripple_offset(x: f32, z: f32, time_s: f32, rain: f32) -> Vec2 {
    if rain <= 0.0 {
        return Vec2::ZERO;
    }
    let r = rain.clamp(0.0, 1.0);
    let p = (x * 1.7 + z * 0.4 + time_s * (2.2 + r * 3.0)).sin();
    let q = (z * 1.3 - x * 0.6 + time_s * (1.7 + r * 2.4) + 1.3).sin();
    Vec2::new(p, q) * (0.08 * r)
}

fn hash01(n: i64) -> f32 {
    let mut h = (n as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    (h >> 40) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traversal::surface::{SurfaceKind, SurfaceRecord};

    #[test]
    fn clear_is_exactly_off() {
        let a = Atmosphere::clear();
        assert_eq!(a.fog_mul.to_bits(), 1.0f32.to_bits());
        assert_eq!(a.rain, 0.0);
        assert_eq!(a.wetness, 0.0);
        assert_eq!(a.cloud, 0.0);
        assert_eq!(a.draw_count(), 0);
        assert_eq!(a.triangle_count(), 0);
        assert_eq!(a.vision_gain().to_bits(), 1.0f32.to_bits());
        assert_eq!(a.wind3(), Vec3::ZERO);
    }

    #[test]
    fn the_same_instant_reads_the_same() {
        let t = 1_768_485_600.0;
        assert_eq!(Atmosphere::at(t), Atmosphere::at(t));
        assert_eq!(rain_rate(t).to_bits(), rain_rate(t).to_bits());
        assert_eq!(wetness_at(t).to_bits(), wetness_at(t).to_bits());
    }

    #[test]
    fn rain_rate_stays_in_range() {
        for i in 0..500 {
            let r = rain_rate(1_700_000_000.0 + i as f64 * 3600.0);
            assert!((0.0..=1.0).contains(&r), "{r}");
        }
    }

    #[test]
    fn the_year_has_dry_hours_and_wet_hours() {
        let mut dry = 0u32;
        let mut wet = 0u32;
        for i in 0..24 * 40 {
            let r = rain_rate(1_700_000_000.0 + i as f64 * 3600.0);
            if r <= RAIN_DRAW_THRESHOLD {
                dry += 1;
            }
            if r > 0.25 {
                wet += 1;
            }
        }
        assert!(dry > 100, "dry hours {dry}");
        assert!(wet > 20, "wet hours {wet}");
    }

    #[test]
    fn one_hour_of_rain_fills_by_the_stated_rate() {
        let w = integrate_wetness(&[1.0], 1.0);
        assert!((w - FILL_PER_HOUR).abs() < 1.0e-5);
    }

    #[test]
    fn a_dry_spell_after_rain_drains() {
        let soaked = integrate_wetness(&[1.0; 8], 1.0);
        let mut rates = vec![1.0f32; 8];
        rates.extend(std::iter::repeat(0.0).take(8));
        let after = integrate_wetness(&rates, 1.0);
        assert!(soaked > 0.95, "{soaked}");
        assert!(after < 0.15, "{after}");
        assert!(after < soaked);
    }

    #[test]
    fn integrate_stays_bounded_and_starts_dry() {
        assert_eq!(integrate_wetness(&[], 1.0), 0.0);
        assert_eq!(integrate_wetness(&[0.0; 12], 1.0), 0.0);
        let w = integrate_wetness(&[2.0, -1.0, 1.0, 0.5], 3.0);
        assert!((0.0..=1.0).contains(&w));
    }

    #[test]
    fn wetness_moves_continuously_across_a_minute() {
        let t = 1_784_134_800.0;
        let a = wetness_at(t);
        let b = wetness_at(t + 60.0);
        assert!((0.0..=1.0).contains(&a));
        assert!((a - b).abs() < 0.02, "{a} vs {b}");
    }

    #[test]
    fn fog_is_exactly_one_when_the_air_and_ground_are_dry() {
        assert_eq!(fog_multiplier(0.0, 0.0).to_bits(), 1.0f32.to_bits());
        let heavier = fog_multiplier(0.8, 0.5);
        assert!(heavier > 1.0);
        assert!(heavier < 2.5);
    }

    #[test]
    fn draw_budget_never_exceeds_two_draws_and_three_tris() {
        for i in 0..200 {
            let a = Atmosphere::at(1_700_000_000.0 + i as f64 * 3_600.0);
            assert!(a.draw_count() <= 2);
            assert!(a.triangle_count() <= RAIN_TRIANGLES + CLOUD_TRIANGLES);
            assert_eq!(
                a.triangle_count(),
                u64::from(a.rain_active()) * RAIN_TRIANGLES
                    + u64::from(a.cloud_active()) * CLOUD_TRIANGLES
            );
        }
    }

    #[test]
    fn soaked_brick_loses_grip_and_a_puddle_is_kept() {
        let mut storm = Atmosphere::clear();
        storm.wetness = 1.0;
        let dry = SurfaceRecord::dry(SurfaceKind::Brick);
        let soaked = storm.paint_surface(dry);
        assert!(soaked.wall_grip() < dry.wall_grip() * 0.5);
        assert!(soaked.footstep_gain(1.5) < dry.footstep_gain(1.5));

        let puddle = SurfaceRecord::wet(SurfaceKind::Asphalt, 1.0);
        let mut drizzle = Atmosphere::clear();
        drizzle.wetness = 0.2;
        let kept = drizzle.paint_surface(puddle);
        assert!((kept.wetness - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn vision_gain_falls_in_rain_and_stays_above_the_floor() {
        let mut a = Atmosphere::clear();
        assert_eq!(a.vision_gain(), 1.0);
        a.rain = 0.5;
        let mid = a.vision_gain();
        a.rain = 1.0;
        let full = a.vision_gain();
        assert!(mid < 1.0 && mid > full);
        assert!((full - 0.70).abs() < 1.0e-5);
    }

    #[test]
    fn scent_takes_the_rain_and_a_horizontal_wind() {
        let a = Atmosphere::at(1_792_452_600.0);
        let mut field = ScentField::new();
        field.deposit(Vec3::new(1.0, 0.0, 2.0), 1.0);
        a.apply_scent(&mut field);
        assert_eq!(field.rain, a.rain);
        assert_eq!(field.wind.y, 0.0);
        assert_eq!(field.wind.x, a.wind.x);
        assert_eq!(field.wind.z, a.wind.y);
    }

    #[test]
    fn rain_washes_a_trail_faster_than_dry_air() {
        let mut wet = ScentField::new();
        let mut dry = ScentField::new();
        wet.deposit(Vec3::ZERO, 1.0);
        dry.deposit(Vec3::ZERO, 1.0);
        let mut storm = Atmosphere::clear();
        storm.rain = 1.0;
        storm.apply_scent(&mut wet);
        // A few seconds at the fixed step. Dry air only half-life
        // decays; rain adds the wash term.
        for _ in 0..(240 * 30) {
            wet.tick();
            dry.tick();
        }
        let wet_left = wet.segments.first().map(|s| s.strength).unwrap_or(0.0);
        let dry_left = dry.segments.first().map(|s| s.strength).unwrap_or(0.0);
        assert!(
            dry_left > 0.05,
            "dry trail should still be there, {dry_left}"
        );
        assert!(
            wet_left < dry_left * 0.5,
            "rain should have washed it harder ({wet_left} vs {dry_left})"
        );
    }

    #[test]
    fn ripple_is_zero_when_dry_and_bounded_when_wet() {
        assert_eq!(ripple_offset(1.0, 2.0, 3.0, 0.0), Vec2::ZERO);
        let a = ripple_offset(4.0, -2.0, 10.0, 1.0);
        let b = ripple_offset(4.0, -2.0, 10.0, 1.0);
        assert_eq!(a, b);
        assert!(a.length() <= 0.08 * 2.0f32.sqrt() + 1.0e-5);
        let later = ripple_offset(4.0, -2.0, 10.5, 1.0);
        assert_ne!(a, later);
    }

    #[test]
    fn wind_is_stronger_in_rain_and_stays_horizontal() {
        let calm = wind_at(1_700_000_000.0, 0.0).length();
        let gale = wind_at(1_700_000_000.0, 1.0).length();
        assert!(gale > calm + 5.0);
        let a = Atmosphere::at(1_776_400_200.0);
        assert_eq!(a.wind3().y, 0.0);
    }
}
