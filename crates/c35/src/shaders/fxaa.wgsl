// FXAA blit: the anti-aliasing of the fallback path (wgpu's WebGL2
// backend, which cannot carry the TAA history resolve). A compact
// console-grade FXAA: luma edge detection, sub-pixel shift along the
// edge. Used only where TAA cannot run, and the ledger names it.

@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;

struct BlitOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> BlitOut {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: BlitOut;
    out.clip = vec4<f32>(corners[idx], 0.0, 1.0);
    out.uv = corners[idx] * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    return out;
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

@fragment
fn fs_main(in: BlitOut) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(scene_tex));
    let uv = in.uv;
    let m = textureSample(scene_tex, scene_sampler, uv).rgb;
    let nw = textureSample(scene_tex, scene_sampler, uv + vec2<f32>(-1.0, -1.0) * texel).rgb;
    let ne = textureSample(scene_tex, scene_sampler, uv + vec2<f32>(1.0, -1.0) * texel).rgb;
    let sw = textureSample(scene_tex, scene_sampler, uv + vec2<f32>(-1.0, 1.0) * texel).rgb;
    let se = textureSample(scene_tex, scene_sampler, uv + vec2<f32>(1.0, 1.0) * texel).rgb;

    let lm = luma(m);
    let lnw = luma(nw);
    let lne = luma(ne);
    let lsw = luma(sw);
    let lse = luma(se);
    let lmin = min(lm, min(min(lnw, lne), min(lsw, lse)));
    let lmax = max(lm, max(max(lnw, lne), max(lsw, lse)));
    if (lmax - lmin < max(0.0312, lmax * 0.125)) {
        return vec4<f32>(m, 1.0); // no edge worth touching
    }

    // Edge direction from the luma cross.
    var dir = vec2<f32>(
        -((lnw + lne) - (lsw + lse)),
        ((lnw + lsw) - (lne + lse)),
    );
    let dir_reduce = max((lnw + lne + lsw + lse) * 0.03125, 1.0 / 128.0);
    let rcp = 1.0 / (min(abs(dir.x), abs(dir.y)) + dir_reduce);
    dir = clamp(dir * rcp, vec2<f32>(-8.0), vec2<f32>(8.0)) * texel;

    let a = 0.5 * (
        textureSample(scene_tex, scene_sampler, uv + dir * (1.0 / 3.0 - 0.5)).rgb
        + textureSample(scene_tex, scene_sampler, uv + dir * (2.0 / 3.0 - 0.5)).rgb
    );
    let b = a * 0.5 + 0.25 * (
        textureSample(scene_tex, scene_sampler, uv + dir * -0.5).rgb
        + textureSample(scene_tex, scene_sampler, uv + dir * 0.5).rgb
    );
    let lb = luma(b);
    var out_col = b;
    if (lb < lmin || lb > lmax) {
        out_col = a; // the wider tap left the local range — fall back
    }
    return vec4<f32>(out_col, 1.0);
}
