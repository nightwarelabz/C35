# C35 Mod Engine — specification (design only)

Owner directive, 2026-10-30 (Chase): "build a mod engine. something people
can use to easily mod the game how they want and make it to where they can
reuse props we have made or make their own."

Status: SPEC ONLY. No tree edits, no repo writes. The implementer is the
build crew; the audience is Chase and that crew.

> **Status as of c158214:** v1 is implemented in `crates/c35/src/modding/`
> (registry, strict JSON parser, loader, validator), with the example mod
> at `crates/c35/example-mods/example-street/`. In this repo the validator
> runs as `cargo run -p c35 --example check_mod -- <mod dir>`;
> `--check-mod` is the DeBrief game binary's flag. v2/v3 remain spec.

## 0. The one-sentence shape

A mod is a folder of text files. It names props from the game's own
registry, or defines new props as recipes in the game's own part
vocabulary, and places them by rule on the game's own anchors. No Rust,
no compiler, no code in v1 or v2. The engine generates every mod prop
through the same painters and the same grade as the base game, so a mod
cannot break the PS1.5 look by construction — it can only dress Greyport
differently.

Laws that bind this spec (from PS15-DIRECTION.md and SUBSYSTEMS.md):
- Zero image assets in the base game; procedural generation throughout.
- Determinism: same inputs, same world, byte for byte.
- Cost ledger: every addition carries its triangle and atlas budget.
- Solo game: networking is deliberately off the board, so there is no
  workshop, no store, no auto-download. Mods travel as folders and zips.
- The beauty law: low-poly with high surface craft, never globby
  Minecraft. The mod engine must not become a hole in that law.

OPEN SEAM: `~/workspace/c35-stage/BEAUTY-PASS.md` did not exist when this
spec was written (a sibling crew is writing it). When it lands, this spec's
§1 vocabulary, §2 painter rules, and §7 renderer dependencies must be
checked against it and the two documents cross-linked. Until then, this
spec states its own requirements and does not assume the beauty pass's
answers.

## 1. What a "prop" is in C35 terms

Today there are no prop files. A prop is procedural code: a list of parts
(boxes, wedges, cylinders, and the other primitives the part vocabulary
already has) with transforms, assembled by a builder function, surfaced
by the painters into shared atlas regions. The operator is 110 parts;
the Slice 1 block is one builder call. A modder cannot reuse what has no
name, so the mod engine's first job is to give every shipped prop a name
and a parameter list.

### 1.1 The prop registry

A single engine-owned table, built at startup from the game's own
builders, mapping a stable string id to a prop definition:

- `id` — namespaced, e.g. `greyport:lamp-post`, `greyport:crate`,
  `greyport:three-bars-banner`, `debrief:desk-chair`. The namespace
  (`greyport:`, `debrief:`) is part of the id; two mods may both define
  `crate`, they may not both define `greyport:crate`.
- `parts` — the ordered part list: primitive, local transform, size.
- `materials` — per-part painter + palette references, by name
  (e.g. `painted-metal`, `wet-concrete`, `sodium-lamp-glass`), not by
  atlas coordinates. Atlas regions are allocated by the engine, never
  hand-addressed by a mod.
- `parameters` — the knobs the builder already varies, exposed with
  defaults and legal ranges: overall size (scale, and per-axis where the
  builder supports it), palette variant, wear amount, and any
  builder-specific switch (banner raised/lowered, crate open/closed).
  A parameter is either in this list or it does not exist to modders.
- `placement` — the prop's physical contract: footprint (width × depth),
  height, collision class (solid, pass-through, climbable, grapple
  anchor yes/no), occluder flag for the light field, and footstep
  surface (what sound the animator's events price when a foot lands on
  it — wood, metal, concrete, wet stone).
- `budget` — triangle count and atlas share at default parameters.
  The cost ledger reads this; a prop whose budget is unknown does not
  ship, base game or mod.
- `tags` — placement vocabulary: `street`, `kerb`, `wall-face`,
  `rooftop`, `interior`, `dockside`. A prop tagged `wall-face` can be
  hung on a wall run; a prop tagged `street` cannot, and the validator
  says so.

Every prop the game ships gets a registry entry. This is not a mod-only
table: the city builder itself composes registry entries (see §7). If a
prop exists only as one-off geometry inside a builder function, it is
not shippable until it is registered.

### 1.2 Reuse, not copy-paste

A mod reuses a shipped prop by id and overrides parameters:

```json
{ "prop": "greyport:lamp-post", "variant": { "height": 4.2, "wear": 0.7 } }
```

The engine rebuilds that prop from its recipe with the overridden
parameters. The modder never copies geometry, never touches atlas
coordinates, never re-implements the lamp. An override outside a
parameter's legal range is a validation error, not a silent clamp —
silent clamps teach modders the wrong range.

## 2. Making your own prop without knowing Rust

### 2.1 The format: JSON, parsed by the engine's own strict parser — ONE choice

Mod files are JSON. Reasons, in order:

- Every modder already knows it, every text editor highlights it, every
  tool on every platform can emit it. The "easily" bar (§6) dies on any
  format that needs explaining first.
- The prop schema is flat-ish: an object, a parts array, a parameters
  object. JSON holds that shape without strain.
- The engine parses it with a small strict parser owned by the engine,
  scoped to the prop/mod schema: unknown fields are errors, nesting
  depth is capped, file size is capped, duplicate keys are errors.
  Strictness is the sandbox — a mod file is data, and data that fails
  the schema never reaches the game (§4).

No new dependency. This is the point of the strict-parser choice: a
general JSON library would bend the zero-dependency posture for no
benefit, because the engine does not need general JSON — it needs one
schema, validated hard. The parser is a few hundred lines in the asset
pipeline (SEEDED today, §7) beside the existing validators, and it is
shared by the manifest, the prop files, and the placement files so there
is exactly one JSON dialect in the mod system.

Rejected, briefly: RON (Rust-ecosystem-native, but modders do not know
it — it fails the "easily" bar on sight); TOML (good for flat config,
awkward for ordered part arrays and nested transforms); YAML (parser
complexity and ambiguity — the "Norway problem" class of silent
misreads — for a system whose whole promise is that bad data is caught
at load, not at runtime).

### 2.2 What a prop definition file contains

One prop per file, `props/<name>.json` inside the mod folder:

- `id` — the mod's namespace plus name, e.g. `my-street:fruit-cart`.
  The namespace is declared once in the manifest (§4) and enforced:
  a prop file claiming another namespace is a validation error.
- `parts` — an array. Each part: `primitive` (one of the engine's part
  vocabulary — box, wedge, cylinder, and whatever the renderer pass
  freezes; the vocabulary is closed, and a part naming a primitive that
  does not exist is a validation error), `size`, `position`,
  `rotation`, and `material` (a painter + palette name from the
  registry's material list, or a palette override within the declared
  palette's legal range).
- `parameters` — optional. A mod prop may expose its own knobs (e.g.
  `awning: true/false`, `crates: 0-4`) with defaults and ranges, so
  other mods — and the modder's own placements — can vary it exactly
  as shipped props vary. A mod prop with no parameters is a fixed prop;
  that is fine and common.
- `placement` — footprint, height, collision class, grapple-anchor flag,
  occluder flag, footstep surface, and tags, exactly as §1.1. A mod
  prop that omits placement gets no collision and no anchor — it is
  dressing only, and the validator says so in the report rather than
  guessing.
- `budget` — declared by the modder, verified by the engine at load:
  the engine generates the prop once and counts real triangles and
  atlas share. A declaration that understates the real budget by more
  than the validator's tolerance is an error. The ledger never trusts
  a mod's self-report.

### 2.3 Procedural stays procedural for mods

A mod prop is a recipe, not a mesh. The engine generates its geometry
from the recipe at load, through the same painters, the same palette
grade, the same dither, and the same atlas allocation as base-game
props. Consequences, all intended:

- A mod prop automatically reads as PS1.5. The grade is not a guideline
  modders must follow; it is a pipeline they pass through.
- A mod prop is deterministic: the same recipe + the same engine
  version generates the same geometry on every machine. The run ledger
  records the mod set and load order (§4), so a modded run is still a
  reproducible run.
- A mod prop inherits engine improvements for free: when the materials
  pass improves the painters, every mod prop painted through them
  improves too.

### 2.4 Image imports: NOT allowed in v1 — the rule and the reason

Rule: mods may not ship bitmap images. No PNG, no JPEG, no texture
files of any kind. A mod folder containing an image file fails
validation with a plain reason ("image imports are not supported;
define the surface as a painter recipe").

Reasons, in order:

- The beauty law is the product. The engine can guarantee a recipe's
  grade; it cannot judge a bitmap's taste. One photoreal texture pack
  and Greyport stops being Greyport.
- The sandbox stays airtight: no image decoder in the mod path means no
  decoder attack surface and no 50 MB texture pack smuggled into a
  "street dressing" mod.
- Zero-dependency stays intact: no decoder crate enters the tree for
  mods.
- The escape valve is real, not a shrug: painters are parameterized
  (noise seed, wear, palette variant, pattern switches such as brick
  coursing, hazard striping, and signage lettering from the UI/text
  engine's glyph set when it lands). A modder who wants a new look
  reaches for a new painter recipe, not a bitmap. If a genuine need
  appears that no painter recipe can express, the revisit is defined
  now: palette-quantized imports only (engine quantizes any imported
  image to the mod's declared palette and caps it at the engine's
  atlas tile size, through the same grade) — never raw bitmaps, and
  only after a modder demonstrates the need with a recipe that fails.
  That revisit is v3-or-later work, not a v1 promise.

## 3. What "mod the game how they want" covers — staged honestly

### v1 — props and placement (this spec's build target)

- New props: define your own (§2), reuse shipped props with parameter
  overrides (§1.2).
- Placement: dress streets, walls, rooftops, and interiors by rule
  (§6) or by anchor (§7). Street dressing, buildings, furniture, gear
  dressing on existing rigs (a new crate on a rooftop, a new banner on
  a wall, a new desk in the debrief room).
- Removal and replacement: a mod may hide a shipped prop instance or
  replace a shipped prop's parameters game-wide (e.g. every lamp-post
  taller) — replacement is by registry id and is reported as a
  conflict when two mods do it (§4).

v1 does NOT do, and the docs must say so in plain words on the first
page: no mission editor; no new enemy types, AI behavior, or dialogue;
no changes to the operator, soldier, mecha, or handler rigs; no new
systems (no new senses, no new stimuli, no new currencies); no changes
to the persistence format or the Archive; no code of any kind. A mod
that needs any of those is a v2 or v3 mod, and the validator refuses
files that claim those powers rather than ignoring them.

### v2 — behavior and tuning (tables, still no code)

- Tuning tables: movement speeds, perception prices, stimulus values,
  skill numbers, weapon numbers, checkpoint patience — every number
  the design already prices, exposed as named tables with legal ranges.
  A tuning mod overrides named values; the engine validates ranges
  exactly as it validates prop parameters.
- Behavior selection, not behavior authorship: where the game already
  has two or more doctrines (e.g. patrol route shapes, search patterns),
  a mod may select among them and set their parameters. It may not
  invent a new doctrine.
- Still no code. v2 is v1's data-only principle extended from props to
  numbers.

### v3 — missions and maps (code arrives, gated)

- New maps: a city block defined the way the city builder defines one
  (registry props + anchors + light field), placed on the world grid.
- New ops: triggers and conditions composed from the op engine's
  vocabulary, graded by the existing debrief grades (Ghost / Surgeon /
  Butcher) — new grade names are not in scope.
- Modder logic arrives with the scripting answer in §5. v3 is the only
  stage that needs it, which is why §5's cost is deferred, not paid
  in v1.

## 4. How mods load and compose

### 4.1 The mod pack

A mod is a folder:

```
mods/my-street/
  mod.json          — manifest
  props/*.json      — prop definitions (§2.2)
  placements.json   — placement rules (§6)
  tuning.json       — v2 only; absent in v1, refused in v1
```

`mod.json` declares: `id` (namespace, e.g. `my-street`), `name`,
`author`, `version` (the mod's own version), `engine` (the engine
version range it was built against, e.g. `>=0.9 <1.0`), `load_after`
(optional list of mod ids this mod expects to load after), and
`description`. Sharing a mod is zipping the folder. Installing a mod is
unzipping it into the game's `mods/` directory. There is no installer,
no registry, no download — consistent with networking being off the
board.

### 4.2 Load order

Deterministic, always: (1) base game; (2) mods sorted by `load_after`
topology (a mod listed in another's `load_after` loads later);
(3) ties broken by folder name, alphabetically. The resolved order is
recorded in the run ledger, so a modded run reproduces exactly given
the same mod set. A `load_after` cycle is a validation error naming
both mods.

### 4.3 Conflicts

Two mods touching the same thing is normal, not an error — but it is
never silent:

- Same prop id defined twice: the later-loaded mod wins, and the
  conflict is reported (in the load report and the in-game mod menu).
  The losing definition is ignored, not merged — partial merges are
  how two good mods make one broken prop.
- Same shipped prop's parameters replaced twice: later wins, reported.
- Same placement anchor claimed twice: placements compose (both props
  are placed) unless their footprints overlap, in which case the later
  placement is dropped and reported. The street never renders two
  crates inside each other because two mods agreed to.
- The load report is a first-class output: a text report at boot and a
  screen in the mod menu listing every loaded mod, every conflict and
  its winner, and every quarantined mod with its reasons (§4.5).

### 4.4 Versioning against engine versions

The manifest's `engine` range is checked before any file in the mod is
read. Outside the range, the mod is refused with the reason ("built
for engine >=0.9 <1.0, running 1.2") — not loaded hopefully. Engine
versions are the C35 version the game ships with; a minor engine
release that changes the prop schema bumps the range the game accepts,
and the release notes say which mods break and why. A mod that
declares no `engine` range is refused: silent compatibility is how a
v1 mod silently corrupts a v3 city.

### 4.5 When a mod breaks, the game still boots

Every mod passes validation before it touches the world: schema,
namespace, part vocabulary, parameter ranges, budget verification,
placement anchors, load-order cycles, engine range. A mod that fails
any check is quarantined: none of its props load, none of its
placements apply, and the game boots without it. The boot screen and
the mod menu list the quarantined mod and its reasons in plain words
with file names and line numbers where the parser can give them. A
quarantined mod never crashes the game, never half-loads a prop, and
never corrupts a save — validation runs before the world builds, and
the world build never sees quarantined data.

Two further honesty rules: a run with any mod loaded is marked MODDED
in the run ledger and in the service record's margin, and debrief
grades from modded runs carry that mark. Mods change the city; they do
not get to launder a grade. And a modded save loads only with the same
mod set (same ids, same versions) or with mods disabled and mod props
stripped — never with a silently different city than the one the save
was made in.

## 5. The scripting question — ONE recommendation

Recommendation: **data-only for v1 and v2; WASM plugins for modder
logic in v3.** That is one answer with a staged arrival, not a menu.

Why WASM, when the day comes:

- Sandboxed by construction. A WASM module cannot read the player's
  files, open a socket, or touch memory outside its sandbox unless the
  host hands it that capability — and the host hands it only a narrow,
  deterministic API: spawn/despawn registry props, read field values
  (light, stimulus, alert state), register triggers on the op engine's
  vocabulary, and write to the mod's own state. The sandbox is the
  runtime's, not hand-rolled policy code the crew must maintain.
- Language-agnostic for modders: Rust, C, Zig, or anything compiling
  to WASM can author a mod. The game does not pick the modder's
  language.
- Deterministic, if the host API is: no wall-clock, no randomness
  except seeded values the engine provides, no host calls outside the
  declared API, and fuel limits (bounded execution per tick) so a
  looping mod degrades to a quarantined mod, not a hung game.

What it costs, stated plainly: a WASM runtime dependency (the
zero-dependency posture bends here — the bend is justified in v3 by
sandboxing, which no dependency-free option provides), the host-API
design and its determinism proof, and fuel accounting in the cost
ledger. That cost is why it is v3-only: v1 and v2 need none of it, and
paying it early would delay the prop system for a power no v1 modder
can use.

Rejected, briefly:

- Lua: the modding world's default, and the most familiar — but it
  means a C dependency (vendored interpreter), which breaks both the
  Rust-purity story and the zero-dependency posture, and its sandbox
  is hand-rolled policy, not a runtime guarantee.
- Rhai: pure Rust and small, but it is a language only this game
  would teach modders, and its sandboxing is opt-in configuration
  rather than a construction guarantee.
- Dynamic libraries / native plugins: disqualified outright. A native
  plugin runs with the game's own privileges — it can read files,
  open sockets, and corrupt saves. There is no sandbox to argue about.

## 6. The "easily" bar — a first-time modder, start to finish

No Rust toolchain. No compiler. A text editor and the game. Six steps:

1. Make a folder: `mods/my-first-prop/`.
2. Write `mod.json`: id, name, engine range (copy the shipped example
   mod's manifest and change the names — the example is the template).
3. Write `props/fruit-cart.json`: parts from the engine's vocabulary,
   materials by painter name, footprint and tags. The shipped example
   mod contains a prop file to copy (strict JSON — no comments; its
   README explains each field).
4. Write `placements.json`: one rule — e.g. "place `my-first-prop:
   fruit-cart` on street `mercer-street`, kerb side east, every
   12 metres, facing the road." Placement is by rule and anchor, not
   by absolute coordinates, so the prop survives city rebuilds.
   Absolute coordinates are allowed but the validator warns they are
   fragile ("this placement will not follow a rebuilt street").
5. Run the validator: `debrief --check-mod mods/my-first-prop` prints
   errors with file names and line numbers, or "mod valid: 1 prop,
   1 placement, 84 triangles, 0 atlas tiles." (v1 mods define no new
   atlas tiles.) Fix errors until it
   says valid — the loop is seconds, not a build cycle.
6. Launch the game, open Mods in the menu, enable `my-first-prop`,
   walk Mercer Street. The prop is there, lit by the same lamps,
   priced by the same footstep surface, graded by the same palette.

The in-game Mods menu is part of v1, not an afterthought: list, enable
/ disable, load report, conflicts and winners, quarantined mods with
reasons. A modder who cannot see what loaded has no way to debug what
did not.

The shipped example mod is a v1 deliverable, not documentation: a
complete mod folder in the game (one reused shipped prop with
overrides, one new prop, one placement rule) that dresses one street.
If the example does not load and show in the game, the mod engine is
not done — the example is its proof.

## 7. Sequencing inside the outside-in order

The mod engine is engine-layer work, and it lands inside the ENGINE
phase — before the city pass, not after it, because the city pass is
its first customer and its proof.

What the mod engine needs from the renderer / materials pass (engine
phase, first):

- The part-primitive vocabulary frozen as the prop language (§2.2):
  the closed list of primitives a mod prop may use, with their
  transform rules. Until the vocabulary is frozen, mod props cannot be
  validated.
- The material registry addressable by name (§1.1): painters and
  palettes as names, not atlas coordinates, so mods reference materials
  the way the engine does.
- Atlas allocation that reserves mod space: atlas regions are
  allocated dynamically at load (base game first, then mods in load
  order), never baked to fixed coordinates — fixed coordinates are how
  a mod silently overwrites a shipped prop's surface.

What the city pass must do to stay mod-friendly (city phase, second):

- Build the block from registry entries from the start (§1.1). No
  one-off geometry inside the block builder: every lamp, crate,
  banner, and bench in the rebuilt Greyport is a registry entry with
  parameters, placement, and budget. A city built from one-offs is a
  city modders cannot reuse.
- Ship the anchor set with the game: a `city-anchors` listing naming
  every street, wall run, rooftop pad, lamp position, and interior the
  city exposes for placement — the same anchors the city's own
  placements use. Mod placements reference anchors, never raw
  coordinates, by default (§6 step 4).
- Dress one street twice: once with the base game, once with the
  shipped example mod (§6). The example mod is the city pass's proof
  that the registry, the anchors, and the validator all work on the
  real city.

What the later phases inherit:

- Weathers (third): rain, fog, and wet surfaces are systems, not
  props — but mod props inherit them automatically, because a mod
  prop's footstep surface and material already flow through the same
  fields (wet stone reprices a mod prop's footstep exactly as it
  reprices a shipped prop's). No mod-engine work is owed to weathers
  beyond that inheritance, and the spec records it so nobody
  re-opens the mod format for rain.
- Lighting (fourth): the same inheritance. A mod prop placed under a
  sodium lamp is lit by the same field, occludes the same field if
  its occluder flag says so, and is seen or hidden by the same
  perception read. Lighting changes the field; mods live in it.
- v2 tuning tables and v3 missions/maps ride the same load,
  validation, conflict, and quarantine machinery (§4) — new file kinds,
  not a new loader. The loader is written once, in v1, and extended,
  never replaced.

## 8. What would make this spec wrong

- If the part vocabulary cannot be frozen without breaking the
  operator or soldier builders, §2.2's closed vocabulary is the first
  thing to renegotiate — but the fallback is a versioned vocabulary
  (v1 primitives only), not an open one.
- If the beauty pass (open seam, §0) changes the painter/material
  regime, §1.1's material references and §2.4's no-bitmap rule must be
  re-read against it before implementation.
- If Chase wants image imports in v1 after all, §2.4's revisit clause
  already defines the only form they may take (palette-quantized,
  capped, graded) — the rule bends in that one direction, never toward
  raw bitmaps.
