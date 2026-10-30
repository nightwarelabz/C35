//! Procedural rain bed — the sound of weather, with no audio assets.
//!
//! A single sticky mixer voice: filtered noise for the hiss and the
//! body of the rain, plus short decaying ticks for drops hitting
//! near the ear. It never ends. Gain is the only thing the weather
//! reading drives (`0` dry, `1` downpour), scaled by [`PEAK_GAIN`]
//! so a storm sits under a voice instead of replacing it.
//!
//! Dry and never started costs nothing: [`RainBed::set_intensity`]
//! does not attach a slot until the first non-zero rate. Once
//! attached, the slot is sticky — one-shot voices cannot steal it —
//! and a later zero only silences the gain.
//!
//! Deterministic: the same mixer seed and the same intensity
//! sequence render the same PCM. No wall clock. The drip schedule
//! is a `u64` sample counter, not an f32 timer, so a long session
//! does not run fast and then freeze.

use crate::sound::mixer::Mixer;
use crate::sound::synth::{Lowpass, Rng, VoiceGen};
use crate::sound::VoiceKind;

/// Mixer gain at rain = 1. Under 1 so the bed does not eat the bus.
pub const PEAK_GAIN: f32 = 0.55;

/// How long one near-field drop rings, in seconds.
const DRIP_SECONDS: f32 = 0.018;

/// Seconds → samples, at least one. The drip clock is this integer,
/// never an f32 second-counter: past ~256 s an f32 time base runs
/// fast, and past ~512 s (`1/44100` falls below half an ulp) it
/// stops advancing and the drips stop for good.
fn samples_from_seconds(seconds: f32, sample_rate: u32) -> u64 {
    let n = (seconds * sample_rate as f32).round() as u64;
    n.max(1)
}

/// Infinite rain. `next` is always `Some`.
pub struct RainLoop {
    rng: Rng,
    hiss: Lowpass,
    body: Lowpass,
    /// Samples rendered. The only clock.
    sample: u64,
    /// Sample index at which the next drip arms.
    next_drip_sample: u64,
    /// Samples of ring still to play. Zero means silent.
    drip_left: u64,
    drip_len: u64,
    drip_freq: f32,
    /// Phase in cycles, wrapped to [0, 1). It does not grow with
    /// the session, so f32 is exact enough here.
    drip_phase: f32,
    sample_rate: f32,
    rate_u: u32,
    drips_started: u64,
    last_drip_sample: u64,
}

impl RainLoop {
    pub fn new(seed: u64, sample_rate: f32) -> RainLoop {
        let rate_u = sample_rate.max(1.0) as u32;
        let mut rng = Rng::new(seed);
        let first = 0.02 + rng.next_f32() * 0.05;
        RainLoop {
            rng,
            hiss: Lowpass::new(1800.0, sample_rate),
            body: Lowpass::new(420.0, sample_rate),
            sample: 0,
            next_drip_sample: samples_from_seconds(first, rate_u),
            drip_left: 0,
            drip_len: samples_from_seconds(DRIP_SECONDS, rate_u),
            drip_freq: 1200.0,
            drip_phase: 0.0,
            sample_rate,
            rate_u,
            drips_started: 0,
            last_drip_sample: 0,
        }
    }

    /// How many drips have armed. The ten-minute test reads this.
    pub fn drips_started(&self) -> u64 {
        self.drips_started
    }

    /// Sample index of the most recent drip arm, or 0 if none yet.
    pub fn last_drip_sample(&self) -> u64 {
        self.last_drip_sample
    }
}

impl VoiceGen for RainLoop {
    fn next(&mut self) -> Option<f32> {
        let white = self.rng.next_bipolar();
        let hiss = self.hiss.process(white);
        let body = self.body.process(white);
        let mut drip = 0.0;
        if self.drip_left > 0 {
            let env = (self.drip_left as f32 / self.drip_len as f32).clamp(0.0, 1.0);
            drip = (self.drip_phase * std::f32::consts::TAU).sin() * env;
            self.drip_phase = (self.drip_phase + self.drip_freq / self.sample_rate) % 1.0;
            self.drip_left -= 1;
        } else if self.sample >= self.next_drip_sample {
            // Arm on this sample; the ring is audible from the next
            // one (sin of phase 0 is silence anyway). The gap until
            // the following drip is a sample countdown from here.
            self.drip_left = self.drip_len;
            self.drip_phase = 0.0;
            self.drip_freq = 900.0 + self.rng.next_f32() * 1600.0;
            let gap = 0.035 + self.rng.next_f32() * 0.11;
            self.next_drip_sample = self.sample + samples_from_seconds(gap, self.rate_u);
            self.drips_started += 1;
            self.last_drip_sample = self.sample;
        }
        self.sample += 1;
        Some((body * 0.62 + hiss * 0.28 + drip * 0.40).clamp(-1.0, 1.0))
    }
}

/// The host-facing bed. One slot, gain tracked to a 0–1 rain rate.
pub struct RainBed {
    attached: bool,
    /// `play_sticky` was refused because the bus was full. Latched:
    /// later calls do not retry, so a full mixer does not record a
    /// fresh drop every frame.
    refused: bool,
}

impl RainBed {
    pub fn new() -> RainBed {
        RainBed {
            attached: false,
            refused: false,
        }
    }

    pub fn attached(&self) -> bool {
        self.attached
    }

    /// True once the bus has refused the slot. Stays true.
    pub fn refused(&self) -> bool {
        self.refused
    }

    /// Drive the bed. `rain` is 0 (dry) to 1 (downpour). A weather
    /// reading's rain rate is the intended input; this module does
    /// not depend on it. A dry bed that was never started stays off
    /// the mixer.
    pub fn set_intensity(&mut self, mixer: &mut Mixer, rain: f32) {
        let rain = rain.clamp(0.0, 1.0);
        if self.refused {
            return;
        }
        if rain == 0.0 && !self.attached {
            return;
        }
        if !self.attached {
            let sr = mixer.sample_rate() as f32;
            let seed = mixer.next_seed();
            if !mixer.play_sticky(Box::new(RainLoop::new(seed, sr)), VoiceKind::Rain, 0.0) {
                self.refused = true;
                return;
            }
            self.attached = true;
        }
        mixer.set_kind_gain(VoiceKind::Rain, rain * PEAK_GAIN);
    }
}

impl Default for RainBed {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound::mixer::{Mixer, MAX_VOICES, SAMPLE_RATE};
    use crate::sound::playback::Playback;

    fn rms(pcm: &[f32]) -> f32 {
        if pcm.is_empty() {
            return 0.0;
        }
        let sum: f32 = pcm.iter().map(|s| s * s).sum();
        (sum / pcm.len() as f32).sqrt()
    }

    fn render(seed: u64, rain: f32, frames: usize) -> Vec<f32> {
        let mut mixer = Mixer::new(SAMPLE_RATE, seed);
        let mut bed = RainBed::new();
        bed.set_intensity(&mut mixer, rain);
        mixer.render(frames)
    }

    #[test]
    fn dry_never_started_is_silence_and_no_slot() {
        let mut pb = Playback::with_default_rate(3);
        pb.set_rain(0.0);
        let pcm = pb.render(256);
        assert!(pcm.iter().all(|s| *s == 0.0));
        assert_eq!(pb.ledger().started[VoiceKind::Rain as usize], 0);
    }

    #[test]
    fn the_same_seed_and_rate_render_the_same_pcm() {
        let a = render(11, 0.7, 2048);
        let b = render(11, 0.7, 2048);
        assert_eq!(a, b);
        assert!(a.iter().any(|s| s.abs() > 0.001));
    }

    #[test]
    fn a_different_seed_is_a_different_rain() {
        let a = render(11, 1.0, 1024);
        let b = render(12, 1.0, 1024);
        assert_ne!(a, b);
    }

    #[test]
    fn louder_rain_is_louder_and_everything_stays_in_range() {
        let dry = render(4, 0.0, 4096);
        let drizzle = render(4, 0.25, 4096);
        let storm = render(4, 1.0, 4096);
        assert_eq!(rms(&dry), 0.0);
        let d = rms(&drizzle);
        let s = rms(&storm);
        assert!(d > 0.0, "{d}");
        assert!(s > d * 1.5, "storm {s} vs drizzle {d}");
        for s in storm.iter().chain(drizzle.iter()) {
            assert!(s.abs() <= 1.0);
        }
    }

    #[test]
    fn zero_after_a_storm_silences_without_dropping_the_slot() {
        let mut mixer = Mixer::new(SAMPLE_RATE, 9);
        let mut bed = RainBed::new();
        bed.set_intensity(&mut mixer, 1.0);
        assert_eq!(mixer.ledger().started[VoiceKind::Rain as usize], 1);
        bed.set_intensity(&mut mixer, 0.0);
        let pcm = mixer.render(512);
        assert!(pcm.iter().all(|s| *s == 0.0));
        assert!(mixer.set_kind_gain(VoiceKind::Rain, PEAK_GAIN));
    }

    #[test]
    fn one_shots_cannot_steal_the_bed() {
        let mut mixer = Mixer::new(SAMPLE_RATE, 2);
        let mut bed = RainBed::new();
        bed.set_intensity(&mut mixer, 0.8);
        for i in 0..MAX_VOICES + 8 {
            mixer.play(
                Box::new(RainLoop::new(100 + i as u64, SAMPLE_RATE as f32)),
                VoiceKind::Footstep,
                0.01,
                0.0,
                false,
            );
        }
        assert!(mixer.ledger().stolen > 0);
        assert_eq!(mixer.active_voices(), MAX_VOICES);
        assert!(mixer.set_kind_gain(VoiceKind::Rain, 0.2));
        let line = mixer.ledger().line();
        assert!(line.contains("rain="), "{line}");
    }

    #[test]
    fn a_full_bus_is_refused_once() {
        use crate::sound::StemKind;
        let mut mixer = Mixer::new(SAMPLE_RATE, 4);
        for i in 0..MAX_VOICES {
            mixer.play_stem(
                StemKind::Stealth,
                Box::new(RainLoop::new(50 + i as u64, SAMPLE_RATE as f32)),
            );
        }
        assert_eq!(mixer.ledger().dropped, 0);
        let mut bed = RainBed::new();
        for _ in 0..40 {
            bed.set_intensity(&mut mixer, 1.0);
        }
        assert!(bed.refused());
        assert!(!bed.attached());
        assert_eq!(mixer.ledger().dropped, 1);
    }

    #[test]
    fn drips_continue_through_ten_minutes() {
        let rate = SAMPLE_RATE;
        let mut rain = RainLoop::new(5, rate as f32);
        let total = rate as u64 * 60 * 10;
        let mut n = 0u64;
        while n < total {
            let _ = rain.next();
            n += 1;
        }
        let started = rain.drips_started();
        // A few drops a second across the whole run, not a burst
        // at the start and then silence.
        assert!(
            (2_000..20_000).contains(&started),
            "drips in 10 min: {started}"
        );
        let tail = total - rate as u64 * 30;
        assert!(
            rain.last_drip_sample() >= tail,
            "last drip at sample {}, wanted >= {tail}",
            rain.last_drip_sample()
        );
    }
}
