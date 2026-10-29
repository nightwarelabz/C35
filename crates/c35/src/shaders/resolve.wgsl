// Temporal AA resolve: the scene pass renders with a sub-pixel jittered
// projection; this pass un-jitters the current frame, reprojects this
// frame's depth into last frame's view to find the history sample, clamps
// that sample to the current frame's 3x3 neighbourhood (the ghost-killer),
// and blends. Output feeds the blit and becomes next frame's history.
// Where the backend cannot carry this (the WebGL2 fallback), the blit's
// FXAA runs instead and the ledger says so — nothing here is faked.

struct ResolveUniforms {
    inv_view_proj: mat4x4<f32>,  // current frame, unjittered
    prev_view_proj: mat4x4<f32>, // previous frame, unjittered
    params: vec4<f32>,           // xy: jitter in UV · z: history valid · w: blend alpha
}

@group(0) @binding(0) var<uniform> ru: ResolveUniforms;
@group(0) @binding(1) var cur_tex: texture_2d<f32>;
@group(0) @binding(2) var depth_tex: texture_depth_2d;
@group(0) @binding(3) var hist_tex: texture_2d<f32>;
@group(0) @binding(4) var lin_samp: sampler;

struct ROut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> ROut {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: ROut;
    out.clip = vec4<f32>(corners[idx], 0.0, 1.0);
    out.uv = corners[idx] * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn fs_main(in: ROut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(cur_tex));
    let px = vec2<i32>(in.clip.xy);
    let max_px = vec2<i32>(dims) - vec2<i32>(1, 1);

    // Current colour, un-jittered: the rasterizer shifted the image by the
    // jitter, so sample back by the same amount.
    let cur = textureSampleLevel(cur_tex, lin_samp, in.uv - ru.params.xy, 0.0).rgb;

    // 3x3 neighbourhood of the raw current frame, for the clamp box.
    var cmin = vec3<f32>(1e9);
    var cmax = vec3<f32>(-1e9);
    for (var i = -1; i <= 1; i++) {
        for (var j = -1; j <= 1; j++) {
            let q = clamp(px + vec2<i32>(i, j), vec2<i32>(0, 0), max_px);
            let c = textureLoad(cur_tex, q, 0).rgb;
            cmin = min(cmin, c);
            cmax = max(cmax, c);
        }
    }

    if (ru.params.z < 0.5) {
        return vec4<f32>(cur, 1.0); // no history yet (first frame / resize)
    }

    // Reproject: this pixel's world position (from depth) into last
    // frame's clip space.
    let depth = textureLoad(depth_tex, px, 0);
    let ndc = vec4<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0, depth, 1.0);
    let wp4 = ru.inv_view_proj * ndc;
    let prev_clip = ru.prev_view_proj * (wp4 / wp4.w);
    if (prev_clip.w <= 0.0) {
        return vec4<f32>(cur, 1.0);
    }
    let prev_ndc = prev_clip.xyz / prev_clip.w;
    let prev_uv = vec2<f32>(prev_ndc.x * 0.5 + 0.5, 0.5 - prev_ndc.y * 0.5);
    if (any(prev_uv < vec2<f32>(0.0)) || any(prev_uv > vec2<f32>(1.0)) || prev_ndc.z > 1.0) {
        return vec4<f32>(cur, 1.0);
    }

    var hist = textureSampleLevel(hist_tex, lin_samp, prev_uv, 0.0).rgb;
    hist = clamp(hist, cmin, cmax); // neighbourhood clamp: history may not invent colours
    let resolved = mix(hist, cur, ru.params.w);
    return vec4<f32>(resolved, 1.0);
}
