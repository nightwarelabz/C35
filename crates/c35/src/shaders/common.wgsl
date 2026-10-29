// Shared uniforms + procedural sky/fog/shadow — concatenated into every
// scene shader. M2: everything visual is driven by the reality layer
// (crates/c35/src/world.rs) through these uniforms — sun, moon, sky
// gradient, fog, season, water state. The shaders hold no opinions about
// what time it is; they render the state they are handed.

struct Uniforms {
    view_proj: mat4x4<f32>,       // jittered when TAA is live
    inv_view_proj: mat4x4<f32>,   // unjittered inverse (sky rays)
    light_view_proj: mat4x4<f32>, // sun shadow space
    cam_pos: vec4<f32>,
    sun_dir: vec4<f32>,     // xyz: direction TO the sun
    sun_color: vec4<f32>,   // rgb: HDR sun colour (black at night)
    sun_glow: vec4<f32>,    // rgb: sky glow colour · w: disc strength
    moon_dir: vec4<f32>,    // xyz: direction TO the moon
    moon_color: vec4<f32>,  // rgb: HDR moon key light · w: disc strength
    amb_sky: vec4<f32>,     // hemisphere ambient, up-facing
    amb_ground: vec4<f32>,  // hemisphere ambient, down-facing
    sky_zenith: vec4<f32>,
    sky_mid: vec4<f32>,
    sky_horizon: vec4<f32>,
    fog: vec4<f32>,         // rgb: fog colour · w: fog density
    season: vec4<f32>,      // x: cold · y: autumn · z: spring · w: snow line (world y)
    water_p: vec4<f32>,     // x: ice · y: wave scale · z: star intensity · w: daylight
    misc: vec4<f32>,        // x: time s · y: shadow strength · z: render scale · w: aspect
    quality: vec4<f32>,     // x: terrain detail fade multiplier · y: water wave octaves · z: shadow PCF texel size (0 = shadows shed)
}

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var shadow_tex: texture_depth_2d;
@group(0) @binding(2) var shadow_samp: sampler_comparison;

// The PCF texel size rides in the quality uniform: the governor resizes
// the shadow map (2048 → 1024 → off), and the filter footprint follows.

// The one sky function — used by the sky dome AND by the water for
// reflections, so the sea reflects exactly the sky that is drawn.
fn sky_color(dir: vec3<f32>) -> vec3<f32> {
    let sd = u.sun_dir.xyz;
    let t = clamp(dir.y, -1.0, 1.0);
    // The warm low-sun horizon belongs near the sun's azimuth; the far
    // side of the sky stays a cool pale band even at dawn.
    let sd_h = normalize(vec2<f32>(sd.x, sd.z) + vec2<f32>(1e-5, 0.0));
    let d_h = normalize(dir.xz + vec2<f32>(1e-5, 0.0));
    let az = 0.35 + 0.65 * pow(max(dot(d_h, sd_h), 0.0), 1.5);
    let low = 1.0 - smoothstep(0.08, 0.42, sd.y);
    let horizon_col = u.sky_horizon.rgb
        * mix(vec3<f32>(0.90, 0.96, 1.06), vec3<f32>(1.07, 0.96, 0.84), az * low);
    var col = mix(horizon_col, u.sky_mid.rgb, smoothstep(0.0, 0.24, t));
    col = mix(col, u.sky_zenith.rgb, smoothstep(0.16, 0.7, t));
    // Below the horizon: dark sea haze.
    col = mix(vec3<f32>(0.030, 0.035, 0.045), col, smoothstep(-0.10, 0.004, t));
    let sun_amt = max(dot(dir, sd), 0.0);
    col += u.sun_glow.rgb * pow(sun_amt, 6.0) * 0.35;   // broad glow
    col += u.sun_glow.rgb * pow(sun_amt, 150.0) * 0.7;  // inner glow
    col += u.sun_glow.rgb * smoothstep(0.99955, 0.99978, sun_amt) * u.sun_glow.w; // disc
    // The moon: a pale disc + halo, mirrored from the sun by the reality
    // layer (phase is not modelled — it is a light, not an ephemeris).
    let moon_amt = max(dot(dir, u.moon_dir.xyz), 0.0);
    col += u.moon_color.rgb * pow(moon_amt, 240.0) * 0.5;
    col += vec3<f32>(0.72, 0.78, 0.90) * smoothstep(0.99988, 0.99996, moon_amt) * u.moon_color.w;
    return col;
}

// Procedural star field: one hash cell per direction chunk, a soft point
// in the middle. Intensity arrives from the reality layer (sun elevation).
fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn star_field(dir: vec3<f32>) -> vec3<f32> {
    let p = dir * 160.0;
    let id = floor(p);
    let f = fract(p) - 0.5;
    let h = hash31(id);
    let d = length(f);
    let m = smoothstep(0.34, 0.03, d) * step(0.992, h);
    let tw = 0.78 + 0.22 * sin(u.misc.x * (1.0 + h * 3.0) + h * 40.0);
    let bright = 0.35 + 0.65 * fract(h * 7.31);
    return vec3<f32>(0.85, 0.90, 1.0) * (m * tw * bright);
}

// Sun shadow factor: 9-tap PCF over the single cascade, normal-offset to
// keep the heightfield off its own acne. misc.y fades shadows out as the
// sun grazes the horizon (where a 2048 map over 760 units cannot hold).
fn shadow_factor(world_pos: vec3<f32>, n: vec3<f32>) -> f32 {
    if (u.misc.y <= 0.001) {
        return 1.0;
    }
    let p = world_pos + n * 2.2;
    let lp = u.light_view_proj * vec4<f32>(p, 1.0);
    if (lp.w <= 0.0) {
        return 1.0;
    }
    let ndc = lp.xyz / lp.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z < 0.0 || ndc.z > 1.0) {
        return 1.0;
    }
    var sum = 0.0;
    for (var i = -1; i <= 1; i++) {
        for (var j = -1; j <= 1; j++) {
            sum += textureSampleCompare(
                shadow_tex, shadow_samp,
                uv + vec2<f32>(f32(i), f32(j)) * u.quality.z,
                ndc.z - 0.0012,
            );
        }
    }
    return mix(1.0, sum / 9.0, u.misc.y);
}

// Exponential distance fog; the fog colour comes from the reality layer,
// and the sun side of the fog still warms toward the glow.
fn apply_fog(col: vec3<f32>, world_pos: vec3<f32>) -> vec3<f32> {
    let rel = world_pos - u.cam_pos.xyz;
    let dist = length(rel);
    let dir = rel / max(dist, 0.001);
    let f = 1.0 - exp(-dist * u.fog.w);
    let sun_amt = pow(max(dot(dir, u.sun_dir.xyz), 0.0), 5.0);
    let fog_col = u.fog.rgb + u.sun_glow.rgb * sun_amt * 0.30;
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

// Two-octave value noise — the terrain detail grain.
fn vnoise2(p: vec2<f32>) -> f32 {
    return vnoise(p) * 0.65 + vnoise(p * 2.13 + 5.2) * 0.35;
}
