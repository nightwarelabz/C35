//! The dual-mode player controller: one movement core
//! over the traversal sim, two camera rigs over it.
//!
//! Per tick, in order: the mode toggle edge switches
//! the rig (and nothing else); look deltas turn the
//! one shared camera yaw/pitch; the wish and speed
//! tier become a `TravInput`; the sim steps ONCE at
//! its fixed step; the locomotion animator and the
//! traversal pose family pose the figure; and the
//! active rig builds the tick's camera. The body does
//! not know which rig is presenting it, and the rigs
//! do not feed back into the body — that is the whole
//! architecture law, kept structurally.

use crate::controller::camera::{
    CameraState, BOB_AMPLITUDE, BOB_STRIDE, BOB_WEIGHT_RATE, BREATH_AMPLITUDE, BREATH_PERIOD_S,
    DIP_DURATION_S, DIP_MAX, DIP_PER_IMPACT, FOV_EASE_RATE, FP_FOV_AIM_DEG, FP_FOV_BASE_DEG,
    FP_FOV_MAX_DEG, LEAN_OFFSET, PITCH_LIMIT_DEG, ROLL_EASE_RATE, STILL_BEAT_TICKS,
    SWING_ROLL_CAP_DEG, SWING_ROLL_PER_SPEED_DEG, TP_BOOM_EASE_RATE, TP_COLLISION_MARGIN,
    TP_FOV_AIM_DEG, TP_FOV_BASE_DEG, TP_FOV_MAX_DEG, TP_MIN_BOOM, WALLRUN_ROLL_DEG,
};
use crate::controller::input::{
    ControllerConfig, PlayerInput, Posture, SpeedTier, ViewMode, CROUCH_DROP, POSTURE_TRANSITION_S,
    STAND_EYE,
};
use crate::locomotion::{AnimInput, Animator};
use crate::operator::{PoseFrame, ViewmodelPose};
use crate::traversal::physics::{StaticWorld, FIXED_DT};
use crate::traversal::wallrun::WallSide;
use crate::traversal::{BodyMode, TravInput, TravSim};
use glam::Vec3;

/// Body turn rate (rad/s): how fast the figure's
/// facing chases its demanded facing (movement
/// direction, or the camera when aiming / committed).
/// Fast enough to feel responsive at the fixed step,
/// finite so a turn is a turn, not a snap.
pub const BODY_TURN_RATE: f32 = 12.0;

/// Pitch (radians) below which first-person body
/// awareness joins the viewmodel: the DeBrief host's
/// own threshold, carried here so engine and host
/// agree on when the legs are in frame.
pub const BODY_AWARENESS_PITCH: f32 = -0.2;

fn damp(cur: f32, target: f32, rate: f32, dt: f32) -> f32 {
    cur + (target - cur) * (1.0 - (-rate * dt).exp())
}

fn move_toward(cur: f32, target: f32, max_delta: f32) -> f32 {
    if (target - cur).abs() <= max_delta {
        target
    } else {
        cur + (target - cur).signum() * max_delta
    }
}

fn wrap_angle(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn rotate_toward(cur: f32, target: f32, max_delta: f32) -> f32 {
    let err = wrap_angle(target - cur);
    cur + err.clamp(-max_delta, max_delta)
}

/// Horizontal forward for a yaw: (sin yaw, 0, cos yaw)
/// — the locomotion facing convention, shared by the
/// camera, the body and the figure.
fn forward(yaw: f32) -> Vec3 {
    let (s, c) = yaw.sin_cos();
    Vec3::new(s, 0.0, c)
}

/// Horizontal right for a yaw: perpendicular to
/// `forward`, pointing at the figure's right hand
/// (-cos yaw, 0, sin yaw).
fn right(yaw: f32) -> Vec3 {
    let (s, c) = yaw.sin_cos();
    Vec3::new(-c, 0.0, s)
}

/// One tick of controller truth, returned by
/// [`PlayerController::tick`].
#[derive(Copy, Clone, Debug)]
pub struct ControllerFrame {
    /// The rig that presented this tick.
    pub mode: ViewMode,
    /// The active rig's camera.
    pub camera: CameraState,
    /// The body's eye position (feet + eased posture
    /// eye height), in BOTH modes: in first person this
    /// is the camera's anchor before lean and bob; in
    /// third person it is where the body's own eye sits
    /// (the light field's read point, the grapple's
    /// neighbourhood), for hosts that price things at
    /// the eye.
    pub eye: Vec3,
    /// The simulated body's feet position (world).
    pub feet: Vec3,
    /// The simulated body's velocity (world, full 3D).
    pub velocity: Vec3,
    /// Body facing yaw (radians, locomotion convention).
    pub body_yaw: f32,
    /// Body facing (horizontal unit vector).
    pub body_forward: Vec3,
    /// Current posture.
    pub posture: Posture,
    /// Current eased eye height above the feet (m).
    pub eye_height: f32,
    /// The traversal mode this tick, mirrored from the
    /// sim (Ground / Air / WallRun / Grapple / Mantle).
    pub traversal_mode: BodyMode,
    /// Horizontal ground speed (m/s), as simulated.
    pub speed: f32,
    /// On the ground, as simulated.
    pub grounded: bool,
    /// The full figure's pose frame for this tick
    /// (locomotion gait under the traversal pose
    /// family). Third person presents the figure with
    /// it; first person may use it for shadows,
    /// reflections and body awareness placement.
    pub figure_pose: PoseFrame,
    /// Whether the weapon layer is open (graph state
    /// from the pose family: closed while the launcher
    /// is committed or the hands are on a ledge).
    pub weapon_open: bool,
    /// First person: the viewmodel pose hint for the
    /// host's `build_viewmodel` call. Third person:
    /// None — the full figure is the presentation.
    pub viewmodel_pose: Option<ViewmodelPose>,
    /// The camera pitch, handed to the host beside the
    /// viewmodel hint: `build_viewmodel` takes no
    /// pitch, so the host pitches the hands by exactly
    /// this. Equals `camera.pitch`.
    pub viewmodel_pitch: f32,
    /// The viewmodel configuration from the controller
    /// config, echoed so the host's `build_viewmodel`
    /// call and the controller can never disagree
    /// about which kit the hands wear.
    pub viewmodel_config: crate::operator::ViewmodelConfig,
    /// First person: true when the camera pitches down
    /// far enough that body awareness joins the
    /// viewmodel (pitch < BODY_AWARENESS_PITCH).
    pub body_awareness: bool,
    /// Lean applied this tick (-1..=1, echoed).
    pub lean: f32,
}

/// The player controller. One per player; the host
/// feeds it one [`PlayerInput`] per fixed tick with
/// the static world, and reads back a
/// [`ControllerFrame`].
pub struct PlayerController {
    /// The traversal sim that owns the body. Public so
    /// hosts can read events, the grapple state and
    /// pose state the frame does not mirror.
    pub sim: TravSim,
    /// The locomotion animator that poses the figure.
    /// Public so hosts can read cycle state (phase,
    /// contacts) and the smoothed ground reference.
    pub animator: Animator,
    config: ControllerConfig,
    mode: ViewMode,
    camera_yaw: f32,
    camera_pitch: f32,
    body_yaw: f32,
    posture: Posture,
    eye_height: f32,
    fov_deg: f32,
    roll: f32,
    bob_phase: f32,
    bob_weight: f32,
    breath_phase: f32,
    still_ticks: u32,
    dip_t: Option<f32>,
    dip_amp: f32,
    boom: f32,
    shoulder: f32,
}

impl PlayerController {
    /// A controller standing at `start` (feet position),
    /// default configuration (first person first).
    pub fn new(start: Vec3) -> PlayerController {
        PlayerController::with_config(start, ControllerConfig::default())
    }

    pub fn with_config(start: Vec3, config: ControllerConfig) -> PlayerController {
        PlayerController {
            sim: TravSim::new(start),
            animator: Animator::new(),
            mode: config.initial_mode,
            boom: config.tp_distance,
            shoulder: config.tp_shoulder,
            config,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            body_yaw: 0.0,
            posture: Posture::Stand,
            eye_height: STAND_EYE,
            fov_deg: match config.initial_mode {
                ViewMode::FirstPerson => FP_FOV_BASE_DEG,
                ViewMode::ThirdPerson => TP_FOV_BASE_DEG,
            },
            roll: 0.0,
            bob_phase: 0.0,
            bob_weight: 0.0,
            breath_phase: 0.0,
            still_ticks: 0,
            dip_t: None,
            dip_amp: 0.0,
        }
    }

    /// The active rig.
    pub fn mode(&self) -> ViewMode {
        self.mode
    }

    /// Switch rigs. This is the whole toggle contract:
    /// ONLY the rig changes. Body position, velocity,
    /// rope, posture, look, FOV and every other piece of
    /// state carry across untouched — the movement core
    /// cannot tell the toggle happened.
    pub fn set_mode(&mut self, mode: ViewMode) {
        self.mode = mode;
    }

    /// Flip to the other rig (see [`Self::set_mode`]).
    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            ViewMode::FirstPerson => ViewMode::ThirdPerson,
            ViewMode::ThirdPerson => ViewMode::FirstPerson,
        };
    }

    /// The current posture.
    pub fn posture(&self) -> Posture {
        self.posture
    }

    /// Install a ground function on the locomotion
    /// animator (see the module docs): a host walking
    /// the figure on box ground passes its own height
    /// function; None restores the island heightfield.
    pub fn set_ground_fn(&mut self, f: Option<fn(f32, f32) -> f32>) {
        self.animator.set_ground_fn(f);
    }

    /// Advance ONE fixed tick (240 Hz): apply the
    /// toggle and look, step the movement core once,
    /// pose the figure, and build the active rig's
    /// camera. Deterministic: same input sequence, same
    /// frames, on any host.
    pub fn tick(&mut self, input: &PlayerInput, world: &StaticWorld) -> ControllerFrame {
        // ---- toggle: the rig switches, nothing else ----
        if input.toggle_mode {
            self.toggle_mode();
        }

        // ---- look: once per tick ----
        self.camera_yaw = wrap_angle(self.camera_yaw + input.look_delta.x);
        let pitch_limit = PITCH_LIMIT_DEG.to_radians();
        self.camera_pitch =
            (self.camera_pitch + input.look_delta.y).clamp(-pitch_limit, pitch_limit);

        // ---- posture: the tier is the posture ----
        let crouch = input.speed.is_crouch();
        self.posture = if crouch {
            Posture::Crouch
        } else {
            Posture::Stand
        };

        // ---- wish: camera space -> world ----
        let cam_fwd = forward(self.camera_yaw);
        let cam_right = right(self.camera_yaw);
        let raw_wish = cam_fwd * input.wish.y + cam_right * input.wish.x;
        let mag = raw_wish.length();
        let (wish_dir, wish_mag) = if mag > 1.0 {
            (raw_wish / mag, 1.0)
        } else if mag > 1e-6 {
            (raw_wish / mag, mag)
        } else {
            (Vec3::ZERO, 0.0)
        };
        let wish_speed = input.speed.speed() * wish_mag;
        let lean = input.lean.clamp(-1.0, 1.0);

        // ---- body facing: movement direction, or the
        // camera when aiming or grapple-committed. While
        // a mantle owns the body the facing is the sim's.
        if self.sim.mode != BodyMode::Mantle {
            let committed = self.sim.grapple.hands_committed();
            let demanded = if input.aim_held || committed || input.hook_press {
                Some(self.camera_yaw)
            } else if wish_mag > 0.01 {
                // yaw for a facing: forward = (sin yaw, cos yaw),
                // so yaw = atan2(x, z) of the wish direction.
                Some(wish_dir.x.atan2(wish_dir.z))
            } else {
                None
            };
            if let Some(target) = demanded {
                self.body_yaw = rotate_toward(self.body_yaw, target, BODY_TURN_RATE * FIXED_DT);
            }
        }
        let body_fwd = forward(self.body_yaw);

        // ---- aim: the camera's full look direction ----
        let cp = self.camera_pitch.cos();
        let aim = Vec3::new(cam_fwd.x * cp, self.camera_pitch.sin(), cam_fwd.z * cp);

        // ---- the movement core: one fixed step ----
        let was_grounded = self.sim.body.grounded;
        self.sim.tick(
            TravInput {
                wish: wish_dir,
                facing: body_fwd,
                speed: wish_speed,
                jump: input.jump,
                hook_press: input.hook_press,
                hook_held: input.hook_held && !input.hook_release,
                crouch,
                aim,
            },
            world,
        );
        let feet = self.sim.body.feet;
        let velocity = self.sim.body.vel;
        let speed_h = self.sim.body.horizontal_speed();
        let grounded = self.sim.body.grounded;
        let committed = self.sim.grapple.hands_committed();

        // Landing: a touchdown with a real impact starts
        // the one eased dip.
        if !was_grounded && grounded && self.sim.body.last_impact > 0.5 {
            self.dip_amp = (self.sim.body.last_impact * DIP_PER_IMPACT).min(DIP_MAX);
            self.dip_t = Some(0.0);
        }

        // ---- figure pose: gait under traversal ----
        let gait = self.animator.update(
            FIXED_DT,
            &AnimInput {
                pos: feet,
                yaw: self.body_yaw,
                speed: speed_h,
                crouch: if crouch { 1.0 } else { 0.0 },
                aim: if input.aim_held { 1.0 } else { 0.0 },
                slide: 0.0,
                carry: true,
            },
        );
        let (figure_pose, weapon_open) = self.sim.pose(&gait);

        // ---- posture eye height: constant-rate travel ----
        let eye_rate = CROUCH_DROP / POSTURE_TRANSITION_S;
        self.eye_height = move_toward(
            self.eye_height,
            self.posture.eye_height(),
            eye_rate * FIXED_DT,
        );
        let eye = feet + Vec3::new(0.0, self.eye_height, 0.0);

        // ---- FOV: eased toward the register the state
        // demands. Aim narrows; the swing widens with
        // speed, capped at the sprint widening; sprint
        // widens only while actually sprinting; the
        // wall-run holds base; stealth (crouch, still)
        // never widens. ----
        let (base, aim_fov, max_fov) = match self.mode {
            ViewMode::FirstPerson => (FP_FOV_BASE_DEG, FP_FOV_AIM_DEG, FP_FOV_MAX_DEG),
            ViewMode::ThirdPerson => (TP_FOV_BASE_DEG, TP_FOV_AIM_DEG, TP_FOV_MAX_DEG),
        };
        let swinging = self.sim.mode == BodyMode::Grapple && self.sim.grapple.is_latched();
        let fov_target = if input.aim_held {
            aim_fov
        } else if swinging {
            let t = (speed_h / crate::locomotion::SPRINT_SPEED).clamp(0.0, 1.0);
            base + (max_fov - base) * t
        } else if self.sim.mode == BodyMode::WallRun {
            base
        } else if !crouch
            && matches!(input.speed, SpeedTier::Sprint)
            && speed_h > crate::locomotion::RUN_SPEED
        {
            max_fov
        } else {
            base
        };
        self.fov_deg = damp(self.fov_deg, fov_target, FOV_EASE_RATE, FIXED_DT);

        // ---- roll: wall-run lean and the capped swing
        // bank, first person only. Lean never rolls. ----
        let mut roll_target = 0.0f32;
        if self.mode == ViewMode::FirstPerson {
            if self.sim.mode == BodyMode::WallRun {
                if let Some(w) = &self.sim.wallrun {
                    roll_target = match w.side {
                        WallSide::Left => -WALLRUN_ROLL_DEG,
                        WallSide::Right => WALLRUN_ROLL_DEG,
                    }
                    .to_radians();
                }
            } else if swinging {
                let lateral = velocity.dot(cam_right);
                roll_target = (lateral * SWING_ROLL_PER_SPEED_DEG)
                    .clamp(-SWING_ROLL_CAP_DEG, SWING_ROLL_CAP_DEG)
                    .to_radians();
            }
        }
        self.roll = damp(self.roll, roll_target, ROLL_EASE_RATE, FIXED_DT);

        // ---- first-person eye offsets: gait bob under
        // the stealth law, the landing dip, idle breath ----
        if speed_h < 0.05 {
            self.still_ticks = self.still_ticks.saturating_add(1);
        } else {
            self.still_ticks = 0;
        }
        let mut bob_y = 0.0f32;
        if self.mode == ViewMode::FirstPerson {
            // Gait bob, legal only standing, unaimed,
            // unleaned, grounded, in plain Ground mode,
            // hands free, and actually moving. Illegal
            // means exactly 0.0 this tick — no fade-out
            // tail leaking motion into a careful approach.
            let bob_legal = self.posture == Posture::Stand
                && !input.aim_held
                && lean == 0.0
                && grounded
                && self.sim.mode == BodyMode::Ground
                && !committed
                && speed_h >= 0.05;
            if bob_legal {
                self.bob_weight = (self.bob_weight + BOB_WEIGHT_RATE * FIXED_DT).min(1.0);
                self.bob_phase += speed_h * FIXED_DT / BOB_STRIDE * std::f32::consts::TAU;
                let ramp = (speed_h / crate::locomotion::SPRINT_SPEED).clamp(0.0, 1.0);
                bob_y += self.bob_phase.sin() * BOB_AMPLITUDE * self.bob_weight * ramp;
            } else {
                self.bob_weight = 0.0;
            }
            // Landing dip: one eased translation down
            // and back, scaled by the fall that caused
            // it. No bounce, no shake.
            if let Some(t) = self.dip_t {
                let t = t + FIXED_DT;
                let u = t / DIP_DURATION_S;
                if u >= 1.0 {
                    self.dip_t = None;
                } else {
                    self.dip_t = Some(t);
                    bob_y -= self.dip_amp * (std::f32::consts::PI * u).sin();
                }
            }
            // Idle breath: standing still, hands free,
            // unaimed — the faintest translation, and
            // only once the stillness beat has passed.
            let breath_legal = self.posture == Posture::Stand
                && !input.aim_held
                && grounded
                && self.sim.mode == BodyMode::Ground
                && !committed
                && self.still_ticks >= STILL_BEAT_TICKS;
            if breath_legal {
                self.breath_phase += FIXED_DT / BREATH_PERIOD_S * std::f32::consts::TAU;
                bob_y += BREATH_AMPLITUDE * self.breath_phase.sin();
            }
        }
        let bob_offset = Vec3::new(0.0, bob_y, 0.0);

        // ---- the rigs ----
        let look_dir = aim;
        let (camera, viewmodel_pose, body_awareness) = match self.mode {
            ViewMode::FirstPerson => {
                let position = eye + cam_right * (lean * LEAN_OFFSET) + bob_offset;
                let pose_hint = if committed {
                    if self.sim.grapple.is_latched() {
                        ViewmodelPose::LauncherSwing
                    } else {
                        ViewmodelPose::LauncherRaise
                    }
                } else if input.aim_held {
                    ViewmodelPose::Aim
                } else {
                    ViewmodelPose::LowReady
                };
                (
                    CameraState {
                        position,
                        target: position + look_dir * 10.0,
                        focus: position,
                        yaw: self.camera_yaw,
                        pitch: self.camera_pitch,
                        roll: self.roll,
                        fov_deg: self.fov_deg,
                        bob_offset,
                        boom_distance: 0.0,
                        boom_requested: 0.0,
                        boom_blocked: false,
                    },
                    Some(pose_hint),
                    self.camera_pitch < BODY_AWARENESS_PITCH,
                )
            }
            ViewMode::ThirdPerson => {
                // Boom geometry eases toward the wished
                // length / shoulder (aim tightens both);
                // collision pull-in applies instantly.
                let want_dist = if input.aim_held {
                    self.config.tp_aim_distance
                } else {
                    self.config.tp_distance
                };
                let want_shoulder = if input.aim_held {
                    self.config.tp_aim_shoulder
                } else {
                    self.config.tp_shoulder
                };
                self.boom = damp(self.boom, want_dist, TP_BOOM_EASE_RATE, FIXED_DT);
                self.shoulder = damp(self.shoulder, want_shoulder, TP_BOOM_EASE_RATE, FIXED_DT);
                let focus = feet + Vec3::new(0.0, self.config.tp_height, 0.0);
                let offset = cam_right * self.shoulder - look_dir * self.boom;
                let len = offset.length();
                let mut used = len;
                let mut blocked = false;
                if len > 1e-6 {
                    let dir = offset / len;
                    if let Some(hit) = world.raycast(focus, dir, len) {
                        let allowed = (hit.dist - TP_COLLISION_MARGIN).max(TP_MIN_BOOM);
                        if allowed < used {
                            used = allowed;
                            blocked = true;
                        }
                    }
                }
                let position = if len > 1e-6 {
                    focus + (offset / len) * used
                } else {
                    focus
                };
                (
                    CameraState {
                        position,
                        target: position + look_dir * 10.0,
                        focus,
                        yaw: self.camera_yaw,
                        pitch: self.camera_pitch,
                        roll: self.roll,
                        fov_deg: self.fov_deg,
                        bob_offset: Vec3::ZERO,
                        boom_distance: used,
                        boom_requested: len,
                        boom_blocked: blocked,
                    },
                    None,
                    false,
                )
            }
        };

        ControllerFrame {
            mode: self.mode,
            camera,
            eye,
            feet,
            velocity,
            body_yaw: self.body_yaw,
            body_forward: body_fwd,
            posture: self.posture,
            eye_height: self.eye_height,
            traversal_mode: self.sim.mode,
            speed: speed_h,
            grounded,
            figure_pose,
            weapon_open,
            viewmodel_pose,
            viewmodel_pitch: self.camera_pitch,
            viewmodel_config: self.config.viewmodel,
            body_awareness,
            lean,
        }
    }
}
