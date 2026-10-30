//! Comigen web host — the same engine, the same vista, in the browser.
//!
//! (This crate is wasm-only by construction — `SurfaceTarget::Canvas`
//! exists only on wasm32 — so the whole crate is cfg-gated to that
//! target; on native targets it compiles to an empty lib, which keeps a
//! plain workspace `cargo build` green.)
//!
//! Renders to a canvas through wgpu (WebGPU where the browser offers it,
//! WebGL2 otherwise — the ledger on the page names the backend that
//! actually rendered). The camera holds the hero pose for the first
//! `READY_FRAMES` frames so a screenshot at readiness is the proof pose,
//! then the drift begins. Frame times are frame-loop wall times measured
//! between rAF callbacks; the page says so.
//!
//! The reality layer drives this host too: the clock is the browser's
//! `Date` (the system clock), and the observer's position comes from the
//! Geolocation API when the visitor grants it. When geolocation is denied
//! or unavailable, the readout below the canvas SAYS SO and names the
//! fallback in force (the equator, with the browser's local UTC offset) —
//! the engine never silently pretends to know where it is. Location is
//! consumed locally by the engine; nothing is transmitted anywhere.
//!
//! There are no time or season controls on this page. The only pinning
//! seam is `window.__comigen_pin_unix`, settable solely by the proof rig
//! before the module loads — it is a measurement instrument, not a UI.
//! M6 adds one more rig seam of the same class: `window.__comigen_present`
//! set to "ps15" before the module loads selects the PS1.5 presentation
//! for the proof drive; the shipped page never sets it.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

const READY_FRAMES: u64 = 90;
const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;

/// What the page knows about where it is.
#[derive(Clone, Copy, PartialEq)]
enum Geo {
    /// Asked, waiting for the visitor's answer.
    Pending,
    /// Granted: (latitude, longitude) in degrees.
    Granted(f32, f32),
    /// Denied, timed out, or no geolocation on this browser.
    Unavailable,
}

impl Geo {
    fn describe(&self) -> String {
        match self {
            Geo::Pending => "geolocation pending — equator fallback in force for now".to_string(),
            Geo::Granted(lat, lon) => {
                format!("geolocation granted — lat {lat:.2}, lon {lon:.2} (used locally only)")
            }
            Geo::Unavailable => {
                "geolocation denied/unavailable — equator fallback in force".to_string()
            }
        }
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(run());
}

fn set_text(id: &str, text: &str) {
    let doc = web_sys::window().unwrap().document().unwrap();
    if let Some(el) = doc.get_element_by_id(id) {
        el.set_text_content(Some(text));
    }
}

fn window_number(name: &str) -> Option<f64> {
    let window = web_sys::window().unwrap();
    js_sys::Reflect::get(&window, &JsValue::from_str(name))
        .ok()
        .and_then(|v| v.as_f64())
}

/// Ask the browser for the visitor's position, once. The answer lands in
/// the shared state; the render loop picks it up when it arrives.
fn request_geolocation(geo: Rc<RefCell<Geo>>) {
    let window = web_sys::window().unwrap();
    let Ok(geolocation) = window.navigator().geolocation() else {
        *geo.borrow_mut() = Geo::Unavailable;
        set_text("location", &geo.borrow().describe());
        return;
    };
    let geo_ok = geo.clone();
    let on_ok = Closure::once(move |pos: web_sys::Position| {
        let coords = pos.coords();
        *geo_ok.borrow_mut() =
            Geo::Granted(coords.latitude() as f32, coords.longitude() as f32);
        set_text("location", &geo_ok.borrow().describe());
    });
    let geo_err = geo.clone();
    let on_err = Closure::once(move |_err: web_sys::PositionError| {
        *geo_err.borrow_mut() = Geo::Unavailable;
        set_text("location", &geo_err.borrow().describe());
    });
    let _ = geolocation.get_current_position_with_error_callback(
        on_ok.as_ref().unchecked_ref(),
        Some(on_err.as_ref().unchecked_ref()),
    );
    on_ok.forget();
    on_err.forget();
    set_text("location", &geo.borrow().describe());
}

/// The observer for this frame: geolocation when granted, else the stated
/// fallback (equator latitude, browser-local UTC offset for solar time).
fn current_observer(geo: &Geo) -> c35::world::Observer {
    match geo {
        Geo::Granted(lat, lon) => c35::world::Observer::new(*lat, *lon),
        _ => c35::world::Observer {
            latitude_deg: None,
            longitude_deg: None,
            // JS getTimezoneOffset() is minutes to ADD to local time to get
            // UTC; the engine wants minutes east of UTC.
            utc_offset_minutes: Some(-(js_sys::Date::new_0().get_timezone_offset() as f32)),
        },
    }
}

async fn run() {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas: HtmlCanvasElement = document
        .get_element_by_id("comigen-canvas")
        .expect("canvas element")
        .dyn_into()
        .expect("element is a canvas");
    canvas.set_width(WIDTH);
    canvas.set_height(HEIGHT);

    // The rig's pin (proof runs only; nothing on the page sets this).
    let pin_unix = window_number("__comigen_pin_unix");
    let clock_source = if pin_unix.is_some() {
        "rig pin (__comigen_pin_unix, measurement seam)".to_string()
    } else {
        "system clock (Date)".to_string()
    };

    let geo = Rc::new(RefCell::new(Geo::Pending));
    request_geolocation(geo.clone());

    let render_scale = 1.0f32;
    // The page probes navigator.gpu.requestAdapter() before init (see
    // index.html): where WebGPU yields no adapter — headless software
    // stacks are the common case — it flags "gl" and we build a GL-only
    // instance, because a canvas that has taken a webgpu context can never
    // become a webgl2 one. The on-page ledger names the backend that
    // actually rendered, either way.
    let force_gl = js_sys::Reflect::get(&window, &JsValue::from_str("__comigen_force_gl"))
        .map(|v| v.as_bool().unwrap_or(false))
        .unwrap_or(false);
    let instance = if force_gl {
        c35::create_instance_with(c35::wgpu::Backends::GL)
    } else {
        c35::create_instance()
    };
    let surface = instance
        .create_surface(c35::wgpu::SurfaceTarget::Canvas(canvas))
        .expect("create surface");
    let mut renderer = c35::Renderer::new_for_surface(
        &instance,
        &surface,
        WIDTH,
        HEIGHT,
        render_scale,
    )
    .await;
    let surface_config = c35::wgpu::SurfaceConfiguration {
        usage: c35::wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: renderer.output_format(), width: WIDTH, height: HEIGHT,
        present_mode: c35::wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: c35::wgpu::CompositeAlphaMode::Auto, view_formats: vec![],
    };
    surface.configure(&renderer.device, &surface_config);
    set_text("backend", &renderer.backend_name());

    // The rig's presentation seam (proof runs only; nothing on the
    // page sets this): "ps15" selects the PS1.5 register with its
    // default fog look multiplier.
    let present_arg = js_sys::Reflect::get(&window, &JsValue::from_str("__comigen_present"))
        .ok()
        .and_then(|v| v.as_string());
    if present_arg.as_deref() == Some("ps15") {
        renderer.set_present_mode(c35::PresentMode::Ps15, 2.0);
    }

    let perf = window.performance().unwrap();
    let state = Rc::new(RefCell::new(State {
        frames: 0,
        last_ms: perf.now(),
        deltas: Vec::new(),
        drift_start: None,
        ready: false,
    }));

    let cb: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let cb2 = cb.clone();
    *cb.borrow_mut() = Some(Closure::new(move || {
        let now = perf.now();
        let mut frame_dt = 0.0f32;
        // Reality for this frame.
        let geo_now = *geo.borrow();
        let unix = pin_unix.unwrap_or_else(|| js_sys::Date::now() / 1000.0);
        let world = c35::world::world_state(unix, &current_observer(&geo_now));
        {
            let mut st = state.borrow_mut();
            let delta = now - st.last_ms;
            st.last_ms = now;
            if delta >= 1000.0 {
                renderer.reset_history();
            }
            if st.frames > 0 && delta > 0.0 && delta < 1000.0 {
                renderer.observe_frame_ms(delta);
                st.deltas.push(delta);
                if st.deltas.len() > 240 {
                    st.deltas.remove(0);
                }
            }
            st.frames += 1;
            if st.frames >= READY_FRAMES && !st.ready {
                st.ready = true;
                st.drift_start = Some(now);
                set_text("status", "ready");
                let _ = js_sys::Reflect::set(
                    &web_sys::window().unwrap(),
                    &JsValue::from_str("__comigen_ready"),
                    &JsValue::TRUE,
                );
            }
            if let Some(start) = st.drift_start {
                frame_dt = ((now - start) / 1000.0) as f32;
            }
            if st.frames % 15 == 0 && !st.deltas.is_empty() {
                let deltas: Vec<f64> = st.deltas.iter().map(|d| *d as f64).collect();
                let (shadow_samples, scene_samples) = renderer.drain_gpu_samples();
                let gpu_scene = if scene_samples.is_empty() {
                    "gpu scene pass   : n/a (timestamp queries not exposed by this backend)".to_string()
                } else {
                    format!(
                        "gpu scene pass   : mean {:.2} ms [wgpu timestamp queries]",
                        c35::mean(&scene_samples)
                    )
                };
                let shadow_line = if renderer.shadow_active() {
                    if shadow_samples.is_empty() {
                        format!(
                            "shadow pass      : {} draw calls (gpu time n/a on this backend)",
                            renderer.shadow_draw_calls_per_frame()
                        )
                    } else {
                        format!(
                            "shadow pass      : {} draw calls, gpu mean {:.2} ms",
                            renderer.shadow_draw_calls_per_frame(),
                            c35::mean(&shadow_samples)
                        )
                    }
                } else {
                    "shadow pass      : skipped — sun below horizon (shadows are sun-only)".to_string()
                };
                let solar_src = match geo_now {
                    Geo::Granted(_, _) => "longitude (geolocation)".to_string(),
                    _ => "browser local UTC offset".to_string(),
                };
                let ledger = format!(
                    "=== COMIGEN COST LEDGER (web) ===\n\
                     backend          : {}\n\
                     output           : {}x{}\n\
                     render scale     : {:.2}\n\
                     presentation     : {}\n\
                     frames rendered  : {}\n\
                     frame time       : mean {:.2} ms, p95 {:.2} ms  [frame-loop wall time between rAF callbacks]\n\
                     {}\n\
                     {}\n\
                     anti-aliasing    : {}\n\
                     culling          : {}\n\
                     draw calls/frame : {}\n\
                     triangles/frame  : {}\n\
                     tracked vram     : {:.2} MiB (engine-tracked allocations, estimate)\n\
                     --- reality reading ---\n\
                     clock source     : {}\n\
                     instant (UTC)    : {}\n\
                     observer         : {}\n\
                     solar time src   : {}\n\
                     sun              : elevation {:+.2} deg, azimuth {:.1} deg, day length {:.1} h\n\
                     season           : cold {:.2} / autumn {:.2} / spring {:.2} / summer {:.2}\n\
                     land & water     : snow line {:+.1}, water ice {:.2}, wave scale {:.2}",
                    renderer.backend_name(),
                    WIDTH,
                    HEIGHT,
                    renderer.render_scale(),
                    renderer.present_description(),
                    st.frames,
                    c35::mean(&deltas),
                    c35::percentile(&deltas, 95.0),
                    gpu_scene,
                    shadow_line,
                    renderer.aa_description(),
                    renderer.culling_description(),
                    renderer.draw_calls_per_frame(),
                    renderer.triangles_per_frame(),
                    renderer.tracked_vram_bytes() as f64 / (1024.0 * 1024.0),
                    clock_source,
                    c35::world::iso_utc(world.unix_seconds),
                    geo_now.describe(),
                    solar_src,
                    world.sun_elevation_deg,
                    world.sun_azimuth_deg,
                    world.day_length_hours,
                    world.cold,
                    world.autumn,
                    world.spring,
                    world.summer,
                    world.snow_line,
                    world.water_ice,
                    world.water_wave_scale,
                );
                set_text("ledger", &ledger);
            }
        }
        let pose = c35::scene::camera_at(frame_dt);
        // (On wasm, wgpu's surface API is the Result form — the
        // CurrentSurfaceTexture enum is native-only.)
        match surface.get_current_texture() {
            Ok(frame) => {
                let view = frame
                    .texture
                    .create_view(&c35::wgpu::TextureViewDescriptor::default());
                renderer.render(&view, &pose, &world, frame_dt);
                frame.present();
            }
            Err(c35::wgpu::SurfaceError::Lost | c35::wgpu::SurfaceError::Outdated) => {
                surface.configure(&renderer.device, &surface_config);
                renderer.reset_history();
            }
            Err(c35::wgpu::SurfaceError::OutOfMemory) => {
                set_text("status", "GPU out of memory — rendering stopped");
                return;
            }
            Err(_) => {}
        }
        web_sys::window()
            .unwrap()
            .request_animation_frame(cb2.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));
    window
        .request_animation_frame(cb.borrow().as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
}

struct State {
    frames: u64,
    last_ms: f64,
    deltas: Vec<f64>,
    drift_start: Option<f64>,
    ready: bool,
}

