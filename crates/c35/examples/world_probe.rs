//! Dev probe: print the reality layer's reading for the M2 proof matrix
//! (Chicago, lat 41.88, lon −87.63) and the Simulacrum season anchors, so
//! the solar model can be checked against any almanac before a single
//! shader consumes it. `cargo run --example world_probe -p c35`.

use c35::world::{iso_utc, world_state, Observer};

fn show(label: &str, unix: f64, obs: &Observer) {
    let w = world_state(unix, obs);
    println!(
        "{label:<26} {}  el {:+7.2}°  az {:6.1}°  day {:4.1}h  cold {:.2}  aut {:.2}  spr {:.2}  sum {:.2}  snowline {:+6.1}  ice {:.2}  dayl {:.2}  stars {:.2}",
        iso_utc(unix),
        w.sun_elevation_deg,
        w.sun_azimuth_deg,
        w.day_length_hours,
        w.cold,
        w.autumn,
        w.spring,
        w.summer,
        w.snow_line,
        w.water_ice,
        w.daylight,
        w.star_intensity,
    );
}

fn main() {
    let chicago = Observer::new(41.88, -87.63);
    // The pinned proof matrix (local wall times -> UTC via the real offsets:
    // Jan = CST −06:00, Apr/Jul/Oct = CDT −05:00).
    show("winter morning 01-15 08:00", 1768485600.0, &chicago); // 14:00Z
    show("summer noon    07-15 12:00", 1784134800.0, &chicago); // 17:00Z
    show("autumn dusk    10-19 18:30", 1792452600.0, &chicago); // 23:30Z
    show("spring night   04-16 23:30", 1776400200.0, &chicago); // 04-17 04:30Z
    println!();
    // Anchor sanity at local noon through the northern year.
    for (label, unix) in [
        ("anchor Jan 15 noon", 1768500000.0),
        ("anchor Apr 16 noon", 1776358800.0),
        ("anchor Jul 20 noon", 1784566800.0),
        ("anchor Oct 19 noon", 1792429200.0),
    ] {
        show(label, unix, &chicago);
    }
    println!();
    // Hemisphere flip: Sydney in December should read summer.
    let sydney = Observer::new(-33.87, 151.21);
    show("Sydney Dec 15 noon", 1797296400.0, &sydney);
    show("Sydney Jun 15 noon", 1781488800.0, &sydney);
    // The stated fallback.
    show("equator fallback noon", 1768500000.0, &Observer::unknown());
}
