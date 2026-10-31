//! Deterministic synthesis primitives — the only sound sources
//! in C35. No audio assets exist (zero-asset law), so every
//! voice is built from these pieces: seeded noise, closed-form
//! oscillators, closed-form envelopes, and a one-pole lowpass.
//!
//! Determinism rules, enforced here and nowhere else:
//! - No wall clock, no thread randomness, no HashMap iteration.
//! - Every noise source is a SplitMix64 stream seeded by the
//!   mixer at spawn time; the same spawn sequence gives the
//!   same samples, bit for bit.
//! - Envelopes are closed-form functions of elapsed time, so a
//!   voice's output depends only on its parameters and how
//!   many samples have been rendered — never on when it was
//!   spawned in wall time.

use std::f32::consts::PI;

/// SplitMix64 — a small, well-understood deterministic PRNG.
/// Written from its public description; the constants are the
/// standard ones. State advances only when noise is drawn.
#[derive(Clone, Debug)]
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        // Fold the seed so a zero seed still decorrelates.
        Rng(seed ^ 0x9E3779B97F4A7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) / ((1u32 << 24) as f32)
    }

    /// Uniform in [-1, 1).
    pub fn next_bipolar(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }
}

/// One voice generator: mono samples, or None when finished.
/// A finished voice stays finished — the mixer drops it and
/// never calls it again. Object-safe by construction.
pub trait VoiceGen {
    fn next(&mut self) -> Option<f32>;
}

/// Linear attack ramp: 0 at t = 0, 1 at t >= attack.
pub fn attack(t: f32, attack: f32) -> f32 {
    if attack <= 0.0 {
        1.0
    } else {
        (t / attack).min(1.0)
    }
}

/// Exponential decay: 1 at t = 0, e^-1 at t = tau.
pub fn decay_exp(t: f32, tau: f32) -> f32 {
    if tau <= 0.0 {
        0.0
    } else {
        (-t / tau).exp()
    }
}

/// Release gate: 1 until the last `release` seconds of a
/// fixed-duration voice, then a linear ramp to 0 at `dur`.
pub fn release_gate(t: f32, dur: f32, release: f32) -> f32 {
    if release <= 0.0 || t <= dur - release {
        1.0
    } else {
        ((dur - t) / release).max(0.0)
    }
}

/// One-pole lowpass. `a` is recomputed when the cutoff moves;
/// the state is one sample of memory, so muffling costs
/// nothing and never rings.
#[derive(Clone, Debug)]
pub struct Lowpass {
    a: f32,
    y: f32,
    sample_rate: f32,
}

impl Lowpass {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Lowpass {
        let mut lp = Lowpass {
            a: 1.0,
            y: 0.0,
            sample_rate,
        };
        lp.set_cutoff(cutoff_hz);
        lp
    }

    pub fn set_cutoff(&mut self, cutoff_hz: f32) {
        let fc = cutoff_hz.max(20.0);
        self.a = 1.0 - (-2.0 * PI * fc / self.sample_rate).exp();
    }

    pub fn process(&mut self, x: f32) -> f32 {
        self.y += self.a * (x - self.y);
        self.y
    }

    /// Bypass: pass through unchanged, state untouched.
    pub fn bypass(x: f32) -> f32 {
        x
    }
}

/// Sine oscillator, phase in cycles [0, 1).
#[derive(Clone, Debug)]
pub struct Sine {
    pub phase: f32,
    pub freq: f32,
    pub sample_rate: f32,
}

impl Sine {
    pub fn new(freq: f32, sample_rate: f32) -> Sine {
        Sine {
            phase: 0.0,
            freq,
            sample_rate,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f32 {
        let s = (self.phase * 2.0 * PI).sin();
        self.phase = (self.phase + self.freq / self.sample_rate) % 1.0;
        s
    }
}

/// Sawtooth oscillator, phase in cycles [0, 1).
#[derive(Clone, Debug)]
pub struct Saw {
    pub phase: f32,
    pub freq: f32,
    pub sample_rate: f32,
}

impl Saw {
    pub fn new(freq: f32, sample_rate: f32) -> Saw {
        Saw {
            phase: 0.0,
            freq,
            sample_rate,
        }
    }

    pub fn set_freq(&mut self, freq: f32) {
        self.freq = freq;
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f32 {
        let s = 2.0 * self.phase - 1.0;
        self.phase = (self.phase + self.freq / self.sample_rate) % 1.0;
        s
    }
}

/// Pitch-drop oscillator: frequency falls exponentially from
/// `f0` toward `f1` with time constant `tau`. The thump of a
/// footfall, the snap of a line — all pitch drops live here.
#[derive(Clone, Debug)]
pub struct DropOsc {
    pub phase: f32,
    pub f0: f32,
    pub f1: f32,
    pub tau: f32,
    pub sample_rate: f32,
    pub t: f32,
}

impl DropOsc {
    pub fn new(f0: f32, f1: f32, tau: f32, sample_rate: f32) -> DropOsc {
        DropOsc {
            phase: 0.0,
            f0,
            f1,
            tau,
            sample_rate,
            t: 0.0,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f32 {
        let freq = self.f1 + (self.f0 - self.f1) * (-self.t / self.tau).exp();
        let s = (self.phase * 2.0 * PI).sin();
        self.phase = (self.phase + freq / self.sample_rate) % 1.0;
        self.t += 1.0 / self.sample_rate;
        s
    }
}

/// A voice made of several voices summed with gains.
/// Finished parts are skipped; the composite finishes when
/// every part has finished. This is how a footstep is a
/// thump AND gravel AND a splash without a special case.
pub struct CompositeVoice {
    parts: Vec<CompositePart>,
}

struct CompositePart {
    gen: Box<dyn VoiceGen>,
    gain: f32,
    done: bool,
}

impl CompositeVoice {
    pub fn new(parts: Vec<(Box<dyn VoiceGen>, f32)>) -> CompositeVoice {
        CompositeVoice {
            parts: parts
                .into_iter()
                .map(|(gen, gain)| CompositePart {
                    gen,
                    gain,
                    done: false,
                })
                .collect(),
        }
    }
}

impl VoiceGen for CompositeVoice {
    fn next(&mut self) -> Option<f32> {
        let mut sum = 0.0f32;
        let mut any = false;
        for part in self.parts.iter_mut() {
            if part.done {
                continue;
            }
            match part.gen.next() {
                Some(s) => {
                    any = true;
                    sum += s * part.gain;
                }
                None => part.done = true,
            }
        }
        if any {
            Some(sum)
        } else {
            None
        }
    }
}

/// A fixed-duration noise burst through a lowpass, with a
/// closed-form attack + exponential decay envelope.
pub struct NoiseBurst {
    rng: Rng,
    lp: Lowpass,
    t: f32,
    dur: f32,
    attack_s: f32,
    tau: f32,
    sample_rate: f32,
}

impl NoiseBurst {
    pub fn new(
        seed: u64,
        cutoff_hz: f32,
        dur: f32,
        attack_s: f32,
        tau: f32,
        sample_rate: f32,
    ) -> NoiseBurst {
        NoiseBurst {
            rng: Rng::new(seed),
            lp: Lowpass::new(cutoff_hz, sample_rate),
            t: 0.0,
            dur,
            attack_s,
            tau,
            sample_rate,
        }
    }
}

impl VoiceGen for NoiseBurst {
    fn next(&mut self) -> Option<f32> {
        if self.t >= self.dur {
            return None;
        }
        let env = attack(self.t, self.attack_s)
            * decay_exp(self.t, self.tau)
            * release_gate(self.t, self.dur, 0.02);
        let s = self.lp.process(self.rng.next_bipolar()) * env;
        self.t += 1.0 / self.sample_rate;
        Some(s)
    }
}

/// A fixed-duration additive ring: inharmonic partials with a
/// shared exponential decay. Metal rings, severed lines ring,
/// and a strike cracks — all partial sums, no samples.
pub struct Ring {
    partials: Vec<(Sine, f32)>,
    t: f32,
    dur: f32,
    tau: f32,
    sample_rate: f32,
}

impl Ring {
    pub fn new(
        base_freq: f32,
        ratios: &[(f32, f32)],
        dur: f32,
        tau: f32,
        sample_rate: f32,
    ) -> Ring {
        Ring {
            partials: ratios
                .iter()
                .map(|(r, a)| (Sine::new(base_freq * r, sample_rate), *a))
                .collect(),
            t: 0.0,
            dur,
            tau,
            sample_rate,
        }
    }
}

impl VoiceGen for Ring {
    fn next(&mut self) -> Option<f32> {
        if self.t >= self.dur {
            return None;
        }
        let env = decay_exp(self.t, self.tau) * release_gate(self.t, self.dur, 0.02);
        let mut sum = 0.0f32;
        for (osc, amp) in self.partials.iter_mut() {
            sum += osc.next() * *amp;
        }
        self.t += 1.0 / self.sample_rate;
        Some(sum * env)
    }
}

/// A fixed-duration pitch-drop thump: the body of a footfall,
/// a landing, a mantle. Sine drop + exponential decay.
pub struct Thump {
    osc: DropOsc,
    t: f32,
    dur: f32,
    tau: f32,
    sample_rate: f32,
}

impl Thump {
    pub fn new(f0: f32, f1: f32, tau: f32, dur: f32, sample_rate: f32) -> Thump {
        Thump {
            osc: DropOsc::new(f0, f1, tau, sample_rate),
            t: 0.0,
            dur,
            tau,
            sample_rate,
        }
    }
}

impl VoiceGen for Thump {
    fn next(&mut self) -> Option<f32> {
        if self.t >= self.dur {
            return None;
        }
        let env = decay_exp(self.t, self.tau) * release_gate(self.t, self.dur, 0.02);
        let s = self.osc.next() * env;
        self.t += 1.0 / self.sample_rate;
        Some(s)
    }
}
