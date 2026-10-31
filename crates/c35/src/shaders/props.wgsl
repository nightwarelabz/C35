// Props (Slice 1): static built-world geometry over the generated
// city atlas. Lit by the same reality-layer terms as the operator —
// wrapped sun diffuse with the real shadow map, hemisphere ambient,
// the moon as key light at night — plus the term that makes streets
// read at night: each vertex carries a BAKED practical-light value
// (the host light field's exposure at that point, evaluated when
// the mesh was built by the same field that answers the light
// read), added here in sodium colour. Material flag 1 is emissive
// surface art (lit windows, lamp heads, wet streaks): the atlas
// texels are the light, boosted and fogged like everything else.

struct PropIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) mat: f32,
    @location(4) light: f32,
}

struct PropOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) mat: f32,
    @location(4) light: f32,
}

@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_samp: sampler;

// The practical lamps, as data (head xyz + intensity in w) — the
// SAME list the host light field evaluates for the bake and the
// read. Wet surfaces mirror these lamps per-fragment, so a pool
// sits where the field says the light is, and streaks toward the
// camera the way water does. Graphics push: the set grew 12 → 24
// so a rebuilt street can carry a lamp per bay.
struct LampSet {
    lamps: array<vec4<f32>, 24>,
}
@group(2) @binding(0) var<uniform> lamp_set: LampSet;

// The sodium of the practical field — the same colour the PS1.5
// horizon band burns with, so baked pools and distant shore agree.
const SODIUM: vec3<f32> = vec3<f32>(1.0, 0.52, 0.18);

@vertex
fn vs_main(in: PropIn) -> PropOut {
    var out: PropOut;
    out.world_pos = in.position;
    out.normal = in.normal;
    out.uv = in.uv;
    out.mat = in.mat;
    out.light = in.light;
    out.clip = u.view_proj * vec4<f32>(in.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: PropOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas_tex, atlas_samp, in.uv);
    var col: vec3<f32>;
    // Material order matters: WET (2) must reach the standard
    // branch below, not the emissive one — the first wet build had
    // every street vertex at mat 2.0 and the whole ground rendered
    // through the emissive path (flat, bright, unlit), which is
    // exactly the bug this ordering fixes.
    if (in.mat > 0.5 && in.mat < 1.5) {
        // Emissive art: the texels are the light. A gentle lift so a
        // lit window sits above the night around it, then the same
        // fog and grade as everything else — distance still takes it.
        // Push 2: the lift runs a little hotter so lamp glass and
        // lit windows cross the bloom knee as sources, not decals.
        col = tex.rgb * 1.6 + tex.rgb * tex.rgb * 0.45;
    } else {
        let n = normalize(in.normal);
        let amb = mix(u.amb_ground.rgb, u.amb_sky.rgb, n.y * 0.5 + 0.5);
        let ndl = dot(n, u.sun_dir.xyz);
        let shadow_raw = shadow_factor(in.world_pos, n);
        let shadow = mix(1.0, shadow_raw, smoothstep(-0.05, 0.25, ndl));
        let wrap = clamp((ndl + 0.38) / 1.38, 0.0, 1.0);
        let moon_diff = max(dot(n, u.moon_dir.xyz), 0.0);
        col = tex.rgb
            * (u.sun_color.rgb * wrap * shadow * 0.82 + u.moon_color.rgb * moon_diff + amb);
        // The baked practicals: sodium pools on the ground the
        // lamps stand over, already occluded and fallen off by the
        // field that baked them.
        col += tex.rgb * SODIUM * in.light * 1.35;
        // Graphics push 2 — hot cores: the ^2.2 term concentrates
        // extra light where the baked field is already strong, so
        // a pool has a heart under its lamp instead of reading as
        // a flat disc of sodium. The wet pass below adds the same
        // term to its own diffuse.
        let core = pow(clamp(in.light, 0.0, 1.3), 2.2);
        col += tex.rgb * SODIUM * core * 0.85;
        // Sky bounce: a whisper of cool ambient the hemisphere term
        // alone doesn't carry at night, so honest shadow stays
        // READABLE (deep blue-grey, per the canon) instead of
        // voiding to black. It is a constant, not a light source:
        // the read never consults pixels, and the field's pools and
        // shadows keep their ratios above it.
        col += tex.rgb * vec3<f32>(0.034, 0.040, 0.058);
        // Graphics push 2 — dry specular: un-wet surfaces still
        // answer the lights with a roughness-keyed glint (the same
        // atlas alpha the wet pass reads). Painted metal, roller
        // shutters and door steel catch the moon and the low sun;
        // masonry, at alpha ~0.9, barely breathes. This is the
        // metal/stone separation the flat diffuse could not give.
        let v = normalize(u.cam_pos.xyz - in.world_pos);
        let rough_s = tex.a;
        let spow_s = mix(64.0, 10.0, rough_s);
        let sgain = mix(0.55, 0.05, rough_s);
        let hm_s = normalize(u.moon_dir.xyz + v);
        col += u.moon_color.rgb
            * (pow(max(dot(n, hm_s), 0.0), spow_s) * sgain);
        let hs_s = normalize(u.sun_dir.xyz + v);
        col += u.sun_color.rgb
            * (pow(max(dot(n, hs_s), 0.0), spow_s) * sgain * 0.8 * shadow);
        if (in.mat > 1.5) {
            // THE WET PASS (mat 2): the street is water over stone.
            // Two carriers, kept apart because they do different
            // jobs: the DIFFUSE pool (the baked field term, lifted
            // for wet scatter — this is the lit geography, the same
            // ground the light read measures) and the SPECULAR
            // streak (per-lamp Blinn, tight lobe, fresnel-weighted,
            // broken up by the surface's own texels). The spec
            // answers lamp positions AND the camera; it stretches
            // toward the viewer the way a reflection does, which no
            // painted streak ever managed.
            // Every term this pass adds is scaled by the weather
            // layer's wetness (u.surface.x): 1.0 — the default, and
            // what every host rendered before the seam existed —
            // multiplies each term by exactly 1.0, so the shipped
            // full-wet look is the wetness = 1 frame, unchanged.
            let wet = u.surface.x;
            col += tex.rgb * SODIUM * in.light * 1.35 * wet;
            col += tex.rgb * SODIUM * core * 0.85 * wet;
            let v = normalize(u.cam_pos.xyz - in.world_pos);
            let n2 = normalize(in.normal);
            let ndv = clamp(dot(n2, v), 0.0, 1.0);
            let fres = 0.25 + 0.75 * pow(1.0 - ndv, 2.0);
            let breakup = 0.3 + 0.7 * clamp(tex.r * 2.2, 0.0, 1.0);
            // Graphics push: the atlas alpha channel carries
            // per-texel ROUGHNESS (painted by the city painters,
            // 0 = glass-smooth, 1 = rough stone). Smooth ground
            // reflects a tight, hot lobe; rough ground a broad,
            // dim one — so the same lamp reads differently on wet
            // asphalt than on worn concrete, with no new texture.
            // Unpainted regions keep alpha 255 (fully rough), the
            // safe default.
            let rough = tex.a;
            let spec_pow = mix(46.0, 13.0, rough);
            // Scaled by wetness here, once — both lobes below (the
            // core streak and its tail) read this gain.
            let spec_gain = mix(5.0, 2.6, rough) * wet;
            for (var i = 0; i < 24; i++) {
                let lp = lamp_set.lamps[i];
                if (lp.w > 0.0) {
                    let lv = lp.xyz - in.world_pos;
                    let d = length(lv);
                    // Push 2: reach 11.0 → 13.5 — pools throw their
                    // specular further down the street.
                    let reach = clamp(1.0 - (d / 13.5) * (d / 13.5), 0.0, 1.0);
                    if (reach > 0.0) {
                        let hv = normalize(lv / max(d, 0.001) + v);
                        let ndh = max(dot(n2, hv), 0.0);
                        let spec = pow(ndh, spec_pow) * spec_gain;
                        col += SODIUM * spec * pow(reach, 1.5) * lp.w * fres * breakup;
                        // Push 2 — the streak's tail: a second
                        // lobe off a vertically squashed normal
                        // stretches a longer, dimmer reflection
                        // toward the camera, the way a wet street
                        // drags a lamp's light behind its core.
                        let ntail = normalize(vec3<f32>(n2.x, n2.y * 0.38, n2.z));
                        let ndht = max(dot(ntail, hv), 0.0);
                        let tail = pow(ndht, spec_pow * 0.42) * spec_gain * 0.5;
                        col += SODIUM * tail * pow(reach, 1.5) * lp.w * fres * breakup;
                    }
                }
            }
            // The moon sheen is a GLISTEN BAND, not a wash: tight
            // power, so it lives only where the floor mirrors the
            // moon toward the camera and moves when the camera does.
            // Roughness loosens it the same way it loosens the lamps.
            let hm = normalize(u.moon_dir.xyz + v);
            let sheen_pow = mix(160.0, 70.0, rough);
            let sheen = pow(max(dot(n2, hm), 0.0), sheen_pow);
            col += u.moon_color.rgb * sheen * mix(0.5, 0.2, rough) * breakup * wet;
        }
    }
    col = apply_fog(col, in.world_pos);
    col = ps15_grade(col, in.clip.xy);
    return vec4<f32>(col, 1.0);
}

// Shadow-caster entry: same geometry, depth only, drawn with the
// uniform-only depth bind group (group 1 is not bound in that pass
// and is not used here).
@vertex
fn vs_depth(in: PropIn) -> @builtin(position) vec4<f32> {
    return u.light_view_proj * vec4<f32>(in.position, 1.0);
}
