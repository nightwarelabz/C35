// Terrain: generated heightfield mesh, procedural colour banding in-shader.
// M2: the banding obeys the reality layer — snow line and season palette
// arrive as uniforms; a fragment detail pass (multi-octave grain + normal
// perturbation, faded with distance) breaks up the smooth slopes; the sun
// term is modulated by the real shadow map; the moon is a faint key light.
// M3: the detail fade reach is governor-driven (quality.x), and the rock
// band is reworked — wall-scale mottling, deeper strata, cool shadow
// tint, talus aprons — so the headwall keeps its structure in shadow.

struct TerrainIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct TerrainOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

@vertex
fn vs_main(in: TerrainIn) -> TerrainOut {
    var out: TerrainOut;
    out.world_pos = in.position;
    out.normal = in.normal;
    out.clip = u.view_proj * vec4<f32>(in.position, 1.0);
    return out;
}

// Detail fade: full grain near the camera, gone by ~450 units so the far
// field cannot shimmer or stipple (each octave is also throttled by its
// own reach). The governor's detail step scales the reach (quality.x).
fn detail_fade(dist: f32) -> f32 {
    return exp(-dist * 0.0055 / max(u.quality.x, 0.05));
}

fn terrain_albedo(wp: vec3<f32>, n: vec3<f32>, grain: f32, fade: f32) -> vec3<f32> {
    let up = n.y; // 1 = flat ground, 0 = cliff
    let cold = u.season.x;
    let autumn_w = u.season.y;
    let spring_w = u.season.z;
    // Two-scale procedural variation so bands never look flat-shaded.
    let detail = vnoise(wp.xz * 0.11) * 0.6 + vnoise(wp.xz * 0.017) * 0.4;
    // Micro detail sampled top-down on gentle ground and side-on on cliffs,
    // so steep faces get grain instead of vertical streaks. Its contrast
    // dies with distance — at range the cells go subpixel and moiré.
    let micro_raw = mix(vnoise(wp.zy * 0.85), vnoise(wp.xz * 0.85), clamp(up, 0.0, 1.0));
    let micro = mix(0.5, micro_raw, clamp(fade * 1.6, 0.0, 1.0));

    var grass = mix(vec3<f32>(0.115, 0.205, 0.058), vec3<f32>(0.315, 0.385, 0.115), detail);
    // Spring: the green comes back bright and yellow-leaning.
    grass = mix(grass, grass * vec3<f32>(1.10, 1.28, 0.82) + vec3<f32>(0.015, 0.045, 0.0), spring_w * 0.65);
    let dry = vec3<f32>(0.45, 0.38, 0.19);
    let sand = vec3<f32>(0.78, 0.63, 0.42);
    // Rock. M3 — the headwall fix: the M2 band read as a flat curtain
    // wherever the light went uniform, because in shadow the whole face
    // takes one ambient tone and its albedo variation lived at micro
    // scale only — variation the detail fade had already calmed at
    // viewing distance. The variation now lives at wall scale too:
    // large mottling (top-down on ledges, face-projected on walls),
    // strata with real contrast that keeps some depth at range, and a
    // cool cast in the mottled lows — a shadowed face keeps its weather.
    let mottle_top = vnoise(wp.xz * 0.013 + 2.9);
    let mottle_face = vnoise(vec2<f32>(wp.x + wp.z * 0.7, wp.y) * 0.017 + 6.3);
    let mottle = mix(mottle_top, mottle_face, 1.0 - up) * 0.62
        + vnoise(wp.xz * 0.055 + 9.1) * 0.38;
    let strata_jitter = (grain - 0.5) * 1.9 + (mottle - 0.5) * 2.6;
    let bed = sin(wp.y * 0.5 + micro * 2.2 + strata_jitter) * 0.5 + 0.5;
    let strata = 0.84 + bed * 0.32 * (1.0 - up) * (0.45 + 0.55 * fade);
    var rock = mix(vec3<f32>(0.235, 0.225, 0.225), vec3<f32>(0.40, 0.345, 0.29), micro * (0.35 + 0.65 * up));
    rock *= 0.76 + 0.48 * mottle; // wall-scale weathering
    // Cool lows: the mottled hollows lean blue-grey, not brown.
    rock = mix(rock, rock * vec3<f32>(0.88, 0.96, 1.14), (1.0 - mottle) * 0.55 * (1.0 - up));
    rock *= strata;
    rock *= 0.86 + 0.28 * grain; // granular breakup in the rock band
    let snow = vec3<f32>(0.92, 0.94, 0.97);

    // Grass -> dry meadow as the ground rises and dries out.
    var col = mix(grass, dry, smoothstep(10.0, 30.0, wp.y) * 0.55);
    // Forest patches: deep green on gentle, low, noisy ground.
    let forest_m = smoothstep(0.55, 0.75, vnoise(wp.xz * 0.021 + 7.3))
        * smoothstep(2.0, 5.0, wp.y) * smoothstep(24.0, 13.0, wp.y)
        * smoothstep(0.62, 0.8, up);
    var veg = mix(col, vec3<f32>(0.055, 0.135, 0.05), forest_m * 0.75);
    // Autumn ambers everything that grows: meadow to amber, forest to rust.
    let amber = mix(vec3<f32>(0.66, 0.33, 0.075), vec3<f32>(0.78, 0.48, 0.13), detail);
    let rust = vec3<f32>(0.46, 0.17, 0.05);
    veg = mix(veg, mix(amber, rust, forest_m), autumn_w * 0.82);
    col = veg;
    // Sandy shore right at the waterline.
    col = mix(sand, col, smoothstep(0.35, 1.9, wp.y));
    // Rock takes over on steep slopes.
    col = mix(rock, col, smoothstep(0.52, 0.78, up));
    // Scree: a talus apron where the walls shed onto gentler, low
    // ground — lighter rubble, strongly grained, feathered by the
    // mottle so it fans like debris instead of ruling a band.
    let talus = smoothstep(0.80, 0.64, up) * smoothstep(0.38, 0.52, up)
        * smoothstep(16.0, 5.0, wp.y) * (0.45 + 0.55 * mottle);
    let rubble = vec3<f32>(0.46, 0.435, 0.40) * (0.78 + 0.44 * grain) * strata;
    col = mix(col, rubble, talus * 0.8);
    // Snow: the line arrives from the reality layer (deep cold puts it at
    // the shoreline), and cold snow clings to steeper ground than summer
    // snow. Patchy by grain so it feathers instead of ruling a line.
    let snow_slope = mix(0.55, 0.28, cold);
    let snow_amt = smoothstep(u.season.w, u.season.w + 9.0, wp.y + detail * 7.0 + (grain - 0.5) * 5.0)
        * smoothstep(snow_slope, snow_slope + 0.30, up);
    col = mix(col, snow, snow_amt);
    // Underwater: sand shading toward teal depth (seen through the surface),
    // then to the sea's own deep colour so the mesh edge vanishes below.
    let below = smoothstep(0.2, -5.5, wp.y);
    col = mix(col, vec3<f32>(0.10, 0.24, 0.235), below * 0.8);
    col = mix(col, vec3<f32>(0.012, 0.085, 0.115), smoothstep(-2.5, -9.0, wp.y));
    // Fine grain over everything above water — the plastic-killer.
    col *= 0.90 + 0.20 * grain;
    return col;
}

@fragment
fn fs_main(in: TerrainOut) -> @location(0) vec4<f32> {
    let wp = in.world_pos;
    let to_cam = u.cam_pos.xyz - wp;
    let dist = length(to_cam);
    let view_dir = to_cam / max(dist, 0.001);
    let fade = detail_fade(dist);

    // Grain field, computed once and shared by albedo + normal work.
    let grain = vnoise2(wp.xz * 0.55) * fade + vnoise(wp.xz * 0.05) * 0.35 * (1.0 - fade);

    // Normal perturbation from the grain gradient: near slopes stop
    // reading as smooth plastic. Fades with distance (no shimmer).
    var n = normalize(in.normal);
    if (fade > 0.01) {
        let e = 0.45;
        let g0 = vnoise2(wp.xz * 0.55);
        let gx = vnoise2((wp.xz + vec2<f32>(e, 0.0)) * 0.55);
        let gz = vnoise2((wp.xz + vec2<f32>(0.0, e)) * 0.55);
        let grad = vec2<f32>(gx - g0, gz - g0) / e;
        let strength = 0.16 * fade * smoothstep(0.25, 0.75, n.y);
        n = normalize(n + vec3<f32>(-grad.x, 0.0, -grad.y) * strength);
    }

    let albedo = terrain_albedo(wp, n, grain, fade);

    // Wrapped diffuse for the sun (a little light wraps the forms), with
    // the real shadow map on the sun term only. The shadow factor fades
    // out as the surface turns away from the sun: a back-facing wall is
    // lit by wrap alone, and holding a texel-exact cast shadow there —
    // where the wall's light-space depth runs parallel to its caster's —
    // bands into acne no bias can hold.
    let ndl = dot(n, u.sun_dir.xyz);
    let shadow_raw = shadow_factor(wp, n);
    let shadow = mix(1.0, shadow_raw, smoothstep(-0.05, 0.25, ndl));
    let wrap = clamp((ndl + 0.38) / 1.38, 0.0, 1.0);
    // Hemisphere ambient from the reality layer.
    let amb = mix(u.amb_ground.rgb, u.amb_sky.rgb, n.y * 0.5 + 0.5);
    // The moon: faint, hard, shadowless key light.
    let moon_diff = max(dot(n, u.moon_dir.xyz), 0.0);
    var col = albedo * (u.sun_color.rgb * wrap * shadow * 0.82 + u.moon_color.rgb * moon_diff + amb);

    // A shadowed wall cools toward blue-grey rather than holding one
    // flat brown: the tint rides the final shadow factor, so it lands
    // exactly where the sun does not.
    let rock_mask = (1.0 - smoothstep(0.52, 0.78, n.y)) * smoothstep(-0.5, 0.5, wp.y);
    col = mix(col, col * vec3<f32>(0.86, 0.94, 1.14), rock_mask * (1.0 - shadow) * 0.5);

    // Wet sheen band at the shoreline: darker albedo + sun/moon glints.
    let wet = smoothstep(1.1, 0.2, wp.y) * smoothstep(-0.4, 0.1, wp.y);
    col *= 1.0 - wet * 0.35;
    let refl = reflect(-u.sun_dir.xyz, n);
    let spec = pow(max(dot(refl, view_dir), 0.0), 70.0) * wet * shadow;
    col += u.sun_color.rgb * spec * 0.55;
    let mrefl = reflect(-u.moon_dir.xyz, n);
    let mspec = pow(max(dot(mrefl, view_dir), 0.0), 70.0) * wet;
    col += u.moon_color.rgb * mspec * 0.4;

    col = apply_fog(col, wp);
    return vec4<f32>(col, 1.0);
}
