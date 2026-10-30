// The operator (M6): rigid-skinned box parts over a generated atlas.
// Lit by the same reality-layer terms as the terrain — wrapped sun
// diffuse with the real shadow map, hemisphere ambient, the moon as
// key light at night — so he stands in the same world the vista does.
// The visor band's material flag switches its term to emissive: the
// banner's red band holds its own light, and the hot point the
// reference shows at the left of the band is painted into the atlas
// texels themselves (operator.rs paints it), so the glow is surface
// art, not a sprite.

struct OpIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) mat: f32,
}

struct OpOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) mat: f32,
}

@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_samp: sampler;

@vertex
fn vs_main(in: OpIn) -> OpOut {
    var out: OpOut;
    out.world_pos = in.position;
    out.normal = in.normal;
    out.uv = in.uv;
    out.mat = in.mat;
    out.clip = u.view_proj * vec4<f32>(in.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: OpOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas_tex, atlas_samp, in.uv);
    let n = normalize(in.normal);
    let amb = mix(u.amb_ground.rgb, u.amb_sky.rgb, n.y * 0.5 + 0.5);
    let ndl = dot(n, u.sun_dir.xyz);
    let shadow_raw = shadow_factor(in.world_pos, n);
    let shadow = mix(1.0, shadow_raw, smoothstep(-0.05, 0.25, ndl));
    let wrap = clamp((ndl + 0.38) / 1.38, 0.0, 1.0);
    let moon_diff = max(dot(n, u.moon_dir.xyz), 0.0);
    var col = tex.rgb
        * (u.sun_color.rgb * wrap * shadow * 0.82 + u.moon_color.rgb * moon_diff + amb);
    // Graphics push 2 — material separation: the atlas alpha is
    // per-texel ROUGHNESS (painted by operator.rs; the figure's
    // pipelines are opaque, so alpha was free). A Blinn term keyed
    // on it gives gunmetal, brass and oiled leather a tight glint
    // from sun and moon while cloth stays matte — the flat diffuse
    // alone read every surface as the same stuff. The cool rim
    // edges the silhouette against dark streets (the banner's
    // figure read), strongest on smooth surfaces, never a glow.
    let v = normalize(u.cam_pos.xyz - in.world_pos);
    let rough = tex.a;
    let spow = mix(90.0, 12.0, rough);
    let sgain = mix(0.80, 0.06, rough);
    let hs_o = normalize(u.sun_dir.xyz + v);
    col += u.sun_color.rgb
        * (pow(max(dot(n, hs_o), 0.0), spow) * sgain * shadow);
    let hm_o = normalize(u.moon_dir.xyz + v);
    col += u.moon_color.rgb
        * (pow(max(dot(n, hm_o), 0.0), spow) * sgain);
    let ndv = clamp(dot(n, v), 0.0, 1.0);
    let rim = pow(1.0 - ndv, 3.0);
    col += u.moon_color.rgb * rim * 0.20 * (1.0 - rough * 0.55);
    if (in.mat > 0.5) {
        // The visor: emissive accent. Its brightness rides the moon
        // key slightly (a lit face reads hotter than a turned one)
        // but never goes out — at the banner's hour it is the one
        // self-lit thing on the man.
        col += tex.rgb * (0.85 + 0.55 * moon_diff + 0.30 * wrap);
    }
    col = apply_fog(col, in.world_pos);
    col = ps15_grade(col, in.clip.xy);
    return vec4<f32>(col, 1.0);
}

// Shadow-caster entry: same geometry, depth only, drawn with the
// uniform-only depth bind group (group 1 is not bound in that pass
// and is not used here).
@vertex
fn vs_depth(in: OpIn) -> @builtin(position) vec4<f32> {
    return u.light_view_proj * vec4<f32>(in.position, 1.0);
}
