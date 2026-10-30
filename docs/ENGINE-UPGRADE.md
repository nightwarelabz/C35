# C35 runtime upgrade

This upgrade strengthens the existing renderer and hosts and adds small,
usable simulation systems. Existing public renderer constructors, mesh
builders, staged poses, and lamp APIs remain available. No production
dependency was added, and the reality-driven time/season policy is unchanged.

## Rendering and memory

- **Correct temporal sampling.** The resolve samples the current raster at
  the projection's jitter offset. Depth and the clamp neighborhood use that
  same raster location. Sky rays use the inverse jittered projection too.
- **Disocclusion rejection.** Previous-frame depth rejects history belonging
  to a different surface. The test uses linear view depth, a 1.5% relative
  tolerance, and a 0.05-unit minimum. This addresses static-scene camera
  motion; animated objects still have no per-object motion vectors.
- **Explicit cuts.** Call `Renderer::reset_history()` after a camera cut,
  teleport, or world reload. Focus/surface recovery invokes it in the hosts.
- **Target reuse.** An equal-size resize with the same internal dimensions
  and presentation mode retains its textures and history. Changing only a
  fog uniform does not allocate a new target set. `target_rebuild_count()`
  exposes actual rebuilds, including initialization.
- **Mesh capacity reuse.** Operator and prop buffers grow geometrically and
  reuse capacity when a mesh shrinks. Index data uploads only when topology
  changes; deforming an existing mesh uploads vertices alone. Empty meshes
  remove their GPU storage. `mesh_upload_stats()` reports cumulative vertex
  bytes, index bytes, and buffer allocations. Allocated capacity, rather than
  current logical mesh length, is charged to tracked VRAM.
- **Readback ownership.** A three-state gate ties each mapping to a copy
  encoded in that submission. A callback completing between encode and
  submit cannot accidentally turn stale data into a new timing sample.
  Each pass retains at most 4,096 undrained samples; drain regularly for
  complete run statistics. Query-set/driver internals remain outside the
  engine's VRAM estimate.
- **Startup errors.** `try_new` and `try_new_for_surface` return
  `RendererInitError` for adapter/device/size failures. Convenience `new`
  methods preserve the existing panic contract. Surface startup prefers an
  offered sRGB format. Invalid render scales normalize to a finite supported
  value; zero output dimensions normalize to one for direct engine callers.

The OpenGL FXAA path does not allocate temporal depth/history resources or
require compute. The Vulkan test checks byte-for-byte CPU/GPU culling parity.

## Fixed ticks and jobs

`timing::FixedClock::new(120, 8)` separates simulation frequency from render
frequency. Advance it with a `Duration`, execute the returned tick range in
order, and interpolate presentation with `StepBatch::alpha`. The clock
accumulates integer time at the requested rate, so 30/60/90/120/144/240 Hz
render schedules all produce exactly 120 ticks per elapsed second.

Catch-up is bounded to eight ticks in this configuration. Excess whole
ticks are discarded and reported as dropped wall time; fractional time is
retained. `reset_accumulator()` drops a paused frame's backlog without
changing tick identity. The windowed host now uses this clock for camera
movement and blending while feeding the governor unclamped frame cost.

Job batches catch worker panics, drain all borrowed chunks, and resume the
first panic on the caller. Workers remain available afterwards. Nested
batches run inline to avoid starving the same pool. `with_workers(0)` is an
explicit inline mode; wasm always uses it. Small visibility sets run inline
instead of paying queue/latch allocation costs for sixteen sphere tests.
Terrain generation still uses the pool.

`Arena` adds removal, mutable iteration, capacity-retaining clear, and
constant-time live-component counts. Entity iteration order remains stable.

## Reusable character animation

```rust
use c35::glam::Vec3;
use c35::operator::{FigureTemplate, PoseFrame};

let template = FigureTemplate::operator();
let mut mesh = template.mesh(); // allocate once
template.skin_into(&mut mesh, &PoseFrame::bind(), Vec3::ZERO, 0.0, 0.0);
// renderer.update_operator(&mesh);
```

`FigureTemplate::from_parts` supports other figures on the same skeleton.
Stable topology, UVs, materials, and part metadata are retained. Bone
transforms are evaluated once per update, face normals are shared across
their four vertices, and the output vectors retain their allocations.
The native movement drive uses this path. Tests compare its geometry with
the original builder across all staged poses and multiple world yaws.

The animator now exposes `footsteps()` from its last simulation update.
Events occur when a foot enters stance, carry the actual ground contact
position, foot identity, speed, and crouch blend, and do not repeat during
stance. Consume after **each simulation tick**, before the next update
replaces the two-slot event buffer.

## Shared world queries, hearing, and rope dynamics

| API | Delivered behavior | Boundary |
| --- | --- | --- |
| `spatial::TriangleBvh` | Balanced static triangle BVH; nearest two-sided ray hits; original triangle tags; barycentrics; segment occlusion; AABB candidate queries | Queries allocate nothing. Rebuild after changing static geometry. This is not capsule collision. |
| `TriangleBvh::from_props` | Builds queries from the renderer's actual prop positions and indices | Invalid indices/non-finite triangle positions return an error. |
| `acoustics::SoundField` | Bounded, deterministic event lifetime; distance and time attenuation; shared occlusion; equal-power stereo pan | Produces audibility/gain data. Does not synthesize samples or open an audio device. |
| `physics::Particle` / `Rope` | Semi-implicit point-mass integration; slack/taut rope; outward radial-velocity removal; tangential release momentum | No character capsule, static collision response, ragdoll, or rigid-body solver. |

`SoundField::sample` accepts an occlusion closure such as
`|a, b| geometry.occluded(a, b)`. AI hearing and an eventual native/WebAudio
playback backend can consume the same `HeardSound::level`, rather than
inventing separate audibility rules. Full fields evict the oldest event and
count evictions. Surface classification remains a host decision.

`systems_probe` connects fixed ticks, animator contact events, sound stimuli,
geometry-based occlusion, and rope dynamics. Its five-second 144 Hz render
schedule produced:

```text
ticks=600 contacts=12 occluded_samples=294 live_sounds=1 evicted=0 dropped=0ns rope_distance=4.00000
```

## Hosts and cost accounting

- The browser now feeds frame-loop cost to the quality governor and resets
  history after a long callback gap. Lost/outdated surfaces reconfigure;
  out-of-memory errors stop rendering with a visible status.
- The windowed host clears held keys and releases mouse capture on focus
  loss. Minimized/occluded windows wait instead of continuously rendering.
  Lost/outdated surfaces reconfigure; out-of-memory exits the event loop.
- Governor timing rejects non-finite/nonpositive samples without advancing
  cadence or corrupting its EMA. Releasing a pin restarts cooldown.
- Native ledgers count the frames actually rendered, exclude cull warmup
  samples, and include simulation/skinning in measured frame cost. Mesh
  traffic, buffer allocations, and target rebuilds are reported separately.
  Triangle accounting includes the temporal resolve; VRAM includes cull and
  readback buffers, the lamp buffer, and actual uniform-buffer sizes.

## Validation and reproduction

```sh
cargo test --workspace --locked
cargo check --workspace --all-targets --locked
cargo check -p comigen-web --target wasm32-unknown-unknown --locked
cargo run -p c35 --example systems_probe --locked
cargo run --release -p c35 --example skinning_probe --locked

# Requires Vulkan and EGL/OpenGL drivers; CI installs Mesa software drivers.
cargo test -p c35 --test render --locked -- --ignored --nocapture
```

CI runs native tests/checks, wasm compilation, and actual Vulkan/OpenGL
renders. GPU tests require an adapter and fail if one is unavailable; they
do not silently skip. They exercise history across frames, presentation and
quality changes, zero-size normalization, unchanged resize reuse, and mesh
topology changes, and compare CPU/GPU culling pixels exactly.

On one Linux container with Rust 1.91.1, the optimized 2,000-frame skinning
probe measured **32.78 µs** for one-shot construction versus **15.09 µs** for
reuse, **2.17×** faster. This measures CPU mesh work, not overall game FPS;
rerun on the target hardware. Debug timings were 761.01 versus 600.66 µs.

Interactive window input, browser context loss, and physical display timing
still need testing on their actual hosts. Wasm compilation and native
OpenGL rendering cover different parts of the browser path; neither is a
claim that browser interaction was exercised.
