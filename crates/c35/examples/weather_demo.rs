//! Weather demo: a 48-hour sky, printed hourly, so the
//! regime reads sensibly — rain raises wetness, dry
//! spells dry it, wind turns smoothly, storms strike.
//!
//! Ticks every 10 simulated minutes (the host's real
//! tick is per-frame; the exponential wetness law makes
//! the result tick-size independent). Strikes are counted
//! per printed hour: the clock schedules them 2–9
//! minutes apart in a storm, so an hourly poll would
//! misleadingly show exactly one.
//!
//! Run: cargo run -q -p c35 --example weather_demo

use c35::weather::{Precip, WeatherState};

fn main() {
    let seed = 0xC35_0001;
    let mut weather = WeatherState::new(seed, 0.0, 100);
    let mut prev_dir = weather.wind_dir();
    let mut max_turn = 0.0f32;
    let mut total_strikes = 0usize;
    let mut wet_peak = 0.0f32;
    let mut hour_strikes = 0usize;
    let mut hour_flash = 0.0f32;

    println!("hour regime    rain  wind   dir    fog   wet   precip  strikes/hr");
    // 288 ten-minute ticks = 48 hours.
    for step in 0..288 {
        let hours = step as f32 / 6.0;
        let day = 100 + (step / 144) as u32;
        let hour_of_day = (step % 144) as f32 / 6.0;
        // Noon = 1.0 daylight, midnight = 0.0.
        let daylight = ((hour_of_day - 6.0) / 12.0 * std::f32::consts::PI)
            .sin()
            .max(0.0);
        // Cool nights, mild days.
        let cold = 0.3 + 0.2 * (1.0 - daylight);
        weather.tick(600.0, hours, day, daylight, cold);
        let turn = (weather.wind_dir() - prev_dir).abs();
        let turn = turn.min(std::f32::consts::TAU - turn);
        if turn > max_turn {
            max_turn = turn;
        }
        prev_dir = weather.wind_dir();
        if weather.wetness() > wet_peak {
            wet_peak = weather.wetness();
        }
        if let Some(s) = weather.last_strike {
            hour_strikes += 1;
            total_strikes += 1;
            hour_flash = s.flash;
        }
        // Print on the hour.
        if (step + 1) % 6 == 0 {
            let hour = (step + 1) / 6;
            let precip = match weather.precip(cold) {
                Precip::None => "none",
                Precip::Rain => "rain",
                Precip::Snow => "snow",
            };
            let strike_col = if hour_strikes > 0 {
                format!("{} (flash {:.2})", hour_strikes, hour_flash)
            } else {
                "-".to_string()
            };
            println!(
                "{:>4} {:?}  {:.2}  {:>5.1}  {:>6.1}  {:.2}  {:.2}  {:<6}  {}",
                hour - 1,
                weather.regime(),
                weather.rain(),
                weather.wind_speed(),
                weather.wind_dir().to_degrees(),
                weather.fog_add(),
                weather.wetness(),
                precip,
                strike_col
            );
            hour_strikes = 0;
            hour_flash = 0.0;
        }
    }
    println!();
    println!("48 h summary: wetness peaked at {wet_peak:.2}, {total_strikes} strikes,");
    println!(
        "largest 10-min wind turn {max_turn:.3} rad ({:.1} deg).",
        max_turn.to_degrees()
    );
    println!("Reads: rain lifts wetness, dry hours pull it back down,");
    println!("wind wanders instead of jumping, storms — and only storms — strike.");
}
