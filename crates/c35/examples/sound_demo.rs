//! Sound demo: a four-second Greyport street, rendered to a
//! WAV file. Not a proof harness — no pass/fail gates, no
//! hashes asserted. It exercises every render path once so
//! a listener can HEAR the layer: footsteps on five
//! surfaces (one occluded behind a wall, one out of
//! radius and therefore silent), a strike, a landing, a
//! mantle, a sever ring, a seam tick, a district scream,
//! a tremor, a report murmur, the nightjar's churr, the
//! grapple's launch/reel/detach, a servo whine, cannon
//! cooling, and a music bed crossfading stealth into
//! suspicion.
//!
//! Run: cargo run -q -p c35 --example sound_demo [--out path]
//! Default output: the system temp dir's c35-sound-demo.wav.
//! Same command renders the same bytes: the mix is fully
//! deterministic (seeded noise, fixed spawn order).

use c35::perception::field::{LightField, Occluder};
use c35::perception::sound::{DistrictKind, DistrictStimulus};
use c35::sound::music::MusicState;
use c35::sound::voices;
use c35::sound::{Playback, VoiceKind, SAMPLE_RATE};
use c35::traversal::events::{StimulusEvent, StimulusKind};
use c35::traversal::surface::{SurfaceKind, SurfaceRecord};
use glam::Vec3;
use std::io::Write;

fn ev(
    tick: u64,
    kind: StimulusKind,
    pos: Vec3,
    surface: SurfaceRecord,
    magnitude: f32,
) -> StimulusEvent {
    StimulusEvent {
        tick,
        kind,
        pos,
        surface,
        magnitude,
    }
}

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

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| {
        std::env::temp_dir()
            .join("c35-sound-demo.wav")
            .to_string_lossy()
            .into_owned()
    });

    // The field: night ambient + one wall between the
    // listener and the concrete footstep, so that step
    // arrives muffled (quarter gain + 700 Hz lowpass).
    let mut field = LightField::new(0.15);
    field.add_occluder(Occluder {
        min: Vec3::new(-5.0, 0.0, 2.0),
        max: Vec3::new(5.0, 3.0, 3.0),
    });

    let mut pb = Playback::with_default_rate(0xC350001);
    pb.set_listener(Vec3::new(0.0, 1.62, 0.0), Vec3::new(0.0, 0.0, -1.0));

    // Footstep magnitudes priced the honest way: the
    // surface's own footstep gain at run speed.
    let run = 3.4f32;
    let events = vec![
        ev(
            1,
            StimulusKind::Footstep,
            Vec3::new(5.0, 0.0, -8.0),
            SurfaceRecord::dry(SurfaceKind::Wood),
            SurfaceRecord::dry(SurfaceKind::Wood).footstep_gain(run),
        ),
        ev(
            2,
            StimulusKind::Footstep,
            Vec3::new(-6.0, 0.0, -10.0),
            SurfaceRecord::wet(SurfaceKind::Stone, 0.8),
            SurfaceRecord::wet(SurfaceKind::Stone, 0.8).footstep_gain(run),
        ),
        ev(
            3,
            StimulusKind::Footstep,
            Vec3::new(3.0, 0.0, -4.0),
            SurfaceRecord::dry(SurfaceKind::Metal),
            SurfaceRecord::dry(SurfaceKind::Metal).footstep_gain(run),
        ),
        // Behind the wall: occluded, muffled.
        ev(
            4,
            StimulusKind::Footstep,
            Vec3::new(0.0, 0.0, 8.0),
            SurfaceRecord::dry(SurfaceKind::Concrete),
            SurfaceRecord::dry(SurfaceKind::Concrete).footstep_gain(run),
        ),
        ev(
            5,
            StimulusKind::Strike,
            Vec3::new(-8.0, 0.0, -6.0),
            SurfaceRecord::dry(SurfaceKind::Stone),
            1.0,
        ),
        ev(
            6,
            StimulusKind::Landing,
            Vec3::new(2.0, 0.0, -12.0),
            SurfaceRecord::dry(SurfaceKind::Asphalt),
            1.2,
        ),
        ev(
            7,
            StimulusKind::Mantle,
            Vec3::new(6.0, 0.0, -5.0),
            SurfaceRecord::dry(SurfaceKind::Brick),
            0.7,
        ),
        ev(
            8,
            StimulusKind::Sever,
            Vec3::new(-10.0, 0.0, -15.0),
            SurfaceRecord::dry(SurfaceKind::Metal),
            1.0,
        ),
        ev(
            9,
            StimulusKind::SeamCross,
            Vec3::new(1.0, 0.0, -3.0),
            SurfaceRecord::dry(SurfaceKind::Stone),
            0.3,
        ),
        // Out of the 12 m strike-fail radius: propagated
        // to nothing, so it spawns no voice at all.
        ev(
            10,
            StimulusKind::StrikeFail,
            Vec3::new(0.0, 0.0, -20.0),
            SurfaceRecord::dry(SurfaceKind::Sky),
            0.4,
        ),
    ];
    pb.render_events(&events, &field);

    // District law: no occlusion, slow falloff.
    pb.render_district(&DistrictStimulus {
        pos: Vec3::new(50.0, 0.0, -100.0),
        magnitude: 1.0,
        kind: DistrictKind::Scream,
    });
    pb.render_district(&DistrictStimulus {
        pos: Vec3::new(-30.0, 0.0, -40.0),
        magnitude: 0.8,
        kind: DistrictKind::Tremor,
    });
    pb.render_district(&DistrictStimulus {
        pos: Vec3::new(100.0, 0.0, 50.0),
        magnitude: 0.6,
        kind: DistrictKind::Report,
    });

    // The nightjar: ambient, no position, no marker.
    pb.render_ambient_churr();

    // Host-driven voices: the hook state machine, a moving
    // mecha's servo, a cooling cannon — spawned directly,
    // not from the event log.
    let sr = SAMPLE_RATE as f32;
    let seed = pb.mixer().next_seed();
    pb.mixer().play(
        voices::grapple_launch_voice(seed, sr),
        VoiceKind::GrappleLaunch,
        0.8,
        -0.3,
        false,
    );
    for _ in 0..2 {
        let seed = pb.mixer().next_seed();
        pb.mixer().play(
            voices::grapple_reel_chunk(seed, sr),
            VoiceKind::GrappleReel,
            0.5,
            -0.3,
            false,
        );
    }
    let seed = pb.mixer().next_seed();
    pb.mixer().play(
        voices::grapple_detach_voice(seed, sr),
        VoiceKind::GrappleDetach,
        0.7,
        -0.3,
        false,
    );
    for _ in 0..2 {
        pb.mixer().play(
            voices::servo_chunk(0.7, sr),
            VoiceKind::Servo,
            0.4,
            0.5,
            false,
        );
    }
    let seed = pb.mixer().next_seed();
    pb.mixer().play(
        voices::cannon_cool_voice(seed, sr),
        VoiceKind::CannonCool,
        0.6,
        0.4,
        false,
    );

    // Music bed: stealth for two seconds, then suspicion;
    // the 2-second crossfade carries across the blocks.
    pb.attach_music();
    let block = (SAMPLE_RATE as usize) / 4; // 0.25 s blocks
    let mut all: Vec<f32> = Vec::new();
    for b in 0..16 {
        if b == 8 {
            pb.music().set_state(MusicState::Suspicion);
        }
        let rendered = pb.render(block);
        all.extend_from_slice(&rendered);
    }

    write_wav(&out, &all, SAMPLE_RATE).expect("wav write");
    println!(
        "wrote {} ({} frames, {} s)",
        out,
        all.len() / 2,
        all.len() / 2 / SAMPLE_RATE as usize
    );
    println!("{}", pb.ledger_line());
    println!(
        "active voices at end: {} (the 4 music stems persist by design)",
        pb.mixer().active_voices()
    );
}
