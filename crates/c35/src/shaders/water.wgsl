// Water: one quad to the horizon. All the life is in the fragment shader —
// analytic wave normals broken up by a noise gradient (so the far field
// never reads as machined stripes), fresnel reflection of the shared sky
// function, and a hard sun glitter path. Two triangles for an ocean.

struct WaterOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> WaterOut {
    // Two triangles spanning ±1500 units at sea level.
    let e = 1500.0;
    var corners = array<vec3<f32>, 6>(
        vec3<f32>(-e, 0.0, -e), vec3<f32>(e, 0.0, -e), vec3<f32>(-e, 0.0, e),
        vec3<f32>(-e, 0.0, e), vec3<f32>(e, 0.0, -e), vec3<f32>(e, 0.0, e),
    );
    var out: WaterOut;
    out.world_pos = corners[idx];
    out.clip = u.view_proj * vec4<f32>(corners[idx], 1.0);
    return out;
}

// Gradient of one directional sine wave, both axes at once.
fn wave_grad(p: vec2<f32>, dir: vec2<f32>, wavelength: f32, speed: f32, steep: f32, t: f32) -> vec2<f32> {
    let d = normalize(dir);
    let k = 6.28318 / wavelength;
    let phase = k * dot(d, p) - speed * t;
    return d * (steep * k * cos(phase));
}

// Numerical gradient of the shared value noise — organic breakup for the
// analytic waves, so repetition never shows.
fn noise_grad(p: vec2<f32>) -> vec2<f32> {
    let e = 0.4;
    let c = vnoise(p);
    return vec2<f32>(vnoise(p + vec2<f32>(e, 0.0)) - c, vnoise(p + vec2<f32>(0.0, e)) - c) / e;
}

fn water_normal(p: vec2<f32>, t: f32, dist: f32) -> vec3<f32> {
    var g = vec2<f32>(0.0);
    g += wave_grad(p, vec2<f32>(0.83, 0.55), 27.0, 0.85, 0.085, t);
    g += wave_grad(p, vec2<f32>(-0.42, 0.91), 13.0, 1.20, 0.060, t);
    g += wave_grad(p, vec2<f32>(0.97, -0.24), 6.1, 1.80, 0.038, t);
    g += wave_grad(p, vec2<f32>(-0.20, -0.98), 3.1, 2.50, 0.022, t);
    // Noise breakup: strong nearby, easing off with distance so the far
    // field calms instead of aliasing.
    let nfade = exp(-dist * 0.006);
    let ng = noise_grad(p * 0.33 + vec2<f32>(t * 0.14, t * 0.09)) * 0.16
        + noise_grad(p * 0.09 - vec2<f32>(t * 0.05, 0.0)) * 0.20;
    g += ng * nfade;
    return normalize(vec3<f32>(-g.x, 1.0, -g.y));
}

@fragment
fn fs_main(in: WaterOut) -> @location(0) vec4<f32> {
    let wp = in.world_pos;
    let to_cam = u.cam_pos.xyz - wp;
    let dist = length(to_cam);
    let view_dir = to_cam / max(dist, 0.001);

    let n = water_normal(wp.xz, u.misc.x, dist);

    let refl_dir = reflect(-view_dir, n);
    let sky = sky_color(vec3<f32>(refl_dir.x, abs(refl_dir.y), refl_dir.z));
    let deep = vec3<f32>(0.012, 0.085, 0.115);
    let shallow = vec3<f32>(0.05, 0.23, 0.24);

    let cos_theta = max(dot(view_dir, n), 0.0);
    let fresnel = 0.02 + 0.98 * pow(1.0 - cos_theta, 5.0);
    var base = mix(deep, shallow, exp(-dist * 0.004) * 0.35);
    var col = mix(base, sky, clamp(fresnel * 1.35, 0.0, 1.0));

    // The glitter path: tight Blinn specular straight off the sun.
    let half_v = normalize(view_dir + u.sun_dir.xyz);
    let ndh = max(dot(n, half_v), 0.0);
    col += u.sun_color.rgb * pow(ndh, 840.0) * 4.0;
    col += u.sun_color.rgb * pow(ndh, 90.0) * 0.15;

    col = apply_fog(col, wp);
    col += (hash21(wp.xz * 13.7) - 0.5) * 0.006; // dither against banding
    let alpha = mix(0.82, 1.0, clamp(fresnel * 1.6, 0.0, 1.0));
    return vec4<f32>(col, alpha);
}
