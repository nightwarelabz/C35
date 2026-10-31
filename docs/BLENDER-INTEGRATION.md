# Blender × C35 — integration exploration

*Commissioned by Chase, 2026-10-30 ~22:30 CDT: "explore integrating blender
within the C35 engine for our design models etc. to make it easier to
create things directly in the world."*

## The boundary that decides everything

Blender never goes *inside* C35. Blender is not a library — it cannot be
embedded, and its GPL would swallow the engine if it were. Integration
means the opposite direction: **Blender becomes the authoring front-end,
C35 stays sovereign.** The engine's own `.c35m` text format (modeler v1,
PR #42) remains the one runtime format; Blender is a tool that writes it,
the way a compiler is a tool that writes binaries. The FROM-SCRATCH law
is untouched — every line of engine code is still ours; an exported mesh
is our data.

Facts on the ground:

- C35 modeler v1 already ships the receiving end: `modeler/format.rs`
  has `load` / `load_from` / `save` / `save_to` for the `.c35m` v1
  grammar (named materials, `v` lines, `f` lines with UVs and optional
  normals). A Blender exporter slots into an existing seam — no new
  engine format work.
- Blender today: **5.2.3 LTS** (Oct 2026, supported to Jul 2028),
  native on Apple Silicon — it runs on the M1 Air, which is also now a
  C35/DeBrief target machine. Pin the pipeline to 5.2 LTS.
- License: exported models are the author's own output and carry no
  GPL encumbrance (same as images out of GIMP). The add-on is in-house
  tooling; C35 never links Blender code. Clean boundary.

## Option A — the exporter add-on (foundation)

A small Blender Python (bpy) add-on: **Export → C35 Model (.c35m)**.

- Triangulates, applies transforms, converts Z-up → C35's Y-up
  (1 unit = 1 m both sides, so scale is honest).
- Blender materials map to C35's named materials (`painted-metal`,
  `lamp-glass`, …) by name; UVs from the active layer; flat normals
  per face to hold the PS1.5 facet read.
- **The era law lives in the exporter, mechanically:** it counts
  triangles and warns past the asset's budget, reports non-flat faces,
  and refuses silent 2M-triangle sculpts. The banner standard is a
  budget, not a vibe.

This alone replaces hand-written build scripts (`modeler/assets.rs`)
as the way authored models are born — the lamp standard's session
transcript becomes something you *model*, not something you code.

## Option B — live link: model it, see it in Greyport (the "directly in the world" step)

- The add-on gains **Export & Push**: one click writes the `.c35m`
  into the game's watched models folder.
- The DeBrief host watches that folder; a changed file hot-swaps the
  mesh in the running game within a frame or two. Blender on one
  screen, Greyport on the other — edit, push, walk around it.
- No socket protocol in v1; the filesystem is the link (the same
  reason the `.c35m` format is text and diff-able). Camera sync
  (frame a shot in-game, jump Blender's view to it) is a later
  luxury, only if the feel demands it.

## Option C — Blender as the world editor (where this fully lands)

- A Greyport scene assembled *in Blender*: instanced props (lamp
  standards, crates, kiosk, watchtowers) placed with transforms,
  each object carrying a `c35_name` custom property.
- A scene exporter writes a placement manifest (text, diff-able,
  house style) that the host's block builder consumes — the street
  stops being hardcoded coordinates and becomes something dressed
  by hand, in a viewport, against the banner as a reference backdrop.
- The operator gets his real home here too: modeled/sculpted in
  Blender at the banner grade, exported through Option A — instead
  of ~114 code-assembled primitive parts.

## What each tool keeps

Blender authors: sculpting, retopo, real UV unwrapping, curves,
modifiers, a true viewport. The in-engine modeler keeps: runtime and
procedural generation, the mod engine, zero-dependency determinism.
They compose — Blender writes `.c35m`, modeler ops can still
post-process it, the engine loads both identically.

## Testability note

Blender runs headless (`blender --background --python …`). The whole
pipeline — add-on, scripted test model, export, engine-side
`format::load`, engine still render — can be built and proven in
this VM without a display, before Chase ever opens it on the Mac.

## Amendment (2026-10-30 22:33 CDT, Chase) — the bridge is its own repo: HOTWIRE

Chase's refinement supersedes the mechanics above: the bridge is
built as **our own tech in its own repo** —
[nightwarelabz/hotwire](https://github.com/nightwarelabz/hotwire) —
so nothing in the link is borrowed or license-entangled. Consequences:

- **Zero bpy in v1.** Blender's only act is its own built-in glTF
  export (.glb) into a drop folder. Hotwire (Rust, ours) converts
  glTF → `.c35m` with the era enforcement of Option A, and the host
  hot-reloads per Option B. The Option-A "exporter add-on" becomes a
  later, optional one-click convenience living in hotwire's repo.
- The contract lives in hotwire's `SPEC.md` (founding commit
  `68ad3a67`): conversion rules, budgets, manifest, host contract.
- Option C (world editing) stands as hotwire's v2.

## Recommendation

**A → B as one spike, C as the follow-on.** (Now executing as
Hotwire v1.)

Spike contents: (1) the exporter add-on; (2) host hot-reload of a
watched models folder; (3) one proof asset authored in Blender,
exported, standing in Greyport, then edited and re-pushed live.
Then C (world editing) once the pipe is proven.
