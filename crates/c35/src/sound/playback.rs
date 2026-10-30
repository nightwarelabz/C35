//! Playback: the layer that makes priced stimuli audible.
//!
//! The division of labor, restated because it is the whole
//! design: traversal PRICES (event magnitudes, surfaces),
//! perception PROPAGATES (falloff, occlusion, district law),
//! playback RENDERS (timbre + gain + pan + muffle). This
//! module never re-prices anything. The gain a voice gets
//! is the heard magnitude propagation delivered; the pan
//! is the heard direction against the listener's facing;
//! the muffle flag is the same segment-blocked answer the
//! light field gave. If it sounds loud, it is loud —
//! because the sim said so, not because playback chose so.
//!
//! Three render paths, kept separate on purpose:
//! - `render_events`: ordinary sounds through room
//!   acoustics (falloff + occlusion).
//! - `render_district`: district stimuli (scream, report,
//!   tremor) — district law, no occlusion, slow falloff.
//! - `render_ambient_churr`: the nightjar — no position,
//!   no propagation, no source marker. It is heard before
//!   it is seen, and the visor never tags it, because
//!   playback never gives it a place.
//!
//! Grapple, servo, and cannon voices are NOT event-log
//! voices: the host spawns them from traversal/mecha state
//! (hook state machine, servo speed, cannon state) through
//! the mixer's `play` with a seed from `next_seed`. They
//! are exported from `voices` for exactly that use.

use crate::perception::field::LightField;
use crate::perception::sound::{propagate_one, DistrictKind, DistrictStimulus};
use crate::sound::footsteps::footstep_voice;
use crate::sound::mixer::{Mixer, SoundLedger, SAMPLE_RATE};
use crate::sound::music::MusicBed;
use crate::sound::voices;
use crate::sound::VoiceKind;
use crate::traversal::events::{StimulusEvent, StimulusKind};
use glam::Vec3;

pub struct Playback {
    mixer: Mixer,
    ear: Vec3,
    facing: Vec3,
    right: Vec3,
    music: MusicBed,
}

impl Playback {
    pub fn new(sample_rate: u32, seed: u64) -> Playback {
        Playback {
            mixer: Mixer::new(sample_rate, seed),
            ear: Vec3::ZERO,
            facing: Vec3::new(0.0, 0.0, -1.0),
            right: Vec3::X,
            music: MusicBed::new(),
        }
    }

    pub fn with_default_rate(seed: u64) -> Playback {
        Playback::new(SAMPLE_RATE, seed)
    }

    /// Place the listener's ears and facing. Right is
    /// facing × up (Y-up world): facing -Z gives right +X.
    pub fn set_listener(&mut self, ear: Vec3, facing: Vec3) {
        self.ear = ear;
        let f = if facing.length_squared() > 1e-12 {
            facing.normalize()
        } else {
            Vec3::new(0.0, 0.0, -1.0)
        };
        self.facing = f;
        self.right = f.cross(Vec3::Y);
        if self.right.length_squared() < 1e-12 {
            self.right = Vec3::X;
        } else {
            self.right = self.right.normalize();
        }
    }

    pub fn mixer(&mut self) -> &mut Mixer {
        &mut self.mixer
    }

    pub fn music(&mut self) -> &mut MusicBed {
        &mut self.music
    }

    /// Attach the music bed's stem slots to the mixer.
    /// (A combined method because the demo cannot hold
    /// `pb.music()` and `pb.mixer()` at once — method
    /// receivers borrow the whole struct, while these two
    /// disjoint fields borrow cleanly inside this impl.)
    pub fn attach_music(&mut self) {
        self.music.attach(&mut self.mixer);
    }

    pub fn ledger_line(&self) -> String {
        self.mixer.ledger().line()
    }

    pub fn ledger(&self) -> &SoundLedger {
        self.mixer.ledger()
    }

    /// Render ordinary events through room acoustics.
    /// Each event is propagated to this ear; unheard events
    /// (out of radius, below threshold) spawn nothing.
    /// Voice timbre comes from kind + surface; gain, pan,
    /// and muffle come from propagation. Nothing here
    /// re-prices.
    pub fn render_events(&mut self, events: &[StimulusEvent], field: &LightField) {
        let sr = self.mixer.sample_rate() as f32;
        for event in events {
            let heard = match propagate_one(event, self.ear, field) {
                Some(h) => h,
                None => continue,
            };
            let voice = self.voice_for_event(event, sr);
            let Some((gen, kind)) = voice else {
                continue;
            };
            let pan = heard.dir.dot(self.right).clamp(-1.0, 1.0);
            let muffled = field.segment_blocked(event.pos, self.ear);
            self.mixer.play(gen, kind, heard.heard, pan, muffled);
        }
    }

    fn voice_for_event(
        &mut self,
        event: &StimulusEvent,
        sr: f32,
    ) -> Option<(Box<dyn crate::sound::synth::VoiceGen>, VoiceKind)> {
        let seed = self.mixer.next_seed();
        let gen = match event.kind {
            StimulusKind::Footstep => footstep_voice(event.surface, seed, sr)?,
            StimulusKind::Strike => voices::strike_voice(event.surface, seed, sr),
            StimulusKind::Landing => voices::landing_voice(seed, sr),
            StimulusKind::Mantle => voices::mantle_voice(seed, sr),
            StimulusKind::Sever => voices::sever_ring_voice(seed, sr),
            StimulusKind::SeamCross => voices::seam_tick_voice(seed, sr),
            StimulusKind::StrikeFail => voices::strike_fail_voice(seed, sr),
        };
        Some((gen, VoiceKind::from_stimulus(event.kind)))
    }

    /// Render a district stimulus by district law: slow
    /// falloff, no occlusion, direction from the source.
    /// A silenced voice apparatus never reaches here — the
    /// governor yields no stimulus, so nothing sounds.
    pub fn render_district(&mut self, stim: &DistrictStimulus) {
        let heard = stim.heard_at(self.ear);
        if heard <= 0.0 {
            return;
        }
        let sr = self.mixer.sample_rate() as f32;
        let seed = self.mixer.next_seed();
        let d = stim.pos - self.ear;
        let pan = if d.length_squared() > 1e-12 {
            d.normalize().dot(self.right).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let (gen, kind) = match stim.kind {
            DistrictKind::Scream => (voices::scream_voice(seed, sr), VoiceKind::Scream),
            DistrictKind::Report => (voices::report_murmur_voice(seed, sr), VoiceKind::Report),
            DistrictKind::Tremor => (voices::tremor_voice(seed, sr), VoiceKind::Tremor),
        };
        // District law: no muffling, ever.
        self.mixer.play(gen, kind, heard, pan, false);
    }

    /// The nightjar: ambient, unlocalized, fixed gentle
    /// gain, center pan, no muffle, no position. The host
    /// calls this when the rare, unexplained churring
    /// occurs — it does not come from the event log, and
    /// it carries no source marker for any visor to tag.
    pub fn render_ambient_churr(&mut self) {
        let sr = self.mixer.sample_rate() as f32;
        let seed = self.mixer.next_seed();
        self.mixer
            .play(voices::churr_voice(seed, sr), VoiceKind::Churr, 0.35, 0.0, false);
    }

    /// Render `frames` stereo frames (interleaved L,R),
    /// advancing the music crossfade by the block's
    /// duration first so stems move with the mix.
    pub fn render(&mut self, frames: usize) -> Vec<f32> {
        let dt = frames as f32 / self.mixer.sample_rate() as f32;
        self.music.update(&mut self.mixer, dt);
        self.mixer.render(frames)
    }
}
