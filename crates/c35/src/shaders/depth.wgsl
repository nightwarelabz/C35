// Shadow caster: the terrain heightfield drawn from the sun. Depth only —
// no fragment stage, no colour. One draw call buys the whole M2 shadow.

struct DepthIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

@vertex
fn vs_main(in: DepthIn) -> @builtin(position) vec4<f32> {
    return u.light_view_proj * vec4<f32>(in.position, 1.0);
}
