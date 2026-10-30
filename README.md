<img width="1774" height="887" alt="c35" src="https://github.com/user-attachments/assets/453573f9-385e-4490-bb9a-5eb6b642794c" />
#C35

# Comigen

**Comigen** is a game graphics engine built on one conviction: gorgeous
visuals should be *cheap*. It is being built to make next-generation games
whose visuals are engineered — procedurally generated, cost-accounted, and
measured — so that the performance is phenomenal *because* the beauty is
mathematical, not in spite of it.

The charge, in the founder's words: the engine "will be extremely powerful —
in the sense that it can make gorgeous visuals for very cheap resources. It
will make the absolute best next gen engineered games — but the performance
will be phenomenal."

## The thesis

Beauty-per-watt comes from spending the frame budget only where the eye can
tell:

1. **Procedural everything.** Geometry, textures, materials and skies are
   generated from math — zero shipped image assets, near-zero memory for
   content, no streaming hitches.
2. **Analytic rendering where it beats raster.** The sky is a function. The
   ocean is two triangles and a fragment shader. Fog, sun and reflections
   are shared math, not baked data.
3. **Cost-accounted rendering — the house law: if it costs, it shows.**
   Every run prints a **cost ledger**: frame time, GPU pass time (where the
   backend exposes timestamp queries), draw calls, triangles, internal
   render scale, and tracked memory. No claim without a number.
4. **Resolution scale as a first-class knob — and a governor that turns
   it.** The scene renders to an offscreen target at an internal scale
   and is *temporally reconstructed* back to output resolution (jittered
   frames + history resolve; FXAA on the WebGL2 fallback path — each
   ledger states which actually ran). Since M3 a **quality governor**
   holds frame time against a target and sheds the engine's own ranked
   costs before it ever drops a frame it could have afforded.
5. **One tree, every target.** Rust + [wgpu](https://wgpu.rs): Vulkan /
   Metal / DX12 natively, WebGPU / WebGL2 in the browser — the same engine
   crate, the same world seed, the same frame, everywhere. Native and
   browser are built **in lockstep**, never ported after the fact.
6. **The world is powered by the real world.** Since M2 the engine's time
   of day, sun, sky and seasons are computed from the real clock and the
   observer's real location through a real solar model — see below.

## The reality layer (M2)

Comigen's founding doctrine, in the founder's words: the engine will
*"always base the time of day, the seasons, etc • powered by real world,
real time events. If it is Fall or Winter in your location, if it is 9PM or
7AM • the engine will reflect that."*

That is now code. The engine crate carries a world-state module
(`crates/c35/src/world.rs`): a UTC instant plus the observer's latitude
(and longitude, or a UTC offset) go in; a real solar model — the NOAA
approximation: fractional year → declination + equation of time → true
solar time → hour angle → elevation/azimuth by spherical geometry — decides
where the sun is. Day length is never tabled; it falls out of the sunrise
hour angle. Season factors are smooth functions of day-of-year,
hemisphere-flipped (December is summer at −33°), anchored where they
proved out in the founder's earlier world, Simulacrum: cold peaks
mid-January, autumn peaks ~Oct 19, spring ~Apr 16 (northern hemisphere).

The vista obeys the computed state end to end:

- **Sun** — disc position, light direction, colour and intensity from the
  solar position; a real 2048² shadow map (one cascade) makes the terrain
  shadow itself, and the pass is skipped outright when the sun is down.
- **Sky** — the gradient, horizon warmth and fog tint follow the sun's
  elevation; at night the stars come out and the moon takes over. The
  moon is no longer a light mirrored from the sun. Its place and its
  phase come from the same UTC instant, through a low-precision lunar
  ephemeris (Meeus, *Astronomical Algorithms*, ch. 47 for the place and
  ch. 48 for the illuminated fraction). That fraction scales the sky
  wash, the sea that reflects the sky, and the key light — a new moon
  adds nothing, so a new-moon night stays dark enough for the PS1.5
  sodium shore to carry the frame. It is the uniforms the sky and water
  shaders already read: the ledger gains no pass and no draw. Every
  reality reading prints elevation, azimuth, and illuminated fraction
  so the claim can be checked against an almanac.
- **Land** — the snow line descends as cold deepens until January puts snow
  on the beach; autumn ambers the foliage band; spring brightens the green.
- **Water** — cold pales the sea; deep winter sheets it with ice: pale,
  hard-glinting, its wave motion nearly stilled.

### Where the hosts get reality

- **Native hosts** read the **system clock**. Location comes from the
  environment: `COMIGEN_LAT` (latitude, degrees) and `COMIGEN_LON`
  (longitude), or `COMIGEN_UTC_OFFSET_MIN` when there is no longitude.
  The windowed host also falls back to the OS local UTC offset on its own.
- **Web host** reads the **system clock** via `Date` and the position via
  the **Geolocation API** when the visitor grants it.
- **When location is unknown, the engine says so.** The ledger of every
  run prints the clock source, the latitude source actually used, and the
  moon (elevation, azimuth, illuminated fraction); with no latitude the
  engine runs at a stated equator fallback — never a silent pretence.
  The web page's readout says the same when geolocation is denied, and
  names the fallback in force.

**Privacy:** location is used locally only. Coarse latitude is all the
engine consumes for its seasons (longitude sets solar time and the moon's
hour angle), and nothing is transmitted anywhere — there is no server
component to send it to.

**The no-controls law:** the shipped hosts carry **no time-of-day or
season controls** — no sliders, no keys, no query parameters. The engine
reflects reality; it does not offer to edit it. The single exception is
the *measurement rig*: the headless hosts accept `COMIGEN_PIN_ISO`
(an ISO-8601 instant) in the environment, and the web page honours a
`__comigen_pin_unix` value injected before load — read by the hosts only,
never by the engine crate, and never exposed in any UI. It exists so a
run can be pinned, measured and audited, the same standing as the hero
camera pose.

## The engine core (M3)

M3 turns the renderer into an engine core, in three systems:

- **A data-oriented core (ECS + job pool).** The scene lives in flat
  typed component arenas — camera, world state, terrain tiles, water,
  sky — with named systems over them (`reality-bind`, `visibility`) and
  a small std-thread job pool for batch work. No hash maps in the scene
  graph, no per-frame allocation in the update path. On targets with no
  threads (wasm32) the pool degrades to inline execution; the core
  never *requires* parallelism.
- **Tiled terrain with culling + LOD.** The heightfield is a 4×4 tile
  grid; each frame the visibility system frustum-culls tiles against
  the camera and picks an index-density LOD per tile by distance
  (borders share vertices across densities; reduced levels carry skirts
  and neighbours stay within one LOD step, so seams stay shut). The
  shadow pass frustum-culls against the light too — but its casters
  always draw at full density: a reduced caster erodes thin
  self-shadows, a defect the M3 winter proof caught and convicted.
  Every ledger prints tiles drawn/culled and triangles before/after.
- **Temporal reconstruction.** Where M2's TAA resolved at the internal
  resolution, M3's history resolve writes at *output* resolution: the
  scene renders at the governor's internal scale (0.70 in the A/B
  proof) and the jittered-frame history blend reconstructs the output
  image — in the M3 A/B, GPU scene time fell 41% (llvmpipe) for an
  image whose broad shading and silhouettes hold; the finest
  high-frequency detail is where reconstruction spends its honesty.
- **The quality governor.** An exponential moving average of frame time
  is held against a target (16.7 ms). Over budget, the governor steps
  down a ranked ladder — render scale 1.00 → 0.85 → 0.70 → 0.55, then
  shadow map 2048 → 1024 → off, then terrain detail reach, then water
  wave octaves — one step per evaluation cadence with cooldowns, and
  steps back up only on deep, sustained headroom. It cannot hunt.
  **Pin law:** an operator may pin a level (windowed host keys 0–4,
  headless `--pin-level`); while pinned the governor *observes and
  reports* overruns and never acts — the operator's word stands, and
  the ledger says so. In the M3 proof drive on the software renderer
  (frame ≈195 ms at full quality — twelve times over target) the
  governor walked the whole ladder in 540 frames and halved the frame
  time to ≈99 ms, then reported the ladder's floor honestly: on a CPU
  rasterizer, 16.7 ms is not in the building. On real hardware the same
  ledger is where the truth will show.

## M4 — relief, GPU culling, motion, step-up

M4 pays the debts M3 named, in the open:

- **Terrain generation v2 — cliff relief.** The height function is now
  the M1–M3 landform *plus* a slope-masked relief layer: where the base
  slope is steep, the sample domain is warped (so the wall's rim breaks
  into teeth and gullies instead of a smooth band) and a ridged
  multifractal displaces the surface — strata ribs, gullies cut deeper
  than ribs rise, and a faint bench terracing. The mask is zero on
  meadows, shores and the bay floor, so gentle terrain is
  vertex-for-vertex what it was; the grid stays 257² and the triangle
  budget is unchanged (131,072 full density; 45,568 submitted at the
  hero pose). The wall reads better — toothed rim, streaked face,
  craggier skyline — and the report says plainly where it still
  underwhelms: in its own shadow, ambient-only shading flattens what
  geometry adds, and per-vertex horizon occlusion is the named next
  lever.
- **GPU compute culling (native).** All tiles now live in one merged
  vertex buffer and one merged index buffer. On compute-capable
  backends a compute pass runs the same frustum test and the same LOD
  bands as the CPU visibility system (the neighbour relaxation runs to
  the same fixed point), writes an indirect-draw argument per tile,
  and the scene pass consumes them. The CPU system still owns the
  shadow pass — and the whole story on WebGL2, which has no compute.
  Every ledger names which culling ran and prices the compute pass
  (llvmpipe timestamps: 0.082 ms in the parity run, up to ~1.15 ms
  under contention on this shared VM). Proof is at the pixel level:
  at both proof poses the GPU path's frame is **byte-identical** to
  the CPU path's, with identical drawn/culled counts — agreement, not
  a superset.
- **The base-vertex law (a defect the web proof convicted).** The
  first M4 web drive died on frame one: the merged draws carried each
  tile's base vertex, and WebGL2 has no base-vertex draw — wgpu's GL
  backend (glow) panics on the request. Native backends take
  base-vertex draws without blinking, so no native proof could see
  it. The merge stays; the assumption went: each tile's vertex offset
  is baked into its index values at build time, every draw on every
  path carries base vertex 0, and the native frames after the fix are
  byte-identical to the frames before it (same sha256 at the winter
  pin, both cull paths). One draw law, all backends.
- **Reconstruction under motion, measured.** M3's A/B was a static
  pose. M4 ran the deterministic drift orbit at scale 1.00 vs
  0.70 + reconstruction and compared matched frames: the error is
  stable across the run (mean |diff| ≈ 0.65 of 255 at frames 60, 150
  and 239 — it does not accumulate), the skyline difference is a thin
  1–2 px edge line, and the water's residual is soft sparkle mottling,
  not directional smear. No fix was applied: the measured weakness is
  the known static-case cost (glitter cannot be accumulated), and a
  reactive mask would trade it against the AA benefit. A fast pan
  would stress it harder — stated as a leftover, not hidden.
- **The governor steps up, on camera.** M3 proved step-down and the
  pin law; step-up needed headroom this bench never produced at 720p.
  At 160×90 output, starting the live governor at level 7, it stepped
  up on its own law — 7→6 at frame 300, 6→5 at 600, 5→4 at 930 —
  each after the sustained-headroom requirement and the 300-frame
  cooldown, then retreated to 5 within one 60-frame down-cooldown
  when the 1024² shadow pass landed (the pass puts the frame EMA at
  ~35–39 ms against ~5–9 ms with it off, and on llvmpipe that cost is
  output-size-independent, so rungs above 5 are unreachable on this
  bench at any output size). It probed 5→4 five times in the 2,400
  frames, retreating the first four times; the fifth probe landed
  exactly on the run's final frame, so that ledger ends mid-probe —
  the ceiling claim rests on the four completed retreats. The
  sustainable ceiling here is level 5; the proof is the log,
  adjustments and EMAs included in the M4 report.

## M5 — fold occlusion, motion at speed

M5 takes the two levers M4 named and left, in the open:

- **Fold occlusion — per-vertex horizon AO, baked from the height
  field itself.** Terrain generation now builds one shared height
  lattice (257², the field's own samples) and, for every vertex,
  walks eight azimuths out over the actual relief — gullies, ribs,
  benches and the wall's own bulk at 40 units — taking each azimuth's
  horizon angle, judged against the local tangent plane, and folding
  it into a sky-visibility term (cos² of the effective horizon,
  azimuth-averaged). The value rides the vertex (one float; the
  ledger prices it: +0.27 MiB tracked) and modulates exactly one
  shader term — the hemisphere ambient, the only light a face inside
  its own cast shadow has. Zero per-frame cost: no pass, no texture,
  nothing at runtime but a vertex attribute. The bake's statistics
  are the design working: island-wide mean visibility 0.987 (open
  ground is untouched — median 0.997), while the steep band where
  the folds live averages 0.868 with a tenth of its vertices below
  0.72. Judged on the M3/M4 crops' own ground — readability of the
  *shadowed* folds — the verdict is **improved, not cured**: the
  winter face's fold-scale tonal structure rises 8% (summer 6%),
  the change lands where it should (the face's darkest pixels move
  2–3× more than its bright ones), and the gully banding is visible
  in the crop in a way M4's flat brown was not. What still bounds
  it is named in the report with numbers: at the proof pose roughly
  a third of the face's displayed light is fog, and at night the
  ambient's share is so small the term is nearly a no-op (the moon
  term is the night term, and it is deliberately unoccluded). One
  alternative was built and convicted by measurement on the way: a
  bent-normal sampling of the ambient gradient cancels the scalar
  term against this two-stop hemisphere model (face |Δ| 1.5 vs 6.0),
  so the scalar ships and the bent direction waits for a directional
  ambient. (`crates/c35/examples/ao_probe.rs` prints the bake's
  distribution.)
- **Motion reconstruction — the fast pan, measured; no fix
  convicted.** M4 measured the drift and left the fast pan as a
  named unknown. M5 first reproduces M4's A/B *exactly* (summer
  drift, matched frames 60/150/239: luma mean |diff| 0.624 / 0.643 /
  0.653 — M4's numbers to the third decimal), then stresses the
  resolve at 4× and 16× the drift rate via a rig-only host flag
  (`--drift-rate`; shipped runs never set it). The error does not
  grow with rate: winter luma means sit in the same 0.71–0.92 band
  at rates 1, 4 and 16, p99 6–10 throughout, flat across frames —
  pose-dependent, not rate-dependent. The reconstruction's error is
  sampling-dominated (the 0.70 internal scale's glitter, the known
  static-case cost), not history-dominated: depth reprojection plus
  the resolve's neighbourhood clamp already reject what motion
  invalidates at every rate this rig can produce (per-frame image
  motion stays at whole-frame phase shift (0, 0) px, frame-to-frame
  mean |Δ| ≤ 0.22). So no velocity term was bought — the brief's
  trigger (error growing with motion) is explicitly not met, and
  the report says what would change the verdict: an operator camera
  with translation and whip motion, which the shipped drift rig
  does not have. That stress belongs to the milestone that builds
  the camera.

## M6 — the PS1.5 pivot, and the operator

M6 turns the engine toward its first game (DeBrief — a 1987
stealth-action immersive sim) and its chosen register:
**PlayStation 1.5 — better than PS1, not quite PS2.** Two builds:

- **The PS1.5 render path** (`PresentMode::Ps15`, beside the classic
  path — an added mode, never a replacement). The scene renders at a
  480p-class internal frame (at 1280×720 out: 853×480, aspect
  preserved), the history resolve runs at that same presentation
  resolution (the mode is a register, not an upsampling trick), and
  the blit samples **nearest** — a crisp texel grid on the output.
  Surfaces re-express through the palette discipline: terrain and
  water detail fields sample a 1.25-unit snapped texel grid, and
  every scene fragment exits through a palette grade — 20 steps per
  channel in sqrt space, ordered-dithered 4×4 Bayer so the steps
  break into texture instead of banding. Fog density becomes a look
  parameter (default ×2.0 — the reality layer's density, multiplied
  by the presentation's own factor), and the night sky gains the
  banner's sodium shore: a warm, lumpy lamp band on the horizon that
  exists only in the dark and answers to the same fog as everything
  else. There is **no vertex wobble and no affine warp anywhere, in
  either mode** — the chunk is resolution and palette, by doctrine;
  both are forbidden as defect and as fake-retro effect alike. On
  this software renderer the payoff is the doctrine's own numbers:
  winter hero frame-loop mean 430.15 ms classic → 125.13 ms PS1.5,
  GPU scene pass 215.80 → 56.91 ms, tracked VRAM 34.31 → 26.49 MiB —
  and the classic path is provably untouched: its winter frame is
  byte-identical to M5's (sha256 `c3d0525f…`), its grazing CPU/GPU
  parity hash is M5's own, and the reality matrix re-reads to the
  decimal on all four season pins. The web host carries the mode
  behind the same rig-seam class as its clock pin
  (`window.__comigen_present = "ps15"`).
- **The operator** (`crates/c35/src/operator.rs`) — the character
  pipeline, and DeBrief's protagonist as an engine citizen. Built
  procedurally, geometry and generated surfaces only: 113 rigid
  parts over a 15-bone skeleton, **1,346 triangles**, surfaced from
  a 256² atlas the engine paints itself (digital camo, vest webbing,
  skin, the visor's gradient with its hot point, and the NSA
  shoulder patch drawn as low-res surface art — eagle, striped
  shield, bitmap letters). Three references govern him: the bust
  portrait for face and visor, the banner for pose and night grade,
  and the full-body model sheet for proportions and everything
  below the chest — the sheet arrived after the first build and
  corrected it (longer legs, tall lace-up boots, capped knee pads,
  the drop-leg holster rig, the belt pouch row, a radio pack that
  clears the shoulders with two whips). Posing is rigid skinning,
  evaluated on the CPU when a host asks; three staged poses ship —
  stand-ready, crouch, aim — with the shoulder/elbow seams where
  aim and a future grapple-reach need them. He draws into the vista
  lit by the same reality layer and casts into the shadow map
  (winter day proof: shadow pass 16 → 17 draw calls, 131,072 →
  132,418 triangles — his exact weight, accounted). The governor's
  pin law holds in both presentations (300 pinned overruns, 0
  adjustments, classic and PS1.5 alike); live on this renderer both
  modes walk the ladder to its floor — measured, and reported
  un-tuned in the M6 report.

## Milestones

**M1 — the vista.** One deterministic procedural island: a generated
heightfield terrain with shader-side colour banding, a pure-math sky, an
analytic-wave ocean that reflects that sky, and sun-tinted fog. The whole
vista cost 4 draw calls and a few MiB of tracked GPU memory.

**M2 — the reality layer, shadows, detail, TAA, a window you can fly in.**

- The reality layer above: the vista is lit by the real sun, in the real
  season, wherever the observer is.
- **Real shadows:** one directional shadow map from the sun (single
  cascade, 2048×2048 — chosen because the vista spans ~760 units and
  1024² would leave >0.7 units per texel; at 2048² a texel is ~0.37
  units). The ledger prices the pass separately. The M1 headwall, flat
  for want of self-shadowing, now stands in its own shadow.
- **Terrain detail pass:** multi-octave noise grain and normal
  perturbation in the terrain shader, strata + grain breakup in the rock
  band, all faded with distance so far slopes cannot shimmer. Still zero
  image assets.
- **Temporal AA:** jittered projection + history resolve replaces the
  plain blit on backends that carry it (native Vulkan/Metal/DX12,
  WebGPU); the WebGL2 fallback runs FXAA instead. Every ledger states
  which anti-aliasing actually ran.
- **A windowed host** (`hosts/windowed`): the vista live in a real
  window — drift orbit until you touch it, then free-fly. See below.

**M3 — the engine core: ECS + jobs, temporal reconstruction, the
governor, tiled culling.**

- The data-oriented core and the governor above: the scene is
  components and systems, the frame is cost-accounted *and*
  self-governing, and the resolve reconstructs output resolution from
  the governor's internal scale.
- The terrain is tiled (4×4) with per-tile frustum culling and distance
  LOD in both passes — at the hero pose the camera pass submits 45,568
  of the 131,072 full-density triangles; the shadow pass keeps its
  casters at full density on purpose (see the core section).
- The headwall art debt from M2 is addressed in the rock band itself:
  wall-scale mottling, deeper strata, a cool cast in shadowed rock,
  talus aprons — and the frozen bay's glint no longer stripes: the ice
  roughens its own normals and the glint breaks into sparkle.

An M2 ledger, from the headless proof host (winter morning pinned by the
measurement rig at Chicago's latitude):

```
=== COMIGEN COST LEDGER (native, headless offscreen) ===
backend          : Vulkan / llvmpipe (LLVM 20.1.2, 256 bits) (Cpu)
output           : 1280x720
render scale     : 1.00 (internal 1280x720)
frames measured  : 30 (after 10 warmup)
frame time       : mean 192.89 ms, p95 204.14 ms  [frame-loop wall time: encode+submit+wait]
gpu shadow pass  : mean 37.44 ms over 30 samples [wgpu timestamp queries]
shadow pass      : 1 draw call, 131072 triangles (single cascade, 2048x2048)
gpu scene pass   : mean 85.91 ms over 30 samples [wgpu timestamp queries]
anti-aliasing    : TAA (jitter + history resolve)
draw calls/frame : 6
triangles/frame  : 131076
tracked vram     : 33.07 MiB (engine-tracked allocations, estimate)
hero pose png    : native-a-winter-morning.png
--- reality reading ---
clock source     : rig pin (COMIGEN_PIN_ISO=2026-01-15T08:00:00-06:00)
instant (UTC)    : 2026-01-15T14:00:00Z
observer         : lat 41.88 (COMIGEN_LAT), lon -87.63 (COMIGEN_LON)
solar time src   : longitude
sun              : elevation +6.15 deg, azimuth 125.9 deg, day length 9.3 h
season           : cold 1.00 / autumn 0.00 / spring 0.00 / summer 0.00
land & water     : snow line -3.0, water ice 1.00, wave scale 0.21
```

The reality reading now continues with the moon, on the same clock:

```
moon             : elevation +.. deg, azimuth .. deg, illuminated 0.000
```

Illuminated fraction is the geocentric disk fraction (Meeus ch. 48). It
is an audit line, not a cost: the moon writes uniforms the sky and sea
already sample, and the pass count and the draw count do not move. The
block above is the M2 capture and predates that line.

(Every number above is a software-renderer number — see Status. On real
hardware the same ledger is the first thing to look at.)

## Workspace layout

```
crates/c35/          the engine (ECS core, governor, renderer, world/reality layer, scene, noise, shaders)
hosts/native/        headless proof host: offscreen run, ledger, hero PNG
hosts/windowed/      windowed host: the vista live, free-fly camera, title-bar ledger
hosts/web/           browser host: same vista on a canvas, on-page ledger
```

## Build & run

Windowed (the one you can fly in):

```sh
cargo run --release -p comigen-windowed
# with your latitude (drives the seasons + solar time via the OS offset):
COMIGEN_LAT=41.88 cargo run --release -p comigen-windowed
```

Camera: the vista drifts until you touch it. Click to capture the mouse;
WASD/QE fly, Shift is speed, Esc releases. Leave it alone for 8 seconds
and the drift takes the camera back. The cost ledger rides in the title
bar; the full reality reading prints to stdout at startup. The quality
governor governs live in this host — keys **0–4** pin ladder levels
**0–4** (a pin is absolute; the governor then only watches and reports),
**`** releases the pin; the title bar shows the level in force.

### On a Mac (Apple silicon)

```sh
# once: install Rust from https://rustup.rs if you don't have it
git clone https://github.com/nightwarelabz/C35.git
cd C35
COMIGEN_LAT=41.88 cargo run --release -p comigen-windowed
```

That's all: no extra targets or toolchains — the host is portable
wgpu/winit (winit 0.30) and your Mac's Metal backend is picked up
automatically. The first build takes a few minutes; after that the window
opens on tonight's real sky at your latitude. (This recipe is verified by
compiling the workspace and cross-checking the host for
`aarch64-apple-darwin`; the window itself is yours to open.)

Headless proof host:

```sh
COMIGEN_LAT=41.88 COMIGEN_LON=-87.63 \
  cargo run --release -p comigen-native -- --frames 120 --out hero.png
# options: --width 1280 --height 720 --scale 1.0 --drift
#          --pose hero|grazing (the culling proof pose)
#          --cull cpu|gpu (visibility path; default gpu where the
#            backend carries compute, cpu elsewhere — the ledger
#            names the path that actually ran)
#          --pin-level N (governor pin; default 0) · --governor (live)
#          --start-level N (with --governor: ladder start, for the
#            step-up proof) · --save-at 60,150,239 (extra frame saves)
#          --target-ms 16.7 (governor target)
#          --present classic|ps15 (M6 presentation; classic default)
#          --ps15-fog 2.0 (the PS1.5 fog look multiplier)
#          --operator stand|aim|crouch (place the operator, staged pose)
#          --pose op-a|op-b (his proof cameras)
# measurement rig only: COMIGEN_PIN_ISO=2026-01-15T08:00:00-06:00 pins the instant
```

Browser:

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown -p comigen-web
wasm-bindgen --target web --out-dir hosts/web/pkg \
  target/wasm32-unknown-unknown/release/comigen_web.wasm
# serve hosts/web/ statically, e.g.:
python3 -m http.server --directory hosts/web 8080
```

The wasm-bindgen CLI version must match the `wasm-bindgen` crate version in
`Cargo.lock`.

Headless Linux notes: the native hosts need a Vulkan (or GL) driver to
find an adapter — on a GPU-less box, Mesa's software driver is enough
(`apt-get install mesa-vulkan-drivers libegl-mesa0` gives you llvmpipe).
The windowed host on X11 also wants `libxkbcommon-x11-0`. In headless
Chromium, WebGPU may decline to hand out an adapter on software stacks;
the web host detects that before touching the canvas and falls back to
wgpu's WebGL2 backend — the on-page ledger always names the backend that
actually rendered (and which AA it ran: WebGL2 carries FXAA, not TAA).

## Status

M6: the PS1.5 presentation mode (480p-class internal frame,
snapped texel surfaces, dithered palette grade, fog as a look
parameter, nearest presentation) beside a classic path that is
byte-identical to M5 — and the operator, DeBrief's protagonist,
standing in the vista as a procedural build (1,346 triangles,
generated atlas, three staged poses, shadow-casting) — on top of
M5 (fold occlusion; motion reconstruction stress-tested, no fix
convicted), M4 (cliff relief, GPU compute culling with CPU/GPU
byte-parity, the governor's step-up proven live), the M3 core
(ECS/jobs, temporal reconstruction, governor, tiled culling/LOD)
and the M2 reality layer, re-run to the decimal in M6's regression
(all four seasons). Proven on native (offscreen, and windowed
under Xvfb) from this tree; the wasm32 build is green and the web
package regenerated, but the headless-Chromium web drive is still
owed: M6's single attempt met the same wall as M5's — this
sandbox's Chromium refuses direct spawn ("Hatch WebGL domain
policy is required" — the policy is supplied by the browser
daemon, not by any flag; the drive died before a CDP endpoint
existed), so the web proofs run in a policy-equipped browser
context, not from the proof shell. The M6 report has the whole record. All numbers here are still
software-renderer numbers (llvmpipe / SwiftShader) — architecture
proof, not hardware benchmarks. The first real-GPU witness will be
an Apple M1 running the windowed host: Metal timestamps feed the
same ledger. Comigen is designated SystemZ's main graphics engine
(the graphics/compositor role its SPEC §16 foresaw), so the native
host path is the strategically first-class one; the hosts remain
standalone applications.

## License

AGPL-3.0 — see [LICENSE](LICENSE).
