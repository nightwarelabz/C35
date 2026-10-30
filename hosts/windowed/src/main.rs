//! Comigen windowed host — the vista LIVE in a real window.
//!
//! This is the operator's viewing deck: the same renderer as the headless
//! proof host, on a real wgpu surface. The camera drifts on the engine's
//! orbit until the operator touches it, then it is a free-fly camera
//! (click to capture the mouse; WASD/QE to move, Shift for speed, Esc
//! releases). Leave it alone for a few seconds and the drift takes it
//! back.
//!
//! The reality layer drives it live: the system clock, and the observer's
//! latitude from COMIGEN_LAT (longitude from COMIGEN_LON, else the OS
//! local UTC offset). There are NO time or season controls here — the
//! window shows the world as it is, where you are, now.
//!
//!   COMIGEN_LAT=41.88 cargo run -p comigen-windowed
//!   COMIGEN_WEATHER=1   opt into clock-seeded rain and cloud (off by default)
//!
//! The cost ledger rides in the title bar (frame ms, GPU pass ms, draw
//! calls, backend, the sun's elevation and the season in force); the full
//! reality reading prints to stdout at startup.
//!
//! The quality governor runs live in this host: it holds the frame
//! against 16.7 ms and sheds its own costs (scale, shadows, detail,
//! water) when the machine can't. Keys **0–4 pin** ladder levels 0–4 —
//! a pin is absolute; the governor then only watches and reports. The
//! backquote key releases the pin. The title bar shows the level in
//! force either way. (These are quality controls, not time controls:
//! the no-time-controls law stands.)

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use c35::scene::CameraPose;
use c35::world::{iso_utc, world_state, Observer, WorldState};
use c35::Renderer;
use glam::Vec3;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

const IDLE_TO_DRIFT_S: f32 = 8.0;
const BLEND_S: f32 = 1.2;

fn env_f32(name: &str) -> Option<f32> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

fn system_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// The OS local UTC offset in minutes (the windowed host's solar-time
/// source when no longitude is given). Refreshed occasionally — offsets
/// change at DST boundaries.
fn local_offset_minutes() -> Option<f32> {
    time::OffsetDateTime::now_local()
        .ok()
        .map(|t| t.offset().whole_minutes() as f32)
}

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
        CameraPose {
            eye: self.pos,
            target: self.pos + self.forward(),
            fov_y_deg: c35::scene::HERO_POSE.fov_y_deg,
        }
    }
}

enum CamMode {
    Drift,
    Manual,
    /// Blending from a captured pose back to the drift orbit.
    Blend { from: CameraPose, t: f32 },
}

struct App {
    window: Option<Arc<Window>>,
    surface: Option<c35::wgpu::Surface<'static>>,
    surface_config: Option<c35::wgpu::SurfaceConfiguration>,
    renderer: Option<Renderer>,
    size: (u32, u32),
    scale: f32,

    lat: Option<f32>,
    lon: Option<f32>,
    offset_cache: Option<f32>,
    offset_checked: Instant,
    world: Option<WorldState>,

    mode: CamMode,
    fly: FlyCam,
    keys: HashSet<KeyCode>,
    grabbed: bool,
    last_input: Instant,
    start: Instant,
    last_frame: Instant,
    frame_ema_ms: f64,
    frame_count: u64,
    /// `COMIGEN_WEATHER=1` opts the vista into [`c35::weather`].
    weather: bool,
}

impl App {
    fn new() -> App {
        let hero = c35::scene::HERO_POSE;
        App {
            window: None,
            surface: None,
            surface_config: None,
            renderer: None,
            size: (1280, 720),
            scale: env_f32("COMIGEN_SCALE").unwrap_or(1.0),
            lat: env_f32("COMIGEN_LAT"),
            lon: env_f32("COMIGEN_LON"),
            offset_cache: local_offset_minutes(),
            offset_checked: Instant::now(),
            world: None,
            mode: CamMode::Drift,
            fly: FlyCam::from_pose(&hero),
            keys: HashSet::new(),
            grabbed: false,
            last_input: Instant::now(),
            start: Instant::now(),
            last_frame: Instant::now(),
            frame_ema_ms: 0.0,
            frame_count: 0,
            weather: std::env::var("COMIGEN_WEATHER").ok().as_deref() == Some("1"),
        }
    }

    fn observer(&mut self) -> Observer {
        if self.lon.is_none() && self.offset_checked.elapsed().as_secs() > 5 {
            self.offset_cache = local_offset_minutes();
            self.offset_checked = Instant::now();
        }
        Observer {
            latitude_deg: self.lat,
            longitude_deg: self.lon,
            utc_offset_minutes: if self.lon.is_some() {
                None
            } else {
                self.offset_cache
            },
        }
    }

    fn current_world(&mut self) -> WorldState {
        let observer = self.observer();
        world_state(system_now(), &observer)
    }

    fn drift_pose(&self) -> CameraPose {
        c35::scene::camera_at(self.start.elapsed().as_secs_f32())
    }

    fn touch(&mut self) {
        self.last_input = Instant::now();
        if matches!(self.mode, CamMode::Drift) {
            // Take the camera where the drift left it.
            self.fly = FlyCam::from_pose(&self.drift_pose());
            self.mode = CamMode::Manual;
        } else if matches!(self.mode, CamMode::Blend { .. }) {
            self.fly = FlyCam::from_pose(&self.current_pose());
            self.mode = CamMode::Manual;
        }
    }

    fn current_pose(&self) -> CameraPose {
        match &self.mode {
            CamMode::Drift => self.drift_pose(),
            CamMode::Manual => self.fly.pose(),
            CamMode::Blend { from, t } => {
                let target = self.drift_pose();
                let k = (*t / BLEND_S).clamp(0.0, 1.0);
                let k = k * k * (3.0 - 2.0 * k);
                CameraPose {
                    eye: from.eye.lerp(target.eye, k),
                    target: from.target.lerp(target.target, k),
                    fov_y_deg: from.fov_y_deg + (target.fov_y_deg - from.fov_y_deg) * k,
                }
            }
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

        match &mut self.mode {
            CamMode::Manual => {
                // Free-fly movement.
                let speed = if self.keys.contains(&KeyCode::ShiftLeft)
                    || self.keys.contains(&KeyCode::ShiftRight)
                {
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
                    self.last_input = Instant::now();
                }
                if self.last_input.elapsed().as_secs_f32() > IDLE_TO_DRIFT_S {
                    let from = self.fly.pose();
                    self.mode = CamMode::Blend { from, t: 0.0 };
                }
            }
            CamMode::Blend { t, .. } => {
                *t += dt;
                if *t >= BLEND_S {
                    self.mode = CamMode::Drift;
                }
            }
            CamMode::Drift => {}
        }

        let world = self.current_world();
        self.world = Some(world);
        let pose = self.current_pose();
        let time = self.start.elapsed().as_secs_f32();
        let weather_on = self.weather;

        // Phase 2 — GPU work (field borrows only; no &mut self calls).
        let mut outdated = false;
        {
            let surface = self.surface.as_ref().unwrap();
            let renderer = self.renderer.as_mut().unwrap();
            match surface.get_current_texture() {
                Ok(frame) => {
                    let view = frame
                        .texture
                        .create_view(&c35::wgpu::TextureViewDescriptor::default());
                    if weather_on {
                        renderer.set_atmosphere(c35::weather::Atmosphere::at(world.unix_seconds));
                    }
                    renderer.render(&view, &pose, &world, time);
                    frame.present();
                    // The governor eats the frame's wall time (vsync-bound
                    // on a real display — a healthy 60 Hz machine sits at
                    // target and the ladder simply never needs to move).
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
                        let season = dominant_season(&world);
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
                                "Comigen — {:.1} ms/frame · {}{} · {} draws · {} · sun {:+.1}° · {} · {}",
                                self.frame_ema_ms,
                                gpu,
                                shadow,
                                renderer.draw_calls_per_frame(),
                                renderer.backend_name(),
                                world.sun_elevation_deg,
                                season,
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
    }
}

fn dominant_season(w: &WorldState) -> String {
    let (name, v) = [
        ("cold", w.cold),
        ("autumn", w.autumn),
        ("spring", w.spring),
        ("summer", w.summer),
    ]
    .into_iter()
    .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    .unwrap();
    format!("{name} {v:.2}")
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Comigen")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let instance = c35::create_instance();
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let size = window.inner_size();
        self.size = (size.width.max(1), size.height.max(1));
        let mut renderer = c35::pollster::block_on(Renderer::new_for_surface(
            &instance,
            &surface,
            self.size.0,
            self.size.1,
            self.scale,
        ));
        renderer.resize(self.size.0, self.size.1);
        self.window = Some(window);
        self.surface = Some(surface);
        self.renderer = Some(renderer);
        self.configure_surface();

        // The startup ledger: what the engine is consuming, said plainly.
        let world = self.current_world();
        let lat_line = match self.lat {
            Some(v) => format!("{v:.2} (COMIGEN_LAT)"),
            None => "unknown — equator fallback in force".to_string(),
        };
        let solar_line = if let Some(lon) = self.lon {
            format!("longitude {lon:.2} (COMIGEN_LON)")
        } else if let Some(off) = self.offset_cache {
            format!("OS local UTC offset {off:.0} min")
        } else {
            "UTC (no local offset available)".to_string()
        };
        println!("=== COMIGEN (windowed) ===");
        println!("backend          : {}", self.renderer.as_ref().unwrap().backend_name());
        println!("anti-aliasing    : {}", self.renderer.as_ref().unwrap().aa_description());
        println!("governor         : live, target 16.7 ms (keys 0-4 pin a level, ` releases)");
        println!("clock source     : system clock (live, no pinning in the operator host)");
        println!("instant (UTC)    : {}", iso_utc(world.unix_seconds));
        println!("observer         : lat {lat_line}");
        println!("solar time src   : {solar_line}");
        println!(
            "sun              : elevation {:+.2} deg, azimuth {:.1} deg, day length {:.1} h",
            world.sun_elevation_deg, world.sun_azimuth_deg, world.day_length_hours
        );
        println!(
            "season           : cold {:.2} / autumn {:.2} / spring {:.2} / summer {:.2}",
            world.cold, world.autumn, world.spring, world.summer
        );
        println!("controls         : drift until input · click = mouse capture · WASD/QE fly · Shift = speed · Esc = release · idle 8 s = drift returns · 0-4 = governor pin · ` = unpin");
        if self.weather {
            let reading = c35::weather::Atmosphere::at(world.unix_seconds);
            println!("{}", reading.ledger_line());
        }
        self.world = Some(world);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
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
                self.touch();
                if !self.grabbed {
                    self.grab(true);
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        ..
                    },
                ..
            } => {
                if code == KeyCode::Escape && state == ElementState::Pressed {
                    if self.grabbed {
                        self.grab(false);
                    }
                    return;
                }
                // Governor pins: keys 0–4 pin ladder levels 0–4, backquote
                // releases. Pinning is not camera input — it must not take
                // the camera off the drift.
                if state == ElementState::Pressed {
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
                                Some(l) => println!(
                                    "[governor] pinned at level {l} — it will only watch and report"
                                ),
                                None => println!("[governor] pin released — governing live"),
                            }
                        }
                        return;
                    }
                }
                self.touch();
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

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.grabbed {
                self.touch();
                self.fly.yaw -= dx as f32 * 0.0021;
                self.fly.pitch = (self.fly.pitch - dy as f32 * 0.0021).clamp(-1.45, 1.45);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run app");
}
