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
    /// Unit vector toward the moon. The moon is a *faint key light only*:
    /// its direction is mirrored from the sun's (azimuth + 180°, elevation
    /// reflected) — phase is not modelled. Black when the sun is up.
    pub moon_dir: Vec3,
    pub moon_color: Vec3,
    /// Moon disc strength for the sky shader (0 in daylight).
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
        * (0.000075 + 0.001868 * gamma.cos() - 0.032077 * gamma.sin()
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
    let summer_wave = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * (season_day - 201.0) / 365.25).cos();
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

    // Hemisphere ambient: strong in day, ember-lit at dusk, moon-dim at night.
    let amb_day_sky = Vec3::new(0.30, 0.375, 0.52) * (0.45 + 0.55 * daylight);
    let amb_sky = mix3(Vec3::new(0.020, 0.028, 0.052), amb_day_sky, daylight)
        + Vec3::new(0.10, 0.045, 0.015) * dusk_glow;
    let amb_ground = mix3(
        Vec3::new(0.010, 0.010, 0.014),
        Vec3::new(0.145, 0.115, 0.095) * (0.4 + 0.6 * daylight),
        daylight,
    );

    // Fog: the horizon colour, away from the sun; a touch denser in cold
    // haze, a touch thinner at night.
    let fog_color = hor * 0.96 + Vec3::new(0.02, 0.015, 0.01);
    let fog_density = 0.00135 * (1.0 + 0.25 * cold) * (1.0 - 0.25 * (1.0 - daylight));

    // Moon: faint key light mirrored from the sun (phase not modelled).
    let moon_in = smoothstep(-1.0, -6.0, el);
    let moon_el = (-el).clamp(5.0, 55.0);
    let moon_az = (sun_azim + 180.0).to_radians();
    let moon_dir = Vec3::new(
        moon_el.to_radians().cos() * moon_az.sin(),
        moon_el.to_radians().sin(),
        -moon_el.to_radians().cos() * moon_az.cos(),
    )
    .normalize();
    let moon_color = Vec3::new(0.42, 0.52, 0.74) * (0.34 * moon_in);
    let moon_disc = 1.5 * moon_in;

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
        moon_dir,
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
