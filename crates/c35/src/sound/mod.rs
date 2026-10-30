//! SOUND PLAYBACK — the layer that makes priced stimuli audible.
//!
//! The division of labor, restated because it is the whole
//! design: traversal PRICES (event magnitudes, surfaces),
//! perception PROPAGATES (falloff, occlusion, district law),
//! this module RENDERS (timbre + gain + pan + muffle).
//! Nothing here decides who heard anything or how loud
//! anything was — it makes audible what the fields already
//! said, through a small deterministic software mixer.
//!
//! Two halves on one bus, per the investigation:
//! - The stimulus half is NOT rebuilt here — it already
//!   exists (traversal's EventLog, perception's
//!   propagation). Playback consumes it.
//! - The playback half is this module: mixer, footstep
//!   timbres from the shared surface fields, synthesized
//!   signature voices (zero audio assets), and a music bed
//!   interface (stems + crossfade) that original
//!   composition will fill.
//!
//! Determinism: same seed + same event sequence = same
//! PCM. No wall clock, no thread randomness, no HashMap
//! iteration anywhere in this module. The run ledger
//! records the sound page via `SoundLedger::line()`.

pub mod footsteps;
pub mod mixer;
pub mod music;
pub mod playback;
pub mod rain;
pub mod synth;
pub mod voices;

pub use footsteps::{footstep_voice, from_mod_footstep};
pub use mixer::{Mixer, SoundLedger, MAX_VOICES, SAMPLE_RATE};
pub use music::{MusicBed, MusicState};
pub use playback::Playback;
pub use rain::{RainBed, RainLoop};
pub use synth::{Rng, VoiceGen};
pub use voices::{
    cannon_cool_voice, churr_voice, grapple_detach_voice, grapple_launch_voice,
    grapple_reel_chunk, landing_voice, mantle_voice, report_murmur_voice, scream_voice,
    seam_tick_voice, servo_chunk, sever_ring_voice, strike_fail_voice, strike_voice,
    tremor_voice,
};

use crate::traversal::events::StimulusKind;

/// Voice taxonomy, for the ledger and for voice selection.
/// Discriminants are ledger indices — do not reorder
/// without noting the ledger's history changes with them.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum VoiceKind {
    Footstep = 0,
    Strike = 1,
    Landing = 2,
    Mantle = 3,
    Sever = 4,
    SeamCross = 5,
    StrikeFail = 6,
    GrappleLaunch = 7,
    GrappleReel = 8,
    GrappleDetach = 9,
    Servo = 10,
    CannonCool = 11,
    Scream = 12,
    Report = 13,
    Tremor = 14,
    Churr = 15,
    Music = 16,
    /// Procedural rain bed. Sticky, not a one-shot. Added at the
    /// end so existing ledger indices do not move.
    Rain = 17,
}

impl VoiceKind {
    pub const COUNT: usize = 18;

    pub fn name(&self) -> &'static str {
        match self {
            VoiceKind::Footstep => "footstep",
            VoiceKind::Strike => "strike",
            VoiceKind::Landing => "landing",
            VoiceKind::Mantle => "mantle",
            VoiceKind::Sever => "sever",
            VoiceKind::SeamCross => "seam",
            VoiceKind::StrikeFail => "strikefail",
            VoiceKind::GrappleLaunch => "launch",
            VoiceKind::GrappleReel => "reel",
            VoiceKind::GrappleDetach => "detach",
            VoiceKind::Servo => "servo",
            VoiceKind::CannonCool => "cool",
            VoiceKind::Scream => "scream",
            VoiceKind::Report => "report",
            VoiceKind::Tremor => "tremor",
            VoiceKind::Churr => "churr",
            VoiceKind::Music => "music",
            VoiceKind::Rain => "rain",
        }
    }

    pub fn from_stimulus(kind: StimulusKind) -> VoiceKind {
        match kind {
            StimulusKind::Footstep => VoiceKind::Footstep,
            StimulusKind::Strike => VoiceKind::Strike,
            StimulusKind::Landing => VoiceKind::Landing,
            StimulusKind::Mantle => VoiceKind::Mantle,
            StimulusKind::Sever => VoiceKind::Sever,
            StimulusKind::SeamCross => VoiceKind::SeamCross,
            StimulusKind::StrikeFail => VoiceKind::StrikeFail,
        }
    }

    pub fn from_usize(i: usize) -> VoiceKind {
        match i {
            0 => VoiceKind::Footstep,
            1 => VoiceKind::Strike,
            2 => VoiceKind::Landing,
            3 => VoiceKind::Mantle,
            4 => VoiceKind::Sever,
            5 => VoiceKind::SeamCross,
            6 => VoiceKind::StrikeFail,
            7 => VoiceKind::GrappleLaunch,
            8 => VoiceKind::GrappleReel,
            9 => VoiceKind::GrappleDetach,
            10 => VoiceKind::Servo,
            11 => VoiceKind::CannonCool,
            12 => VoiceKind::Scream,
            13 => VoiceKind::Report,
            14 => VoiceKind::Tremor,
            15 => VoiceKind::Churr,
            16 => VoiceKind::Music,
            17 => VoiceKind::Rain,
            _ => VoiceKind::Music,
        }
    }
}

/// Music stems, in mix order. The bed's target mixes are
/// indexed by these positions.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StemKind {
    Stealth,
    Suspicion,
    Combat,
    Conversation,
}
