//! Signature voices — every sound that is NOT a footstep,
//! synthesized from the same primitives, no audio assets.
//!
//! The rule for all of these: the voice renders what
//! another system already decided. The Type 2's scream is
//! rendered only when `Type2Governor::scream()` yields a
//! district stimulus (a silenced voice apparatus yields
//! nothing, so nothing sounds). The grapple voices are
//! spawned by the host from the hook state machine, not
//! from the event log. Mantle and landing gains come from
//! the priced event magnitudes via the propagation layer.
//! Playback never judges; it only makes audible.

use crate::sound::synth::{
    attack, decay_exp, release_gate, CompositeVoice, NoiseBurst, Ring, Rng, Saw, Sine,
    Thump, VoiceGen,
};
use crate::traversal::surface::{SurfaceKind, SurfaceRecord};

/// Claw strike at its impact point: a crack plus the
/// surface's own ring. Metal rings long, stone cracks
/// short, wood knocks — the surface speaks, not a
/// generic "hit" sample.
pub fn strike_voice(
    surface: SurfaceRecord,
    seed: u64,
    sample_rate: f32,
) -> Box<dyn VoiceGen> {
    let (ring_base, ring_dur, ring_tau): (f32, f32, f32) = match surface.kind {
        SurfaceKind::Metal => (320.0, 0.5, 0.15),
        SurfaceKind::Stone | SurfaceKind::Brick => (200.0, 0.2, 0.06),
        SurfaceKind::Wood => (260.0, 0.15, 0.05),
        _ => (180.0, 0.18, 0.05),
    };
    Box::new(CompositeVoice::new(vec![
        (
            Box::new(NoiseBurst::new(seed, 5000.0, 0.05, 0.001, 0.015, sample_rate)),
            0.9,
        ),
        (
            Box::new(Ring::new(
                ring_base,
                &[(1.0, 0.6), (2.76, 0.3)],
                ring_dur,
                ring_tau,
                sample_rate,
            )),
            0.6,
        ),
    ]))
}

/// A landing: body-weight thump plus a low boom. The gain
/// the caller passes already prices the impact speed (the
/// event magnitude, propagated) — this voice only shapes it.
pub fn landing_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(CompositeVoice::new(vec![
        (
            Box::new(Thump::new(80.0, 35.0, 0.09, 0.30, sample_rate)),
            1.0,
        ),
        (
            Box::new(NoiseBurst::new(seed, 300.0, 0.18, 0.002, 0.07, sample_rate)),
            0.4,
        ),
    ]))
}

/// A mantle completing: body thump plus cloth scuff.
pub fn mantle_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(CompositeVoice::new(vec![
        (
            Box::new(Thump::new(110.0, 50.0, 0.07, 0.22, sample_rate)),
            0.8,
        ),
        (
            Box::new(NoiseBurst::new(seed, 1200.0, 0.15, 0.005, 0.05, sample_rate)),
            0.4,
        ),
    ]))
}

/// A severed line ringing down the street: a long metallic
/// ring, the rarest and loudest small sound in the field.
pub fn sever_ring_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    let _ = seed;
    Box::new(Ring::new(
        180.0,
        &[(1.0, 0.6), (2.76, 0.35), (5.40, 0.2), (8.93, 0.1)],
        1.2,
        0.35,
        sample_rate,
    ))
}

/// A seam crossing: a soft tick, quiet by design — the seam
/// is felt more than heard.
pub fn seam_tick_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(NoiseBurst::new(seed, 2000.0, 0.05, 0.001, 0.02, sample_rate))
}

/// A strike that failed to bite: a dull empty snap, no ring.
/// Sky never rings — the failure sounds empty on purpose.
pub fn strike_fail_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(CompositeVoice::new(vec![
        (
            Box::new(Thump::new(140.0, 90.0, 0.03, 0.10, sample_rate)),
            0.6,
        ),
        (
            Box::new(NoiseBurst::new(seed, 1500.0, 0.04, 0.001, 0.015, sample_rate)),
            0.3,
        ),
    ]))
}

/// Grapple launch: a sharp crack — line leaving the drum.
pub fn grapple_launch_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(CompositeVoice::new(vec![
        (
            Box::new(NoiseBurst::new(seed, 6000.0, 0.04, 0.001, 0.012, sample_rate)),
            1.0,
        ),
        (
            Box::new(Ring::new(3000.0, &[(1.0, 0.4)], 0.08, 0.03, sample_rate)),
            0.4,
        ),
    ]))
}

/// One second of grapple reel: ratchet clicks at 12 Hz.
/// The host re-spawns a chunk each second while reeling —
/// the mixer is fire-and-forget, so a continuous reel is
/// the host's loop, not a voice that never ends.
pub struct ReelChunk {
    rng: Rng,
    click: Sine,
    t: f32,
    dur: f32,
    last_click: i32,
    click_t: f32,
    sample_rate: f32,
}

impl ReelChunk {
    pub fn new(seed: u64, sample_rate: f32) -> ReelChunk {
        ReelChunk {
            rng: Rng::new(seed),
            click: Sine::new(1800.0, sample_rate),
            t: 0.0,
            dur: 1.0,
            last_click: -1,
            click_t: 1.0,
            sample_rate,
        }
    }
}

impl VoiceGen for ReelChunk {
    fn next(&mut self) -> Option<f32> {
        if self.t >= self.dur {
            return None;
        }
        let idx = (self.t * 12.0).floor() as i32;
        if idx != self.last_click {
            self.last_click = idx;
            self.click_t = 0.0;
        }
        let click_env = if self.click_t < 0.015 {
            decay_exp(self.click_t, 0.005)
        } else {
            0.0
        };
        let s = self.click.next() * click_env * 0.5
            + self.rng.next_bipolar() * click_env * 0.3;
        self.click_t += 1.0 / self.sample_rate;
        self.t += 1.0 / self.sample_rate;
        Some(s)
    }
}

pub fn grapple_reel_chunk(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(ReelChunk::new(seed, sample_rate))
}

/// Grapple detach: a snap plus the line whipping free.
pub fn grapple_detach_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(CompositeVoice::new(vec![
        (
            Box::new(NoiseBurst::new(seed, 5000.0, 0.06, 0.001, 0.02, sample_rate)),
            0.9,
        ),
        (
            Box::new(Thump::new(900.0, 200.0, 0.02, 0.12, sample_rate)),
            0.5,
        ),
    ]))
}

/// Mecha servo whine, one 0.5 s chunk: two partials whose
/// gain tracks the servo speed the host passes (0..1).
/// The host re-spawns chunks while the machine moves, the
/// same fire-and-forget rule as the reel.
pub struct ServoChunk {
    a: Sine,
    b: Sine,
    t: f32,
    dur: f32,
    gain: f32,
    sample_rate: f32,
}

impl ServoChunk {
    pub fn new(speed01: f32, sample_rate: f32) -> ServoChunk {
        ServoChunk {
            a: Sine::new(900.0, sample_rate),
            b: Sine::new(1400.0, sample_rate),
            t: 0.0,
            dur: 0.5,
            gain: (0.15 + 0.65 * speed01.clamp(0.0, 1.0)),
            sample_rate,
        }
    }
}

impl VoiceGen for ServoChunk {
    fn next(&mut self) -> Option<f32> {
        if self.t >= self.dur {
            return None;
        }
        let env = attack(self.t, 0.05) * release_gate(self.t, self.dur, 0.05);
        let s = (self.a.next() * 0.6 + self.b.next() * 0.3) * env * self.gain;
        self.t += 1.0 / self.sample_rate;
        Some(s)
    }
}

pub fn servo_chunk(speed01: f32, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(ServoChunk::new(speed01, sample_rate))
}

/// Cannon cooling after fire: metallic ticks at growing
/// intervals over steam hiss, 1.5 s. The ticks are the
/// cannon speaking as it cools — a read, not an effect.
pub struct CannonCool {
    tick: Sine,
    steam: NoiseBurst,
    t: f32,
    dur: f32,
    next_tick: f32,
    gap: f32,
    tick_amp: f32,
    tick_t: f32,
    sample_rate: f32,
}

impl CannonCool {
    pub fn new(seed: u64, sample_rate: f32) -> CannonCool {
        CannonCool {
            tick: Sine::new(2400.0, sample_rate),
            steam: NoiseBurst::new(seed ^ 0x1234567, 3000.0, 1.5, 0.05, 0.5, sample_rate),
            t: 0.0,
            dur: 1.5,
            next_tick: 0.0,
            gap: 0.12,
            tick_amp: 0.7,
            tick_t: 1.0,
            sample_rate,
        }
    }
}

impl VoiceGen for CannonCool {
    fn next(&mut self) -> Option<f32> {
        if self.t >= self.dur {
            return None;
        }
        if self.t >= self.next_tick {
            self.tick_t = 0.0;
            self.next_tick = self.t + self.gap;
            self.gap *= 1.5;
            self.tick_amp *= 0.7;
        }
        let tick_env = if self.tick_t < 0.03 {
            decay_exp(self.tick_t, 0.01) * self.tick_amp
        } else {
            0.0
        };
        let tick_s = self.tick.next() * tick_env;
        let steam_s = self.steam.next().unwrap_or(0.0) * 0.25;
        self.tick_t += 1.0 / self.sample_rate;
        self.t += 1.0 / self.sample_rate;
        Some(tick_s + steam_s)
    }
}

pub fn cannon_cool_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(CannonCool::new(seed, sample_rate))
}

/// The Type 2's scream: two detuned saws sweeping up and
/// falling back over 2 seconds, plus breath noise. This is
/// the district-level voice — the caller passes the
/// district-heard magnitude as gain, never room acoustics.
/// Rendered ONLY when the governor yields a stimulus: a
/// silenced voice apparatus yields nothing, so nothing
/// sounds, and that silence is the system working.
pub struct Scream {
    a: Saw,
    b: Saw,
    rng: Rng,
    t: f32,
    dur: f32,
    sample_rate: f32,
}

impl Scream {
    pub fn new(seed: u64, sample_rate: f32) -> Scream {
        Scream {
            a: Saw::new(600.0, sample_rate),
            b: Saw::new(600.0 * 1.007, sample_rate),
            rng: Rng::new(seed),
            t: 0.0,
            dur: 2.0,
            sample_rate,
        }
    }

    fn sweep_freq(t: f32) -> f32 {
        if t < 0.6 {
            600.0 + (1300.0 - 600.0) * (t / 0.6)
        } else {
            1300.0 + (500.0 - 1300.0) * ((t - 0.6) / 1.4)
        }
    }
}

impl VoiceGen for Scream {
    fn next(&mut self) -> Option<f32> {
        if self.t >= self.dur {
            return None;
        }
        let vib = 30.0 * (self.t * 2.0 * std::f32::consts::PI * 6.0).sin();
        let f = Self::sweep_freq(self.t) + vib;
        self.a.set_freq(f);
        self.b.set_freq(f * 1.007);
        let env = attack(self.t, 0.15) * release_gate(self.t, self.dur, 0.4);
        let s = (self.a.next() * 0.5 + self.b.next() * 0.4) * env
            + self.rng.next_bipolar() * 0.12 * env;
        self.t += 1.0 / self.sample_rate;
        Some(s)
    }
}

pub fn scream_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(Scream::new(seed, sample_rate))
}

/// The nightjar's eerie churring: a mid-frequency carrier
/// with slow pitch wander and rapid amplitude chopping,
/// 2.5 seconds, rendered AMBIENT — no position, no pan
/// law, no source marker. It is heard before it is seen,
/// and the visor never tags it, because playback never
/// gives it a place to tag.
pub struct Churr {
    carrier: Sine,
    rng: Rng,
    t: f32,
    dur: f32,
    sample_rate: f32,
}

impl Churr {
    pub fn new(seed: u64, sample_rate: f32) -> Churr {
        Churr {
            carrier: Sine::new(950.0, sample_rate),
            rng: Rng::new(seed),
            t: 0.0,
            dur: 2.5,
            sample_rate,
        }
    }
}

impl VoiceGen for Churr {
    fn next(&mut self) -> Option<f32> {
        if self.t >= self.dur {
            return None;
        }
        // Slow pitch wander around 950 Hz.
        let wander = 60.0 * (self.t * 2.0 * std::f32::consts::PI * 0.7).sin();
        self.carrier.freq = 950.0 + wander;
        // Rapid amplitude chopping at 17 Hz: the churr.
        let am = 0.25 + 0.75 * (0.5 + 0.5 * (self.t * 2.0 * std::f32::consts::PI * 17.0).sin());
        let env = attack(self.t, 0.3) * release_gate(self.t, self.dur, 0.5);
        let s = self.carrier.next() * am * env * 0.6
            + self.rng.next_bipolar() * 0.06 * env;
        self.t += 1.0 / self.sample_rate;
        Some(s)
    }
}

pub fn churr_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(Churr::new(seed, sample_rate))
}

/// A Weathervane's telephone report as the district hears
/// it: short tonal blips, no words. Dialogue is not built;
/// the report's SOUND is a presence, not a sentence — a
/// placeholder timbre until dialogue exists, and documented
/// as one.
pub struct ReportMurmur {
    blips: Vec<(f32, f32)>,
    osc: Sine,
    t: f32,
    dur: f32,
    sample_rate: f32,
}

impl ReportMurmur {
    pub fn new(seed: u64, sample_rate: f32) -> ReportMurmur {
        let mut rng = Rng::new(seed);
        let times = [0.05, 0.28, 0.45, 0.70, 0.88];
        let blips = times
            .iter()
            .map(|t| (*t, 300.0 + rng.next_f32() * 200.0))
            .collect();
        ReportMurmur {
            blips,
            osc: Sine::new(300.0, sample_rate),
            t: 0.0,
            dur: 1.0,
            sample_rate,
        }
    }
}

impl VoiceGen for ReportMurmur {
    fn next(&mut self) -> Option<f32> {
        if self.t >= self.dur {
            return None;
        }
        let mut s = 0.0f32;
        for (bt, bf) in self.blips.iter() {
            let dt = self.t - bt;
            if (0.0..0.10).contains(&dt) {
                self.osc.freq = *bf;
                s += self.osc.next() * decay_exp(dt, 0.03) * 0.5;
            }
        }
        // Advance the oscillator even between blips so phase
        // stays continuous; the sample above already advanced
        // it on blip samples, and silence needs no advance.
        self.t += 1.0 / self.sample_rate;
        Some(s)
    }
}

pub fn report_murmur_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(ReportMurmur::new(seed, sample_rate))
}

/// Mecha ground tremor: felt more than heard — a low
/// rumble at 38 Hz plus lowpassed noise, 1.2 seconds.
pub fn tremor_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(CompositeVoice::new(vec![
        (
            Box::new(Thump::new(45.0, 38.0, 0.4, 1.2, sample_rate)),
            0.7,
        ),
        (
            Box::new(NoiseBurst::new(seed, 120.0, 1.2, 0.2, 0.4, sample_rate)),
            0.4,
        ),
    ]))
}
