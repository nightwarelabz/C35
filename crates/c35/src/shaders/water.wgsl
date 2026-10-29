// Water: one quad to the horizon. All the life is in the fragment shader —
// analytic wave normals broken up by a noise gradient (so the far field
// never reads as machined stripes), fresnel reflection of the shared sky
// function, and a hard sun glitter path. Two triangles for an ocean.
// M2: the reality layer calms and freezes it — wave motion scales down as
// cold deepens, ice patches sheet over in deep winter with a hard pale
// glint, and at night the glitter path runs off the moon instead.
// M3: the governor sheds wave octaves (quality.y), and the frozen sheet
// roughens its own normals so the ice glint sparkles instead of striping.

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

fn water_normal(p: vec2<f32>, t: f32, dist: f32, amp: f32) -> vec3<f32> {
    var g = vec2<f32>(0.0);
    g += wave_grad(p, vec2<f32>(0.83, 0.55), 27.0, 0.85, 0.085, t);
    // The governor sheds wave octaves from the top down (quality.y):
    // the long swell always stays; the chop is what costs.
    let oct = i32(u.quality.y + 0.5);
    if (oct >= 2) {
        g += wave_grad(p, vec2<f32>(-0.42, 0.91), 13.0, 1.20, 0.060, t);
    }
    if (oct >= 3) {
        g += wave_grad(p, vec2<f32>(0.97, -0.24), 6.1, 1.80, 0.038, t);
    }
    if (oct >= 4) {
        g += wave_grad(p, vec2<f32>(-0.20, -0.98), 3.1, 2.50, 0.022, t);
    }
    // Noise breakup: strong nearby, easing off with distance so the far
    // field calms instead of aliasing.
    let nfade = exp(-dist * 0.006);
    let ng = noise_grad(p * 0.33 + vec2<f32>(t * 0.14, t * 0.09)) * 0.16
        + noise_grad(p * 0.09 - vec2<f32>(t * 0.05, 0.0)) * 0.20;
    g += ng * nfade;
    return normalize(vec3<f32>(-g.x * amp, 1.0, -g.y * amp));
}

@fragment
fn fs_main(in: WaterOut) -> @location(0) vec4<f32> {
    let wp = in.world_pos;
    let to_cam = u.cam_pos.xyz - wp;
    let dist = length(to_cam);
    let view_dir = to_cam / max(dist, 0.001);

    let ice = u.water_p.x;
    let wave_scale = u.water_p.y;
    // Time itself slows as the sea freezes; amplitude dies with it.
    let t = u.misc.x * wave_scale;
    let amp = mix(1.0, 0.22, ice);
    var n = water_normal(wp.xz, t, dist, amp);
    if (ice > 0.01) {
        // M3: frozen water is not a mirror. As the amp dies the normal
        // field goes uniform and the sun glint smeared into wind-stripes
        // (M2 leftover) — so ice roughens the normal with its own grain
        // and the glint breaks into sparkle over the sheet instead.
        let ig = noise_grad(wp.xz * 0.85 + 2.3);
        n = normalize(n + vec3<f32>(-ig.x, 0.0, -ig.y) * 0.35 * ice);
    }

    let refl_dir = reflect(-view_dir, n);
    let sky = sky_color(vec3<f32>(refl_dir.x, abs(refl_dir.y), refl_dir.z));
    // Cold pales the sea's own colours toward grey-blue.
    let deep = mix(vec3<f32>(0.012, 0.085, 0.115), vec3<f32>(0.045, 0.075, 0.095), u.season.x * 0.6);
    let shallow = mix(vec3<f32>(0.05, 0.23, 0.24), vec3<f32>(0.10, 0.16, 0.17), u.season.x * 0.6);

    let cos_theta = max(dot(view_dir, n), 0.0);
    let fresnel = 0.02 + 0.98 * pow(1.0 - cos_theta, 5.0);
    var base = mix(deep, shallow, exp(-dist * 0.004) * 0.35);
    // The sea's own body darkens with the daylight — unlit teal at
    // midnight would glow; the sky reflection carries the night.
    base *= 0.13 + 0.87 * u.water_p.w;
    var col = mix(base, sky, clamp(fresnel * 1.35, 0.0, 1.0));

    // Glitter paths: sun by day (shadowed by the terrain), moon by night.
    let shadow = shadow_factor(wp, vec3<f32>(0.0, 1.0, 0.0));
    let half_v = normalize(view_dir + u.sun_dir.xyz);
    let ndh = max(dot(n, half_v), 0.0);
    col += u.sun_color.rgb * (pow(ndh, 840.0) * 4.0 + pow(ndh, 90.0) * 0.15) * shadow;
    let mhalf = normalize(view_dir + u.moon_dir.xyz);
    let mndh = max(dot(n, mhalf), 0.0);
    col += u.moon_color.rgb * (pow(mndh, 700.0) * 2.2 + pow(mndh, 80.0) * 0.10);

    // Ice sheet: patchy by large-scale noise, sheeting over as the ice
    // factor grows — pale, hard-glinting, nearly matte between patches.
    if (ice > 0.01) {
        let ice_patch = smoothstep(0.78 - ice * 0.45, 0.92 - ice * 0.35, vnoise(wp.xz * 0.045 + 3.7));
        let ice_col = vec3<f32>(0.62, 0.70, 0.78) * (0.35 + 0.65 * u.water_p.w)
            + u.amb_sky.rgb * 0.5;
        var iced = ice_col;
        // The sheet's glint is noise-broken too: no clean band survives.
        let glint_break = 0.35 + 0.65 * vnoise(wp.xz * 1.4) * vnoise(wp.xz * 0.23 + 8.1);
        iced += u.sun_color.rgb * pow(ndh, 260.0) * 1.4 * shadow * glint_break;
        iced += u.moon_color.rgb * pow(mndh, 260.0) * 0.8 * glint_break;
        iced += sky * 0.18;
        col = mix(col, iced, ice_patch * ice * 0.92);
    }

    col = apply_fog(col, wp);
    col += (hash21(wp.xz * 13.7) - 0.5) * 0.006; // dither against banding
    let alpha = mix(0.82, 1.0, clamp(fresnel * 1.6, 0.0, 1.0));
    return vec4<f32>(col, alpha);
}
