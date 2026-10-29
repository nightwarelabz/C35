// Shared uniforms + procedural sky/fog — concatenated into every scene shader.

struct Uniforms {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    sun_dir: vec4<f32>,   // xyz: direction TO the sun
    sun_color: vec4<f32>, // rgb: HDR sun colour
    misc: vec4<f32>,      // x: time s, y: fog density, z: render scale, w: aspect
}

@group(0) @binding(0) var<uniform> u: Uniforms;

// The one sky function — used by the sky dome AND by the water for reflections,
// so the sea reflects exactly the sky that is drawn.
fn sky_color(dir: vec3<f32>) -> vec3<f32> {
    let sd = u.sun_dir.xyz;
    let t = clamp(dir.y, -1.0, 1.0);
    let zenith = vec3<f32>(0.075, 0.205, 0.45);
    let mid = vec3<f32>(0.35, 0.46, 0.66);
    let horizon = vec3<f32>(0.99, 0.60, 0.30);
    var col = mix(horizon, mid, smoothstep(0.0, 0.24, t));
    col = mix(col, zenith, smoothstep(0.16, 0.7, t));
    // Below the horizon: warm sea haze.
    col = mix(vec3<f32>(0.40, 0.34, 0.32), col, smoothstep(-0.10, 0.004, t));
    let sun_amt = max(dot(dir, sd), 0.0);
    col += vec3<f32>(1.0, 0.42, 0.14) * pow(sun_amt, 6.0) * 0.27;  // broad warm glow
    col += vec3<f32>(1.0, 0.60, 0.26) * pow(sun_amt, 150.0) * 0.6; // inner glow
    col += vec3<f32>(1.0, 0.82, 0.58) * smoothstep(0.99955, 0.99978, sun_amt) * 9.0; // disc
    return col;
}

// Exponential distance fog, tinted warm toward the sun (aerial perspective).
fn apply_fog(col: vec3<f32>, world_pos: vec3<f32>) -> vec3<f32> {
    let rel = world_pos - u.cam_pos.xyz;
    let dist = length(rel);
    let dir = rel / max(dist, 0.001);
    let f = 1.0 - exp(-dist * u.misc.y);
    let sun_amt = pow(max(dot(dir, u.sun_dir.xyz), 0.0), 5.0);
    let fog_col = mix(vec3<f32>(0.52, 0.46, 0.50), vec3<f32>(1.06, 0.57, 0.29), sun_amt);
    return mix(col, fog_col, clamp(f, 0.0, 1.0));
}

// Cheap deterministic 2D value noise for shader-side micro detail.
fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let s = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}
