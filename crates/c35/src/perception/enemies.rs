//! The three adopted enemies' brains — behavior only.
//!
//! No model work lives here: the Hound's bars plate sits
//! on his harness's left strap, the Weathervane's scope
//! and telephone are kit, the Neighbour wears no insignia
//! anywhere — but a brain does not draw kit. What a brain
//! does is decide: the Hound follows scent, the
//! Weathervane reports, the Neighbour watches and writes.
//! Each brain's output is posture, movement intent, and
//! records — never a combat script, never an icon.
//!
//! The Neighbour's enforcement is structural: there is
//! no appearance field on his brain at all. He cannot be
//! read by what he wears, because there is nothing to
//! read — only by what he does, which is the whole
//! doctrine.

use crate::perception::economy::ConfigKind;
use crate::perception::sound::DistrictStimulus;
use glam::Vec3;

// ----------------------------------------------------------------
// HOUND
// ----------------------------------------------------------------

/// What the Hound's body is saying: nose down pulling is
/// tracking; head up still is scent held; hackles and a
/// straining line are contact. The handler reads the dog
/// the way the player reads the soldier: by posture.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HoundPosture {
    /// Nose down, pulling on the line: on a trail.
    NoseDown,
    /// Head up, still: scent held, waiting on the wind.
    HeadUp,
    /// Hackles up, line straining: contact, or its edge.
    Straining,
    /// At the handler's heel: no trail, no lock.
    Heel,
}

/// The Hound's state. Tracking follows the scent field;
/// Held waits on a lost trail (the nose, not the clock,
/// decides when it is lost); Contact is the edge of the
/// target, not the kill — the Hound holds, the handler
/// decides.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HoundState {
    Tracking,
    Held,
    Contact,
    Heel,
}

/// The Hound's brain: a scent follower with a handler
/// tether. The tether is a leash law: past it, the dog
/// returns to the handler before it follows anything.
/// Grief is priced in the record: when the handler goes
/// down, grief increments, and the debrief carries it.
pub struct HoundBrain {
    pub state: HoundState,
    pub pos: Vec3,
    /// 0.0 (cold trail) to 1.0 (the target's heels).
    pub lock: f32,
    /// Handler grief, counted: the record's price for a
    /// handler lost. Never reset mid-op.
    pub grief: u32,
    /// Maximum distance from the handler, in metres.
    pub tether: f32,
    /// Ticks held without scent before giving up.
    pub held_ticks: u64,
    /// Current travel heading: the forward hemisphere
    /// [`ScentField::sample_heading`] follows. Updated
    /// from every move direction the brain outputs.
    pub heading: Vec3,
    pub tick: u64,
}

impl HoundBrain {
    /// Nose radius: how far ahead the Hound smells the
    /// line. Three metres — a nose, not a radar.
    pub const NOSE_RADIUS: f32 = 3.0;

    pub fn new(pos: Vec3, tether: f32) -> HoundBrain {
        HoundBrain {
            state: HoundState::Heel,
            pos,
            lock: 0.0,
            grief: 0,
            tether,
            held_ticks: 0,
            heading: Vec3::X,
            tick: 0,
        }
    }

    /// The handler went down. Grief increments, once per
    /// call, and the Hound's state does not change on
    /// grief alone — a grieving dog still tracks, which
    /// is why the record prices it.
    pub fn note_handler_down(&mut self) {
        self.grief += 1;
    }

    /// Follow a scent field directly: sample the line
    /// in the current heading's forward hemisphere,
    /// then tick on the sample. The heading updates
    /// from every move direction output, so the dog
    /// walks the line bead by bead instead of
    /// oscillating across each bead it passes. Hosts
    /// that pre-sample may call [`HoundBrain::tick`]
    /// instead; hosts that hold the field call this.
    pub fn follow(
        &mut self,
        scent: &crate::perception::scent::ScentField,
        handler_pos: Vec3,
        target_close: bool,
    ) -> HoundOutput {
        let sample = scent.sample_heading(self.pos, Self::NOSE_RADIUS, self.heading);
        let out = self.tick(sample, handler_pos, target_close);
        if let Some(dir) = out.move_dir {
            self.heading = dir;
        }
        out
    }

    /// One tick. `scent` is the scent field's sample at
    /// the nose: (direction to the strongest segment,
    /// strength). `handler_pos` is the tether anchor.
    /// `target_close` is true when the target is within
    /// contact range (the host measures it — the nose
    /// does not measure metres).
    pub fn tick(
        &mut self,
        scent: Option<(Vec3, f32)>,
        handler_pos: Vec3,
        target_close: bool,
    ) -> HoundOutput {
        self.tick += 1;
        let to_handler = handler_pos - self.pos;
        let handler_dist = to_handler.length();

        // Tether first: past the leash, the dog comes
        // back. No trail outruns the handler.
        if handler_dist > self.tether {
            let dir = to_handler / handler_dist.max(1e-6);
            self.state = HoundState::Heel;
            return HoundOutput {
                state: self.state,
                posture: HoundPosture::Heel,
                move_dir: Some(dir),
                strain: 0.0,
            };
        }

        match scent {
            Some((dir, strength)) => {
                // Crossing a segment sharpens the lock:
                // lock rises with strength, faster on a
                // strong segment. The trail freshens the
                // dog's certainty, not the ground.
                self.lock = (self.lock + strength * 0.02).min(1.0);
                self.held_ticks = 0;
                if target_close && self.lock > 0.6 {
                    self.state = HoundState::Contact;
                    return HoundOutput {
                        state: self.state,
                        posture: HoundPosture::Straining,
                        move_dir: Some(dir),
                        strain: self.lock,
                    };
                }
                self.state = HoundState::Tracking;
                HoundOutput {
                    state: self.state,
                    posture: HoundPosture::NoseDown,
                    move_dir: Some(dir),
                    strain: self.lock * 0.5,
                }
            }
            None => {
                // No scent: the lock cools, the head comes
                // up, and after ~5 seconds held (1200 ticks)
                // the dog gives up and heels. A lost trail
                // is a held nose, not an instant reset.
                self.lock = (self.lock - 0.005).max(0.0);
                self.held_ticks += 1;
                if self.held_ticks > 1200 || self.lock <= 0.0 {
                    self.state = HoundState::Heel;
                    HoundOutput {
                        state: self.state,
                        posture: HoundPosture::Heel,
                        move_dir: None,
                        strain: 0.0,
                    }
                } else {
                    self.state = HoundState::Held;
                    HoundOutput {
                        state: self.state,
                        posture: HoundPosture::HeadUp,
                        move_dir: None,
                        strain: 0.0,
                    }
                }
            }
        }
    }
}

pub struct HoundOutput {
    pub state: HoundState,
    pub posture: HoundPosture,
    pub move_dir: Option<Vec3>,
    /// Line strain 0.0–1.0: the handler reads it.
    pub strain: f32,
}

// ----------------------------------------------------------------
// WEATHERVANE
// ----------------------------------------------------------------

/// A report: what the Weathervane saw, where, how sure.
/// The report travels as a district stimulus (sound.rs)
/// — the telephone wire is the fiction, the district bus
/// is the mechanism. The host converts a report into a
/// DistrictStimulus::Report and feeds the district.
#[derive(Copy, Clone, Debug)]
pub struct ReportEvent {
    pub pos: Vec3,
    pub confidence: f32,
    pub tick: u64,
}

/// The rooftop observer's brain: a scheduled sensor, not
/// a soldier. He watches posts on a rotation, sweeps his
/// scope, and REPORTS — rarely shoots, poor hearing,
/// silhouette against the sky as his tell. His power is
/// not his gun; it is that his telephone reaches the
/// street faster than any runner.
pub struct WeathervaneBrain {
    /// Observation posts, walked on schedule.
    pub posts: Vec<Vec3>,
    pub post_i: usize,
    /// Ticks until the next scheduled rotation.
    pub rotation_in: u64,
    /// Ticks the current target has been observed.
    pub observed_ticks: u64,
    /// Ticks between reports (a reporting observer who
    /// reports every tick is a telephone that never
    /// hangs up).
    pub report_cooldown: u64,
    pub cooldown_left: u64,
    /// Sound sensitivity: poor hearing, 0.3 of a
    /// soldier's ear. He watches; he does not listen.
    pub hearing: f32,
    pub tick: u64,
}

impl WeathervaneBrain {
    /// Rotation every ~60 s, report cooldown ~30 s, at
    /// the 240 Hz fixed step.
    pub fn new(posts: Vec<Vec3>) -> WeathervaneBrain {
        WeathervaneBrain {
            posts,
            post_i: 0,
            rotation_in: 14_400,
            observed_ticks: 0,
            report_cooldown: 7_200,
            cooldown_left: 0,
            hearing: 0.3,
            tick: 0,
        }
    }

    pub fn post(&self) -> Option<Vec3> {
        self.posts.get(self.post_i).copied()
    }

    /// Is the observer skylined — rooftop post, seen
    /// from below against the sky? The host measures
    /// geometry; the brain states the tell: an observer
    /// on a roofline is visible from the street he
    /// watches, which is why the vertical layer is
    /// contested ground.
    pub fn skylined(&self, observer_below: bool) -> bool {
        observer_below && !self.posts.is_empty()
    }

    /// One tick. `target_seen` is the scope's verdict
    /// (the host prices light and line for the scope the
    /// same way it prices a soldier's eyes — the scope
    /// is magnification, not a second sense).
    /// Returns a report when observation has been
    /// sustained past ~2 s and the cooldown has cleared.
    pub fn tick(&mut self, target_seen: bool, target_pos: Option<Vec3>) -> Option<ReportEvent> {
        self.tick += 1;
        if self.cooldown_left > 0 {
            self.cooldown_left -= 1;
        }
        // Scheduled rotation: the post changes on the
        // clock, not on the target. A rotation that
        // follows the target is a hunter, not a watch.
        if self.rotation_in > 0 {
            self.rotation_in -= 1;
        } else {
            self.rotation_in = 14_400;
            if !self.posts.is_empty() {
                self.post_i = (self.post_i + 1) % self.posts.len();
            }
            self.observed_ticks = 0;
        }
        if target_seen {
            self.observed_ticks += 1;
        } else {
            self.observed_ticks = 0;
        }
        if self.observed_ticks > 480 && self.cooldown_left == 0 {
            if let Some(p) = target_pos {
                self.cooldown_left = self.report_cooldown;
                self.observed_ticks = 0;
                return Some(ReportEvent {
                    pos: p,
                    confidence: 0.9,
                    tick: self.tick,
                });
            }
        }
        None
    }

    /// Convert a report into the district stimulus the
    /// district bus carries. The telephone wire is the
    /// fiction; this conversion is the mechanism.
    pub fn report_stimulus(report: &ReportEvent) -> DistrictStimulus {
        DistrictStimulus {
            pos: report.pos,
            magnitude: report.confidence,
            kind: crate::perception::sound::DistrictKind::Report,
        }
    }
}

// ----------------------------------------------------------------
// NEIGHBOUR
// ----------------------------------------------------------------

/// What the Neighbour observed, as behavior — never
/// appearance. Configuration is what the target was
/// DOING and wearing as kit (coat vs black set is
/// behavior-visible: a man in a black balaclava at a
/// checkpoint is a behavior, not a label).
#[derive(Copy, Clone, Debug)]
pub struct BehaviourNote {
    /// The target acted suspiciously this tick
    /// (watched too long, moved wrong, hid).
    pub suspicious: bool,
    /// The configuration the target was seen in.
    pub config: ConfigKind,
    /// Distance to the target in metres.
    pub distance: f32,
}

/// A written description: the Neighbour's output, and
/// the other archive's input. A description is a record,
/// not a combat state — it changes checkpoint reads and
/// blocks the Wanderer, it does not draw a weapon.
#[derive(Copy, Clone, Debug)]
pub struct Description {
    pub config: ConfigKind,
    pub confidence: f32,
    /// Ticks until the description expires (descriptions
    /// age out — an old description is a rumor).
    pub ttl_ticks: u64,
}

/// The plain-clothes informer's brain: a behaviour
/// scorer. Watch → follow → write, on thresholds. He
/// watches from a distance (close watching is a
/// soldier's job and a Neighbour who does it is caught);
/// he follows at a distance; he writes when the
/// notebook is full. His success state is a description
/// entered into the other archive — and from there, the
/// city itself gets harder.
pub struct NeighbourBrain {
    /// 0.0–1.0: how much watching has accumulated.
    pub watch: f32,
    /// 0.0–1.0: how much following has accumulated.
    pub follow: f32,
    /// 0.0–1.0: how much writing has accumulated.
    pub write: f32,
    /// The description, once written. None until then.
    pub description: Option<Description>,
    pub tick: u64,
}

impl Default for NeighbourBrain {
    fn default() -> Self {
        Self::new()
    }
}

impl NeighbourBrain {
    pub fn new() -> NeighbourBrain {
        NeighbourBrain {
            watch: 0.0,
            follow: 0.0,
            write: 0.0,
            description: None,
            tick: 0,
        }
    }

    /// Is a description active (written and unexpired)?
    /// While active, the Wanderer will not appear — an
    /// active description is a city that knows your face.
    pub fn blocks_wanderer(&self) -> bool {
        self.description.is_some()
    }

    /// One tick. Watching accumulates on suspicious
    /// behavior at range (over 10 m — a Neighbour
    /// closer than that is not informing, he is
    /// loitering). Following needs sustained watching;
    /// writing needs sustained following. Each stage
    /// decays without its input — a scorer that only
    /// rises is a countdown, not a judgment.
    pub fn tick(&mut self, note: BehaviourNote) -> Option<Description> {
        self.tick += 1;
        // Age out an old description.
        if let Some(d) = &mut self.description {
            if d.ttl_ticks > 0 {
                d.ttl_ticks -= 1;
            } else {
                self.description = None;
            }
        }
        if self.description.is_some() {
            return self.description;
        }
        if note.suspicious && note.distance > 10.0 {
            self.watch = (self.watch + 0.01).min(1.0);
        } else {
            self.watch = (self.watch - 0.005).max(0.0);
        }
        if self.watch > 0.4 {
            self.follow = (self.follow + 0.008).min(1.0);
        } else {
            self.follow = (self.follow - 0.005).max(0.0);
        }
        if self.follow > 0.6 {
            self.write = (self.write + 0.01).min(1.0);
        } else {
            self.write = (self.write - 0.005).max(0.0);
        }
        if self.write >= 1.0 {
            // ~10 minutes of sustained observation at
            // the fixed step, in the worst case — a
            // description is earned, never instant.
            let d = Description {
                config: note.config,
                confidence: 0.85,
                ttl_ticks: 240 * 3600,
            };
            self.description = Some(d);
            return Some(d);
        }
        None
    }
}
