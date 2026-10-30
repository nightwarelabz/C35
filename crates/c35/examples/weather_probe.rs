//! Weather probe. No GPU, no window.
//!
//! Prints the clock-seeded atmosphere at the reality layer's four
//! proof instants, then times [`Atmosphere::at`].
//!
//! ```sh
//! cargo run --example weather_probe -p c35
//! ```
//!
//! Opt the vista into the same reading with the native host:
//!
//! ```sh
//! cargo run --release -p comigen-native -- --weather --frames 30
//! ```
//!
//! or the windowed host:
//!
//! ```sh
//! COMIGEN_WEATHER=1 cargo run --release -p comigen-windowed
//! ```
//!
//! Leaving the flag off keeps the previous frame (no extra draws).

use std::time::Instant;

use c35::weather::Atmosphere;

fn main() {
    let pins = [
        ("winter morning 01-15 08:00", 1_768_485_600.0),
        ("summer noon    07-15 12:00", 1_784_134_800.0),
        ("autumn dusk    10-19 18:30", 1_792_452_600.0),
        ("spring night   04-16 23:30", 1_776_400_200.0),
    ];
    println!("=== COMIGEN WEATHER (clock-seeded, no network) ===");
    for (label, unix) in pins {
        let a = Atmosphere::at(unix);
        println!(
            "{label:<26} rain {:5.2}  wet {:5.2}  wind {:4.1} m/s  fog x{:4.2}  cloud {:4.2}  draws {}  tris {}",
            a.rain,
            a.wetness,
            a.wind.length(),
            a.fog_mul,
            a.cloud,
            a.draw_count(),
            a.triangle_count(),
        );
    }
    let off = Atmosphere::clear();
    println!(
        "{:<26} rain {:5.2}  wet {:5.2}  wind {:4.1} m/s  fog x{:4.2}  cloud {:4.2}  draws {}  tris {}  (the default)",
        "clear (opt-in off)",
        off.rain,
        off.wetness,
        off.wind.length(),
        off.fog_mul,
        off.cloud,
        off.draw_count(),
        off.triangle_count(),
    );

    let n = 20_000u32;
    let t0 = Instant::now();
    let mut acc = 0.0f32;
    for i in 0..n {
        let a = Atmosphere::at(1_700_000_000.0 + f64::from(i) * 60.0);
        acc += a.rain + a.wetness + a.cloud;
    }
    let elapsed = t0.elapsed();
    let ns = elapsed.as_nanos() / u128::from(n);
    println!(
        "eval             : {n} calls in {:.2} ms  ({ns} ns/call, checksum {acc:.3})",
        elapsed.as_secs_f64() * 1000.0
    );
    println!("budget           : rain sheet 2 tris @ 8 m, cloud veil 1 tri; both skipped under their thresholds");
    println!(
        "ledger           : {}",
        Atmosphere::at(pins[2].1).ledger_line()
    );
}
