//! Rain bed probe. No GPU.
//!
//! Renders three seconds of procedural rain (drizzle, then a
//! downpour, then dry) to a WAV, and times one second of the mixer.
//!
//! ```sh
//! cargo run --release --example rain_probe -p c35
//! ```
//!
//! Default output is the temp dir's `c35-rain.wav`. Pass a path as
//! the first argument to write somewhere else. The mix is
//! deterministic: the same command writes the same bytes.

use c35::sound::{Playback, SAMPLE_RATE};
use std::io::Write;

fn write_wav(path: &str, samples: &[f32], sample_rate: u32) -> std::io::Result<()> {
    let mut pcm: Vec<i16> = Vec::with_capacity(samples.len());
    for s in samples {
        pcm.push((s.clamp(-1.0, 1.0) * 32767.0) as i16);
    }
    let data_bytes = (pcm.len() * 2) as u32;
    let mut f = std::fs::File::create(path)?;
    let mut hdr = Vec::with_capacity(44);
    hdr.extend_from_slice(b"RIFF");
    hdr.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    hdr.extend_from_slice(b"WAVE");
    hdr.extend_from_slice(b"fmt ");
    hdr.extend_from_slice(&16u32.to_le_bytes());
    hdr.extend_from_slice(&1u16.to_le_bytes());
    hdr.extend_from_slice(&2u16.to_le_bytes());
    hdr.extend_from_slice(&sample_rate.to_le_bytes());
    hdr.extend_from_slice(&(sample_rate * 4).to_le_bytes());
    hdr.extend_from_slice(&4u16.to_le_bytes());
    hdr.extend_from_slice(&16u16.to_le_bytes());
    hdr.extend_from_slice(b"data");
    hdr.extend_from_slice(&data_bytes.to_le_bytes());
    f.write_all(&hdr)?;
    for s in &pcm {
        f.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}

fn rms(pcm: &[f32]) -> f32 {
    if pcm.is_empty() {
        return 0.0;
    }
    let sum: f32 = pcm.iter().map(|s| s * s).sum();
    (sum / pcm.len() as f32).sqrt()
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| {
        std::env::temp_dir()
            .join("c35-rain.wav")
            .to_string_lossy()
            .into_owned()
    });

    let sec = SAMPLE_RATE as usize;
    let mut pb = Playback::with_default_rate(0xC35);
    let mut pcm = Vec::new();

    // One second each: drizzle, downpour, dry. The dry second is
    // silence with the slot still held, which is what a storm
    // ending sounds like.
    for (label, rate) in [("drizzle", 0.25f32), ("downpour", 1.0), ("dry", 0.0)] {
        pb.set_rain(rate);
        let block = pb.render(sec);
        println!(
            "{label:<10} rain {rate:.2}  rms {:.4}  peak {:.4}",
            rms(&block),
            block.iter().map(|s| s.abs()).fold(0.0f32, f32::max),
        );
        pcm.extend(block);
    }
    println!("ledger       : {}", pb.ledger_line());

    let t0 = std::time::Instant::now();
    let mut timed = Playback::with_default_rate(1);
    timed.set_rain(1.0);
    let _ = timed.render(sec);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("mix 1s       : {ms:.2} ms of CPU for {sec} stereo frames");

    write_wav(&out, &pcm, SAMPLE_RATE).expect("write wav");
    println!("wrote        : {out}");
}
