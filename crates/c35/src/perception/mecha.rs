//! Mecha governors — fixed tables, not scripts, not minds.
//!
//! Both mechas are AUTONOMOUS analog governors: cams,
//! relays, gyroscopes, punched doctrine. Fixed, learnable
//! stimulus/response tables, threshold machinery with
//! fixed timers, no doubt, no learning. Damage compounds
//! through the governor in fixed, readable ways — a
//! blinded machine leans on audio, and the player learns
//! that lean the same way he learns a patrol route.
//! The opposite of a modern utility-AI, on purpose: a
//! machine that can be learned can be beaten, and a
//! machine that learns cannot be learned.
//!
//! Neither mecha has a health bar. State is read across
//! components through the visor's sense view (visor.rs),
//! never as a number. The Type 1's cannon cannot be
//! destroyed — damage locks it into cooling, it never
//! becomes scrap. The Type 2's voice can be silenced —
//! and silencing it removes the scream, not the charge.

// ----------------------------------------------------------------
// TYPE 1
// ----------------------------------------------------------------

use glam::Vec3;

/// Sensor state: the machine's eyes and ears. Blind is
/// not dead — a blinded machine leans on audio in fixed,
/// readable ways (see audio_reliance).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SensorState {
    Nominal,
    Degraded,
    Blind,
}

/// Locomotion state: legs, gyros, stride. Hobbled is
/// slower and louder; immobilized is a turret, not a
/// corpse — it still sees, still shoots, still reports.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LocoState {
    Nominal,
    Hobbled,
    Immobilized,
}

/// Cannon state: ready, charging (telegraphed), cooling
/// (locked out by damage or by firing). NEVER destroyed
/// — the cannon is the one component the ladder cannot
/// remove, only delay.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CannonState {
    Ready,
    Charging,
    Cooling,
}

/// Power-coolant state: the bare back's exposed loop.
/// Nominal is armored; Exposed is the intended kill path
/// (from above, via grapple); Breached is the kill —
/// the machine stops, it does not explode into a
/// health bar.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CoolantState {
    Nominal,
    Exposed,
    Breached,
}

/// Which component a hit addressed. Damage is addressed,
/// never generic: a hit to "the mecha" is a hit to
/// nothing, and the governor says so by ignoring it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MechaComponent {
    Sensors,
    Locomotion,
    Cannon,
    Coolant,
}

/// The Type 1's governor: four component states, fixed
/// timers, compensation outputs. Damage moves states
/// down fixed tables; timers move cooling back up.
/// Nothing here learns, doubts, or improvises.
pub struct Type1Governor {
    pub sensors: SensorState,
    pub locomotion: LocoState,
    pub cannon: CannonState,
    pub coolant: CoolantState,
    /// Ticks of cannon cooling left (0 = ready).
    pub cannon_cool_ticks: u64,
    /// Ticks of eye sweep remaining (telegraph).
    pub eye_sweep_ticks: u64,
    /// Ticks since the last servo-noise burst.
    pub servo_ticks: u64,
}

impl Default for Type1Governor {
    fn default() -> Self {
        Self::new()
    }
}

impl Type1Governor {
    pub fn new() -> Type1Governor {
        Type1Governor {
            sensors: SensorState::Nominal,
            locomotion: LocoState::Nominal,
            cannon: CannonState::Ready,
            coolant: CoolantState::Nominal,
            cannon_cool_ticks: 0,
            eye_sweep_ticks: 0,
            servo_ticks: 0,
        }
    }

    /// Apply addressed damage. Each component has a
    /// fixed ladder, one step per damaging hit:
    /// sensors Nominal → Degraded → Blind;
    /// locomotion Nominal → Hobbled → Immobilized;
    /// cannon any → Cooling (1200 ticks, ~5 s lockout);
    /// coolant Nominal → Exposed → Breached.
    /// A second hit to an already-terminal state is
    /// wasted — the ladder has no step past the end.
    pub fn damage(&mut self, component: MechaComponent) {
        match component {
            MechaComponent::Sensors => {
                self.sensors = match self.sensors {
                    SensorState::Nominal => SensorState::Degraded,
                    SensorState::Degraded => SensorState::Blind,
                    SensorState::Blind => SensorState::Blind,
                };
            }
            MechaComponent::Locomotion => {
                self.locomotion = match self.locomotion {
                    LocoState::Nominal => LocoState::Hobbled,
                    LocoState::Hobbled => LocoState::Immobilized,
                    LocoState::Immobilized => LocoState::Immobilized,
                };
            }
            MechaComponent::Cannon => {
                // Never destroyed: damage locks cooling.
                self.cannon = CannonState::Cooling;
                self.cannon_cool_ticks = 1200;
            }
            MechaComponent::Coolant => {
                self.coolant = match self.coolant {
                    CoolantState::Nominal => CoolantState::Exposed,
                    CoolantState::Exposed => CoolantState::Breached,
                    CoolantState::Breached => CoolantState::Breached,
                };
            }
        }
    }

    /// The kill ladder, as a stage the player can read:
    /// avoid (untouched) → blind/deafen (sensors down)
    /// → hobble (locomotion down) → kill (coolant
    /// breached). A stage is a summary of states, never
    /// a separate number — the ladder IS the components.
    pub fn ladder_stage(&self) -> &'static str {
        if self.coolant == CoolantState::Breached {
            "kill"
        } else if self.locomotion != LocoState::Nominal {
            "hobble"
        } else if self.sensors != SensorState::Nominal {
            "blind/deafen"
        } else {
            "avoid"
        }
    }

    /// Is the machine dead? Only coolant breach kills.
    /// Everything else is a wounded machine, still in
    /// the fight, still readable.
    pub fn is_dead(&self) -> bool {
        self.coolant == CoolantState::Breached
    }

    /// Audio reliance: how much the machine leans on
    /// sound over sight. Nominal 0.3, degraded 0.6,
    /// blind 1.0 — a blinded machine hears everything,
    /// which is why blinding it is a tactic, not a win.
    pub fn audio_reliance(&self) -> f32 {
        match self.sensors {
            SensorState::Nominal => 0.3,
            SensorState::Degraded => 0.6,
            SensorState::Blind => 1.0,
        }
    }

    /// Movement speed multiplier from locomotion state.
    pub fn speed_factor(&self) -> f32 {
        match self.locomotion {
            LocoState::Nominal => 1.0,
            LocoState::Hobbled => 0.45,
            LocoState::Immobilized => 0.0,
        }
    }

    /// One tick: cool the cannon, sweep the eye,
    /// tick the servo clock. Telegraphs are outputs:
    /// servo noise bursts every ~2 s, eye sweeps run
    /// 480 ticks once triggered by the host (a sweep is
    /// a telegraph the player reads, not a timer the
    /// machine hides).
    pub fn tick(&mut self) {
        if self.cannon_cool_ticks > 0 {
            self.cannon_cool_ticks -= 1;
            if self.cannon_cool_ticks == 0 && self.cannon == CannonState::Cooling {
                self.cannon = CannonState::Ready;
            }
        }
        if self.eye_sweep_ticks > 0 {
            self.eye_sweep_ticks -= 1;
        }
        self.servo_ticks += 1;
    }

    /// Trigger an eye sweep telegraph (480 ticks).
    pub fn trigger_sweep(&mut self) {
        self.eye_sweep_ticks = 480;
    }

    /// Servo noise right now: bursts on a ~2 s cycle
    /// (480 ticks on, 480 off, offset by state). The
    /// player hears the machine before he sees it.
    pub fn servo_noise(&self) -> f32 {
        if self.servo_ticks % 960 < 480 {
            0.7
        } else {
            0.2
        }
    }

    /// Ground tremor right now: locomotion-driven, zero
    /// when immobilized, half when hobbled. Felt more
    /// than heard — a district stimulus at close range.
    pub fn tremor(&self) -> f32 {
        match self.locomotion {
            LocoState::Nominal => 0.6,
            LocoState::Hobbled => 0.3,
            LocoState::Immobilized => 0.0,
        }
    }
}

// ----------------------------------------------------------------
// TYPE 2
// ----------------------------------------------------------------

/// The Type 2's state: momentum doctrine as a machine.
/// Idle is nearly quiet; windup telegraphs; charging is
/// a straight line; overshoot is the price; recovery is
/// the window. There is no "hunt" state — she does not
/// hunt, she charges.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Type2State {
    Idle,
    Windup,
    Charging,
    Overshoot,
    Recovery,
}

/// Voice apparatus state: the scream's organ. Nominal
/// screams; silenced does not — and silencing changes
/// nothing else. A silenced Type 2 still charges.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum VoiceState {
    Nominal,
    Silenced,
}

/// The Type 2's governor: momentum, overshoot, recovery,
/// scream, silence. Escalation gating lives OUTSIDE
/// this governor: the host asks the alert budget whether
/// a Type 2 event is bought, and only then triggers a
/// charge. A Type 2 that appears without a bought
/// escalation is a population error, and this governor
/// refuses to be one — `trigger_charge` returns false
/// unless the host passes escalation_allowed.
pub struct Type2Governor {
    pub state: Type2State,
    pub voice: VoiceState,
    /// Charge direction (horizontal, unit).
    pub charge_dir: Vec3,
    /// Current charge speed in m/s.
    pub speed: f32,
    /// Ticks left in the current state.
    pub state_ticks: u64,
    /// Recovery seconds owed after an overshoot, in
    /// ticks — the window the player earned.
    pub recovery_owed: u64,
}

impl Default for Type2Governor {
    fn default() -> Self {
        Self::new()
    }
}

impl Type2Governor {
    /// Charge speed: extremely fast, fixed. She does
    /// not accelerate into a charge — she IS fast, and
    /// the windup is the only warning.
    pub const CHARGE_SPEED: f32 = 14.0;
    /// Windup telegraph: 1 second of scream and stillness
    /// before the line. A charge without windup is a
    /// jump scare, not a doctrine.
    pub const WINDUP_TICKS: u64 = 240;

    pub fn new() -> Type2Governor {
        Type2Governor {
            state: Type2State::Idle,
            voice: VoiceState::Nominal,
            charge_dir: Vec3::X,
            speed: 0.0,
            state_ticks: 0,
            recovery_owed: 0,
        }
    }

    /// Silence the voice apparatus. The scream stops;
    /// the charge does not. Silencing is a component
    /// kill, not a machine kill.
    pub fn silence_voice(&mut self) {
        self.voice = VoiceState::Silenced;
    }

    /// Trigger a charge. Returns false unless the host
    /// passes escalation_allowed (the alert budget
    /// bought this event) and the governor is idle.
    /// A refused trigger is not an error — it is the
    /// doctrine refusing population.
    pub fn trigger_charge(&mut self, dir: Vec3, escalation_allowed: bool) -> bool {
        if !escalation_allowed || self.state != Type2State::Idle {
            return false;
        }
        let flat = Vec3::new(dir.x, 0.0, dir.z);
        self.charge_dir = if flat.length() > 1e-6 {
            flat.normalize()
        } else {
            Vec3::X
        };
        self.state = Type2State::Windup;
        self.state_ticks = Self::WINDUP_TICKS;
        true
    }

    /// The scream, as a district stimulus — but only
    /// while the voice is nominal. A silenced machine
    /// emits nothing, and the district stays quiet,
    /// which is exactly why silencing matters.
    pub fn scream(&self, pos: Vec3) -> Option<crate::perception::sound::DistrictStimulus> {
        if self.voice != VoiceState::Nominal {
            return None;
        }
        Some(crate::perception::sound::DistrictStimulus {
            pos,
            magnitude: 1.0,
            kind: crate::perception::sound::DistrictKind::Scream,
        })
    }

    /// Contact is gib-capable: the host reads this flag
    /// on collision, it does not read a damage number.
    /// Contact during Charging gibs; contact otherwise
    /// does not (she is not dangerous standing still —
    /// she is dangerous moving).
    pub fn contact_gib_capable(&self) -> bool {
        self.state == Type2State::Charging
    }

    /// One tick of the momentum machine. Windup counts
    /// down into Charging; charging runs until the host
    /// reports the line ended (target passed or wall
    /// hit) via `end_charge`; overshoot runs its course
    /// into recovery; recovery counts down into idle.
    /// She cannot climb: this governor never changes
    /// height — vertical movement is not a state here,
    /// it is an absence, and the host must not add it.
    pub fn tick(&mut self) {
        match self.state {
            Type2State::Idle => {
                self.speed = 0.0;
            }
            Type2State::Windup => {
                self.speed = 0.0;
                if self.state_ticks > 0 {
                    self.state_ticks -= 1;
                }
                if self.state_ticks == 0 {
                    self.state = Type2State::Charging;
                    self.speed = Self::CHARGE_SPEED;
                }
            }
            Type2State::Charging => {
                self.speed = Self::CHARGE_SPEED;
            }
            Type2State::Overshoot => {
                // Overshoot bleeds speed linearly over
                // ~1 s, then owes recovery.
                if self.state_ticks > 0 {
                    self.state_ticks -= 1;
                    self.speed = Self::CHARGE_SPEED
                        * (self.state_ticks as f32 / 240.0);
                } else {
                    self.state = Type2State::Recovery;
                    self.state_ticks = self.recovery_owed;
                }
            }
            Type2State::Recovery => {
                self.speed = 0.0;
                if self.state_ticks > 0 {
                    self.state_ticks -= 1;
                }
                if self.state_ticks == 0 {
                    self.state = Type2State::Idle;
                }
            }
        }
    }

    /// The charge line ended (target passed, wall hit,
    /// street furniture paid). Overshoot runs ~1 s,
    /// then recovery is owed: 2 seconds of stillness,
    /// the window the player earned by surviving the
    /// line. Returns false when not charging — ending
    /// a charge that never started is a host bug, and
    /// the governor says so quietly.
    pub fn end_charge(&mut self) -> bool {
        if self.state != Type2State::Charging {
            return false;
        }
        self.state = Type2State::Overshoot;
        self.state_ticks = 240;
        self.recovery_owed = 480;
        true
    }
}
