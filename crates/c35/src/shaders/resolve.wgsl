// Temporal reconstruction (M3; M2 shipped this pass as same-resolution
// TAA). The scene pass renders at the governor's internal scale with a
// sub-pixel jittered projection; THIS pass runs at output resolution —
// each output pixel un-jitters and samples the internal frame, reprojects
// its depth into last frame's view to find the history sample (history
// lives at output resolution too), clamps that sample to the current
// frame's 3x3 neighbourhood (the ghost-killer), and blends. The blend at
// output resolution over a jittered low-res input stream IS the
// upsampler: detail the internal frame never held in one frame is
// accumulated across frames. The depth it reprojects by is the internal
// depth, so disocclusion edges are as coarse as the internal grid —
// stated plainly; the clamp keeps them honest.
// Where the backend cannot carry this (the WebGL2 fallback), the blit's
// FXAA runs instead and the ledger says so — nothing here is faked.

struct ResolveUniforms {
    inv_view_proj: mat4x4<f32>,  // current frame, unjittered
    prev_view_proj: mat4x4<f32>, // previous frame, unjittered
    params: vec4<f32>,           // xy: jitter in UV · z: history valid · w: blend alpha
    vel: vec4<f32>,              // motion rejection: gain, threshold, resolve dimensions
    temporal: vec4<f32>,         // previous jitter xy, relative / absolute depth tolerance
}

@group(0) @binding(0) var<uniform> ru: ResolveUniforms;
@group(0) @binding(1) var cur_tex: texture_2d<f32>;
@group(0) @binding(2) var depth_tex: texture_depth_2d;
@group(0) @binding(3) var hist_tex: texture_2d<f32>;
@group(0) @binding(4) var lin_samp: sampler;
@group(0) @binding(5) var hist_depth_tex: texture_depth_2d;

fn linear_depth(z: f32) -> f32 {
    return 0.5 * 4000.0 / (4000.0 - z * (4000.0 - 0.5));
}

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
    // This pass's pixels are OUTPUT pixels; the current frame and its
    // depth live at the internal resolution. Map between the two by UV.
    let cur_dims = vec2<f32>(textureDimensions(cur_tex));
    let sample_uv = in.uv + ru.params.xy;
    let cur_px = clamp(
        vec2<i32>(sample_uv * cur_dims),
        vec2<i32>(0, 0),
        vec2<i32>(cur_dims) - vec2<i32>(1, 1),
    );
    let max_px = vec2<i32>(cur_dims) - vec2<i32>(1, 1);

    // Current colour, un-jittered: the rasterizer shifted the image by the
    // jitter, so address that same shifted raster location.
    let cur = textureSampleLevel(cur_tex, lin_samp, sample_uv, 0.0).rgb;

    // 3x3 neighbourhood of the raw current frame, for the clamp box.
    var cmin = vec3<f32>(1e9);
    var cmax = vec3<f32>(-1e9);
    for (var i = -1; i <= 1; i++) {
        for (var j = -1; j <= 1; j++) {
            let q = clamp(cur_px + vec2<i32>(i, j), vec2<i32>(0, 0), max_px);
            let c = textureLoad(cur_tex, q, 0).rgb;
            cmin = min(cmin, c);
            cmax = max(cmax, c);
        }
    }

    if (ru.params.z < 0.5) {
        return vec4<f32>(cur, 1.0); // no history yet (first frame / resize)
    }

    // Reproject: this pixel's world position (from the internal depth)
    // into last frame's clip space; the history texture is output-sized,
    // so the previous UV addresses it directly.
    let depth = textureLoad(depth_tex, cur_px, 0);
    let ndc = vec4<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0, depth, 1.0);
    let wp4 = ru.inv_view_proj * ndc;
    let prev_clip = ru.prev_view_proj * (wp4 / wp4.w);
    if (prev_clip.w <= 0.0) {
        return vec4<f32>(cur, 1.0);
    }
    let prev_ndc = prev_clip.xyz / prev_clip.w;
    let prev_uv = vec2<f32>(prev_ndc.x * 0.5 + 0.5, 0.5 - prev_ndc.y * 0.5);
    if (any(prev_uv < vec2<f32>(0.0)) || any(prev_uv > vec2<f32>(1.0)) || prev_ndc.z > 1.0 || prev_ndc.z < 0.0) {
        return vec4<f32>(cur, 1.0);
    }

    // Reject history whose old visible surface was at another depth.
    // Compare linear view depths, so tolerance has a world-space meaning
    // at both the near plane and the far shore. The previous depth is
    // raster-sized and must be addressed with its OWN raster jitter.
    let hist_px = clamp(vec2<i32>((prev_uv + ru.temporal.xy) * cur_dims), vec2<i32>(0), max_px);
    let old_depth = textureLoad(hist_depth_tex, hist_px, 0);
    let expected = linear_depth(prev_ndc.z);
    if (abs(linear_depth(old_depth) - expected) > max(ru.temporal.w, expected * ru.temporal.z)) {
        return vec4<f32>(cur, 1.0);
    }

    // M7 velocity-aware history rejection: the displacement this
    // pixel's surface travelled since last frame, in resolve pixels.
    // Sub-threshold motion (camera stillness, TAA jitter, slow drift)
    // changes the blend not at all — the term is exactly zero there,
    // so every static frame is bit-identical to the M1–M6 resolve.
    // Past the threshold the blend alpha rises at the gain per pixel:
    // fast-disoccluding surfaces (a running man's limbs, silhouette
    // edges under a tracking camera) stop inheriting stale history
    // the neighbourhood clamp alone would let through.
    let disp_px = length((prev_uv - in.uv) * ru.vel.zw);
    let excess = max(disp_px - ru.vel.y, 0.0);
    let alpha = clamp(ru.params.w + ru.vel.x * excess, 0.0, 1.0);

    var hist = textureSampleLevel(hist_tex, lin_samp, prev_uv, 0.0).rgb;
    hist = clamp(hist, cmin, cmax); // neighbourhood clamp: history may not invent colours
    let resolved = mix(hist, cur, alpha);
    return vec4<f32>(resolved, 1.0);
}

