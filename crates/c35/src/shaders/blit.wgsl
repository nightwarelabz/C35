// Blit: the final step to the output. On the temporal-reconstruction
// path the resolve already wrote at output resolution, so this is a
// straight copy; on the WebGL2 fallback path the FXAA variant of this
// pass (fxaa.wgsl, same bindings) runs over the internal target instead.

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

@fragment
fn fs_main(in: BlitOut) -> @location(0) vec4<f32> {
    return textureSample(scene_tex, scene_sampler, in.uv);
}
