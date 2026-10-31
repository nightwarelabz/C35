//! The music bed interface: stems, intensity, crossfade.
//!
//! What this is: the plumbing the shipped score will fill.
//! Four stems — stealth, suspicion, combat, conversation —
//! each a persistent mixer slot whose gain the bed drives.
//! The host (and, when it exists, the op engine driving
//! from alert state) sets the bed's state; the bed
//! crossfades linearly over 2 seconds. Music is a state
//! output of the sim, never a soundtrack pasted over it.
//!
//! What this is NOT: the score. The stem voices here are
//! soft placeholder pads built from the same synth
//! primitives — enough to prove the stem gains, the
//! crossfade, and the bus routing. The shipped score will
//! be original composition against the Greyport reference
//! playlist (the playlist itself is never shippable), and
//! it will replace these pad generators, not this
//! interface. Nothing in the bed hard-codes a melody.

use crate::sound::mixer::Mixer;
use crate::sound::synth::{attack, Sine, VoiceGen};
use crate::sound::StemKind;

/// The bed's state, set by the host. Suspicion and combat
/// are alert states; conversation is the debrief-room
/// state; stealth is the default. The op engine will drive
/// these transitions from alert state when it exists — the
/// bed does not read the alert budget itself, because the
/// mapping from budget to music state is an op-engine
/// decision, not a mixer decision.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MusicState {
    Stealth,
    Suspicion,
    Combat,
    Conversation,
}

impl MusicState {
    /// Target stem gains for a state: [stealth, suspicion,
    /// combat, conversation]. A state never silences the
    /// bed entirely — a low bed under everything keeps the
    /// city from going dead quiet between states.
    pub fn target_mix(&self) -> [f32; 4] {
        match self {
            MusicState::Stealth => [0.50, 0.0, 0.0, 0.0],
            MusicState::Suspicion => [0.20, 0.60, 0.0, 0.0],
            MusicState::Combat => [0.0, 0.20, 0.80, 0.0],
            MusicState::Conversation => [0.20, 0.0, 0.0, 0.50],
        }
    }
}

fn stem_index(stem: StemKind) -> usize {
    match stem {
        StemKind::Stealth => 0,
        StemKind::Suspicion => 1,
        StemKind::Combat => 2,
        StemKind::Conversation => 3,
    }
}

/// A placeholder pad: a soft chord of detuned sines with a
/// slow attack and a gentle amplitude sway. Infinite — it
/// plays until the bed's gain for its stem reaches zero
/// (the mixer still renders it at gain 0, which is free
/// work the bed could stop; see the open note in the
/// report about stem parking).
pub struct PadVoice {
    partials: Vec<Sine>,
    t: f32,
    sample_rate: f32,
}

impl PadVoice {
    pub fn new(freqs: &[f32], sample_rate: f32) -> PadVoice {
        PadVoice {
            partials: freqs.iter().map(|f| Sine::new(*f, sample_rate)).collect(),
            t: 0.0,
            sample_rate,
        }
    }
}

impl VoiceGen for PadVoice {
    fn next(&mut self) -> Option<f32> {
        let env = attack(self.t, 1.5);
        // Gentle 0.1 Hz sway so a held pad never sits dead.
        let sway = 0.85 + 0.15 * (self.t * 2.0 * std::f32::consts::PI * 0.1).sin();
        let mut sum = 0.0f32;
        for osc in self.partials.iter_mut() {
            sum += osc.next();
        }
        self.t += 1.0 / self.sample_rate;
        let n = self.partials.len().max(1) as f32;
        Some(sum / n * env * sway * 0.35)
    }
}

fn placeholder_freqs(stem: StemKind) -> &'static [f32] {
    match stem {
        // Low and dark: A2 + E3.
        StemKind::Stealth => &[110.0, 164.81],
        // Tenser: D3 + A3 + F3 (minor colour).
        StemKind::Suspicion => &[146.83, 220.0, 174.61],
        // Tense and low: C2 + G2 + Bb2 (dissonant flat 7th).
        StemKind::Combat => &[65.41, 98.0, 116.54],
        // Warm major: E3 + G#3 + B3.
        StemKind::Conversation => &[164.81, 207.65, 246.94],
    }
}

pub struct MusicBed {
    state: MusicState,
    from: [f32; 4],
    to: [f32; 4],
    fade_t: f32,
    attached: bool,
}

impl MusicBed {
    pub const FADE_SECONDS: f32 = 2.0;

    pub fn new() -> MusicBed {
        MusicBed {
            state: MusicState::Stealth,
            from: [0.0; 4],
            to: MusicState::Stealth.target_mix(),
            fade_t: 0.0,
            attached: false,
        }
    }

    pub fn state(&self) -> MusicState {
        self.state
    }

    /// Spawn the four stem slots (once) at zero gain.
    /// Call before the first render that should carry music.
    pub fn attach(&mut self, mixer: &mut Mixer) {
        if self.attached {
            return;
        }
        let sr = mixer.sample_rate() as f32;
        for stem in [
            StemKind::Stealth,
            StemKind::Suspicion,
            StemKind::Combat,
            StemKind::Conversation,
        ] {
            if !mixer.stem_attached(stem) {
                mixer.play_stem(stem, Box::new(PadVoice::new(placeholder_freqs(stem), sr)));
            }
        }
        self.attached = true;
    }

    /// Request a state change. The crossfade starts from
    /// whatever the current mix is, so rapid state changes
    /// never click — they just re-aim the fade.
    pub fn set_state(&mut self, state: MusicState) {
        if state == self.state {
            return;
        }
        self.from = self.current_mix();
        self.to = state.target_mix();
        self.fade_t = 0.0;
        self.state = state;
    }

    /// Current mix (the fade's position). Pure function of
    /// from/to/fade_t — no mixer reads, so it is testable
    /// without audio.
    pub fn current_mix(&self) -> [f32; 4] {
        let k = (self.fade_t / Self::FADE_SECONDS).min(1.0);
        let mut out = [0.0; 4];
        for (i, o) in out.iter_mut().enumerate() {
            *o = self.from[i] + (self.to[i] - self.from[i]) * k;
        }
        out
    }

    /// Advance the crossfade by `dt` seconds and write the
    /// stem gains into the mixer. Call once per render
    /// block (or per tick) with the block's duration.
    pub fn update(&mut self, mixer: &mut Mixer, dt: f32) {
        self.fade_t += dt;
        let mix = self.current_mix();
        for stem in [
            StemKind::Stealth,
            StemKind::Suspicion,
            StemKind::Combat,
            StemKind::Conversation,
        ] {
            mixer.set_stem_gain(stem, mix[stem_index(stem)]);
        }
    }
}

impl Default for MusicBed {
    fn default() -> Self {
        Self::new()
    }
}
