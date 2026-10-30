//! The reality layer — Comigen's founding doctrine, as code.
//!
//! The world a Comigen game shows is powered by the real world: the clock
//! and the observer's coarse location go in, and a real solar model — not
//! a lookup, not a slider — decides where the sun is, how long the day is,
//! and what season the land is wearing. If it is fall where you are, the
//! foliage ambers. If it is 9 PM, the sun is down. The engine *reflects*
//! reality; it does not offer controls for it (the shipped hosts carry no
//! time or season controls at all — see the hosts' docs).
//!
//! Input: a UTC instant (Unix seconds) and an [`Observer`] (latitude in
//! degrees — the sign carries the hemisphere — plus longitude *or* a UTC
//! offset for local solar time). Output: a [`WorldState`] carrying every
//! quantity the renderer consumes, plus the audit numbers the cost ledger
//! prints, so a run's claim can be checked against any almanac.
//!
//! The solar math is the standard NOAA approximation: fractional year →
//! declination + equation of time → true solar time → hour angle →
//! elevation/azimuth by spherical geometry. Day length is not stored or
//! tabled anywhere — it falls out of the sunrise hour angle.
//!
//! The moon is the same clock. Geocentric ecliptic place is a truncated
//! Meeus ch. 47 series (the periodic terms that move the disk by more
//! than a hundredth of a degree, plus the chapter's additive terms);
//! the illuminated fraction is Meeus ch. 48's low-accuracy formula.
//! Both are pure functions of the UTC instant — no target branch, no
//! table beyond the published coefficients — so native and web evaluate
//! the same disk. The fraction scales the sky wash, the sea, and the
//! key light the existing shaders already read. Nothing here is a pass.
//!
//! Season factors are smooth functions of day-of-year, hemisphere-flipped,
//! anchored where Simulacrum's seasons proved out: cold peaks mid-January,
//! autumn peaks ~Oct 19, spring ~Apr 16 (northern hemisphere). They agree
//! with the solar geometry by construction — both are keyed to the same
//! day-of-year — and the anchors are the sanity references, printed by the
//! `world_probe` example.

use glam::Vec3;

/// Where the observer stands. Only coarse latitude is required; longitude
/// (preferred) or a UTC offset supplies local solar time. Location is
/// consumed locally by the engine and never transmitted anywhere.
#[derive(Copy, Clone, Debug)]
pub struct Observer {
    /// Degrees, +north / −south. `None` = location unknown: the engine
    /// runs at the equator fallback and says so (see `latitude_known`).
    pub latitude_deg: Option<f32>,
    /// Degrees, +east / −west. Preferred source of local solar time.
    pub longitude_deg: Option<f32>,
    /// Minutes east of UTC. Used only when longitude is absent.
    pub utc_offset_minutes: Option<f32>,
}

impl Observer {
    pub fn new(latitude_deg: f32, longitude_deg: f32) -> Observer {
        Observer {
            latitude_deg: Some(latitude_deg),
            longitude_deg: Some(longitude_deg),
            utc_offset_minutes: None,
        }
    }

    pub fn unknown() -> Observer {
        Observer {
            latitude_deg: None,
            longitude_deg: None,
            utc_offset_minutes: None,
        }
    }
}

/// Everything the renderer needs for one instant at one place, plus the
/// audit fields the ledger prints.
#[derive(Copy, Clone, Debug)]
pub struct WorldState {
    pub unix_seconds: f64,
    /// Fractional day-of-year in the observer's local solar reckoning.
    pub day_of_year: f32,
    pub latitude_deg: f32,
    pub latitude_known: bool,

    /// Unit vector toward the sun, world space (north = −Z, east = +X).
    pub sun_dir: Vec3,
    pub sun_elevation_deg: f32,
    pub sun_azimuth_deg: f32,
    /// HDR sun colour including intensity; black when the sun is down.
    pub sun_color: Vec3,
    /// Sky glow colour around the sun (twilight ember included), and the
    /// sun disc strength (0 once the disc has fully set).
    pub sun_glow: Vec3,
    pub sun_disc: f32,
    /// Unit vector toward the moon (geocentric, Meeus ch. 47). The key
    /// light is black when the sun is up, the moon is below the horizon,
    /// or the disk is dark — phase is [`WorldState::moon_illuminated`].
    pub moon_dir: Vec3,
    pub moon_elevation_deg: f32,
    pub moon_azimuth_deg: f32,
    /// Geocentric illuminated fraction of the lunar disk, 0 = new, 1 = full.
    /// Meeus ch. 48. Sky wash, sea glitter, and the key light all scale by it.
    pub moon_illuminated: f32,
    pub moon_color: Vec3,
    /// Moon disc surface strength for the sky shader (0 in daylight and
    /// when the moon has set). The terminator itself is this fraction,
    /// applied in the sky shader — a new moon draws no disk.
    pub moon_disc: f32,

    pub ambient_sky: Vec3,
    pub ambient_ground: Vec3,
    pub sky_zenith: Vec3,
    pub sky_mid: Vec3,
    pub sky_horizon: Vec3,
    pub fog_color: Vec3,
    pub fog_density: f32,
    /// 0 in daylight, 1 in full night — drives the star field.
    pub star_intensity: f32,
    /// Broad 0..1 "how daylit is it" factor (twilight partial).
    pub daylight: f32,

    // Season factors, 0..1, hemisphere-aware.
    pub cold: f32,
    pub autumn: f32,
    pub spring: f32,
    pub summer: f32,
    /// World-space height where snow takes hold (before slope/detail).
    pub snow_line: f32,
    /// 0 = open water, 1 = hard frozen sheen.
    pub water_ice: f32,
    /// Multiplier on the water's wave motion (ice and cold calm it).
    pub water_wave_scale: f32,

    /// Hours of daylight this date at this latitude (geometry-derived).
    pub day_length_hours: f32,
}

// ---- civil calendar (Howard Hinnant's algorithms; pure integer math,
// deterministic on every target) ----

/// Days since 1970-01-01 for a civil date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y_adj = if m <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = y_adj - era * 400; // [0, 399]
    let mp = ((m + 9) % 12) as i64; // [0, 11], Mar = 0
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// (year, month, day) for days since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Day-of-year (1-based, fractional part zero) for a civil date.
fn day_of_year(y: i64, m: u32, d: u32) -> i64 {
    days_from_civil(y, m, d) - days_from_civil(y, 1, 1) + 1
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp01((x - e0) / (e1 - e0));
    t * t * (3.0 - 2.0 * t)
}

fn mix3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

/// Circular distance in days between two fractional days-of-year.
fn day_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).abs() % 365.25;
    if d > 365.25 / 2.0 {
        365.25 - d
    } else {
        d
    }
}

fn gauss_day(f: f32, peak: f32, sigma: f32) -> f32 {
    let d = day_distance(f, peak);
    (-(d * d) / (sigma * sigma)).exp()
}

// ---- moon (Meeus, Astronomical Algorithms, ch. 47 and ch. 48) ----
//
// Low precision on purpose: the disk in the sky is a fraction of a
// degree wide, and the illuminated-fraction test is one percentage
// point. Ch. 48's short formula is the one Meeus rates to the second
// decimal of k. Ch. 47 supplies the place. Geocentric — horizontal
// parallax is under a degree and is not applied. Nutation is omitted.
// Every angle is reduced into a single turn before it reaches sin, so
// the large mean-motion terms do not depend on libm range reduction.

fn julian_centuries(unix_seconds: f64) -> f64 {
    let jd = unix_seconds / 86400.0 + 2_440_587.5;
    (jd - 2_451_545.0) / 36_525.0
}

fn wrap_deg(x: f64) -> f64 {
    x.rem_euclid(360.0)
}

fn sin_deg(d: f64) -> f64 {
    wrap_deg(d).to_radians().sin()
}

fn cos_deg(d: f64) -> f64 {
    wrap_deg(d).to_radians().cos()
}

fn finite_f32(x: f64) -> f32 {
    let y = x as f32;
    if y.is_finite() {
        y
    } else {
        0.0
    }
}

/// Moon's mean longitude L′ (degrees), Meeus ch. 47.
fn moon_mean_longitude(t: f64) -> f64 {
    218.316_447_7
        + (481_267.881_234_21 + (-0.001_578_6 + (1.0 / 538_841.0 - t / 65_194_000.0) * t) * t) * t
}

/// Mean elongation D (degrees).
fn moon_mean_elongation(t: f64) -> f64 {
    297.850_192_1
        + (445_267.111_403_4 + (-0.001_881_9 + (1.0 / 545_868.0 - t / 113_065_000.0) * t) * t) * t
}

/// Sun's mean anomaly M (degrees).
fn sun_mean_anomaly(t: f64) -> f64 {
    357.529_109_2 + (35_999.050_290_9 + (-0.000_153_6 + t / 24_490_000.0) * t) * t
}

/// Moon's mean anomaly M′ (degrees).
fn moon_mean_anomaly(t: f64) -> f64 {
    134.963_396_4
        + (477_198.867_505_5 + (0.008_741_4 + (1.0 / 69_699.0 - t / 14_712_000.0) * t) * t) * t
}

/// Moon's argument of latitude F (degrees).
fn moon_arg_latitude(t: f64) -> f64 {
    93.272_095
        + (483_202.017_523_3 + (-0.003_653_9 + (-1.0 / 3_526_000.0 + t / 863_310_000.0) * t) * t)
            * t
}

/// Illuminated fraction of the lunar disk, Meeus ch. 48 (low accuracy).
///
/// Geocentric and independent of the observer. `0` is new, `1` is full.
/// The result is clamped to `[0, 1]`; the formula already lands there
/// for any finite instant.
pub fn moon_illuminated_fraction(unix_seconds: f64) -> f64 {
    let t = julian_centuries(unix_seconds);
    let d = wrap_deg(moon_mean_elongation(t));
    let m = wrap_deg(sun_mean_anomaly(t));
    let mp = wrap_deg(moon_mean_anomaly(t));
    // Phase angle, degrees. Coefficients are Meeus's.
    let i = 180.0 - d - 6.289 * sin_deg(mp) + 2.100 * sin_deg(m)
        - 1.274 * sin_deg(2.0 * d - mp)
        - 0.658 * sin_deg(2.0 * d)
        - 0.214 * sin_deg(2.0 * mp)
        - 0.110 * sin_deg(d);
    let k = (1.0 + cos_deg(i)) * 0.5;
    if k.is_finite() {
        k.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// (D, M, M′, F, Σl, Σb). Σl and Σb are in units of 10⁻⁶ degree, the
/// unit of Meeus tables 47.A and 47.B. Rows are the terms at or above
/// 0.01° in longitude or 0.017° in latitude — what is left moves the
/// disk by less than its own low-precision budget. A zero coefficient
/// means that row does not contribute to that coordinate.
const LUNAR_TERMS: [(i8, i8, i8, i8, f64, f64); 26] = [
    // longitude, table 47.A, |Σl| ≥ 10_000
    (0, 0, 1, 0, 6_288_774.0, 0.0),
    (2, 0, -1, 0, 1_274_027.0, 0.0),
    (2, 0, 0, 0, 658_314.0, 0.0),
    (0, 0, 2, 0, 213_618.0, 0.0),
    (0, 1, 0, 0, -185_116.0, 0.0),
    (0, 0, 0, 2, -114_332.0, 0.0),
    (2, 0, -2, 0, 58_793.0, 0.0),
    (2, -1, -1, 0, 57_066.0, 0.0),
    (2, 0, 1, 0, 53_322.0, 0.0),
    (2, -1, 0, 0, 45_758.0, 0.0),
    (0, 1, -1, 0, -40_923.0, 0.0),
    (1, 0, 0, 0, -34_720.0, 0.0),
    (0, 1, 1, 0, -30_383.0, 0.0),
    (2, 0, 0, -2, 15_327.0, 0.0),
    (0, 0, 1, 2, -12_528.0, 0.0),
    (0, 0, 1, -2, 10_980.0, 0.0),
    (4, 0, -1, 0, 10_675.0, 0.0),
    (0, 0, 3, 0, 10_034.0, 0.0),
    // latitude, table 47.B, |Σb| ≥ 17_000
    (0, 0, 0, 1, 0.0, 5_128_122.0),
    (0, 0, 1, 1, 0.0, 280_602.0),
    (0, 0, 1, -1, 0.0, 277_693.0),
    (2, 0, 0, -1, 0.0, 173_237.0),
    (2, 0, -1, 1, 0.0, 55_413.0),
    (2, 0, -1, -1, 0.0, 46_271.0),
    (2, 0, 0, 1, 0.0, 32_573.0),
    (0, 0, 2, 1, 0.0, 17_198.0),
];

fn eccentricity_scale(sun_anomaly_factor: i8, e: f64) -> f64 {
    match sun_anomaly_factor {
        1 | -1 => e,
        2 | -2 => e * e,
        _ => 1.0,
    }
}

/// Geocentric ecliptic longitude and latitude, degrees, of date.
fn lunar_ecliptic(t: f64) -> (f64, f64) {
    let lp = wrap_deg(moon_mean_longitude(t));
    let d = wrap_deg(moon_mean_elongation(t));
    let m = wrap_deg(sun_mean_anomaly(t));
    let mp = wrap_deg(moon_mean_anomaly(t));
    let f = wrap_deg(moon_arg_latitude(t));
    let e = 1.0 + (-0.002_516 - 0.000_007_4 * t) * t;

    let mut sigma_l = 0.0;
    let mut sigma_b = 0.0;
    for (d_n, m_n, mp_n, f_n, lon_u, lat_u) in LUNAR_TERMS {
        let arg =
            d * f64::from(d_n) + m * f64::from(m_n) + mp * f64::from(mp_n) + f * f64::from(f_n);
        let scale = eccentricity_scale(m_n, e);
        let s = sin_deg(arg);
        sigma_l += lon_u * scale * s;
        sigma_b += lat_u * scale * s;
    }

    // Additive terms, Meeus ch. 47 (still in 10⁻⁶ degree).
    let a1 = wrap_deg(119.75 + 131.849 * t);
    let a2 = wrap_deg(53.09 + 479_264.290 * t);
    let a3 = wrap_deg(313.45 + 481_266.484 * t);
    sigma_l += 3_958.0 * sin_deg(a1) + 1_962.0 * sin_deg(lp - f) + 318.0 * sin_deg(a2);
    sigma_b += -2_235.0 * sin_deg(lp)
        + 382.0 * sin_deg(a3)
        + 175.0 * sin_deg(a1 - f)
        + 175.0 * sin_deg(a1 + f)
        + 127.0 * sin_deg(lp - mp)
        - 115.0 * sin_deg(lp + mp);

    (wrap_deg(lp + sigma_l / 1_000_000.0), sigma_b / 1_000_000.0)
}

/// Mean obliquity of the ecliptic, degrees (Meeus ch. 22). Nutation omitted.
fn mean_obliquity_deg(t: f64) -> f64 {
    23.439_291_1 + (-0.013_004_167 + (-0.000_000_163_9 + 0.000_000_503_6 * t) * t) * t
}

/// Greenwich mean sidereal time, degrees (Meeus ch. 12).
fn gmst_deg(unix_seconds: f64) -> f64 {
    let jd = unix_seconds / 86400.0 + 2_440_587.5;
    let d = jd - 2_451_545.0;
    let t = d / 36_525.0;
    let theta =
        280.460_618_37 + 360.985_647_366_29 * d + 0.000_387_933 * t * t - t * t * t / 38_710_000.0;
    wrap_deg(theta)
}

/// Equatorial coordinates (right ascension, declination), degrees.
fn ecliptic_to_equatorial(lon_deg: f64, lat_deg: f64, t: f64) -> (f64, f64) {
    let eps = mean_obliquity_deg(t);
    let sb = sin_deg(lat_deg);
    let cb = cos_deg(lat_deg);
    let sl = sin_deg(lon_deg);
    let cl = cos_deg(lon_deg);
    let se = sin_deg(eps);
    let ce = cos_deg(eps);
    // cos(latitude) is ~1 for the moon (β stays inside ±6°). The floor
    // keeps a pathological instant from dividing by zero.
    let cb_safe = if cb.abs() < 1.0e-8 {
        1.0e-8_f64.copysign(cb)
    } else {
        cb
    };
    let dec = (sb * ce + cb * se * sl)
        .clamp(-1.0, 1.0)
        .asin()
        .to_degrees();
    let ra = (sl * ce - (sb / cb_safe) * se).atan2(cl).to_degrees();
    (wrap_deg(ra), dec)
}

/// Longitude the moon's hour angle is reckoned from. Longitude wins;
/// a UTC offset stands in as its standard meridian (15° per hour);
/// with neither, the observer is taken to sit on the Greenwich meridian
/// — the same fallback the solar model uses when it has no longitude.
fn observer_longitude_deg(observer: &Observer) -> f64 {
    if let Some(lon) = observer.longitude_deg {
        f64::from(lon)
    } else if let Some(off) = observer.utc_offset_minutes {
        f64::from(off) / 4.0
    } else {
        0.0
    }
}

struct MoonPlace {
    dir: Vec3,
    elevation_deg: f32,
    azimuth_deg: f32,
    illuminated: f32,
}

/// Horizontal place of the moon in the engine's world frame
/// (east = +X, up = +Y, north = −Z), plus the illuminated fraction.
fn moon_place(unix_seconds: f64, latitude_deg: f32, observer: &Observer) -> MoonPlace {
    let t = julian_centuries(unix_seconds);
    let (lon, lat) = lunar_ecliptic(t);
    let (ra, dec) = ecliptic_to_equatorial(lon, lat, t);
    let lst = wrap_deg(gmst_deg(unix_seconds) + observer_longitude_deg(observer));
    let mut ha_deg = wrap_deg(lst - ra);
    if ha_deg > 180.0 {
        ha_deg -= 360.0;
    }
    let ha = ha_deg.to_radians();
    let dec_r = dec.to_radians();
    let phi = f64::from(latitude_deg).to_radians();
    // Same horizon basis as the sun, so a shared shader reads both.
    let east = -dec_r.cos() * ha.sin();
    let north = dec_r.sin() * phi.cos() - dec_r.cos() * phi.sin() * ha.cos();
    let up = dec_r.sin() * phi.sin() + dec_r.cos() * phi.cos() * ha.cos();
    let dir = {
        let v = Vec3::new(finite_f32(east), finite_f32(up), finite_f32(-north));
        let l2 = v.length_squared();
        if l2 > 1.0e-12 {
            v * l2.sqrt().recip()
        } else {
            Vec3::Y
        }
    };
    let elev = finite_f32(up.clamp(-1.0, 1.0).asin().to_degrees());
    let azim = finite_f32(east.atan2(north).to_degrees().rem_euclid(360.0));
    let k = finite_f32(moon_illuminated_fraction(unix_seconds));
    MoonPlace {
        dir,
        elevation_deg: elev,
        azimuth_deg: azim,
        illuminated: k.clamp(0.0, 1.0),
    }
}

/// Compute the full world state for one instant and observer.
pub fn world_state(unix_seconds: f64, observer: &Observer) -> WorldState {
    let (lat_deg, lat_known) = match observer.latitude_deg {
        Some(v) => (v.clamp(-89.0, 89.0), true),
        None => (0.0, false), // stated fallback: the equator
    };

    // Local reckoning for the date and the clock: longitude is the honest
    // source (solar time is longitude); a UTC offset stands in as "the
    // observer sits on their standard meridian"; otherwise UTC itself.
    let local_shift_min = if let Some(lon) = observer.longitude_deg {
        lon as f64 * 4.0
    } else if let Some(off) = observer.utc_offset_minutes {
        off as f64
    } else {
        0.0
    };
    let local_seconds = unix_seconds + local_shift_min * 60.0;
    let local_days = (local_seconds / 86400.0).floor() as i64;
    let (year, month, day) = civil_from_days(local_days);
    let doy = day_of_year(year, month, day) as f32;
    let utc_hour = ((unix_seconds.rem_euclid(86400.0)) / 3600.0) as f32;
    let doy_frac = doy + ((local_seconds.rem_euclid(86400.0)) / 86400.0) as f32 - 1.0;

    // ---- NOAA solar model ----
    // Fractional year (radians), keyed to the UTC date + hour.
    let gamma = 2.0 * std::f32::consts::PI / 365.0 * (doy - 1.0 + (utc_hour - 12.0) / 24.0);
    // Solar declination (radians).
    let decl = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin()
        - 0.006758 * (2.0 * gamma).cos()
        + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos()
        + 0.00148 * (3.0 * gamma).sin();
    // Equation of time (minutes).
    let eqtime = 229.18
        * (0.000075 + 0.001868 * gamma.cos()
            - 0.032077 * gamma.sin()
            - 0.014615 * (2.0 * gamma).cos()
            - 0.040849 * (2.0 * gamma).sin());

    // True solar time (minutes). With a longitude: UTC clock + 4 min/deg +
    // equation of time. With only an offset: the local clock + eqtime (the
    // observer is taken to sit on their standard meridian).
    let utc_min = unix_seconds.rem_euclid(86400.0) / 60.0;
    let tst_min = if let Some(lon) = observer.longitude_deg {
        utc_min + eqtime as f64 + 4.0 * lon as f64
    } else if let Some(off) = observer.utc_offset_minutes {
        utc_min + off as f64 + eqtime as f64
    } else {
        utc_min + eqtime as f64
    };
    // Hour angle: 0 at solar noon, +15°/hour after, wrapped to (−180, 180].
    let mut ha_deg = (tst_min / 4.0 - 180.0).rem_euclid(360.0);
    if ha_deg > 180.0 {
        ha_deg -= 360.0;
    }
    let ha = (ha_deg as f32).to_radians();
    let phi = lat_deg.to_radians();

    // Sun unit vector in the local horizon frame, from spherical geometry.
    let east = -decl.cos() * ha.sin();
    let north = decl.sin() * phi.cos() - decl.cos() * phi.sin() * ha.cos();
    let up = decl.sin() * phi.sin() + decl.cos() * phi.cos() * ha.cos();
    let sun_elev = up.clamp(-1.0, 1.0).asin().to_degrees();
    let sun_azim = east.atan2(north).to_degrees().rem_euclid(360.0);
    // World frame: north = −Z, east = +X, up = +Y.
    let sun_dir = Vec3::new(east, up, -north).normalize();

    // Day length from the sunrise hour angle — geometry, not a table.
    let cos_h0 = -phi.tan() * decl.tan();
    let day_length = if cos_h0 < -1.0 {
        24.0 // polar day
    } else if cos_h0 > 1.0 {
        0.0 // polar night
    } else {
        2.0 * cos_h0.acos().to_degrees() / 15.0
    };

    // ---- seasons (hemisphere-flipped day-of-year; Simulacrum anchors) ----
    let season_day = if lat_deg < 0.0 {
        (doy_frac + 182.6).rem_euclid(365.25)
    } else {
        doy_frac
    };
    // Cold peaks Jan 15, bottoms out mid-July.
    let cold_wave = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * (season_day - 15.0) / 365.25).cos();
    let cold = cold_wave.powf(1.35);
    let autumn = gauss_day(season_day, 292.0, 26.0); // peak ~Oct 19
    let spring = gauss_day(season_day, 106.0, 24.0); // peak ~Apr 16
    let summer_wave =
        0.5 + 0.5 * (2.0 * std::f32::consts::PI * (season_day - 201.0) / 365.25).cos();
    let summer = summer_wave.powf(1.2);

    // ---- light & palette derivation ----
    let el = sun_elev;
    let daylight = smoothstep(-2.0, 3.0, el);
    let dusk_glow = (1.0 - smoothstep(0.0, 9.0, el)) * smoothstep(-7.0, -0.5, el); // twilight ember

    // Sun: colour warms to ember at the horizon, whitens as it climbs;
    // intensity shaped by elevation. Zero at and below the horizon.
    let warm_t = smoothstep(0.0, 35.0, el);
    let sun_tint = mix3(
        Vec3::new(1.0, 0.36, 0.12),
        Vec3::new(1.0, 0.93, 0.84),
        warm_t,
    );
    let sun_intensity = if el > 0.0 {
        3.3 * el.to_radians().sin().clamp(0.0, 1.0).powf(0.55)
    } else {
        0.0
    };
    let sun_color = sun_tint * sun_intensity;
    // The glow outlives the disc: at dusk the sun is gone but its ember
    // still lights the horizon (and the fog, and the water's reflection).
    let sun_glow = sun_tint * (sun_intensity * 0.9 + dusk_glow * 1.3);
    let sun_disc = 7.0 * smoothstep(-1.5, 0.5, el);

    // Sky gradient keyframes by elevation, blended — day / golden / ember /
    // night. (M1's golden hour is the el≈9.5° keyframe family.)
    let (day_zen, day_mid, day_hor) = (
        Vec3::new(0.10, 0.28, 0.58),
        Vec3::new(0.38, 0.53, 0.76),
        Vec3::new(0.72, 0.76, 0.80),
    );
    let (gold_zen, gold_mid, gold_hor) = (
        Vec3::new(0.075, 0.205, 0.45),
        Vec3::new(0.35, 0.46, 0.66),
        Vec3::new(0.99, 0.60, 0.30),
    );
    let (ember_zen, ember_mid, ember_hor) = (
        Vec3::new(0.030, 0.055, 0.150),
        Vec3::new(0.150, 0.150, 0.290),
        Vec3::new(0.930, 0.360, 0.130),
    );
    let (night_zen, night_mid, night_hor) = (
        Vec3::new(0.004, 0.008, 0.020),
        Vec3::new(0.008, 0.014, 0.033),
        Vec3::new(0.020, 0.030, 0.052),
    );
    let to_day = smoothstep(18.0, 45.0, el);
    let to_gold = smoothstep(2.0, 10.0, el);
    let to_ember = smoothstep(-6.5, -0.5, el);
    let mut zen = mix3(night_zen, ember_zen, to_ember);
    let mut mid = mix3(night_mid, ember_mid, to_ember);
    let mut hor = mix3(night_hor, ember_hor, to_ember);
    zen = mix3(zen, gold_zen, to_gold);
    mid = mix3(mid, gold_mid, to_gold);
    hor = mix3(hor, gold_hor, to_gold);
    zen = mix3(zen, day_zen, to_day);
    mid = mix3(mid, day_mid, to_day);
    hor = mix3(hor, day_hor, to_day);
    // Cold seasons pale the whole sky a touch.
    let pale = Vec3::new(0.92, 0.97, 1.06);
    zen *= 1.0 + (pale.x - 1.0) * cold * daylight;
    mid *= 1.0 + (pale.y - 1.0) * cold * daylight;
    hor *= 1.0 + (pale.z - 1.0) * cold * daylight;

    // Moon, from the same instant. The night gate is the old one —
    // zero for every sun elevation ≥ −1° — so a daylight frame's moon
    // term stays exactly black. Above the horizon, and in proportion
    // to the illuminated fraction, the moon washes the sky (and
    // therefore the sea, which reflects that sky) and supplies the
    // key light.
    let moon = moon_place(unix_seconds, lat_deg, observer);
    let night = smoothstep(-1.0, -6.0, el);
    let above = smoothstep(-0.8, 3.0, moon.elevation_deg);
    let lamp = night * above * moon.illuminated;
    let wash = Vec3::new(0.050, 0.068, 0.120) * lamp;
    zen += wash * 0.50;
    mid += wash * 0.80;
    hor += wash * 0.35;

    // Hemisphere ambient: strong in day, ember-lit at dusk, and at
    // night only as bright as the moon's illuminated fraction allows.
    let amb_day_sky = Vec3::new(0.30, 0.375, 0.52) * (0.45 + 0.55 * daylight);
    let amb_sky = mix3(Vec3::new(0.020, 0.028, 0.052), amb_day_sky, daylight)
        + Vec3::new(0.10, 0.045, 0.015) * dusk_glow
        + Vec3::new(0.028, 0.038, 0.070) * lamp;
    let amb_ground = mix3(
        Vec3::new(0.010, 0.010, 0.014),
        Vec3::new(0.145, 0.115, 0.095) * (0.4 + 0.6 * daylight),
        daylight,
    ) + Vec3::new(0.012, 0.014, 0.020) * lamp;

    // Fog: the horizon colour, away from the sun; a touch denser in cold
    // haze, a touch thinner at night. The moon wash is already in `hor`.
    let fog_color = hor * 0.96 + Vec3::new(0.02, 0.015, 0.01);
    let fog_density = 0.00135 * (1.0 + 0.25 * cold) * (1.0 - 0.25 * (1.0 - daylight));

    // Key light matches the pre-ephemeris full-moon night when the disk
    // is full and up (0.34 × the pale blue). A new moon, or a moon that
    // has set, contributes nothing. Disc *surface* brightness is not
    // scaled by the fraction — the sky shader's terminator does that,
    // so a quarter shows a half disk at the full disk's surface
    // brightness while the landscape and the sea see half the light.
    let moon_color = Vec3::new(0.42, 0.52, 0.74) * (0.34 * lamp);
    let moon_disc = 1.5 * night * above;

    let star_intensity = smoothstep(-8.0, -16.0, el);

    // ---- land & water season state ----
    // Snow line: rides above the peaks in the warm half of the year and
    // descends only as cold DEEPENS (a cool October day is not snow
    // weather; a deep-January freeze puts snow on the beach).
    let snow_line = 34.0 - 37.0 * smoothstep(0.45, 1.0, cold);
    // Ice: only deep cold freezes the sea.
    let water_ice = smoothstep(0.58, 0.88, cold);
    let water_wave_scale = (1.0 - 0.72 * water_ice) * (1.0 - 0.25 * cold);

    WorldState {
        unix_seconds,
        day_of_year: doy_frac,
        latitude_deg: lat_deg,
        latitude_known: lat_known,
        sun_dir,
        sun_elevation_deg: el,
        sun_azimuth_deg: sun_azim,
        sun_color,
        sun_glow,
        sun_disc,
        moon_dir: moon.dir,
        moon_elevation_deg: moon.elevation_deg,
        moon_azimuth_deg: moon.azimuth_deg,
        moon_illuminated: moon.illuminated,
        moon_color,
        moon_disc,
        ambient_sky: amb_sky,
        ambient_ground: amb_ground,
        sky_zenith: zen,
        sky_mid: mid,
        sky_horizon: hor,
        fog_color,
        fog_density,
        star_intensity,
        daylight,
        cold,
        autumn,
        spring,
        summer,
        snow_line,
        water_ice,
        water_wave_scale,
        day_length_hours: day_length,
    }
}

/// Format an instant as an ISO-8601 UTC string (for ledger audit lines).
pub fn iso_utc(unix_seconds: f64) -> String {
    let days = (unix_seconds / 86400.0).floor() as i64;
    let secs = unix_seconds.rem_euclid(86400.0) as i64;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unix seconds for a UTC civil instant, through the engine's calendar.
    fn unix_utc(y: i64, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> f64 {
        let days = days_from_civil(y, m, d) as f64;
        days * 86400.0 + f64::from(hh) * 3600.0 + f64::from(mm) * 60.0 + f64::from(ss)
    }

    /// One percentage point of the disk. USNO publishes k to two
    /// decimals; this is the bound that fails the pin.
    const USNO_ABS: f64 = 0.01;

    fn pin_fraction(unix: f64, usno: f64, label: &str) {
        let k = moon_illuminated_fraction(unix);
        let err = (k - usno).abs();
        assert!(
            err <= USNO_ABS,
            "{label}: computed {k:.4}, USNO {usno:.2}, |Δ| {err:.4} exceeds {USNO_ABS}"
        );
        // The renderer stores the same fraction, not a second model.
        let w = world_state(unix, &Observer::new(41.88, -87.63));
        let stored = f64::from(w.moon_illuminated);
        assert!(
            (stored - k).abs() < 1.0e-5,
            "{label}: WorldState fraction {stored} drifted from {k}"
        );
    }

    // USNO, "Fraction of the Moon Illuminated", geocentric, at midnight
    // 5.0 hours west of Greenwich — 05:00 UTC. Table for 2026:
    // https://aa.usno.navy.mil/calculated/moon/fraction?submit=Get+Data&task=00&tz=5&tz_label=false&tz_sign=-1&year=2026
    //
    // The phase each date belongs to is USNO, "Dates of Primary Phases
    // of the Moon" (Universal Time):
    // https://aa.usno.navy.mil/calculated/moon/phases?date=2025-08-19&format=p&nump=50&submit=Get+Data
    //   New Moon       2026 Jan 18  19:52
    //   First Quarter  2026 Jan 26  04:47
    //   Full Moon      2026 Jan  3  10:03   (northern winter)
    //   Full Moon      2026 Jul 29  14:36
    // The pinned number is the published daily fraction at 05:00 UTC,
    // not a phase instant rounded by hand.

    #[test]
    fn usno_new_moon_2026_01_18() {
        let u = unix_utc(2026, 1, 18, 5, 0, 0);
        assert_eq!(iso_utc(u), "2026-01-18T05:00:00Z");
        // Published 0.00. Still falling into the new moon at 19:52 UT
        // (Jan 17 is 0.03, Jan 19 is 0.00).
        pin_fraction(u, 0.00, "new moon 2026-01-18");
    }

    #[test]
    fn usno_first_quarter_2026_01_26() {
        let u = unix_utc(2026, 1, 26, 5, 0, 0);
        assert_eq!(iso_utc(u), "2026-01-26T05:00:00Z");
        // Published 0.50, and the fraction is increasing through the
        // day (Jan 25 is 0.39, Jan 27 is 0.62): first quarter, thirteen
        // minutes after the 04:47 UT phase.
        pin_fraction(u, 0.50, "first quarter 2026-01-26");
    }

    #[test]
    fn usno_full_moon_2026_07_29() {
        let u = unix_utc(2026, 7, 29, 5, 0, 0);
        assert_eq!(iso_utc(u), "2026-07-29T05:00:00Z");
        // Published 1.00. Full moon the same day at 14:36 UT.
        pin_fraction(u, 1.00, "full moon 2026-07-29");
    }

    #[test]
    fn usno_winter_full_moon_2026_01_03() {
        let u = unix_utc(2026, 1, 3, 5, 0, 0);
        assert_eq!(iso_utc(u), "2026-01-03T05:00:00Z");
        // Published 1.00. Full moon the same day at 10:03 UT, in the
        // engine's deep-winter cold (the cold factor peaks Jan 15).
        pin_fraction(u, 1.00, "winter full moon 2026-01-03");
    }

    #[test]
    fn fraction_is_deterministic_and_geocentric() {
        let u = unix_utc(2026, 1, 26, 5, 0, 0);
        let a = moon_illuminated_fraction(u);
        let b = moon_illuminated_fraction(u);
        assert_eq!(a.to_bits(), b.to_bits());
        let chicago = world_state(u, &Observer::new(41.88, -87.63));
        let greenwich = world_state(u, &Observer::unknown());
        assert_eq!(
            chicago.moon_illuminated.to_bits(),
            greenwich.moon_illuminated.to_bits()
        );
        // Place follows the observer. The unknown-location fallback is
        // the Greenwich meridian, not Chicago.
        let same_place = chicago.moon_elevation_deg == greenwich.moon_elevation_deg
            && chicago.moon_azimuth_deg == greenwich.moon_azimuth_deg;
        assert!(!same_place, "observer did not move the moon");
    }

    #[test]
    fn daylight_pin_moon_term_is_black() {
        // M2 winter-morning proof pin: 2026-01-15 08:00 CST = 14:00 UTC.
        // The sun is up, so the moon term is exactly black — the same
        // gate as the mirrored moon — and a daylight frame does not
        // move because of this change.
        let w = world_state(
            unix_utc(2026, 1, 15, 14, 0, 0),
            &Observer::new(41.88, -87.63),
        );
        assert!(w.sun_elevation_deg > 1.0, "sun {}", w.sun_elevation_deg);
        assert_eq!(w.moon_color, Vec3::ZERO);
        assert_eq!(w.moon_disc, 0.0);
        assert!(w.moon_dir.is_finite());
        let len = w.moon_dir.length();
        assert!((len - 1.0).abs() < 1.0e-5, "moon dir length {len}");
    }

    #[test]
    fn new_moon_night_key_stays_dark() {
        // Chicago, the pinned new moon. The PS1.5 sodium shore is a
        // horizon band of about 0.14 (shaders/common.wgsl). The key
        // light has to stay well under that, or the moon — not the
        // shore — carries the frame.
        let w = world_state(
            unix_utc(2026, 1, 18, 5, 0, 0),
            &Observer::new(41.88, -87.63),
        );
        assert!(w.sun_elevation_deg < -8.0, "sun {}", w.sun_elevation_deg);
        assert!(w.moon_illuminated < 0.02, "k {}", w.moon_illuminated);
        assert!(
            w.moon_color.length() < 0.02,
            "moon key {} elev {}",
            w.moon_color.length(),
            w.moon_elevation_deg
        );
    }

    #[test]
    fn winter_full_moon_lights_sky_when_up() {
        // Local midnight in Chicago (CST, UTC−6) on the winter full
        // moon's night. The disk is essentially full and, opposite a
        // sun at southern declination, high in the northern sky. The
        // sea reflects `sky_*` and glitters off `moon_color`; both have
        // to be carrying moonlight here and not on the new-moon pin.
        let w = world_state(unix_utc(2026, 1, 3, 6, 0, 0), &Observer::new(41.88, -87.63));
        assert!(w.sun_elevation_deg < -6.0, "sun {}", w.sun_elevation_deg);
        assert!(
            w.moon_elevation_deg > 20.0,
            "moon el {} az {}",
            w.moon_elevation_deg,
            w.moon_azimuth_deg
        );
        assert!(w.moon_illuminated > 0.95, "k {}", w.moon_illuminated);
        assert!(w.moon_color.length() > 0.2, "key {}", w.moon_color.length());
        // Night zenith with no moon wash is (0.004, 0.008, 0.020).
        assert!(w.sky_zenith.x > 0.01, "zenith {:?}", w.sky_zenith);
        assert!(w.sky_horizon.length() > 0.04, "horizon {:?}", w.sky_horizon);
    }
}
