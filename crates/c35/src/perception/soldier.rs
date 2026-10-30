//! The soldier's brain — a state machine over postures.
//!
//! The soldier is a conscript: he patrols, he hesitates,
//! he can be wrong, he degrades in rain and cold and fear,
//! and he fears the mecha. His states are carried in
//! posture and sound, never icons — the host reads
//! [`SoldierOutput::posture`] and [`SoldierOutput::voice`]
//! and plays them, and the player reads the soldier the
//! same way the soldier reads the player: by how he
//! stands and what he says.
//!
//! The checkpoint HALT verb is states, not a script:
//! Question → Detention → Force, graduated on papers,
//! compliance, and a timer. A checkpoint that jumps
//! straight to force is a broken checkpoint, and a
//! checkpoint that never reaches force is a decoration.

use crate::perception::pricing::{Attention, Memory, Posture};
use crate::perception::sound::HeardEvent;
use glam::Vec3;

/// What a soldier's body is saying. These are animation
/// and audio cues, not HUD states — the host maps them
/// to the pose vocabulary and the voice lines, and the
/// player reads them at patrol distance.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PostureSignal {
    /// Patrol gait, weapon lowered, at ease.
    AtEase,
    /// Head up, weapon half-raised, looking toward.
    Alert,
    /// Weapon raised, aimed at the last known position.
    Aiming,
    /// Sweeping a search pattern, weapon ready.
    Searching,
    /// Stepping back out of a mecha's arc, weapon down.
    Yielding,
    /// Hand up, voice raised: HALT.
    Questioning,
    /// Hands on the detainee, escorting.
    Detaining,
}

/// What a soldier's voice is saying. Voice lines are data
/// (the host voices them); the brain decides the line,
/// never the audio. A silent brain is a broken brain —
/// states must be audible, or the player cannot read them.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum VoiceLine {
    Halt,
    Papers,
    StepAside,
    SearchCall,
    ContactCall,
    StandDown,
    YieldCall,
}

/// The soldier's state. Checkpoint is a state with its own
/// graduation inside it; BodyFound is a state that ends
/// in Alarm (the body is reported, the district hears).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SoldierState {
    Patrol,
    Suspicious,
    Search,
    Alarmed,
    Combat,
    Checkpoint,
    BodyFound,
}

/// The checkpoint's graduation. Question checks papers;
/// Detention holds a failed check; Force answers
/// non-compliance or flight. Each phase has a timer —
/// a phase that never advances is a stuck soldier.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CheckpointPhase {
    Question,
    Detention,
    Force,
}

/// Papers, as the checkpoint sees them: valid, invalid,
/// or absent. The brain does not judge forgery quality —
/// the host's papers system does. The brain judges the
/// verdict it is handed.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum PapersState {
    #[default]
    Valid,
    Invalid,
    Absent,
}

/// One tick of everything the soldier can perceive.
/// The host fills this from the fields — light sample,
/// propagated sound, body discovery, mecha proximity —
/// and the brain decides. Nothing in here is a decision;
/// everything in here is a price.
#[derive(Clone, Debug, Default)]
pub struct SoldierInput {
    /// Priced visibility of the target (light × motion ×
    /// posture), or 0.0 when the target is out of sight.
    pub visibility: f32,
    /// Target position when visible (for memory).
    pub target_pos: Option<Vec3>,
    /// Target posture when visible (for the read).
    pub target_posture: Option<Posture>,
    /// Sound events that reached this soldier's ears
    /// this tick (already propagated by sound.rs).
    pub heard: Vec<HeardEvent>,
    /// A body was found in view this tick.
    pub body_seen: bool,
    /// Checkpoint mode: the soldier is ON a checkpoint
    /// and a subject stands before him.
    pub at_checkpoint: bool,
    /// The subject's papers verdict.
    pub papers: PapersState,
    /// The subject complies (stands, answers, waits).
    pub compliant: bool,
    /// A mecha is near enough to fear.
    pub mecha_near: bool,
    /// The soldier stands inside the mecha's arc.
    pub mecha_in_arc: bool,
    /// Grace at the soldier's feet (slows certainty).
    pub grace: f32,
    /// Rain 0.0–1.0, cold 0.0–1.0: degradation terms.
    pub rain: f32,
    pub cold: f32,
}

/// What the brain decided this tick: state, posture,
/// voice, and where to move (None = hold position).
/// Movement is an intent — the host's navmesh walks it.
#[derive(Clone, Debug)]
pub struct SoldierOutput {
    pub state: SoldierState,
    pub posture: PostureSignal,
    pub voice: Option<VoiceLine>,
    pub move_to: Option<Vec3>,
    /// True while yielding out of a mecha's arc: the
    /// soldier's duty pauses, his fear does not.
    pub yielding: bool,
}

/// The soldier's brain: state, attention, memory, route,
/// checkpoint phase, and the timers that keep phases
/// honest. Ticks at the fixed step; the host calls
/// [`SoldierBrain::tick`] once per tick with a priced
/// input and plays the output.
pub struct SoldierBrain {
    pub state: SoldierState,
    pub attention: Attention,
    pub memory: Memory,
    pub pos: Vec3,
    /// Patrol route waypoints, walked in order.
    pub route: Vec<Vec3>,
    pub route_i: usize,
    pub checkpoint_phase: CheckpointPhase,
    /// Ticks spent in the current checkpoint phase.
    pub checkpoint_ticks: u64,
    /// Ticks spent in the current state.
    pub ticks_in_state: u64,
    /// Fear level 0.0–1.0: rises near mechas, decays.
    pub fear: f32,
    /// Perceiver id for the deterministic wobble.
    pub id: u64,
    pub tick: u64,
}

impl SoldierBrain {
    pub fn new(id: u64, pos: Vec3, route: Vec<Vec3>) -> SoldierBrain {
        SoldierBrain {
            state: SoldierState::Patrol,
            attention: Attention::soldier(),
            memory: Memory::new(),
            pos,
            route,
            route_i: 0,
            checkpoint_phase: CheckpointPhase::Question,
            checkpoint_ticks: 0,
            ticks_in_state: 0,
            fear: 0.0,
            id,
            tick: 0,
        }
    }

    fn set_state(&mut self, s: SoldierState) {
        if self.state != s {
            self.state = s;
            self.ticks_in_state = 0;
            if s == SoldierState::Checkpoint {
                self.checkpoint_phase = CheckpointPhase::Question;
                self.checkpoint_ticks = 0;
            }
        }
    }

    /// One fixed-step tick. Order of business, every tick:
    /// fear first (it degrades everything after it),
    /// checkpoint second (a checkpoint overrides patrol
    /// sense — the soldier's eyes are on the subject),
    /// sight third, sound fourth, memory decay fifth,
    /// state transitions last. A brain that checks sound
    /// before fear is a brain that hears clearly while
    /// terrified, and that is not a conscript.
    pub fn tick(&mut self, input: SoldierInput) -> SoldierOutput {
        self.tick += 1;
        self.ticks_in_state += 1;

        // Fear: rises near a mecha, decays away from one.
        // Fear degrades attention gain and is itself a
        // state the player reads (yielding posture).
        if input.mecha_near {
            self.fear = (self.fear + 0.01).min(1.0);
        } else {
            self.fear = (self.fear - 0.002).max(0.0);
        }
        let yielding = input.mecha_near && input.mecha_in_arc;
        if yielding {
            // Yielding pauses duty but not sense: the
            // soldier still sees, he just steps aside.
            // Fall through to sense below with the yield
            // flag carried in the output.
        }

        // Gain: degradation terms multiply attention gain
        // down. Rain, cold, fear, grace — one multiplier,
        // never special cases.
        let gain = (1.0 - 0.3 * input.rain - 0.2 * input.cold - 0.3 * self.fear)
            .max(0.2)
            * (1.0 - 0.7 * input.grace);

        // Checkpoint overrides patrol sense while engaged.
        if input.at_checkpoint
            && matches!(
                self.state,
                SoldierState::Patrol
                    | SoldierState::Suspicious
                    | SoldierState::Checkpoint
            )
        {
            self.set_state(SoldierState::Checkpoint);
            return self.tick_checkpoint(&input, yielding);
        }

        // Sight: priced visibility, wobbled (a conscript
        // can be wrong), accumulated into attention, and
        // remembered as a last known position.
        if input.visibility > 0.0 {
            if let Some(p) = input.target_pos {
                let wobbled =
                    input.visibility * (1.0 + crate::perception::pricing::wobble(self.tick, self.id));
                self.attention.observe(wobbled, gain);
                if wobbled > self.attention.notice {
                    self.memory.see(p);
                }
            }
        } else {
            self.attention.observe(0.0, gain);
        }

        // Sound: the loudest heard event this tick is a
        // score like a sighting, and its position is a
        // memory — a soldier searches toward a sound he
        // cannot see, and that is the whole point of
        // throwing weight.
        let mut loudest: Option<&HeardEvent> = None;
        for h in &input.heard {
            let better = match loudest {
                None => true,
                Some(l) => h.heard > l.heard,
            };
            if better {
                loudest = Some(h);
            }
        }
        if let Some(h) = loudest {
            if h.heard > 0.3 {
                self.attention.observe(h.heard, gain);
                if self.memory.lkp.is_none() {
                    self.memory.see(h.pos);
                }
            }
        }

        // Body found: confirmation, not accumulation. A
        // body in view ends all doubt — and the report
        // that follows is the district's business.
        if input.body_seen {
            self.attention.confirm();
            self.set_state(SoldierState::BodyFound);
            return SoldierOutput {
                state: self.state,
                posture: PostureSignal::Alert,
                voice: Some(VoiceLine::ContactCall),
                move_to: None,
                yielding,
            };
        }

        // Memory decay: a search that runs out of memory
        // returns to patrol. No memory, no search — a
        // soldier who searches forever is a script.
        let memory_live = if self.memory.lkp.is_some() {
            self.memory.decay_tick()
        } else {
            false
        };

        // Transitions.
        match self.state {
            SoldierState::Patrol => {
                if self.attention.is_alarmed() {
                    self.set_state(SoldierState::Alarmed);
                } else if self.attention.is_suspicious() {
                    self.set_state(SoldierState::Suspicious);
                }
            }
            SoldierState::Suspicious => {
                if self.attention.is_alarmed() {
                    self.set_state(SoldierState::Alarmed);
                } else if !self.attention.is_suspicious() {
                    self.set_state(SoldierState::Patrol);
                } else if memory_live {
                    self.set_state(SoldierState::Search);
                }
            }
            SoldierState::Search => {
                if self.attention.is_alarmed() {
                    self.set_state(SoldierState::Alarmed);
                } else if !memory_live {
                    self.set_state(SoldierState::Patrol);
                }
            }
            SoldierState::Alarmed => {
                // Alarmed holds until attention fully
                // decays AND memory is spent: an alarmed
                // soldier does not shrug and go back to
                // patrol because one tick went quiet.
                if self.attention.level < 0.1 && !memory_live {
                    self.set_state(SoldierState::Patrol);
                }
            }
            SoldierState::Combat => {
                if self.attention.level < 0.1 && !memory_live {
                    self.set_state(SoldierState::Patrol);
                }
            }
            SoldierState::BodyFound => {
                // One tick of BodyFound, then Alarm: the
                // body is reported and the district hears.
                // The host reads the BodyFound output and
                // raises the report; the brain moves on.
                self.set_state(SoldierState::Alarmed);
            }
            SoldierState::Checkpoint => {
                // Left checkpoint mode (subject gone):
                // back to patrol, phase reset by set_state.
                self.set_state(SoldierState::Patrol);
            }
        }

        // Combat entry: alarmed AND target visible AND
        // close is the host's call to make (range lives
        // in the weapon layer), but the brain reports the
        // posture: alarmed with a live sighting aims.
        let (posture, voice, move_to) = match self.state {
            SoldierState::Patrol => {
                let next = self.route.get(self.route_i).copied();
                (PostureSignal::AtEase, None, next)
            }
            SoldierState::Suspicious => {
                (PostureSignal::Alert, None, self.memory.lkp)
            }
            SoldierState::Search => (
                PostureSignal::Searching,
                if self.ticks_in_state == 1 {
                    Some(VoiceLine::SearchCall)
                } else {
                    None
                },
                self.memory.lkp,
            ),
            SoldierState::Alarmed => (
                PostureSignal::Aiming,
                if self.ticks_in_state == 1 {
                    Some(VoiceLine::ContactCall)
                } else {
                    None
                },
                self.memory.lkp,
            ),
            SoldierState::Combat => (PostureSignal::Aiming, None, self.memory.lkp),
            SoldierState::BodyFound => (PostureSignal::Alert, Some(VoiceLine::ContactCall), None),
            SoldierState::Checkpoint => (PostureSignal::Questioning, None, None),
        };

        SoldierOutput {
            state: self.state,
            posture,
            voice,
            move_to,
            yielding,
        }
    }

    /// The checkpoint graduation, ticked while engaged.
    /// Question (papers, ~2 s) → Detention (failed check,
    /// ~4 s) → Force (non-compliance or flight). Valid
    /// papers release immediately; compliance holds the
    /// phase; flight or refusal advances it. A phase that
    /// never advances is a stuck soldier, so every phase
    /// has a timer and every timer has an exit.
    fn tick_checkpoint(&mut self, input: &SoldierInput, yielding: bool) -> SoldierOutput {
        self.checkpoint_ticks += 1;
        // 240 Hz fixed step: 2 s = 480 ticks, 4 s = 960.
        match self.checkpoint_phase {
            CheckpointPhase::Question => {
                if input.papers == PapersState::Valid {
                    self.set_state(SoldierState::Patrol);
                    return SoldierOutput {
                        state: SoldierState::Patrol,
                        posture: PostureSignal::AtEase,
                        voice: Some(VoiceLine::StandDown),
                        move_to: self.route.get(self.route_i).copied(),
                        yielding,
                    };
                }
                if !input.compliant || self.checkpoint_ticks > 480 {
                    self.checkpoint_phase = CheckpointPhase::Detention;
                    self.checkpoint_ticks = 0;
                }
                SoldierOutput {
                    state: SoldierState::Checkpoint,
                    posture: PostureSignal::Questioning,
                    voice: if self.checkpoint_ticks == 1 {
                        Some(VoiceLine::Halt)
                    } else if self.checkpoint_ticks == 2 {
                        Some(VoiceLine::Papers)
                    } else {
                        None
                    },
                    move_to: None,
                    yielding,
                }
            }
            CheckpointPhase::Detention => {
                if input.papers == PapersState::Valid && input.compliant {
                    self.set_state(SoldierState::Patrol);
                    return SoldierOutput {
                        state: SoldierState::Patrol,
                        posture: PostureSignal::AtEase,
                        voice: Some(VoiceLine::StandDown),
                        move_to: self.route.get(self.route_i).copied(),
                        yielding,
                    };
                }
                if !input.compliant || self.checkpoint_ticks > 960 {
                    self.checkpoint_phase = CheckpointPhase::Force;
                    self.checkpoint_ticks = 0;
                }
                SoldierOutput {
                    state: SoldierState::Checkpoint,
                    posture: PostureSignal::Detaining,
                    voice: if self.checkpoint_ticks == 1 {
                        Some(VoiceLine::StepAside)
                    } else {
                        None
                    },
                    move_to: None,
                    yielding,
                }
            }
            CheckpointPhase::Force => SoldierOutput {
                state: SoldierState::Checkpoint,
                posture: PostureSignal::Aiming,
                voice: Some(VoiceLine::ContactCall),
                move_to: None,
                yielding,
            },
        }
    }
}
