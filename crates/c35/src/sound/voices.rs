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
    attack, decay_exp, release_gate, CompositeVoice, DropOsc, Lowpass, NoiseBurst, Ring, Rng, Saw,
    Sine, Thump, VoiceGen,
};
use crate::traversal::surface::{SurfaceKind, SurfaceRecord};

/// Claw strike at its impact point: a crack plus the
/// surface's own ring. Metal rings long, stone cracks
/// short, wood knocks — the surface speaks, not a
/// generic "hit" sample.
pub fn strike_voice(surface: SurfaceRecord, seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    let (ring_base, ring_dur, ring_tau): (f32, f32, f32) = match surface.kind {
        SurfaceKind::Metal => (320.0, 0.5, 0.15),
        SurfaceKind::Stone | SurfaceKind::Brick => (200.0, 0.2, 0.06),
        SurfaceKind::Wood => (260.0, 0.15, 0.05),
        _ => (180.0, 0.18, 0.05),
    };
    Box::new(CompositeVoice::new(vec![
        (
            Box::new(NoiseBurst::new(
                seed,
                5000.0,
                0.05,
                0.001,
                0.015,
                sample_rate,
            )),
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
            Box::new(NoiseBurst::new(
                seed,
                1200.0,
                0.15,
                0.005,
                0.05,
                sample_rate,
            )),
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
    Box::new(NoiseBurst::new(
        seed,
        2000.0,
        0.05,
        0.001,
        0.02,
        sample_rate,
    ))
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
            Box::new(NoiseBurst::new(
                seed,
                1500.0,
                0.04,
                0.001,
                0.015,
                sample_rate,
            )),
            0.3,
        ),
    ]))
}

/// Grapple launch: a sharp crack — line leaving the drum.
pub fn grapple_launch_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(CompositeVoice::new(vec![
        (
            Box::new(NoiseBurst::new(
                seed,
                6000.0,
                0.04,
                0.001,
                0.012,
                sample_rate,
            )),
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
        let s = self.click.next() * click_env * 0.5 + self.rng.next_bipolar() * click_env * 0.3;
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
            Box::new(NoiseBurst::new(
                seed,
                5000.0,
                0.06,
                0.001,
                0.02,
                sample_rate,
            )),
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
        let s = self.carrier.next() * am * env * 0.6 + self.rng.next_bipolar() * 0.06 * env;
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

/// The visor motif: three notes on a stacked fifth —
/// C5, G5, C6 — the fingerprint of the operator's visor
/// system. Every visor voice below is built from these
/// frequencies; the power-on is the anchor, the power-off
/// plays them falling, the tag is the smallest unit.
const VISOR_C5: f32 = 523.25;
const VISOR_G5: f32 = 783.99;
const VISOR_C6: f32 = 1046.50;

/// One motif note, closed-form like the reference render:
/// sine fundamental plus 2nd and 3rd harmonics under a fast
/// attack and an exponential decay, gated so notes never
/// bleed into each other.
fn visor_note(dt: f32, freq: f32, gate: f32, tau: f32, amps: (f32, f32, f32)) -> f32 {
    if dt < 0.0 || dt >= gate {
        return 0.0;
    }
    let e = attack(dt, 0.006) * decay_exp(dt, tau) * release_gate(dt, gate, 0.02);
    let p = 2.0 * std::f32::consts::PI * freq * dt;
    e * (amps.0 * p.sin() + amps.1 * (2.0 * p).sin() + amps.2 * (3.0 * p).sin())
}

/// The visor powering on: the approved signature sound of
/// the whole visor system, rendered through the synth
/// primitives. A relay thunk, a charge rise (exponential
/// triangle sweep 180 -> 2350 Hz with a sub sine and a
/// band of hiss), a held shimmer at the top, then the
/// motif — C5, G5, C6 — visor online. A whisper of room
/// noise runs under everything: the street never fully
/// leaves. Host-spawned from visor state, never the event
/// log — like the grapple voices, it is the operator's own
/// equipment speaking, heard at the ear.
pub struct VisorPowerOn {
    thunk: DropOsc,
    hiss: Lowpass,
    rng: Rng,
    sweep_phase: f32,
    sub_phase: f32,
    t: f32,
    dur: f32,
    sample_rate: f32,
}

impl VisorPowerOn {
    pub fn new(seed: u64, sample_rate: f32) -> VisorPowerOn {
        VisorPowerOn {
            // The thunk's pitch fall is the DropOsc's
            // exponential where the reference render falls
            // linearly 130 -> 48 Hz; tau chosen so the fall
            // lands in the same place by the gate's end.
            thunk: DropOsc::new(130.0, 48.0, 0.05, sample_rate),
            hiss: Lowpass::new(585.0, sample_rate),
            rng: Rng::new(seed),
            sweep_phase: 0.0,
            sub_phase: 0.0,
            t: 0.0,
            dur: 1.65,
            sample_rate,
        }
    }
}

impl VoiceGen for VisorPowerOn {
    fn next(&mut self) -> Option<f32> {
        if self.t >= self.dur {
            return None;
        }
        let t = self.t;
        let mut s = 0.0f32;
        // 1. Relay thunk, 0.00-0.14.
        if t < 0.14 {
            let e = attack(t, 0.004) * decay_exp(t, 0.045);
            s += self.thunk.next() * e * 0.55;
        }
        // 2 + 3. Charge rise 0.06-0.91, shimmer 0.91-1.05.
        // One phase runs through both, so the shimmer is
        // the rise parked, not a new oscillator.
        if (0.06..1.05).contains(&t) {
            let tt = t - 0.06;
            let rise_ratio: f32 = 2350.0 / 180.0;
            let freq = if tt < 0.85 {
                180.0 * rise_ratio.powf(tt / 0.85)
            } else {
                2350.0
            };
            if tt < 0.85 {
                let ramp = (tt / 0.8 + 0.2).min(1.0);
                let tri = 2.0 * (2.0 * self.sweep_phase.fract() - 1.0).abs() - 1.0;
                s += tri * attack(tt, 0.05) * decay_exp(tt, 10.0) * ramp * 0.16;
                s += (self.sub_phase * 2.0 * std::f32::consts::PI).sin() * ramp * 0.10;
                let hiss_env = (tt * 3.0).min(1.0);
                s += self.hiss.process(self.rng.next_bipolar()) * hiss_env * 0.05;
            } else {
                let st = tt - 0.85;
                let trem = 0.85 + 0.15 * (st * 2.0 * std::f32::consts::PI * 28.0).sin();
                s += (self.sweep_phase * 2.0 * std::f32::consts::PI).sin()
                    * trem
                    * attack(st, 0.01)
                    * decay_exp(st, 0.09)
                    * 0.07;
            }
            self.sweep_phase = (self.sweep_phase + freq / self.sample_rate) % 1.0;
            self.sub_phase = (self.sub_phase + freq / (2.0 * self.sample_rate)) % 1.0;
        }
        // 4. The motif: C5 at 1.05, G5 at 1.19, C6 at 1.33.
        s += visor_note(t - 1.05, VISOR_C5, 0.16, 0.055, (0.30, 0.12, 0.05));
        s += visor_note(t - 1.19, VISOR_G5, 0.16, 0.055, (0.30, 0.12, 0.05));
        s += visor_note(t - 1.33, VISOR_C6, 0.16, 0.055, (0.30, 0.12, 0.05));
        // 5. Room breath under the whole span.
        let breath = 0.4 + 0.6 * (t / 0.5).min(1.0);
        s += self.rng.next_bipolar() * 0.012 * breath;
        self.t += 1.0 / self.sample_rate;
        Some(s * release_gate(t, self.dur, 0.03))
    }
}

pub fn visor_power_on_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(VisorPowerOn::new(seed, sample_rate))
}

/// The visor powering off: the power-on mirrored. The
/// motif falls (C6, G5, C5 — shorter, softer), the charge
/// sinks back down its own sweep, and a relay release
/// click closes the switch. 0.9 seconds. Host-spawned
/// from visor state, heard at the ear.
pub struct VisorPowerOff {
    hiss: Lowpass,
    rng: Rng,
    sweep_phase: f32,
    sub_phase: f32,
    click: DropOsc,
    t: f32,
    dur: f32,
    sample_rate: f32,
}

impl VisorPowerOff {
    pub fn new(seed: u64, sample_rate: f32) -> VisorPowerOff {
        VisorPowerOff {
            hiss: Lowpass::new(585.0, sample_rate),
            rng: Rng::new(seed),
            sweep_phase: 0.0,
            sub_phase: 0.0,
            click: DropOsc::new(260.0, 110.0, 0.012, sample_rate),
            t: 0.0,
            dur: 0.90,
            sample_rate,
        }
    }
}

impl VoiceGen for VisorPowerOff {
    fn next(&mut self) -> Option<f32> {
        if self.t >= self.dur {
            return None;
        }
        let t = self.t;
        let mut s = 0.0f32;
        // The motif falling: C6 at 0.00, G5 at 0.12, C5 at
        // 0.24 — the power-on's notes at 0.6 amplitude,
        // tighter gates, quicker decay.
        s += visor_note(t, VISOR_C6, 0.13, 0.045, (0.18, 0.072, 0.03));
        s += visor_note(t - 0.12, VISOR_G5, 0.13, 0.045, (0.18, 0.072, 0.03));
        s += visor_note(t - 0.24, VISOR_C5, 0.13, 0.045, (0.18, 0.072, 0.03));
        // The charge sinking, 0.30-0.75: the rise's sweep
        // run backwards, fading as it falls.
        if (0.30..0.75).contains(&t) {
            let tt = t - 0.30;
            let k = tt / 0.45;
            let sink_ratio: f32 = 180.0 / 2350.0;
            let freq = 2350.0 * sink_ratio.powf(k);
            let fade = (1.0 - k) * decay_exp(tt, 0.30);
            let tri = 2.0 * (2.0 * self.sweep_phase.fract() - 1.0).abs() - 1.0;
            s += tri * attack(tt, 0.01) * fade * 0.12;
            s += (self.sub_phase * 2.0 * std::f32::consts::PI).sin()
                * attack(tt, 0.01)
                * fade
                * 0.07;
            let hiss_env = (tt * 10.0).min(1.0) * (1.0 - k);
            s += self.hiss.process(self.rng.next_bipolar()) * hiss_env * 0.04;
            self.sweep_phase = (self.sweep_phase + freq / self.sample_rate) % 1.0;
            self.sub_phase = (self.sub_phase + freq / (2.0 * self.sample_rate)) % 1.0;
        }
        // Relay release click at 0.78.
        let dt = t - 0.78;
        if (0.0..0.08).contains(&dt) {
            s += self.click.next() * decay_exp(dt, 0.010) * 0.35;
            if dt < 0.03 {
                s += self.rng.next_bipolar() * decay_exp(dt, 0.004) * 0.15;
            }
        }
        // The room breath leaving with the power.
        s += self.rng.next_bipolar() * 0.010 * (1.0 - t / self.dur);
        self.t += 1.0 / self.sample_rate;
        Some(s * release_gate(t, self.dur, 0.02))
    }
}

pub fn visor_power_off_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(VisorPowerOff::new(seed, sample_rate))
}

/// A visor tag: the smallest unit of the motif language —
/// one note pair, G5 -> C6, a 0.18 s blip the operator
/// hears when the visor tags a sound. Quiet and dry: no
/// thunk, no rise, no room. It is a punctuation mark, not
/// a sentence. Host-spawned from visor state.
pub struct VisorTag {
    t: f32,
    dur: f32,
    sample_rate: f32,
}

impl VisorTag {
    pub fn new(sample_rate: f32) -> VisorTag {
        VisorTag {
            t: 0.0,
            dur: 0.18,
            sample_rate,
        }
    }
}

impl VoiceGen for VisorTag {
    fn next(&mut self) -> Option<f32> {
        if self.t >= self.dur {
            return None;
        }
        let t = self.t;
        let mut s = 0.0f32;
        for (t0, freq) in [(0.0, VISOR_G5), (0.09, VISOR_C6)] {
            let dt = t - t0;
            if (0.0..0.09).contains(&dt) {
                let e = attack(dt, 0.004) * decay_exp(dt, 0.030) * release_gate(dt, 0.09, 0.01);
                let p = 2.0 * std::f32::consts::PI * freq * dt;
                s += e * (0.16 * p.sin() + 0.05 * (2.0 * p).sin());
            }
        }
        self.t += 1.0 / self.sample_rate;
        Some(s)
    }
}

pub fn visor_tag_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    let _ = seed;
    Box::new(VisorTag::new(sample_rate))
}

/// Mecha ground tremor: felt more than heard — a low
/// rumble at 38 Hz plus lowpassed noise, 1.2 seconds.
pub fn tremor_voice(seed: u64, sample_rate: f32) -> Box<dyn VoiceGen> {
    Box::new(CompositeVoice::new(vec![
        (Box::new(Thump::new(45.0, 38.0, 0.4, 1.2, sample_rate)), 0.7),
        (
            Box::new(NoiseBurst::new(seed, 120.0, 1.2, 0.2, 0.4, sample_rate)),
            0.4,
        ),
    ]))
}
