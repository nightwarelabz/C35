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
//! sequence render the same PCM. No wall clock.

use crate::sound::mixer::Mixer;
use crate::sound::synth::{Lowpass, Rng, VoiceGen};
use crate::sound::VoiceKind;

/// Mixer gain at rain = 1. Under 1 so the bed does not eat the bus.
pub const PEAK_GAIN: f32 = 0.55;

/// How long one near-field drop rings, in seconds.
const DRIP_SECONDS: f32 = 0.018;

/// Infinite rain. `next` is always `Some`.
pub struct RainLoop {
    rng: Rng,
    hiss: Lowpass,
    body: Lowpass,
    t: f32,
    next_drip: f32,
    drip_left: f32,
    drip_freq: f32,
    drip_phase: f32,
    sample_rate: f32,
}

impl RainLoop {
    pub fn new(seed: u64, sample_rate: f32) -> RainLoop {
        let mut rng = Rng::new(seed);
        let next_drip = 0.02 + rng.next_f32() * 0.05;
        RainLoop {
            rng,
            hiss: Lowpass::new(1800.0, sample_rate),
            body: Lowpass::new(420.0, sample_rate),
            t: 0.0,
            next_drip,
            drip_left: 0.0,
            drip_freq: 1200.0,
            drip_phase: 0.0,
            sample_rate,
        }
    }
}

impl VoiceGen for RainLoop {
    fn next(&mut self) -> Option<f32> {
        let white = self.rng.next_bipolar();
        let hiss = self.hiss.process(white);
        let body = self.body.process(white);
        let mut drip = 0.0;
        if self.drip_left > 0.0 {
            let env = (self.drip_left / DRIP_SECONDS).clamp(0.0, 1.0);
            drip = (self.drip_phase * std::f32::consts::TAU).sin() * env;
            self.drip_phase = (self.drip_phase + self.drip_freq / self.sample_rate) % 1.0;
            self.drip_left -= 1.0 / self.sample_rate;
        } else if self.t >= self.next_drip {
            self.drip_left = DRIP_SECONDS;
            self.drip_phase = 0.0;
            self.drip_freq = 900.0 + self.rng.next_f32() * 1600.0;
            // A few drops a second up close. The hiss carries the rest.
            self.next_drip = self.t + 0.035 + self.rng.next_f32() * 0.11;
        }
        self.t += 1.0 / self.sample_rate;
        Some((body * 0.62 + hiss * 0.28 + drip * 0.40).clamp(-1.0, 1.0))
    }
}

/// The host-facing bed. One slot, gain tracked to a 0–1 rain rate.
pub struct RainBed {
    attached: bool,
}

impl RainBed {
    pub fn new() -> RainBed {
        RainBed { attached: false }
    }

    pub fn attached(&self) -> bool {
        self.attached
    }

    /// Drive the bed. `rain` is 0 (dry) to 1 (downpour). A weather
    /// reading's rain rate is the intended input; this module does
    /// not depend on it. A dry bed that was never started stays off
    /// the mixer.
    pub fn set_intensity(&mut self, mixer: &mut Mixer, rain: f32) {
        let rain = rain.clamp(0.0, 1.0);
        if rain == 0.0 && !self.attached {
            return;
        }
        if !self.attached {
            let sr = mixer.sample_rate() as f32;
            let seed = mixer.next_seed();
            if !mixer.play_sticky(Box::new(RainLoop::new(seed, sr)), VoiceKind::Rain, 0.0) {
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
    fn one_second_of_rain_is_cheap_on_the_cpu() {
        // A bound, not a benchmark trophy: one second of stereo rain
        // in a debug build stays well under a frame budget's worth of
        // CPU. Release is faster. If this fails, the bed grew a loop
        // it should not have.
        let t0 = std::time::Instant::now();
        let pcm = render(1, 1.0, SAMPLE_RATE as usize);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(pcm.len(), SAMPLE_RATE as usize * 2);
        assert!(ms < 200.0, "{ms} ms for 1s of rain (debug)");
    }
}
