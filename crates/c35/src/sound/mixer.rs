//! The mixer: a small deterministic software mixer in the
//! SoLoud SHAPE (voices, buses, per-voice gain/pan/filter) —
//! studied, never copied, and written from scratch in-house.
//! No new dependency; the whole bus is a few hundred lines.
//!
//! The bus contract:
//! - Bounded voices: MAX_VOICES slots, preallocated. The
//!   render loop never allocates; spawn allocates one Box,
//!   at event time, never per frame.
//! - Slot order is mix order. Voices are summed in slot
//!   order every frame, so the same spawn sequence renders
//!   the same PCM, bit for bit.
//! - Stealing is deterministic: the quietest non-stem slot
//!   goes, ties broken by lowest slot index. A stolen voice
//!   is counted in the ledger, never silently.
//! - Music stems are persistent slots that stealing never
//!   touches; the bed owns their gains.
//! - Occlusion muffles: an occluded voice passes through a
//!   700 Hz one-pole lowpass before panning. Sound leaks
//!   through walls (the propagation layer already cut it
//!   to a quarter) — it does not stop at them.
//! - The master bus soft-clips with tanh after a 0.8 gain,
//!   so a loud street never hard-clips into a square wave.

use crate::sound::synth::{Lowpass, VoiceGen};
use crate::sound::{StemKind, VoiceKind};

/// Hard bound on simultaneous voices, stems included.
/// 32 voices of cheap oscillator math is negligible next
/// to a frame of rendering; the bound is what keeps it
/// negligible no matter how loud the street gets.
pub const MAX_VOICES: usize = 32;

/// Sample rate for everything the mixer renders. 44100 Hz:
/// standard, and high enough that no voice here aliases
/// audibly at its partial frequencies.
pub const SAMPLE_RATE: u32 = 44100;

/// Cutoff for an occluded (muffled) voice, in Hz.
pub const MUFFLE_CUTOFF_HZ: f32 = 700.0;

/// The sound section of the run ledger. The host owns the
/// run ledger; this struct is the sound page of it, and the
/// host appends `line()` to its ledger output. Counts are
/// per voice kind, so a run's sound is auditable the same
/// way its triangles are.
#[derive(Clone, Debug, Default)]
pub struct SoundLedger {
    pub started: [u32; VoiceKind::COUNT],
    pub stolen: u32,
    pub dropped: u32,
    pub peak: f32,
    pub frames: u64,
}

impl SoundLedger {
    pub fn record_start(&mut self, kind: VoiceKind) {
        self.started[kind as usize] += 1;
    }

    pub fn record_steal(&mut self) {
        self.stolen += 1;
    }

    pub fn record_drop(&mut self) {
        self.dropped += 1;
    }

    pub fn record_peak(&mut self, level: f32) {
        if level > self.peak {
            self.peak = level;
        }
    }

    /// One ledger line. Only kinds that actually sounded
    /// are listed, so a quiet run's line stays short.
    pub fn line(&self) -> String {
        let mut kinds = String::new();
        for (i, count) in self.started.iter().enumerate() {
            if *count > 0 {
                let kind = VoiceKind::from_usize(i);
                kinds.push_str(&format!(" {}={}", kind.name(), count));
            }
        }
        format!(
            "sound frames={} peak={:.3} stolen={} dropped={}{}",
            self.frames, self.peak, self.stolen, self.dropped, kinds
        )
    }
}

struct Slot {
    gen: Box<dyn VoiceGen>,
    gain: f32,
    pan: f32,
    muffled: bool,
    lp: Lowpass,
    stem: Option<StemKind>,
    kind: VoiceKind,
    /// Sticky slots (the rain bed) are never stolen. Stems are
    /// protected separately, by `stem`.
    sticky: bool,
    dead: bool,
}

pub struct Mixer {
    sample_rate: u32,
    master: f32,
    slots: Vec<Slot>,
    seed_base: u64,
    seed_counter: u64,
    ledger: SoundLedger,
}

impl Mixer {
    pub fn new(sample_rate: u32, seed: u64) -> Mixer {
        Mixer {
            sample_rate,
            master: 0.8,
            slots: Vec::with_capacity(MAX_VOICES),
            seed_base: seed,
            seed_counter: 0,
            ledger: SoundLedger::default(),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Deterministic per-spawn seed. The host (or playback)
    /// asks for one seed per voice it constructs; the same
    /// spawn sequence yields the same seeds, hence the same
    /// noise, hence the same mix.
    pub fn next_seed(&mut self) -> u64 {
        self.seed_counter += 1;
        self.seed_base ^ self.seed_counter.wrapping_mul(0x9E3779B97F4A7C15)
    }

    /// Spawn a one-shot voice. If every slot is busy, the
    /// quietest non-stem, non-sticky slot is stolen (ties:
    /// lowest index); if every slot is protected, the voice
    /// is dropped and counted — never a crash, never a hang.
    pub fn play(
        &mut self,
        gen: Box<dyn VoiceGen>,
        kind: VoiceKind,
        gain: f32,
        pan: f32,
        muffled: bool,
    ) {
        let gain = gain.clamp(0.0, 2.0);
        let pan = pan.clamp(-1.0, 1.0);
        let slot = Slot {
            gen,
            gain,
            pan,
            muffled,
            lp: Lowpass::new(MUFFLE_CUTOFF_HZ, self.sample_rate as f32),
            stem: None,
            kind,
            sticky: false,
            dead: false,
        };
        if self.slots.len() < MAX_VOICES {
            self.slots.push(slot);
            self.ledger.record_start(kind);
            return;
        }
        // Steal the quietest unprotected slot, lowest index on ties.
        let mut victim: Option<usize> = None;
        let mut victim_gain = f32::INFINITY;
        for (i, s) in self.slots.iter().enumerate() {
            if s.stem.is_some() || s.sticky {
                continue;
            }
            if s.gain < victim_gain {
                victim_gain = s.gain;
                victim = Some(i);
            }
        }
        match victim {
            Some(i) => {
                self.slots[i] = slot;
                self.ledger.record_start(kind);
                self.ledger.record_steal();
            }
            None => {
                self.ledger.record_drop();
            }
        }
    }

    /// Spawn a persistent music stem. Stems are never
    /// stolen; if the bus is full of stems, the spawn is
    /// dropped and counted, same as any other voice.
    pub fn play_stem(&mut self, stem: StemKind, gen: Box<dyn VoiceGen>) {
        if self.slots.len() >= MAX_VOICES {
            self.ledger.record_drop();
            return;
        }
        self.slots.push(Slot {
            gen,
            gain: 0.0,
            pan: 0.0,
            muffled: false,
            lp: Lowpass::new(MUFFLE_CUTOFF_HZ, self.sample_rate as f32),
            stem: Some(stem),
            kind: VoiceKind::Music,
            sticky: false,
            dead: false,
        });
        self.ledger.record_start(VoiceKind::Music);
    }

    /// Spawn a voice that stealing never takes (the rain bed).
    /// A second call for the same kind retargets the gain instead
    /// of stacking a slot. Returns false only when the bus is
    /// already full and the voice had to be dropped.
    pub fn play_sticky(&mut self, gen: Box<dyn VoiceGen>, kind: VoiceKind, gain: f32) -> bool {
        let gain = gain.clamp(0.0, 1.0);
        if self
            .slots
            .iter()
            .any(|s| s.sticky && s.kind == kind && !s.dead)
        {
            self.set_kind_gain(kind, gain);
            return true;
        }
        if self.slots.len() >= MAX_VOICES {
            self.ledger.record_drop();
            return false;
        }
        self.slots.push(Slot {
            gen,
            gain,
            pan: 0.0,
            muffled: false,
            lp: Lowpass::new(MUFFLE_CUTOFF_HZ, self.sample_rate as f32),
            stem: None,
            kind,
            sticky: true,
            dead: false,
        });
        self.ledger.record_start(kind);
        true
    }

    /// Set the gain of every sticky slot of `kind`. Returns
    /// whether one was found. One-shots of the same kind are
    /// left alone.
    pub fn set_kind_gain(&mut self, kind: VoiceKind, gain: f32) -> bool {
        let gain = gain.clamp(0.0, 1.0);
        let mut found = false;
        for slot in self.slots.iter_mut() {
            if slot.sticky && slot.kind == kind && !slot.dead {
                slot.gain = gain;
                found = true;
            }
        }
        found
    }

    /// Set a stem's gain (the bed's crossfade writes here).
    /// No stem slot, no effect — the bed attaches first.
    pub fn set_stem_gain(&mut self, stem: StemKind, gain: f32) {
        for slot in self.slots.iter_mut() {
            if slot.stem == Some(stem) {
                slot.gain = gain.clamp(0.0, 1.0);
            }
        }
    }

    pub fn stem_attached(&self, stem: StemKind) -> bool {
        self.slots.iter().any(|s| s.stem == Some(stem))
    }

    pub fn active_voices(&self) -> usize {
        self.slots.iter().filter(|s| !s.dead).count()
    }

    pub fn ledger(&self) -> &SoundLedger {
        &self.ledger
    }

    /// Render `frames` stereo frames, interleaved L,R.
    /// Slot order is mix order; dead slots are swept only
    /// after the call, so a voice that finishes mid-call
    /// still mixes in slot order for the whole call.
    pub fn render(&mut self, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; frames * 2];
        for f in 0..frames {
            let mut left = 0.0f32;
            let mut right = 0.0f32;
            for slot in self.slots.iter_mut() {
                if slot.dead {
                    continue;
                }
                let sample = match slot.gen.next() {
                    Some(s) => s,
                    None => {
                        slot.dead = true;
                        continue;
                    }
                };
                let mono = if slot.muffled {
                    slot.lp.process(sample)
                } else {
                    sample
                } * slot.gain;
                // Equal-power pan: pan -1 = hard left, +1 = hard right.
                let angle = (slot.pan + 1.0) * std::f32::consts::FRAC_PI_4;
                left += mono * angle.cos();
                right += mono * angle.sin();
            }
            // Master bus: 0.8 gain, tanh soft clip.
            let l = (left * self.master).tanh();
            let r = (right * self.master).tanh();
            self.ledger.record_peak(left.abs().max(right.abs()));
            out[f * 2] = l;
            out[f * 2 + 1] = r;
        }
        self.ledger.frames += frames as u64;
        self.slots.retain(|s| !s.dead);
        out
    }
}
