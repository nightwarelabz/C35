// Weather draws (see crates/c35/src/weather.rs).
//
// Two entry pairs, one uniform:
//   vs_cloud / fs_cloud — one fullscreen triangle, blended over the
//     sky before terrain covers the ground. Cover comes from the
//     weather uniform. Inactive hours never issue the draw.
//   vs_rain / fs_rain — one camera-facing quad, two triangles,
//     RAIN_SHEET_DISTANCE metres in front of the eye. Streaks are a
//     column hash and a falling window. Depth-tested, not depth-
//     written, so a nearer wall occludes them.
//
// No textures. No lamp buffer — night streaks pick up a warm cast
// from the reality layer's daylight term and the drop's height
// relative to the eye, which is where Greyport's sodium lives.
// The shared uniform block is group 1; group 0 is the scene block
// every other shader uses. Neither draw resizes that block.

struct WeatherU {
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    cam_fwd: vec4<f32>,
    // x rain, y wetness, z wind x, w wind z
    params: vec4<f32>,
    // x cloud cover, y tan(fov/2)*aspect, z tan(fov/2), w sheet distance
    extra: vec4<f32>,
}
@group(1) @binding(0) var<uniform> w: WeatherU;

struct CloudOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

@vertex
fn vs_cloud(@builtin(vertex_index) idx: u32) -> CloudOut {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: CloudOut;
    out.ndc = corners[idx];
    out.clip = vec4<f32>(corners[idx], 1.0, 1.0);
    return out;
}

@fragment
fn fs_cloud(in: CloudOut) -> @location(0) vec4<f32> {
    let p = u.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let dir = normalize(p.xyz / p.w - u.cam_pos.xyz);
    let cover = w.extra.x;
    // Below the haze line the sky is already the sea. No veil.
    if (cover < 0.001 || dir.y < 0.02) {
        return vec4<f32>(0.0);
    }
    let drift = w.params.zw * u.misc.x * 0.004;
    let uv = dir.xz / max(dir.y, 0.08) * 0.15 + drift;
    let n = vnoise(uv) * 0.65 + vnoise(uv * 2.1 + vec2<f32>(4.0, 1.2)) * 0.35;
    let edge = 1.0 - cover;
    let dens = smoothstep(edge - 0.08, edge + 0.22, n);
    if (dens < 0.004) {
        return vec4<f32>(0.0);
    }
    // A cool grey of the sky that is already up there, never a
    // second sun. Night skies are already dark, so the veil is too.
    var col = mix(u.sky_zenith.rgb, u.sky_horizon.rgb, 0.55);
    col *= vec3<f32>(0.72, 0.76, 0.84);
    col += (hash21(in.ndc * 400.0) - 0.5) * 0.008;
    let alpha = dens * 0.62;
    return vec4<f32>(ps15_grade(col, in.clip.xy), alpha);
}

struct RainOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
}

@vertex
fn vs_rain(@builtin(vertex_index) idx: u32) -> RainOut {
    var corner = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    );
    let c = corner[idx];
    let dist = max(w.extra.w, 1.0);
    let center = u.cam_pos.xyz + w.cam_fwd.xyz * dist;
    let world = center
        + w.cam_right.xyz * c.x * w.extra.y * dist * 1.08
        + w.cam_up.xyz * c.y * w.extra.z * dist * 1.08;
    var out: RainOut;
    out.world_pos = world;
    out.clip = u.view_proj * vec4<f32>(world, 1.0);
    return out;
}

// One streak column. `hash > rain` columns are empty, so a drizzle
// is a few lines and a downpour fills the sheet. The falling window
// is a fract of world height plus time, drifted by wind.
fn rain_streak(world: vec3<f32>) -> f32 {
    let rain = w.params.x;
    let wind = w.params.zw;
    let t = u.misc.x;
    let drifted = world.xz + wind * t * 0.35;
    let column = drifted.x * 2.4;
    let id = floor(column);
    let fx = fract(column) - 0.5;
    let h = hash21(vec2<f32>(id, 3.17));
    if (h > rain) {
        return 0.0;
    }
    let fall = fract(world.y * 0.55 + drifted.y * 0.08 - t * (3.5 + rain * 4.0) + h * 9.0);
    let width = 0.012 + h * 0.014;
    let line = 1.0 - smoothstep(0.0, width, abs(fx));
    let len = smoothstep(0.0, 0.05, fall) * (1.0 - smoothstep(0.28, 0.55, fall));
    return line * len;
}

@fragment
fn fs_rain(in: RainOut) -> @location(0) vec4<f32> {
    let streak = rain_streak(in.world_pos);
    if (streak < 0.004) {
        return vec4<f32>(0.0);
    }
    let rain = w.params.x;
    let daylight = clamp(u.water_p.w, 0.0, 1.0);
    let night = 1.0 - daylight;
    // Drops at or below the eye catch the street's warm cast.
    // Above the eye they stay a cool grey. No lamp list is read.
    let below = 1.0 - smoothstep(0.0, 10.0, in.world_pos.y - u.cam_pos.y);
    let warm = night * (0.25 + 0.75 * below);
    let tint = mix(vec3<f32>(0.58, 0.64, 0.74), vec3<f32>(1.0, 0.55, 0.22), warm);
    let alpha = streak * (0.20 + 0.55 * rain);
    var col = tint * (0.45 + 0.55 * night);
    col = apply_fog(col, in.world_pos);
    return vec4<f32>(ps15_grade(col, in.clip.xy), alpha);
}
