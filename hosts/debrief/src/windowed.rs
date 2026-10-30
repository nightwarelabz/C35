//! DeBrief — windowed mode: the Slice 1 block, LIVE in a real window.
//!
//! The same scene build as the headless proof host (shared
//! `configure_scene`, shared camera rigs, shared drive routes — if a
//! still and the window disagree, the window is wrong), on a real
//! wgpu surface. PS1.5 presentation by default. The patrol walks his
//! loop in real time on the same fixed 60 Hz simulation step the
//! headless drives use, his pose frame carried from the very update
//! that stepped him; the operator stands his post. This is a camera
//! and a living street, not the game yet: no player control of
//! either man.
//!
//! Controls (the comigen-windowed rig — the owner's hands know it):
//!   click          capture the mouse      Esc   release it
//!   WASD / QE      fly                    Shift speed
//!   F1 / F2        the hero-street / patrol-echo camera rigs, held
//!   F3 / F4        run the open / shadow crossing live, follow cam
//!   0–4 / `        governor pin / release (starts pinned at 0, the
//!                  proof rig's law; a pin is absolute)
//!   P              capture a still of the current frame (PNG, --out)
//!
//! The ledger rides the title bar: frame ms, GPU ms, draws, backend,
//! presentation, governor state — footage carries its own proof.
//!
//! Proof hooks (no interaction needed):
//!   --smoke <secs> run the living street for N seconds, then exit 0
//!   --proof        F1 capture → F2 capture → both crossings live →
//!                  exit 0 (CSVs + frames land in --out)

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use c35::glam::{Vec2, Vec3};
use c35::locomotion::{AnimInput, Animator, Controller};
use c35::operator::{OperatorMesh, PoseFrame};
use c35::scene::CameraPose;
use c35::world::WorldState;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::{
    block::Block, configure_scene, drive_route, echo_pose, flat_ground, follow_pose, hero_pose,
    merge_meshes, op_mesh, patrol_legs, print_reality, soldier, Reality,
};

const STEP: f32 = 1.0 / 60.0;
/// The operator's post — the hero still's placement, stood in the
/// street: spot (-2.0, 0, 3.0), yaw 168°, stand-ready.
const POST: Vec3 = Vec3::new(-2.0, 0.0, 3.0);
const POST_YAW_DEG: f32 = 168.0;

struct FlyCam {
    pos: Vec3,
    yaw: f32,
    pitch: f32,
}

impl FlyCam {
    fn from_pose(pose: &CameraPose) -> FlyCam {
        let dir = (pose.target - pose.eye).normalize_or_zero();
        FlyCam {
            pos: pose.eye,
            yaw: (-dir.x).atan2(-dir.z),
            pitch: dir.y.clamp(-1.0, 1.0).asin(),
        }
    }
    fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(-sy * cp, sp, -cy * cp).normalize()
    }
    fn pose(&self) -> CameraPose {
        CameraPose { eye: self.pos, target: self.pos + self.forward(), fov_y_deg: 50.0 }
    }
}

enum CamMode {
    /// Free-fly (seeded from a rig when input takes a held frame).
    Fly,
    /// A canonical rig, held exactly until fly input arrives.
    Bookmark(CameraPose),
    /// The follow rig, while a crossing runs.
    Drive,
}

/// A crossing running live — the headless drive's state machine,
/// stepped on the same fixed step, logging the same light read.
struct DriveRun {
    which: &'static str,
    ctrl: Controller,
    anim: Animator,
    anchor: Vec3,
    /// The operator's pose frame from the last step (the headless
    /// drive renders the frame its step produced; so do we).
    last_frame: PoseFrame,
    csv: String,
    frames: usize,
    lit_frames: usize,
    settle: usize,
}

impl DriveRun {
    fn new(which: &'static str) -> DriveRun {
        let (start, legs) = drive_route(which);
        let ctrl = Controller::new(start, legs);
        let mut anim = Animator::new();
        anim.set_ground_fn(Some(flat_ground));
        // Warm standing at the start, exactly as the headless drive.
        let a0 = *ctrl.agent();
        let warm = AnimInput {
            pos: Vec3::new(a0.pos.x, 0.0, a0.pos.y),
            yaw: a0.yaw,
            speed: 0.0,
            crouch: 0.0,
            aim: 0.0,
            slide: 0.0,
            carry: true,
        };
        let last_frame = anim.update(0.0, &warm);
        DriveRun {
            which,
            ctrl,
            anim,
            anchor: Vec3::new(a0.pos.x, 0.0, a0.pos.y),
            last_frame,
            csv: String::from(
                "frame,t,op_x,op_z,gait,speed,exposure,lit,lamp,soldier_x,soldier_z,soldier_yaw_deg,facing_dot,soldier_dist\n",
            ),
            frames: 0,
            lit_frames: 0,
            settle: 0,
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum ProofStage {
    Off,
    F1Wait,
    F2Wait,
    OpenDrive,
    ShadowDrive,
    Done,
}

pub struct Params {
    pub out_dir: String,
    pub present: String,
    pub width: u32,
    pub height: u32,
    pub proof: bool,
    pub smoke_secs: Option<f32>,
}

struct App {
    // DROP ORDER IS LOAD-BEARING (the windowed-host exit segfault,
    // G1 GL/Mesa + local Wayland/GL repro exit 139): the surface must
    // die before the device, and both before the window, while the
    // event loop's display connection is still alive. run() tears
    // these three down explicitly, in this order, before returning;
    // this declaration order is the same order, as backstop.
    surface: Option<c35::wgpu::Surface<'static>>,
    surface_config: Option<c35::wgpu::SurfaceConfiguration>,
    renderer: Option<c35::Renderer>,
    window: Option<Arc<Window>>,
    size: (u32, u32),

    blk: Block,
    world: WorldState,
    present: String,
    out_dir: String,

    // The living street's simulation (fixed 60 Hz steps).
    patrol_ctrl: Controller,
    patrol_anim: Animator,
    patrol_frame: PoseFrame,
    drive: Option<DriveRun>,
    acc: f32,
    sim_time: f32,
    sim_steps: u64,
    last_mesh: Option<OperatorMesh>,

    mode: CamMode,
    fly: FlyCam,
    keys: HashSet<KeyCode>,
    grabbed: bool,
    start: Instant,
    last_frame: Instant,
    frame_ema_ms: f64,
    frame_count: u64,
    capture_count: u32,

    proof: ProofStage,
    proof_frames_at_stage: u64,
    smoke_secs: Option<f32>,
    should_exit: bool,
}

impl App {
    fn new(blk: Block, world: WorldState, params: &Params) -> App {
        let patrol_ctrl = Controller::new(Vec2::new(-8.6, -40.0), patrol_legs());
        let mut patrol_anim = Animator::new();
        patrol_anim.set_ground_fn(Some(flat_ground));
        // Warm the patrol standing at his loop's start (headless rig).
        let pa0 = *patrol_ctrl.agent();
        let warm = AnimInput {
            pos: Vec3::new(pa0.pos.x, 0.0, pa0.pos.y),
            yaw: pa0.yaw,
            speed: 0.0,
            crouch: 0.0,
            aim: 0.0,
            slide: 0.0,
            carry: true,
        };
        let patrol_frame = patrol_anim.update(0.0, &warm);
        App {
            window: None,
            surface: None,
            surface_config: None,
            renderer: None,
            size: (params.width.max(1), params.height.max(1)),
            blk,
            world,
            present: params.present.clone(),
            out_dir: params.out_dir.clone(),
            patrol_ctrl,
            patrol_anim,
            patrol_frame,
            drive: None,
            acc: 0.0,
            sim_time: 0.0,
            sim_steps: 0,
            last_mesh: None,
            mode: CamMode::Bookmark(hero_pose()),
            fly: FlyCam::from_pose(&hero_pose()),
            keys: HashSet::new(),
            grabbed: false,
            start: Instant::now(),
            last_frame: Instant::now(),
            frame_ema_ms: 0.0,
            frame_count: 0,
            capture_count: 0,
            proof: if params.proof { ProofStage::F1Wait } else { ProofStage::Off },
            proof_frames_at_stage: 0,
            smoke_secs: params.smoke_secs,
            should_exit: false,
        }
    }

    fn current_pose(&self) -> CameraPose {
        match &self.mode {
            CamMode::Fly => self.fly.pose(),
            CamMode::Bookmark(p) => *p,
            CamMode::Drive => match &self.drive {
                Some(d) => follow_pose(d.anchor, d.ctrl.agent().yaw),
                None => self.fly.pose(),
            },
        }
    }

    /// Fly input takes the camera: a held rig seeds the fly cam.
    fn touch(&mut self) {
        if matches!(self.mode, CamMode::Bookmark(_)) {
            self.fly = FlyCam::from_pose(&self.current_pose());
            self.mode = CamMode::Fly;
        }
    }

    fn grab(&mut self, on: bool) {
        let Some(window) = &self.window else { return };
        if on {
            if window.set_cursor_grab(CursorGrabMode::Locked).is_err() {
                let _ = window.set_cursor_grab(CursorGrabMode::Confined);
            }
            window.set_cursor_visible(false);
            self.grabbed = true;
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            self.grabbed = false;
        }
    }

    fn configure_surface(&mut self) {
        let (Some(surface), Some(renderer)) = (&self.surface, &self.renderer) else {
            return;
        };
        let config = c35::wgpu::SurfaceConfiguration {
            usage: c35::wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: renderer.output_format(),
            width: self.size.0,
            height: self.size.1,
            present_mode: c35::wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: c35::wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        surface.configure(&renderer.device, &config);
        self.surface_config = Some(config);
    }

    fn start_drive(&mut self, which: &'static str) {
        if self.drive.is_some() {
            return;
        }
        println!("[drive] {which} crossing — live, follow camera");
        self.drive = Some(DriveRun::new(which));
        self.mode = CamMode::Drive;
    }

    fn finish_drive(&mut self) {
        let Some(d) = self.drive.take() else { return };
        // The camera stays where the follow rig left it, handed to
        // the fly cam; the operator returns to his post.
        self.fly = FlyCam::from_pose(&follow_pose(d.anchor, d.ctrl.agent().yaw));
        self.mode = CamMode::Fly;
        let path = format!("{}/drive-{}-windowed.csv", self.out_dir, d.which);
        if let Err(e) = std::fs::write(&path, &d.csv) {
            eprintln!("[drive] could not write {path}: {e}");
        }
        println!(
            "[drive] {} complete: {} steps, {:.1} m travelled, lit {}/{} ({:.1}%) — log {path}",
            d.which,
            d.frames,
            d.ctrl.agent().distance,
            d.lit_frames,
            d.frames,
            100.0 * d.lit_frames as f64 / d.frames.max(1) as f64,
        );
        match self.proof {
            ProofStage::OpenDrive => {
                self.proof = ProofStage::ShadowDrive;
                self.start_drive("shadow");
            }
            ProofStage::ShadowDrive => {
                self.proof = ProofStage::Done;
                println!("[proof] sequence complete — exiting");
                self.should_exit = true;
            }
            _ => {}
        }
    }

    /// One fixed simulation step: the patrol always; the crossing
    /// while one runs. The light read is logged exactly as the
    /// headless drive logs it — same field, same columns.
    fn step_sim(&mut self) {
        self.sim_time += STEP;
        self.sim_steps += 1;
        // The patrol's step (loop restarts seamlessly, as headless).
        let p_input = self.patrol_ctrl.update(STEP);
        self.patrol_frame = self.patrol_anim.update(STEP, &p_input);
        if self.patrol_ctrl.agent().done {
            self.patrol_ctrl = Controller::new(Vec2::new(-8.6, -40.0), patrol_legs());
        }
        let Some(d) = &mut self.drive else { return };
        let input = d.ctrl.update(STEP);
        d.last_frame = d.anim.update(STEP, &input);
        let a = *d.ctrl.agent();
        let pa = *self.patrol_ctrl.agent();
        let i = d.frames;
        // THE LIGHT READ — the same field that baked the street,
        // sampled at the operator's chest, logged against the
        // patrol's facing. Not an AI: a measurement.
        let sample_h = 1.15 - 0.35 * input.crouch;
        let sample = Vec3::new(a.pos.x, sample_h, a.pos.y);
        let exposure = self.blk.field.exposure(sample);
        let lit = self.blk.field.lit(sample);
        if lit {
            d.lit_frames += 1;
        }
        let lamp = self.blk.field.nearest_lamp(sample).map(|(idx, _)| idx).unwrap_or(usize::MAX);
        let s_fwd = Vec3::new(pa.yaw.sin(), 0.0, pa.yaw.cos());
        let to_op = Vec3::new(a.pos.x - pa.pos.x, 0.0, a.pos.y - pa.pos.y);
        let dist = to_op.length();
        let facing_dot = if dist > 1e-4 { s_fwd.dot(to_op / dist) } else { 0.0 };
        d.csv.push_str(&format!(
            "{},{:.3},{:.2},{:.2},{},{:.2},{:.3},{},{},{:.2},{:.2},{:.1},{:.3},{:.2}\n",
            i,
            i as f32 / 60.0,
            a.pos.x,
            a.pos.y,
            a.gait.name(),
            a.speed,
            exposure,
            if lit { 1 } else { 0 },
            lamp,
            pa.pos.x,
            pa.pos.y,
            pa.yaw.to_degrees(),
            facing_dot,
            dist
        ));
        d.frames += 1;
        // The follow anchor's smoothing, the headless rig's own law.
        let target_anchor = Vec3::new(a.pos.x, d.anim.ground_y(), a.pos.y);
        let k = 1.0 - (-STEP / 0.14f32).exp();
        d.anchor = d.anchor + (target_anchor - d.anchor) * k;
        if a.done && a.speed < 0.05 {
            d.settle += 1;
        }
    }

    /// Both men, posed and merged through the engine's character
    /// slot — the headless host's construction, unchanged.
    fn build_mesh(&self) -> OperatorMesh {
        let pa = *self.patrol_ctrl.agent();
        let so = soldier::soldier_mesh(
            &self.patrol_frame,
            Vec3::new(pa.pos.x, 0.0, pa.pos.y),
            self.patrol_anim.ground_y(),
            pa.yaw.to_degrees(),
        );
        let op = match &self.drive {
            Some(d) => {
                let a = *d.ctrl.agent();
                c35::operator::build_operator_frame(
                    &d.last_frame,
                    Vec3::new(a.pos.x, 0.0, a.pos.y),
                    d.anim.ground_y(),
                    a.yaw.to_degrees(),
                )
            }
            None => op_mesh(&PoseFrame::from_pose(c35::OperatorPose::StandReady), POST, POST_YAW_DEG),
        };
        merge_meshes(&op, &so)
    }

    /// A still of the current live state, taken the headless way:
    /// twelve renders of the frozen state into an offscreen target
    /// in the surface's own format, read back to a PNG (channel
    /// order corrected when the surface is BGRA — the headless
    /// rig's format is RGBA, and the file must read the same).
    fn capture(&mut self, path: &str) {
        let pose = self.current_pose();
        let world = self.world;
        let t = self.sim_time;
        let (w, h) = self.size;
        let Some(renderer) = &mut self.renderer else { return };
        let Some(mesh) = &self.last_mesh else { return };
        renderer.update_operator(mesh);
        let format = renderer.output_format();
        let tex = renderer.device.create_texture(&c35::wgpu::TextureDescriptor {
            label: Some("debrief window capture"),
            size: c35::wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: c35::wgpu::TextureDimension::D2,
            format,
            usage: c35::wgpu::TextureUsages::RENDER_ATTACHMENT | c35::wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        for _ in 0..12 {
            renderer.render(&view, &pose, &world, t);
            let _ = renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });
        }
        // Readback (the headless save_png's shape, format-aware).
        let bpr = (w * 4).div_ceil(256) * 256;
        let buf = renderer.device.create_buffer(&c35::wgpu::BufferDescriptor {
            label: Some("capture readback"),
            size: (bpr * h) as u64,
            usage: c35::wgpu::BufferUsages::MAP_READ | c35::wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = renderer.device.create_command_encoder(&c35::wgpu::CommandEncoderDescriptor {
            label: Some("capture copy"),
        });
        encoder.copy_texture_to_buffer(
            c35::wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: c35::wgpu::Origin3d::ZERO,
                aspect: c35::wgpu::TextureAspect::All,
            },
            c35::wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: c35::wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bpr),
                    rows_per_image: Some(h),
                },
            },
            c35::wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        renderer.queue.submit([encoder.finish()]);
        buf.slice(..).map_async(c35::wgpu::MapMode::Read, |_| {});
        let _ = renderer.device.poll(c35::wgpu::PollType::Wait { submission_index: None, timeout: None });
        let range = buf.slice(..).get_mapped_range();
        let swap_rb = format == c35::wgpu::TextureFormat::Bgra8UnormSrgb;
        let mut pixels = Vec::with_capacity((w * h * 4) as usize);
        for row in 0..h as usize {
            let start = row * bpr as usize;
            let line = &range[start..start + (w * 4) as usize];
            if swap_rb {
                for px in line.chunks_exact(4) {
                    pixels.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
                }
            } else {
                pixels.extend_from_slice(line);
            }
        }
        drop(range);
        buf.unmap();
        let img = image::RgbaImage::from_raw(w, h, pixels).expect("pixel buffer size");
        img.save(path).expect("save png");
        println!("saved {path}");
    }

    fn proof_advance(&mut self) {
        match self.proof {
            ProofStage::F1Wait => {
                if self.frame_count - self.proof_frames_at_stage >= 30 {
                    let p = format!("{}/window-f1-hero.png", self.out_dir);
                    self.capture(&p);
                    self.mode = CamMode::Bookmark(echo_pose());
                    self.proof = ProofStage::F2Wait;
                    self.proof_frames_at_stage = self.frame_count;
                    println!("[proof] F1 captured; F2 (patrol-echo rig) held");
                }
            }
            ProofStage::F2Wait => {
                if self.frame_count - self.proof_frames_at_stage >= 30 {
                    let p = format!("{}/window-f2-echo.png", self.out_dir);
                    self.capture(&p);
                    self.proof = ProofStage::OpenDrive;
                    println!("[proof] F2 captured; open crossing next");
                    self.start_drive("open");
                }
            }
            _ => {}
        }
    }

    fn draw(&mut self) {
        if self.surface.is_none() || self.renderer.is_none() {
            return;
        }
        // Phase 1 — housekeeping (all &mut self, no GPU borrows held).
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        self.frame_ema_ms = if self.frame_ema_ms == 0.0 {
            dt as f64 * 1000.0
        } else {
            self.frame_ema_ms * 0.95 + dt as f64 * 1000.0 * 0.05
        };

        // Free-fly movement (not while a crossing owns the camera).
        if matches!(self.mode, CamMode::Fly) {
            let speed = if self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight) {
                90.0
            } else {
                22.0
            };
            let fwd = self.fly.forward();
            let right = fwd.cross(Vec3::Y).normalize_or_zero();
            let mut mv = Vec3::ZERO;
            if self.keys.contains(&KeyCode::KeyW) {
                mv += fwd;
            }
            if self.keys.contains(&KeyCode::KeyS) {
                mv -= fwd;
            }
            if self.keys.contains(&KeyCode::KeyD) {
                mv += right;
            }
            if self.keys.contains(&KeyCode::KeyA) {
                mv -= right;
            }
            if self.keys.contains(&KeyCode::KeyE) {
                mv += Vec3::Y;
            }
            if self.keys.contains(&KeyCode::KeyQ) {
                mv -= Vec3::Y;
            }
            if mv != Vec3::ZERO {
                self.fly.pos += mv.normalize() * speed * dt;
            }
        }

        // The living street, on fixed steps however fast we render.
        self.acc += dt;
        let mut steps = 0usize;
        while self.acc >= STEP && steps < 30 {
            self.step_sim();
            self.acc -= STEP;
            steps += 1;
        }
        if steps == 30 {
            self.acc = 0.0;
        }
        // A finished crossing hands back (settle law: 45 still steps,
        // the headless cap of 20000 steps kept).
        if let Some(d) = &self.drive {
            if d.settle >= 45 || d.frames >= 20000 {
                self.finish_drive();
            }
        }
        self.last_mesh = Some(self.build_mesh());
        let pose = self.current_pose();
        let world = self.world;
        let time = self.sim_time;

        // Phase 2 — GPU work (field borrows only; no &mut self calls).
        let mut outdated = false;
        {
            let surface = self.surface.as_ref().unwrap();
            let renderer = self.renderer.as_mut().unwrap();
            if let Some(mesh) = &self.last_mesh {
                renderer.update_operator(mesh);
            }
            match surface.get_current_texture() {
                Ok(frame) => {
                    let view = frame.texture.create_view(&c35::wgpu::TextureViewDescriptor::default());
                    renderer.render(&view, &pose, &world, time);
                    frame.present();
                    renderer.observe_frame_ms(dt as f64 * 1000.0);
                    self.frame_count += 1;

                    // The ledger rides the title bar.
                    if self.frame_count % 30 == 0 {
                        let (shadow_ms, scene_ms) = renderer.drain_gpu_samples();
                        let gpu = if scene_ms.is_empty() {
                            "gpu n/a".to_string()
                        } else {
                            format!("gpu {:.1} ms", c35::mean(&scene_ms))
                        };
                        let shadow = if renderer.shadow_active() && !shadow_ms.is_empty() {
                            format!(" + shadow {:.1} ms", c35::mean(&shadow_ms))
                        } else {
                            String::new()
                        };
                        let gov = {
                            let g = renderer.governor();
                            if g.is_pinned() {
                                format!("gov pinned L{}", g.level())
                            } else {
                                format!("gov L{}", g.level())
                            }
                        };
                        if let Some(window) = &self.window {
                            window.set_title(&format!(
                                "DeBrief — Greyport · {:.1} ms/frame · {}{} · {} draws · {} tris · {} · {} · {}",
                                self.frame_ema_ms,
                                gpu,
                                shadow,
                                renderer.draw_calls_per_frame(),
                                renderer.triangles_per_frame(),
                                renderer.backend_name(),
                                renderer.present_description(),
                                gov,
                            ));
                        }
                    }
                }
                Err(c35::wgpu::SurfaceError::Outdated) => outdated = true,
                Err(_) => {}
            }
        }
        if outdated {
            self.configure_surface();
        }
        self.proof_advance();
        if let Some(secs) = self.smoke_secs {
            if self.start.elapsed().as_secs_f32() >= secs {
                println!(
                    "[smoke] {:.1} s up: {} frames rendered, {} sim steps, patrol at ({:.1}, {:.1})",
                    secs,
                    self.frame_count,
                    self.sim_steps,
                    self.patrol_ctrl.agent().pos.x,
                    self.patrol_ctrl.agent().pos.y
                );
                self.should_exit = true;
            }
        }
    }
}

pub fn run(blk: Block, reality: &Reality, params: Params) {
    print_reality(reality);
    let mut app = App::new(blk, reality.world, &params);
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.run_app(&mut app).expect("run app");
    println!(
        "windowed session ended: {} frames rendered, {} sim steps ({:.1} s of street time)",
        app.frame_count, app.sim_steps, app.sim_time
    );
    // TEARDOWN ORDER — THE EXIT-SEGFAULT FIX. The session runs fine
    // and this line prints, then the old code segfaulted: run()'s
    // locals drop the event loop before the app, and App's fields
    // dropped the window before the device, so the renderer's drop
    // (its timestamp buffer is the last Arc keeping wgpu's Global
    // alive) dropped Mesa's EGL instance into a dead display
    // connection — local Wayland/GL repro exit 139, backtrace
    // through libEGL_mesa into libwayland-client's marshal. Tear the
    // three down here, in this order, while the event loop is still
    // alive: surface, then device, then window. The loop then drops
    // last with nothing GPU or window left behind. Teardown-only;
    // no behaviour, scene, or presentation change.
    drop(app.surface.take());
    drop(app.renderer.take());
    drop(app.window.take());
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("DeBrief — Greyport")
            .with_inner_size(winit::dpi::LogicalSize::new(self.size.0 as f64, self.size.1 as f64));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let instance = c35::create_instance();
        let surface = instance.create_surface(window.clone()).expect("create surface");
        let size = window.inner_size();
        self.size = (size.width.max(1), size.height.max(1));
        let mut renderer = c35::pollster::block_on(c35::Renderer::new_for_surface(
            &instance,
            &surface,
            self.size.0,
            self.size.1,
            1.0,
        ));
        renderer.resize(self.size.0, self.size.1);
        // THE SAME scene build as the headless rig — one definition.
        configure_scene(&mut renderer, &self.blk, &self.present);
        self.window = Some(window);
        self.surface = Some(surface);
        self.renderer = Some(renderer);
        self.configure_surface();

        let r = self.renderer.as_ref().unwrap();
        println!("=== DEBRIEF (windowed) ===");
        println!("backend          : {}", r.backend_name());
        println!("anti-aliasing    : {}", r.aa_description());
        println!("presentation     : {}", r.present_description());
        println!("surface format   : {:?}", r.output_format());
        println!(
            "output           : {}x{} (internal {}x{})",
            self.size.0,
            self.size.1,
            r.internal_size().0,
            r.internal_size().1
        );
        println!("governor         : pinned at level 0 (proof rig law; 0-4 re-pin, ` releases to live)");
        println!("controls         : click = mouse capture · WASD/QE fly · Shift = speed · Esc = release · F1/F2 = hero/echo rigs · F3/F4 = open/shadow crossing · P = still");
        self.start = Instant::now();
        self.last_frame = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.size = (size.width.max(1), size.height.max(1));
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(self.size.0, self.size.1);
                }
                self.configure_surface();
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } => {
                if !matches!(self.mode, CamMode::Drive) {
                    self.touch();
                }
                if !self.grabbed {
                    self.grab(true);
                }
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(code), state, .. },
                ..
            } => {
                if code == KeyCode::Escape && state == ElementState::Pressed {
                    if self.grabbed {
                        self.grab(false);
                    }
                    return;
                }
                if state == ElementState::Pressed {
                    // Governor pins (not camera input).
                    let pin = match code {
                        KeyCode::Digit0 => Some(Some(0)),
                        KeyCode::Digit1 => Some(Some(1)),
                        KeyCode::Digit2 => Some(Some(2)),
                        KeyCode::Digit3 => Some(Some(3)),
                        KeyCode::Digit4 => Some(Some(4)),
                        KeyCode::Backquote => Some(None),
                        _ => None,
                    };
                    if let Some(level) = pin {
                        if let Some(renderer) = &mut self.renderer {
                            renderer.pin_governor(level);
                            match level {
                                Some(l) => println!("[governor] pinned at level {l}"),
                                None => println!("[governor] pin released — governing live"),
                            }
                        }
                        return;
                    }
                    match code {
                        KeyCode::F1 => {
                            if !matches!(self.mode, CamMode::Drive) {
                                self.mode = CamMode::Bookmark(hero_pose());
                                println!("[camera] F1 — hero-street rig");
                            }
                            return;
                        }
                        KeyCode::F2 => {
                            if !matches!(self.mode, CamMode::Drive) {
                                self.mode = CamMode::Bookmark(echo_pose());
                                println!("[camera] F2 — patrol-echo rig");
                            }
                            return;
                        }
                        KeyCode::F3 => {
                            self.start_drive("open");
                            return;
                        }
                        KeyCode::F4 => {
                            self.start_drive("shadow");
                            return;
                        }
                        KeyCode::KeyP => {
                            self.capture_count += 1;
                            let p = format!("{}/window-capture-{}.png", self.out_dir, self.capture_count);
                            self.capture(&p);
                            return;
                        }
                        _ => {}
                    }
                }
                if !matches!(self.mode, CamMode::Drive) {
                    self.touch();
                }
                match state {
                    ElementState::Pressed => {
                        self.keys.insert(code);
                    }
                    ElementState::Released => {
                        self.keys.remove(&code);
                    }
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: winit::event::DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.grabbed && !matches!(self.mode, CamMode::Drive) {
                self.touch();
                self.fly.yaw -= dx as f32 * 0.0021;
                self.fly.pitch = (self.fly.pitch - dy as f32 * 0.0021).clamp(-1.45, 1.45);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
        if self.should_exit {
            event_loop.exit();
            return;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
