// GPU tile culling (M4): the camera-pass visibility law as a compute
// pass. One invocation per terrain tile: a frustum sphere test against
// the camera, a distance LOD pick, and the neighbour relaxation — then
// the tile's slot in the indirect-draw buffer is written (culled tiles
// get a zero-count draw). The scene pass issues one indirect draw per
// slot; nothing is read back for the render itself.
//
// The law here is the CPU visibility system's law (crates/c35/src/
// vista.rs), float for float: the planes arrive as uniforms computed
// by the same Frustum::from_view_proj, the LOD bands are the same
// distances, and the relaxation is the same in-place scan to the same
// fixed point — so the GPU path draws the same tile set at the same
// LODs as the CPU path, which the proof runs verify at the pixel level
// (byte-identical frames). The CPU system still runs every frame: it
// owns the shadow-pass list and is the whole story on the WebGL2 path,
// which has no compute.

struct TileParam {
    sphere: vec4<f32>, // xyz: bounding-sphere centre · w: radius
    lod0: vec4<u32>,   // first_index, index_count, unused, unused (indices are absolute)
    lod1: vec4<u32>,
    lod2: vec4<u32>,
}

struct CullUniforms {
    planes: array<vec4<f32>, 6>,
    eye: vec4<f32>,  // xyz: camera eye
    dims: vec4<u32>, // x: tiles per side · y: tile count
}

struct DrawArgs {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

@group(0) @binding(0) var<storage, read> params: array<TileParam>;
@group(0) @binding(1) var<storage, read_write> draws: array<DrawArgs>;
@group(0) @binding(2) var<uniform> cu: CullUniforms;

// Sized for the largest grid the vista contemplates (8x8 = 64); the
// shipped 4x4 uses the first 16 slots.
const MAX_TILES: u32 = 64u;
const CULLED: u32 = 255u;
var<workgroup> steps: array<u32, MAX_TILES>;

// The vista's LOD bands (vista::lod_step_for): surface distance —
// centre distance minus bounding radius, clamped at zero.
fn lod_step(dist: f32) -> u32 {
    if (dist < 140.0) {
        return 0u;
    }
    if (dist < 300.0) {
        return 1u;
    }
    return 2u;
}

@compute @workgroup_size(64)
fn main(@builtin(local_invocation_id) lid: vec3<u32>) {
    let i = lid.x;
    let count = cu.dims.y;
    if (i < count) {
        let p = params[i];
        var visible = true;
        for (var k = 0; k < 6; k++) {
            let pl = cu.planes[k];
            if (dot(pl.xyz, p.sphere.xyz) + pl.w < -p.sphere.w) {
                visible = false;
            }
        }
        if (visible) {
            let d = max(length(p.sphere.xyz - cu.eye.xyz) - p.sphere.w, 0.0);
            steps[i] = lod_step(d);
        } else {
            steps[i] = CULLED;
        }
    }
    workgroupBarrier();

    // Neighbour relaxation: identical algorithm to the CPU's
    // clamp_neighbours (same scan order, in place, to the fixed point,
    // cap 8) — run by one invocation; the grid is tiny and this keeps
    // the two paths' LOD assignments exactly, not approximately, equal.
    if (i == 0u) {
        let n = cu.dims.x;
        for (var sweep = 0u; sweep < 8u; sweep++) {
            var changed = false;
            for (var tj = 0u; tj < n; tj++) {
                for (var ti = 0u; ti < n; ti++) {
                    let idx = tj * n + ti;
                    if (steps[idx] == CULLED) {
                        continue;
                    }
                    var finest = steps[idx];
                    for (var dj = -1; dj <= 1; dj++) {
                        for (var di = -1; di <= 1; di++) {
                            if (di == 0 && dj == 0) {
                                continue;
                            }
                            let ai = i32(ti) + di;
                            let aj = i32(tj) + dj;
                            if (ai < 0 || aj < 0 || ai >= i32(n) || aj >= i32(n)) {
                                continue;
                            }
                            let j = u32(aj) * n + u32(ai);
                            if (steps[j] != CULLED) {
                                finest = min(finest, steps[j]);
                            }
                        }
                    }
                    let max_step = finest + 1u;
                    if (steps[idx] > max_step) {
                        steps[idx] = max_step;
                        changed = true;
                    }
                }
            }
            if (!changed) {
                break;
            }
        }
    }
    workgroupBarrier();

    if (i < count) {
        let p = params[i];
        var args = DrawArgs(0u, 0u, 0u, 0, 0u);
        if (steps[i] != CULLED) {
            var lod = p.lod0;
            if (steps[i] == 1u) {
                lod = p.lod1;
            } else if (steps[i] == 2u) {
                lod = p.lod2;
            }
            args.index_count = lod.y;
            args.instance_count = 1u;
            args.first_index = lod.x;
            // Always 0: the merged index buffer holds absolute indices
            // (the tile's vertex offset is baked in at build), and a
            // non-zero base vertex is a draw WebGL2 cannot make — its
            // backend panics on the request. One law for every path.
            args.base_vertex = 0;
        }
        draws[i] = args;
    }
}
