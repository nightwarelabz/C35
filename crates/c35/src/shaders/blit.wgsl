// Blit: the final step to the output. On the temporal-reconstruction
// path the resolve already wrote at output resolution, so this is a
// straight copy; on the WebGL2 fallback path the FXAA variant of this
// pass (fxaa.wgsl, same bindings) runs over the internal target instead.
//
// Graphics push: bindings 2–3 carry the lamp bloom — the half-res
// halo the bloom chain left in A, sampled linear so the halo stays
// soft light over the scene's crisp texel grid. The halo is added
// after the scene sample, before the output encode, in the same
// linear space the scene sample arrives in.

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

@fragment
fn fs_main(in: BlitOut) -> @location(0) vec4<f32> {
    let col = textureSample(scene_tex, scene_sampler, in.uv).rgb;
    // Lamp bloom: the halo adds, never replaces — 0.62 (0.55 before
    // push 2, whose hotter lamp cores earn a slightly stronger
    // halo) keeps them hot without washing the street around them.
    let halo = textureSample(bloom_tex, bloom_samp, in.uv).rgb;
    return vec4<f32>(col + halo * 0.62, 1.0);
}
