//! Comigen web host — the same engine, the same vista, in the browser.
//!
//! Renders to a canvas through wgpu (WebGPU where the browser offers it,
//! WebGL2 otherwise — the ledger on the page names the backend that
//! actually rendered). The camera holds the hero pose for the first
//! `READY_FRAMES` frames so a screenshot at readiness is the proof pose,
//! then the drift begins. Frame times are frame-loop wall times measured
//! between rAF callbacks; the page says so.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

const READY_FRAMES: u64 = 90;
const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;

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
    surface.configure(
        &renderer.device,
        &c35::wgpu::SurfaceConfiguration {
            usage: c35::wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: renderer.output_format(),
            width: WIDTH,
            height: HEIGHT,
            present_mode: c35::wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: c35::wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        },
    );
    set_text("backend", &renderer.backend_name());

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
        {
            let mut st = state.borrow_mut();
            let delta = now - st.last_ms;
            st.last_ms = now;
            if st.frames > 0 && delta > 0.0 && delta < 1000.0 {
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
                let gpu = renderer.drain_gpu_samples();
                let gpu_line = if gpu.is_empty() {
                    "gpu scene pass   : n/a (timestamp queries not exposed by this backend)".to_string()
                } else {
                    format!(
                        "gpu scene pass   : mean {:.2} ms [wgpu timestamp queries]",
                        c35::mean(&gpu)
                    )
                };
                let ledger = format!(
                    "=== COMIGEN COST LEDGER (web) ===\n\
                     backend          : {}\n\
                     output           : {}x{}\n\
                     render scale     : {:.2}\n\
                     frames rendered  : {}\n\
                     frame time       : mean {:.2} ms, p95 {:.2} ms  [frame-loop wall time between rAF callbacks]\n\
                     {}\n\
                     draw calls/frame : {}\n\
                     triangles/frame  : {}\n\
                     tracked vram     : {:.2} MiB (engine-tracked allocations, estimate)",
                    renderer.backend_name(),
                    WIDTH,
                    HEIGHT,
                    renderer.render_scale(),
                    st.frames,
                    c35::mean(&deltas),
                    c35::percentile(&deltas, 95.0),
                    gpu_line,
                    renderer.draw_calls_per_frame(),
                    renderer.triangles_per_frame(),
                    renderer.tracked_vram_bytes() as f64 / (1024.0 * 1024.0),
                );
                set_text("ledger", &ledger);
            }
        }
        let pose = c35::scene::camera_at(frame_dt);
        match surface.get_current_texture() {
            Ok(frame) => {
                let view = frame
                    .texture
                    .create_view(&c35::wgpu::TextureViewDescriptor::default());
                renderer.render(&view, &pose, frame_dt);
                frame.present();
            }
            Err(_) => {
                // Surface lost/outdated (tab resize etc.) — skip this frame.
            }
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
