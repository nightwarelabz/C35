//! The first-person player controller: input + camera
//! above the traversal sim.
//!
//! The split of labour is strict. [`TravSim`] owns the
//! body — capsule, gravity, grapple, wall-run, mantle,
//! and the mode priority Mantle > Grapple > WallRun >
//! Ground/Air. This controller owns what the player
//! MEANT and what the player SEES: it turns axes and
//! look deltas into the sim's `TravInput`, mirrors the
//! sim's mode instead of re-deciding it (while a
//! traversal mode owns the body, ground input is still
//! sampled and forwarded — the sim itself decides what
//! it may steer — and locomotion simply resumes on
//! exit), and it builds the frame's [`CameraState`]
//! from the simulated body: posture eye height, lean
//! geometry, eased FOV, and a head-bob that the stealth
//! law can silence to exactly zero.
//!
//! Time discipline: look deltas and posture are applied
//! once per `update`; the sim is stepped at its own
//! fixed 240 Hz from an accumulator, and pressed edges
//! (jump, grapple) are consumed by the FIRST substep
//! only, so one press can never fire twice inside one
//! frame. Same input sequence, same trajectory.

use crate::controller::camera::{
    CameraState, AIM_FOV_DEG, BASE_FOV_DEG, BOB_AMPLITUDE, BOB_STEALTH_SPEED, BOB_STRIDE,
    BOB_WEIGHT_RATE, FOV_EASE_RATE, LEAN_OFFSET, LEAN_ROLL_DEG, PITCH_LIMIT_DEG,
    ROLL_EASE_RATE, SPRINT_FOV_DEG, SWING_FOV_DEG, WALLRUN_ROLL_DEG,
};
use crate::controller::input::{
    ControllerConfig, PlayerInput, Posture, CROUCH_WALK_SPEED, POSTURE_TRANSITION_S,
    PRONE_CAPSULE, PRONE_EYE, SPRINT_SPEED, STAND_CAPSULE, STAND_EYE, WALK_SPEED,
};
use crate::traversal::physics::{StaticWorld, FIXED_DT};
use crate::traversal::wallrun::WallSide;
use crate::traversal::{BodyMode, TravInput, TravSim};
use glam::Vec3;

/// Physics substeps one update may run at most. At the
/// sim's 240 Hz this covers a 133 ms frame; past it the
/// backlog is dropped, not spiralled — a hitching host
/// slows the game for a frame rather than detonating
/// the step count.
pub const MAX_SUBSTEPS: u32 = 32;

/// Is the space between capsule heights `from_h` and
/// `to_h` clear above `feet`? A single upward raycast
/// against the static world — the traversal world query
/// exposes exactly this, so standing up under a lintel
/// is refused by geometry, not by hope.
fn headroom_clear(world: &StaticWorld, feet: Vec3, from_h: f32, to_h: f32) -> bool {
    if to_h <= from_h {
        return true;
    }
    let origin = feet + Vec3::new(0.0, from_h + 0.02, 0.0);
    world.raycast(origin, Vec3::Y, to_h - from_h).is_none()
}

fn move_toward(cur: f32, target: f32, max_delta: f32) -> f32 {
    if (target - cur).abs() <= max_delta {
        target
    } else {
        cur + (target - cur).signum() * max_delta
    }
}

/// Exponential approach: frame-rate independent, no
/// overshoot, most of the travel early.
fn damp(cur: f32, target: f32, rate: f32, dt: f32) -> f32 {
    cur + (target - cur) * (1.0 - (-rate * dt).exp())
}

/// The player controller. One per player; the host
/// feeds it input and a world, and reads back a camera.
pub struct PlayerController {
    /// The traversal sim that owns the body. Public so
    /// hosts can read events and pose state the camera
    /// does not mirror.
    pub sim: TravSim,
    config: ControllerConfig,
    yaw: f32,
    pitch: f32,
    posture: Posture,
    crouch_latched: bool,
    eye_height: f32,
    capsule_height: f32,
    fov_deg: f32,
    roll: f32,
    bob_phase: f32,
    bob_weight: f32,
    sprint_band: bool,
    accumulator: f32,
}

impl PlayerController {
    /// A controller standing at `start` (feet position),
    /// default configuration (hold-to-crouch).
    pub fn new(start: Vec3) -> PlayerController {
        PlayerController::with_config(start, ControllerConfig::default())
    }

    pub fn with_config(start: Vec3, config: ControllerConfig) -> PlayerController {
        PlayerController {
            sim: TravSim::new(start),
            config,
            yaw: 0.0,
            pitch: 0.0,
            posture: Posture::Stand,
            crouch_latched: false,
            eye_height: STAND_EYE,
            capsule_height: STAND_CAPSULE,
            fov_deg: BASE_FOV_DEG,
            roll: 0.0,
            bob_phase: 0.0,
            bob_weight: 0.0,
            sprint_band: false,
            accumulator: 0.0,
        }
    }

    /// The current posture (its target state; the
    /// heights in [`CameraState`] may still be easing
    /// toward it).
    pub fn posture(&self) -> Posture {
        self.posture
    }

    /// Horizontal facing for a camera yaw: the figure's
    /// convention — local -Z is forward, so facing is
    /// (-sin yaw, 0, -cos yaw). The same yaw the host
    /// hands `build_viewmodel` as `yaw_deg`.
    pub fn facing(&self) -> Vec3 {
        let (s, c) = self.yaw.sin_cos();
        Vec3::new(-s, 0.0, -c)
    }

    /// Advance one rendered frame: apply look + posture,
    /// step the sim at its fixed rate, and produce the
    /// frame's camera. `dt` is the frame's seconds
    /// (clamped at 0; the sim never reads a wall clock).
    pub fn update(
        &mut self,
        input: &PlayerInput,
        world: &StaticWorld,
        dt: f32,
    ) -> CameraState {
        let dt = dt.max(0.0);

        // ---- look: once per frame, never per substep ----
        self.yaw = wrap_angle(self.yaw + input.look_yaw_delta);
        let pitch_limit = PITCH_LIMIT_DEG.to_radians();
        self.pitch = (self.pitch + input.look_pitch_delta).clamp(-pitch_limit, pitch_limit);

        // ---- posture: resolve the target state ----
        if self.config.crouch_toggle && input.crouch_pressed {
            self.crouch_latched = !self.crouch_latched;
        }
        let crouch_want = if self.config.crouch_toggle {
            self.crouch_latched
        } else {
            input.crouch_held
        };
        let mut jump_edge = input.jump_pressed;
        let mut want = self.posture;
        if input.prone_pressed {
            want = if self.posture == Posture::Prone {
                Posture::Stand
            } else {
                Posture::Prone
            };
        } else if self.posture == Posture::Prone {
            if jump_edge {
                // A jump press from prone stands; the
                // press is spent standing, it does not
                // also jump.
                want = Posture::Stand;
                jump_edge = false;
            }
            // Otherwise prone holds; crouch is ignored.
        } else {
            want = if crouch_want {
                Posture::Crouch
            } else {
                Posture::Stand
            };
        }
        // Rising needs the room: a taller capsule under a
        // low ceiling defers the stand until it clears.
        // The latch/intent is untouched — only the state
        // waits.
        if want.capsule_height() > self.posture.capsule_height()
            && !headroom_clear(
                world,
                self.sim.body.feet,
                self.posture.capsule_height(),
                want.capsule_height(),
            )
        {
            want = self.posture;
        }
        self.posture = want;

        // ---- wish: camera-relative axes -> sim input ----
        let fwd = self.facing();
        let (s, c) = self.yaw.sin_cos();
        let right = Vec3::new(c, 0.0, -s);
        let raw_wish = fwd * input.move_forward + right * input.move_strafe;
        let mag = raw_wish.length();
        let (wish_dir, mag) = if mag > 1.0 {
            (raw_wish / mag, 1.0)
        } else if mag > 1e-6 {
            (raw_wish / mag, mag)
        } else {
            (Vec3::ZERO, 0.0)
        };
        let sprinting = self.posture == Posture::Stand
            && !input.aim_held
            && input.sprint_held
            && input.move_forward > 0.25
            && mag > 0.0;
        let band_speed = if self.posture != Posture::Stand {
            CROUCH_WALK_SPEED
        } else if sprinting {
            SPRINT_SPEED
        } else {
            WALK_SPEED
        };
        self.sprint_band = sprinting;
        let wish_speed = band_speed * mag;
        let cp = self.pitch.cos();
        let aim = Vec3::new(fwd.x * cp, self.pitch.sin(), fwd.z * cp);

        // ---- sim: fixed steps, edges fire once ----
        self.accumulator += dt;
        let mut steps = 0;
        let mut hook_edge = input.hook_press;
        while self.accumulator >= FIXED_DT && steps < MAX_SUBSTEPS {
            self.sim.tick(
                TravInput {
                    wish: wish_dir,
                    facing: fwd,
                    speed: wish_speed,
                    jump: jump_edge,
                    hook_press: hook_edge,
                    hook_held: input.hook_held,
                    crouch: self.posture != Posture::Stand,
                    aim,
                },
                world,
            );
            jump_edge = false;
            hook_edge = false;
            self.accumulator -= FIXED_DT;
            steps += 1;
        }
        if steps == MAX_SUBSTEPS {
            self.accumulator = 0.0;
        }

        // ---- posture heights: constant-rate travel ----
        let cap_rate = (STAND_CAPSULE - PRONE_CAPSULE) / POSTURE_TRANSITION_S;
        let eye_rate = (STAND_EYE - PRONE_EYE) / POSTURE_TRANSITION_S;
        self.capsule_height = move_toward(
            self.capsule_height,
            self.posture.capsule_height(),
            cap_rate * dt,
        );
        self.eye_height = move_toward(self.eye_height, self.posture.eye_height(), eye_rate * dt);

        // ---- camera law ----
        let speed_h = self.sim.body.horizontal_speed();
        let swinging = self.sim.mode == BodyMode::Grapple && self.sim.grapple.is_latched();
        let fov_target = if input.aim_held {
            AIM_FOV_DEG
        } else if swinging {
            SWING_FOV_DEG
        } else if self.sprint_band && speed_h > WALK_SPEED * 1.05 {
            SPRINT_FOV_DEG
        } else {
            BASE_FOV_DEG
        };
        self.fov_deg = damp(self.fov_deg, fov_target, FOV_EASE_RATE, dt);

        let lean = input.lean.clamp(-1.0, 1.0);
        let mut roll_target = lean * LEAN_ROLL_DEG.to_radians();
        if self.sim.mode == BodyMode::WallRun {
            if let Some(w) = &self.sim.wallrun {
                let toward_wall = match w.side {
                    WallSide::Left => -1.0,
                    WallSide::Right => 1.0,
                };
                roll_target += toward_wall * WALLRUN_ROLL_DEG.to_radians();
            }
        }
        self.roll = damp(self.roll, roll_target, ROLL_EASE_RATE, dt);

        // Head-bob, under the stealth law: legal only
        // standing, unaimed, grounded, in plain Ground
        // mode, at or above the stealth speed. Illegal
        // means exactly 0.0 this frame — no fade-out tail
        // leaking motion into a careful approach. Legal
        // fades IN by weight so resuming a walk does not
        // pop.
        let bob_legal = self.posture == Posture::Stand
            && !input.aim_held
            && self.sim.body.grounded
            && self.sim.mode == BodyMode::Ground
            && speed_h >= BOB_STEALTH_SPEED;
        let bob = if bob_legal {
            self.bob_weight = (self.bob_weight + BOB_WEIGHT_RATE * dt).min(1.0);
            self.bob_phase += speed_h * dt / BOB_STRIDE * std::f32::consts::TAU;
            let ramp = ((speed_h - BOB_STEALTH_SPEED) / (WALK_SPEED - BOB_STEALTH_SPEED))
                .clamp(0.0, 1.0);
            self.bob_phase.sin() * BOB_AMPLITUDE * self.bob_weight * ramp
        } else {
            self.bob_weight = 0.0;
            0.0
        };

        let eye = self.sim.body.feet
            + Vec3::new(0.0, self.eye_height, 0.0)
            + right * (lean * LEAN_OFFSET)
            + Vec3::new(0.0, bob, 0.0);

        CameraState {
            eye,
            feet: self.sim.body.feet,
            yaw: self.yaw,
            yaw_deg: self.yaw.to_degrees(),
            pitch: self.pitch,
            roll: self.roll,
            fov_deg: self.fov_deg,
            posture: self.posture,
            eye_height: self.eye_height,
            capsule_height: self.capsule_height,
            speed: speed_h,
            mode: self.sim.mode,
            aiming: input.aim_held,
            grounded: self.sim.body.grounded,
            bob_offset: bob,
            lean,
        }
    }
}

fn wrap_angle(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}
