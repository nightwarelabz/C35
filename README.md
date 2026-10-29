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
   offscreen target at a configurable internal scale and blits up, so
   quality scales to the hardware instead of the other way round.
   (Temporal reconstruction is planned for M2 and will be labelled when
   it lands — the M1 upscale is a plain blit and says so.)
5. **One tree, every target.** Rust + [wgpu](https://wgpu.rs): Vulkan /
   Metal / DX12 natively, WebGPU / WebGL2 in the browser — the same engine
   crate, the same world seed, the same frame, everywhere. Native and
   browser are built **in lockstep**, never ported after the fact.

## Milestone 1 — the vista

One deterministic procedural island at golden hour: a generated heightfield
terrain with shader-side colour banding (shore sand, grass, rock, snow),
a pure-math sky with sun disc and warm horizon, an analytic-wave ocean that
reflects that same sky, and distance fog tinted by the sun. Fixed seed —
both hosts render the identical world. A named **hero camera pose** is the
proof shot; a slow drift begins from it.

The whole vista costs **4 draw calls** (sky, terrain, water, blit) and a
few MiB of tracked GPU memory — terrain mesh included. The ledger prints
the real numbers on every run.

## Workspace layout

```
crates/c35/          the engine (renderer core, scene, noise, shaders)
hosts/native/        headless proof host: offscreen run, ledger, hero PNG
hosts/web/           browser host: same vista on a canvas, on-page ledger
```

## Build & run

Native (headless — no window needed):

```sh
cargo run --release -p comigen-native -- --frames 120 --out hero.png
# options: --width 1280 --height 720 --scale 0.65 --drift
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

Headless Linux note: the native host needs a Vulkan (or GL) driver to find
an adapter — on a GPU-less box, Mesa's software driver is enough
(`apt-get install mesa-vulkan-drivers libegl-mesa0` gives you llvmpipe).
In headless Chromium, WebGPU may decline to hand out an adapter on software
stacks; the web host detects that before touching the canvas and falls back
to wgpu's WebGL2 backend — the on-page ledger always names the backend that
actually rendered.

## Status

M1: renderer core + vista + ledger, proven on native (offscreen) and in
headless Chromium from this tree. All M1 numbers are software-renderer
numbers (llvmpipe / SwiftShader class) — the M1 proof is architectural:
one engine, both targets, measured. Real-GPU benchmarks come with real
hardware runs.

## License

AGPL-3.0 — see [LICENSE](LICENSE).
