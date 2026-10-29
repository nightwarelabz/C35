// Sky dome: one fullscreen triangle, pure math, no textures, no geometry.

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> SkyOut {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: SkyOut;
    out.ndc = corners[idx];
    out.clip = vec4<f32>(corners[idx], 1.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: SkyOut) -> @location(0) vec4<f32> {
    let p = u.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let dir = normalize(p.xyz / p.w - u.cam_pos.xyz);
    var col = sky_color(dir);
    // Subtle dither against gradient banding.
    col += (hash21(in.ndc * 731.7) - 0.5) * 0.008;
    return vec4<f32>(col, 1.0);
}
