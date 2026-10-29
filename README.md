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
4. **Resolution scale as a first-class knob.** The scene renders to an
   offscreen target at a configurable internal scale and reconstructs up —
   since M2 with jittered-projection temporal AA (TAA) where the backend
   carries it, FXAA on the WebGL2 fallback path; each ledger states which
   anti-aliasing actually ran.
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
  elevation; at night the stars come out and a moon — a faint key light
  mirrored from the sun (its phase is not modelled; it is a light, not an
  ephemeris) — takes over.
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
  run prints the clock source and the latitude source actually used; with
  no latitude the engine runs at a stated equator fallback — never a
  silent pretence. The web page's readout says the same when geolocation
  is denied, and names the fallback in force.

**Privacy:** location is used locally only. Coarse latitude is all the
engine consumes for its seasons (longitude only sets solar time), and
nothing is transmitted anywhere — there is no server component to send it
to.

**The no-controls law:** the shipped hosts carry **no time-of-day or
season controls** — no sliders, no keys, no query parameters. The engine
reflects reality; it does not offer to edit it. The single exception is
the *measurement rig*: the headless hosts accept `COMIGEN_PIN_ISO`
(an ISO-8601 instant) in the environment, and the web page honours a
`__comigen_pin_unix` value injected before load — read by the hosts only,
never by the engine crate, and never exposed in any UI. It exists so a
run can be pinned, measured and audited, the same standing as the hero
camera pose.

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

(Every number above is a software-renderer number — see Status. On real
hardware the same ledger is the first thing to look at.)

## Workspace layout

```
crates/c35/          the engine (renderer core, world/reality layer, scene, noise, shaders)
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
bar; the full reality reading prints to stdout at startup.

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
# options: --width 1280 --height 720 --scale 0.65 --drift
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

M2: reality layer + shadows + terrain detail + TAA/FXAA + windowed host,
proven on native (offscreen and windowed-under-Xvfb) and in headless
Chromium from this tree. All numbers here are still software-renderer
numbers (llvmpipe / SwiftShader) — architecture proof, not hardware
benchmarks. The first real-GPU witness will be an Apple M1 running the
windowed host: Metal timestamps feed the same ledger.

## License

AGPL-3.0 — see [LICENSE](LICENSE).
