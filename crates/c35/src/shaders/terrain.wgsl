// Terrain: generated heightfield mesh, procedural colour banding in-shader.

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

fn terrain_albedo(wp: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let up = n.y; // 1 = flat ground, 0 = cliff
    // Two-scale procedural variation so bands never look flat-shaded.
    let detail = vnoise(wp.xz * 0.11) * 0.6 + vnoise(wp.xz * 0.017) * 0.4;
    // Micro detail sampled top-down on gentle ground and side-on on cliffs,
    // so steep faces get grain instead of vertical streaks.
    let micro = mix(vnoise(wp.zy * 0.85), vnoise(wp.xz * 0.85), clamp(up, 0.0, 1.0));

    let grass = mix(vec3<f32>(0.115, 0.205, 0.058), vec3<f32>(0.315, 0.385, 0.115), detail);
    let dry = vec3<f32>(0.45, 0.38, 0.19);
    let sand = vec3<f32>(0.78, 0.63, 0.42);
    let rock = mix(vec3<f32>(0.235, 0.225, 0.225), vec3<f32>(0.40, 0.345, 0.29), micro * (0.35 + 0.65 * up));
    let snow = vec3<f32>(0.92, 0.94, 0.97);

    // Grass -> dry meadow as the ground rises and dries out.
    var col = mix(grass, dry, smoothstep(10.0, 30.0, wp.y) * 0.55);
    // Forest patches: deep green on gentle, low, noisy ground.
    let forest = smoothstep(0.55, 0.75, vnoise(wp.xz * 0.021 + 7.3))
        * smoothstep(2.0, 5.0, wp.y) * smoothstep(24.0, 13.0, wp.y)
        * smoothstep(0.62, 0.8, up);
    col = mix(col, vec3<f32>(0.055, 0.135, 0.05), forest * 0.75);
    // Sandy shore right at the waterline.
    col = mix(sand, col, smoothstep(0.35, 1.9, wp.y));
    // Rock takes over on steep slopes, with faint sedimentary strata.
    let strata = 1.0 + (sin(wp.y * 0.5 + micro * 2.2) * 0.5 + 0.5) * 0.16 * (1.0 - up);
    col = mix(rock * strata, col, smoothstep(0.52, 0.78, up));
    // Snow on high, gentle ground.
    let snow_amt = smoothstep(27.0, 38.0, wp.y + detail * 6.0) * smoothstep(0.55, 0.85, up);
    col = mix(col, snow, snow_amt);
    // Underwater: sand shading toward teal depth (seen through the surface),
    // then to the sea's own deep colour so the mesh edge vanishes below.
    let below = smoothstep(0.2, -5.5, wp.y);
    col = mix(col, vec3<f32>(0.10, 0.24, 0.235), below * 0.8);
    col = mix(col, vec3<f32>(0.012, 0.085, 0.115), smoothstep(-2.5, -9.0, wp.y));
    return col;
}

@fragment
fn fs_main(in: TerrainOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let wp = in.world_pos;
    let view_dir = normalize(u.cam_pos.xyz - wp);
    let albedo = terrain_albedo(wp, n);

    // Wrapped diffuse: golden-hour light wraps a little around the forms.
    let ndl = dot(n, u.sun_dir.xyz);
    let wrap = clamp((ndl + 0.38) / 1.38, 0.0, 1.0);
    // Hemisphere ambient: sky above, warm ground bounce below.
    let amb = mix(vec3<f32>(0.145, 0.115, 0.095), vec3<f32>(0.30, 0.375, 0.52), n.y * 0.5 + 0.5);
    var col = albedo * (u.sun_color.rgb * wrap * 0.82 + amb);

    // Wet sheen band at the shoreline: darker albedo + sun glints.
    let wet = smoothstep(1.1, 0.2, wp.y) * smoothstep(-0.4, 0.1, wp.y);
    col *= 1.0 - wet * 0.35;
    let refl = reflect(-u.sun_dir.xyz, n);
    let spec = pow(max(dot(refl, view_dir), 0.0), 70.0) * wet;
    col += u.sun_color.rgb * spec * 0.55;

    col = apply_fog(col, wp);
    return vec4<f32>(col, 1.0);
}
