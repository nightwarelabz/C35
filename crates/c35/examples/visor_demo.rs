//! Visor demo: the visor system's three signature voices,
//! each rendered through the mixer to its own WAV file.
//! Not a proof harness — no pass/fail gates, no hashes
//! asserted. It exists so a listener can HEAR the motif
//! language: the power-on (the anchor), the power-off
//! (the mirror), and the tag (the smallest unit).
//!
//! Run: cargo run -q -p c35 --example visor_demo [out-dir]
//! Default output: the system temp dir. Writes
//! visor-power-on.wav, visor-power-off.wav, visor-tag.wav.
//! Same command renders the same bytes: the mix is fully
//! deterministic (seeded noise, fixed spawn order).

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
    hdr.extend_from_slice(&(sample_rate * 2 * 2).to_le_bytes());
    hdr.extend_from_slice(&4u16.to_le_bytes());
    hdr.extend_from_slice(&16u16.to_le_bytes());
    hdr.extend_from_slice(b"data");
    hdr.extend_from_slice(&data_bytes.to_le_bytes());
    f.write_all(&hdr)?;
    for s in pcm {
        f.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}

/// Render one visor voice on a fresh Playback (so the
/// ledger line is that voice's alone) for exactly its
/// full duration, then write the WAV and report it.
fn render_one(
    dir: &str,
    file: &str,
    dur_s: f32,
    trigger: fn(&mut Playback),
) -> std::io::Result<()> {
    let mut pb = Playback::with_default_rate(0xC350001);
    trigger(&mut pb);
    let frames = (dur_s * SAMPLE_RATE as f32).round() as usize;
    let samples = pb.render(frames);
    let path = format!("{}/{}", dir, file);
    write_wav(&path, &samples, SAMPLE_RATE)?;
    let bytes = std::fs::metadata(&path)?.len();
    println!(
        "wrote {} ({} bytes, {} frames, {:.2} s)",
        path,
        bytes,
        frames,
        frames as f32 / SAMPLE_RATE as f32
    );
    println!("{}", pb.ledger_line());
    Ok(())
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| {
        std::env::temp_dir().to_string_lossy().into_owned()
    });
    render_one(&dir, "visor-power-on.wav", 1.65, Playback::render_visor_power_on)
        .expect("visor power-on render");
    render_one(&dir, "visor-power-off.wav", 0.90, Playback::render_visor_power_off)
        .expect("visor power-off render");
    render_one(&dir, "visor-tag.wav", 0.18, Playback::render_visor_tag)
        .expect("visor tag render");
}
