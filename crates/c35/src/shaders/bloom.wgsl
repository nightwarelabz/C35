// Lamp bloom: the night grade's halo pass (graphics push, 2026-10-30).
//
// The scene target holds the graded frame (palette + dither already
// applied in the scene shaders). This chain runs between the scene
// pass and the blit, at HALF the internal resolution, and the blit
// adds the result back — so halos stay soft light, never a surface:
// they never touch the palette steps of the scene itself, and they
// never feed the TAA history (no ghosting on moving lamps).
//
// Three passes, one module:
//   fs_threshold — bright-pass the scene colour: only texels hotter
//     than the knee survive, so daylight streets do not bloom and
//     night lamps, lit windows, wet spec streaks and the visor do.
//   fs_blur — one 9-tap separable gaussian axis; the host runs it
//     twice (horizontal, then vertical) through the ping-pong pair.
// The blur targets are Rgba8Unorm (linear storage): the threshold
// samples the Srgb scene target (decoded to linear on read), the
// blur works in that linear space, and the blit adds the linear
// halo to its own linear sample before the output encode — one
// colour space end to end, no double gamma anywhere.
//
// Cost: threshold + 2 blur passes at (internal/2)² — at the PS1.5
// 854×480 internal frame that is 427×240 × 3 passes, ~0.3 Mpx of
// trivial work. The blit pays one extra tap. On the G1's ledger
// this is noise next to the scene pass; if it ever is not, the
// governor sheds render scale long before bloom matters.

struct BloomParams {
    dir: vec2<f32>, // blur axis in texels: (1,0) horizontal, (0,1) vertical
    pad: vec2<f32>,
}

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_samp: sampler;
@group(0) @binding(2) var<uniform> bp: BloomParams;

struct BOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> BOut {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: BOut;
    out.clip = vec4<f32>(corners[idx], 0.0, 1.0);
    out.uv = corners[idx] * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    return out;
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

// Bright-pass with a soft knee: nothing below 0.75 luma survives,
// everything above 1.0 passes whole. The knee keeps lamp cores
// from clipping into flat discs while letting the graded frame's
// hottest texels — lamp glass, lit windows, wet streak peaks,
// the visor band — through. Daylight (sun-lit masonry sits well
// under the knee after the grade) does not bloom.
@fragment
fn fs_threshold(in: BOut) -> @location(0) vec4<f32> {
    let col = textureSample(src_tex, src_samp, in.uv).rgb;
    let l = luma(col);
    let w = clamp((l - 0.75) / 0.25, 0.0, 1.0);
    return vec4<f32>(col * w, 1.0);
}

// 9-tap gaussian, sigma ~2 texels at half res — wide enough that
// the halo reads as light falloff, tight enough that a lamp does
// not wash its whole street. Two axes compose to a 2D blur.
@fragment
fn fs_blur(in: BOut) -> @location(0) vec4<f32> {
    let texel = bp.dir / vec2<f32>(textureDimensions(src_tex));
    var col = textureSample(src_tex, src_samp, in.uv).rgb * 0.227027;
    col += textureSample(src_tex, src_samp, in.uv + texel * 1.0).rgb * 0.1945946;
    col += textureSample(src_tex, src_samp, in.uv - texel * 1.0).rgb * 0.1945946;
    col += textureSample(src_tex, src_samp, in.uv + texel * 2.0).rgb * 0.1216216;
    col += textureSample(src_tex, src_samp, in.uv - texel * 2.0).rgb * 0.1216216;
    col += textureSample(src_tex, src_samp, in.uv + texel * 3.0).rgb * 0.054054;
    col += textureSample(src_tex, src_samp, in.uv - texel * 3.0).rgb * 0.054054;
    col += textureSample(src_tex, src_samp, in.uv + texel * 4.0).rgb * 0.016216;
    col += textureSample(src_tex, src_samp, in.uv - texel * 4.0).rgb * 0.016216;
    return vec4<f32>(col, 1.0);
}
