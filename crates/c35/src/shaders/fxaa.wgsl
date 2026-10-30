// FXAA blit: the anti-aliasing of the fallback path (wgpu's WebGL2
// backend, which cannot carry the TAA history resolve). A compact
// console-grade FXAA: luma edge detection, sub-pixel shift along the
// edge. Used only where TAA cannot run, and the ledger names it.
//
// Graphics push: bindings 2–3 carry the lamp bloom, added after
// the FXAA filter so halos stay soft light and edges stay filtered.

@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
@group(0) @binding(2) var bloom_tex: texture_2d<f32>;
@group(0) @binding(3) var bloom_samp: sampler;

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
    // Solidity tune: the old threshold let faint wire edges pass
    // untouched, so they shimmered under the 2.2x upscale. Catch
    // them (0.024 / 10%) — but clamp the shift tighter below, so
    // the smoothing never drags colour across a solid face.
    // The branch below is if/else, not an early return, so the
    // bloom add at the tail reaches every pixel — flat bright
    // areas (a lit window's face, lamp glass) bloom exactly like
    // filtered edges do.
    var out_col: vec3<f32>;
    if (lmax - lmin < max(0.024, lmax * 0.10)) {
        out_col = m; // no edge worth touching
    } else {

    // Edge direction from the luma cross.
    var dir = vec2<f32>(
        -((lnw + lne) - (lsw + lse)),
        ((lnw + lsw) - (lne + lse)),
    );
    let dir_reduce = max((lnw + lne + lsw + lse) * 0.03125, 1.0 / 128.0);
    let rcp = 1.0 / (min(abs(dir.x), abs(dir.y)) + dir_reduce);
    dir = clamp(dir * rcp, vec2<f32>(-6.0), vec2<f32>(6.0)) * texel;

    let a = 0.5 * (
        textureSample(scene_tex, scene_sampler, uv + dir * (1.0 / 3.0 - 0.5)).rgb
        + textureSample(scene_tex, scene_sampler, uv + dir * (2.0 / 3.0 - 0.5)).rgb
    );
    let b = a * 0.5 + 0.25 * (
        textureSample(scene_tex, scene_sampler, uv + dir * -0.5).rgb
        + textureSample(scene_tex, scene_sampler, uv + dir * 0.5).rgb
    );
    let lb = luma(b);
    out_col = b;
    if (lb < lmin || lb > lmax) {
        out_col = a; // the wider tap left the local range — fall back
    }
    }
    // Lamp bloom, added after the filter: halos stay soft light,
    // edges stay FXAA'd.
    let halo = textureSample(bloom_tex, bloom_samp, uv).rgb;
    return vec4<f32>(out_col + halo * 0.55, 1.0);
}
