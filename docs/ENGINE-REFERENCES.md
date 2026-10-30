# ENGINE REFERENCES — what C35 / DeBrief may take from open engines

Research and design only. No tree edits, no repo writes, NO code copied
into the tree by this document. Written 2026-10-30 after Chase's
question: can we take the best pieces of open-sourced engines —
DarkPlaces first, and others — per area, so we don't develop entirely
from scratch but have a crude board to base work on?

Short answer: yes — and the license wall the first draft of this audit
assumed is DOWN. Both house repos are AGPLv3 (verified below), and
every engine piece in scope is combinable with AGPLv3, because
DarkPlaces is GPL-2.0-**or-later**, not GPL-2.0-only. The work that
remains is the PORT: C and C++ and QuakeC into Rust, inside C35's own
architecture. No piece in this audit can be pasted — different
languages, different engine shapes — but every piece in the license
table marked ADAPT/PORT may be read, followed, and re-expressed in
Rust inside the AGPLv3 tree.

Standing law this audit serves, not revises: zero-dependency doctrine
(legal permission is not architectural adoption — see §3's last
paragraph), determinism, the cost ledger, the one-field light law,
Rapier stays PARKED, networking stays off the board (solo by design),
and the procedural, zero-image-asset rule.

## 0. The license reversal — verified from the repos

Chase's correction (2026-10-30): the game and the engines ALREADY
have AGPL licenses. Verified, not assumed:

- **nightwarelabz/C35, `LICENSE` at main `c22ff942`**: the full GNU
  AGPL text, "Version 3, 19 November 2007" — blob
  `0ad25db4bd1d86c452db3f9602ccdbe172438f52`, read through the
  GitHub connector.
- **nightwarelabz/DeBrief, `LICENSE`**: the identical AGPLv3 text,
  the identical blob SHA. Same license, both repos.
- **DarkPlaces' exact version statement** — the single most important
  verification in this document, read from the repo's own file
  (`xonotic/darkplaces`, `darkplaces.h`, commit `d93f9c42`):

  > "you can redistribute it and/or modify it under the terms of the
  > GNU General Public License as published by the Free Software
  > Foundation; either version 2 of the License, or (at your option)
  > any later version."

  That is **GPL-2.0-or-later**. The "or any later version" clause is
  the whole ballgame: the licensee may choose GPLv3 terms, and GPLv3
  combines with AGPLv3 under AGPLv3 §13 (which expressly permits
  linking or combining a covered work with a GPLv3 work into a single
  combined work, conveyed under AGPLv3, with the GPLv3 part remaining
  governed by GPLv3).

Combination rules applied in §3, stated once:

- **GPL-3.0 / GPL-3.0-or-later → AGPLv3: COMBINABLE.** AGPLv3 §13
  says so expressly.
- **GPL-2.0-or-later → AGPLv3: COMBINABLE**, by choosing the later
  (v3) terms. This is DarkPlaces' case.
- **GPL-2.0-only → AGPLv3: NOT COMBINABLE.** GPLv2-only and AGPLv3
  are mutually incompatible (no "or later" escape hatch). Any piece
  found to be GPL-2.0-only stays REIMPLEMENT-ONLY. **No piece in
  this audit's scope was found to be GPL-2.0-only** — but see the
  header-census caveat below.
- **Permissive (MIT / Apache-2.0 / BSD / zlib): COMBINABLE**, as a
  dependency or vendored code, with notices kept.

Caveats, stated plainly (this is a research audit, not legal advice):

1. **Per-file headers govern over repo-level summaries.** The
   DarkPlaces verification above is the master header in
   `darkplaces.h`. Before ANY adaptation, the build crew runs a
   header census over every file it touches: a single file carrying
   a GPL-2.0-only header (no "or later") is uncombinable and stays
   reimplement-only, whatever the repo's headline license says.
2. **Adapted code keeps its notices.** Ported work carries the
   original copyright notices and a note of the port, in the file.
3. **The combined work is conveyed under AGPLv3.** AGPLv3 §13's
   network-use duty already applies to C35's web host today
   (remote users must be offered Corresponding Source); adapting
   GPL pieces changes nothing about that duty — it only widens the
   source that must be offered.
4. **Legal permission is not architectural adoption.** Recast and
   SoLoud are legally vendible, but vendoring C/C++ code bends the
   zero-dependency law. §4 says, per subsystem, whether the
   recommendation is ADAPT (port the design), VENDOR (legally open,
   needs Chase's ruling), or DESIGN-ONLY.

## 1. DarkPlaces — the FPS reference (Quake-derived, Nexuiz/Xonotic)

What it is: the C engine forked from id's GPL Quake source by
LadyHavoc and contributors, kept compatible with Quake 1 while
adding a modern GL renderer, CSQC/SSQC/MenuQC QuakeC contexts, and
Quake 3 format support. Nexuiz and Xonotic run on it. Language C,
renderer OpenGL — so for C35 (Rust + wgpu) every piece below is a
PORT, never a paste. The value is the design, the data structures,
and the tuning shapes, all proven at FPS speed for twenty years.

### 1.1 FPS movement and physics feel — TAKE THE ARCHITECTURE

- **The wishdir/wishspeed model.** Quake movement is not a force
  simulation: each tick, input forms a desired direction
  (`wishdir`) and desired speed (`wishspeed`), and the player's
  velocity is accelerated toward it (`accelerate`) with ground
  friction bleeding speed when input stops. Air movement keeps the
  same shape with a different acceleration constant (air control),
  which is why Quake-family movement preserves momentum and
  supports bunny-hopping: velocity is never zeroed by the engine,
  only steered. C35's capsule controller should adopt this shape —
  it is the reason the feel is crisp — with C35's own constants in
  meters, on the fixed step, ledger-provable.
- **Data-driven physics sets.** Xonotic's `common/physics/player.qc`
  resolves movement constants through a `g_physics_<set>_<option>`
  cvar pattern, so whole movement feels (classic Quake, Xonotic,
  Nexuiz-tuned) swap as data. C35 should steal the PATTERN: the
  movement constants live in a named set the traversal course can
  swap, not in code.
- **Verified Quake-unit hull (from `player.qc`):** standing hull
  mins `-16 -16 -20`, maxs `16 16 36`, eye offset `0 0 30`; crouch
  hull `-16 -16 -20` to its crouch max. These are Quake units and
  must be rescaled to C35's meters during the port — the SHAPE
  (a 32-wide, 56-tall capsule-ish box, eye near the top) transfers,
  the numbers do not transfer raw.
- **Hook-aware movement.** `player.qc` disables crouch while a hook
  is attached (`have_hook` forces `do_crouch = false`). Small rule,
  worth porting verbatim in spirit: traversal states override
  posture states, in the controller, not in input handling.
- **What does not transfer:** the BSP collision world (trace
  against brush leaves) — C35's world is procedural geometry, so
  the capsule-vs-mesh solver is C35's own; only the velocity
  model ports. The QuakeWorld netcode does not transfer at all
  (solo by design).

### 1.2 The grapple — see §5, the fast-win

### 1.3 Dynamic lights and shadows — TAKE THE PASS MODEL

- **Per-pixel lighting passes.** DarkPlaces shades in passes:
  static lighting (lightmap / lightgrid / vertex), then one pass
  per dynamic light, then fog (optional photon mapping). Lights are
  omnidirectional with linear falloff; spotlights are simulated
  with cubemap filters. For C35's lighting upgrade, the take is the
  PASS DISCIPLINE: every practical lamp in Greyport becomes a pass
  against the same field the perceivers read — which is exactly
  C35's one-field law with a proven implementation shape behind it.
- **Two shadow techniques, both real:** stencil shadow volumes
  (no penumbra, high fill-rate cost — the technique Doom 3 also
  used) and shadow mapping, with `r_shadows_throwdistance` bounding
  the shadowed area. C35 already has 2048² shadow maps; the take is
  the BOUNDING discipline (shadowed area is a budgeted region, not
  the whole world) and the per-light pass accounting in the ledger.
- **Lightgrid sampling override.** Client-side QuakeC may override
  lightgrid sampling (diffuse, ambient, light vector) per model —
  the mechanism by which a model can strobe or sample light from a
  different position. C35's visor sense-view is the same idea from
  the other side (reading the field, not overriding it); the
  mechanism to study is how sampling is decoupled from rendering.
- **`.rtlights` files.** Authored realtime-light definitions that
  improve performance and appearance. C35's version is procedural
  (lamps generated with the block), not authored files — take the
  DATA SHAPE (position, radius, color, falloff, shadow flag), not
  the file workflow.

### 1.4 Particles — the weather engine's crude board

- **Data-driven particles.** DarkPlaces particles are configurable
  through `effectinfo.txt`, reloadable at runtime
  (`cl_particles_reloadeffects`). Rain, splash, runoff, smoke,
  sparks all become EFFECT DEFINITIONS (emitter shape, rate, size,
  velocity, life, gravity, texture) rather than code. C35's weather
  engine should adopt exactly this: a particle-effect table the
  weather state drives, with rain as one entry. The wetness value
  the traversal course already consumes through the shared surface
  fields is C35's own addition — DarkPlaces has no such field.
- **What does not transfer:** the particle TEXTURES (authored
  images) — C35's particles must be procedural points/sprites from
  the painters, under the zero-image-asset rule.

### 1.5 Sound — TAKE THE MODEL, NOT THE CODE

- **3D positional audio + music playback, multiple formats.**
  DarkPlaces' audio is the reference shape for C35's PLAYBACK layer
  only: positional mixing, attenuation by distance, occlusion
  hooks. The STIMULUS layer (sound as simulation data every
  perceiver reads) is C35's own invention and has no DarkPlaces
  equivalent — do not look for it there.
- **What does not transfer:** the C audio backends themselves
  (OpenAL/SDL glue) — C35 needs native output + WebAudio on the
  web host, which is a different backend pair. See SoLoud (§2) for
  the playback-layer alternative.

### 1.6 Skeletal animation — TAKE THE SKINNING TRICK

- **Matrix Palette Skinning.** DarkPlaces converts weight-based
  skeletal formats (MD2/MD3/MD4/MD5/IQM — it loads MD3 and IQM
  natively) to Matrix Palette Skinning at load: the base mesh is
  rebuilt from weights and basepose, and each weight stores only
  influence + bone index, discarding relative position/normal data
  — faster and smaller than weight-relative skinning. C35's
  animator (PoseFrame seam, procedural gaits) should adopt this
  conversion for clip playback: authored clips arrive as
  bone-weight data, the runtime skins by matrix palette.
- **What does not transfer:** the model FORMATS as pipelines
  (MD3/IQM loaders are C code against authored files) — C35's
  characters are procedural, so the take is the skinning math and
  the clip data shape, not a loader.

### 1.7 QuakeC as the original mod-scripting model — TAKE THE LESSON

- **Three contexts, one language.** DarkPlaces runs QuakeC in
  SSQC (server/game logic), CSQC (client/presentation), and MenuQC
  (menus) — a VM with hundreds of builtins exposing engine features
  (skeletal animation, particles, networking) to modders. The
  LESSON for C35's mod engine (already spec'd data-only → WASM):
  the context split is right (game logic / presentation / menus
  are separate sandboxes), but a QuakeC VM is the WRONG runtime
  for C35 — it would be a second language runtime beside the
  spec'd WASM stage, and QuakeC is GPL (combinable, §3, but
  architecturally rejected). Take the context split; do not take
  the VM.

## 2. Three more engines, one per gap

### 2.1 id Tech 4 (Doom 3) — unified lighting, the one-field law's cousin

**Justification in one line:** id Tech 4's unified lighting — every
light affects every surface, no lightmaps — is the shipped,
proven cousin of C35's one-field law (one practical-light field
serves renderer AND every perceiver).

- **What to take:** the unified-lighting architecture (all lights
  dynamic, all surfaces lit by the same pass structure); the MD5
  skeletal format as the clip-playback format reference (cleaner
  than MD3: separate mesh + animation files, named bones); the
  stencil-shadow discipline (and the warning: the GPL release
  REMOVED Carmack's Reverse for patent reasons — C35 must not
  reintroduce a z-fail stencil path from memory; use shadow maps,
  which C35 already has, or a clean-room depth-fail design).
- **What does not transfer:** the C++ scene graph and entity
  system (C35's ECS/job core replaces it); the MegaTexture pipeline
  (needs authored textures — fights zero image assets); GUI
  rendered onto model surfaces (needs authored UI art — C35's
  readable papers/signage are procedural text, a different system).
- **License:** GPL-3.0-or-later — ADAPT/PORT permitted (§3).

### 2.2 Bevy (Rust) — ECS / job-core patterns, NOT adoption

**Justification in one line:** Bevy is the Rust-native ECS
reference, and C35's ECS/job core is already working — so Bevy is
a PATTERN reference for the working system, never a replacement
for it.

- **What to take (patterns only):** archetype storage (entities
  with the same component set stored together, which is what makes
  queries fast); schedule/systems with explicit ordering and
  parallel execution (C35's job pool already does this — compare
  shapes, keep the better one); change detection (only systems
  whose inputs changed run — a proven optimization for the
  perception and stimulus passes); query filters as data.
- **What does NOT transfer / what not to do:** do NOT adopt Bevy.
  Adopting Bevy means replacing C35's renderer, ECS, and app
  scaffolding with Bevy's — that is a rewrite of the engine, not
  acceleration, and Bevy's wgpu assumptions would fight C35's
  PS1.5 presentation and governor law. The audit's recommendation
  is explicit: read Bevy's ECS, port nothing wholesale, steal
  shapes where C35's core is weaker.
- **License:** MIT OR Apache-2.0 (dual, at the licensee's option)
  — PERMISSIVE, dependency or vendored code legally open (§3),
  with per-crate exceptions noted in Bevy's CREDITS.md.

### 2.3 RecastNavigation — navigation, the adopted design made concrete

**Justification in one line:** SUBSYSTEMS already adopts
Recast-design for navigation, and RecastNavigation IS that design
(voxelize → watershed → contour → polygon mesh, plus Detour
pathfinding) under a permissive license.

- **What to take:** the bake pipeline shape (voxelize the block's
  real geometry, watershed-partition walkable areas, extract
  contours, build the polygon mesh); Detour's A* over polygons
  with string-pulling (path smoothing); Detour's OFF-MESH
  CONNECTIONS, which map directly onto traversal's typed links —
  grapple-swing links, wall-run links, mantle links, drop links
  are off-mesh connections with a verb type and a price, exactly
  as the traversal brief specifies; DetourCrowd's separation and
  avoidance as the crowd-layer shape for citizens later.
- **Port cost and form:** the pipeline is BAKE-TIME (run at
  district build, not per frame), which fits C35's procedural
  generation perfectly — the bake consumes the same geometry the
  renderer and physics read. Legally the C++ code may be vendored
  (zlib), but the recommendation is a RUST PORT of the pipeline
  shape (vendoring C++ bends zero-dependency; a port keeps the
  tree Rust-only and deterministic on the fixed step). Either form
  needs Chase's ruling only if vendoring is chosen; the port needs
  no ruling.
- **License:** zlib — PERMISSIVE (§3).

### 2.4 Also verified (bonus rows, same table)

- **The Dark Mod (id Tech 4 mod, first-person Thief-like stealth).**
  Justification: it is the only shipped open stealth game with a
  stimulus/response AI model and a player-visibility meter, and
  SUBSYSTEMS already design-adopts it twice. Take: the
  stimulus/response architecture (stimuli with radius and falloff,
  AI senses, alert states, search behavior) for the AI/perception
  engine, and the LIGHT GEM (player visibility sampled from the
  light at the player) as the design cousin of C35's light read.
  Code is GPLv3 — ADAPT/PORT permitted. Assets are CC BY-NC-SA —
  DO NOT REUSE (the non-commercial clause bars the game's
  distribution terms; also, C35's art is procedural, not TDM's).
- **SoLoud (C/C++ audio engine).** Justification: the permissive
  playback-layer reference for C35's sound engine (the stimulus
  layer stays C35's own). Take: the mixer architecture (voices,
  buses, filters, 3D positional sources, streams) as the shape of
  the playback layer. Core is zlib/libpng — PERMISSIVE — but some
  modules (MP3/OGG support) carry other permissive licenses, so a
  module-by-module check rides any vendoring decision, and
  binding C++ bends zero-dependency exactly as Recast does.

## 3. LICENSE TABLE — the most important part

Verification method: each row was checked against the project's own
license statement (repo LICENSE file, file header, or project
license page) during this audit, 2026-10-30. "Verified" means the
auditor read the statement; it does not mean a lawyer reviewed it.
Per-file headers govern over repo headlines — run the header census
(§0, caveat 1) before adapting anything.

| Piece | License (verified) | What it permits | Consequence for C35 |
|---|---|---|---|
| nightwarelabz/C35 | AGPLv3 | The house license; copyleft, network-use source offer (§13) | The tree's license. Adapted pieces land here. |
| nightwarelabz/DeBrief | AGPLv3 | Same as C35 (identical LICENSE blob) | The game's license. Same consequence. |
| DarkPlaces engine | GPL-2.0-or-later — verified: `darkplaces.h` header, xonotic/darkplaces @ `d93f9c42`: "either version 2 … or (at your option) any later version" | Copy, modify, convey under GPLv2 or any later version, at the licensee's option | **ADAPT/PORT PERMITTED.** Choose v3 terms, combine under AGPLv3 §13. C→Rust port work remains; the legal wall is down. Header census first. |
| Xonotic game code (qcsrc: `server/hook.qc`, `common/physics/player.qc`, `common/weapons/weapon/hook.qc`) | GPLv3+ — verified: xonotic.org states the game is "free to play and modify under the copyleft GPLv3+ license" | Copy, modify, convey under GPLv3 or later | **ADAPT/PORT PERMITTED.** AGPLv3 §13 expressly permits combining with GPLv3 works. QuakeC→Rust port work remains. |
| id Tech 4 (Doom 3 / BFG source) | GPL-3.0-or-later — verified: DoomWiki + id Tech 4 license lines | Copy, modify, convey under GPLv3 or later | **ADAPT/PORT PERMITTED.** Same §13 route as Xonotic. |
| The Dark Mod — code | GPLv3 (some parts revised BSD) — verified: project license line | Copy, modify, convey under GPLv3 | **ADAPT/PORT PERMITTED** for code. |
| The Dark Mod — assets | CC BY-NC-SA — verified: same project license line | Non-commercial sharing with attribution and share-alike only | **DO NOT REUSE.** The NC clause bars the game's distribution; C35 art is procedural anyway. |
| Bevy | MIT OR Apache-2.0 (dual, licensee's option) — verified: Bevy README license section | Use, modify, convey, including in closed works, with notices | **PERMISSIVE — dependency or vendored code legally open.** Recommendation is patterns-only (§2.2), not adoption; per-crate exceptions in CREDITS.md. |
| RecastNavigation (Recast + Detour) | zlib — verified: recastnav.com: "licensed under the ZLib license. See License.txt" | Use, modify, convey, including in closed works, with notices | **PERMISSIVE — vendor or port legally open.** Recommendation is a Rust port (§2.3); vendoring needs Chase's ruling under zero-dependency. |
| SoLoud | zlib/libpng (core); some modules under other permissive licenses — verified: SoLoud manual/legal pages | Use, modify, convey, including commercially, with notices | **PERMISSIVE with a module check.** Core vendoring legally open; check each module's license before vendoring; binding C++ needs Chase's ruling under zero-dependency. |

**What changed vs the pre-correction draft:** every GPL row above
read "reimplement the ideas only" before Chase's AGPL correction.
That is now WRONG for every row except the (empty) GPL-2.0-only
case: DarkPlaces, Xonotic, id Tech 4, and The Dark Mod code are all
ADAPT/PORT. The only DO-NOT-USE row is The Dark Mod's NC assets,
and the only REIMPLEMENT-ONLY trigger left is a future
GPL-2.0-only header found by the census.

## 4. Per-subsystem take list (SEEDED + ABSENT)

Port cost is honest, not optimistic: SMALL = a week-scale port of a
shape or table; MEDIUM = a milestone-scale port with course proof;
LARGE = a multi-milestone rework. "Adapt" means port the design
into Rust inside the AGPLv3 tree (legally permitted per §3);
"Vendor" means the code may be used directly (permissive rows
only) and still needs Chase's ruling under zero-dependency.

SEEDED:

- **Animator engine** — DarkPlaces matrix-palette skinning (§1.6)
  + id Tech 4 MD5 clip shape. Take: skinning conversion + clip
  data shape. Cost: MEDIUM. (Clip playback beside procedural
  posing; the PoseFrame seam stays.)
- **Lighting engine** — id Tech 4 unified lighting (§2.1) +
  DarkPlaces per-light passes (§1.3). Take: unified pass
  structure + shadowed-area budgeting. Cost: LARGE (this is the
  lighting milestone itself, not a shortcut past it).
- **Materials / painter engine** — DarkPlaces per-light data
  model (gloss map, deluxemap concept, attenuation shapes).
  Take: the DATA SHAPE for what a material carries per light,
  not the texture files. Cost: SMALL-MEDIUM.
- **Camera engine** — No strong reference. DarkPlaces view-bob
  and roll as TUNING reference only. Cost: SMALL. (The bible's
  camera law — free low shoulder outdoors, composed placed
  angles indoors — is bespoke; no engine ports it.)
- **AI / perception engine** — The Dark Mod stimulus/response
  (§2.4) + light-gem visibility sampling. Take: stimulus
  architecture (radius, falloff, senses, alert states, search).
  Cost: MEDIUM. (The one-field law stays C35's own; TDM supplies
  the response machinery, not the field.)
- **Navigation engine** — RecastNavigation (§2.3). Take: bake
  pipeline + Detour A* + off-mesh connections = typed links.
  Cost: MEDIUM (bake pipeline) + SMALL (link concept). Form:
  Rust port recommended; vendoring legally open, ruling needed.
- **Asset pipeline** — DarkPlaces PK3/ZIP packaging shape +
  IQM/MD5 format references (design only). Take: packaging and
  format SHAPES for the factory's validators. Cost: SMALL.
  (meshoptimizer already planned; nothing here replaces it.)
- **Input engine** — Xonotic/DarkPlaces binding layer. Take: the
  bind table shape (action → keys, remappable) and one verified
  fact: **E is Xonotic's default hook key** — Chase's E binding
  is literally the Nexuiz lineage's own default. Cost: SMALL.

ABSENT:

- **Physics engine** — DarkPlaces/Xonotic movement model (§1.1)
  + grapple (§5). Take: wishdir/wishspeed/accelerate/friction,
  data-driven physics sets, hook-aware posture override. Cost:
  MEDIUM. (Capsule-vs-mesh solver stays custom; Rapier stays
  PARKED — this audit does not recommend un-parking it, and no
  physics reference in it changes that: Quake movement is a
  controller model, not a general solver, which is exactly what
  C35's narrow physics slice needs.)
- **Sound engine** — SoLoud mixer architecture (§2.4) for the
  PLAYBACK layer + DarkPlaces positional model (§1.5). Take:
  voice/bus/filter/3D-source shape. Cost: MEDIUM. (The stimulus
  layer is C35's own; nothing here builds it.)
- **Weather & atmosphere engine** — DarkPlaces data-driven
  particles (§1.4) + height-fog model. Take: effect-definition
  tables (rain as one entry) + fog fading with height. Cost:
  MEDIUM. (Wetness-as-field and sense repricing are C35's own.)
- **UI / text engine** — No strong reference. id Tech 4
  GUI-on-surfaces is REJECTED as a take (needs authored art).
  Take only the CONCEPT of world-space readable surfaces.
  Cost: SMALL (concept) — the glyph atlas + layout are C35's
  own build.
- **World persistence engine** — No engine reference. Serialization,
  save format, versioning, and integrity are generic engineering,
  not an engine take. Cost: n/a — C35's own build, no take.
- **Op engine (mission logic + grading)** — The Dark Mod
  mission/trigger logic as DESIGN reference. Take: trigger and
  condition shapes for op generation. Cost: SMALL-MEDIUM.
  (Grading — Ghost / Surgeon / Butcher — is DeBrief's own.)
- **Dialogue engine** — The Dark Mod conversation system as
  DESIGN reference. Take: branching-exchange state shape.
  Cost: SMALL. (Handler/Control voices and checkpoint
  interrogations are DeBrief's own content.)

## 5. The grapple fast-win — match Nexuiz from the source, not memory

The single most valuable extraction in this audit. Xonotic's
grapple is Nexuiz's direct descendant (Xonotic forked from
Nexuiz), and its behavior lives in the GAME code (GPLv3+,
adaptable per §3), not the engine:

- **Where it lives (verified against the Xonotic qcsrc doxygen):**
  - `server/hook.qc` — the hook entity and its per-think pull:
    `FireGrapplingHook` (line 311), `GrapplingHookThink` (line
    103), `GrapplingHookTouch` (line 273), `GrapplingHook_Stop`
    (line 66), `RemoveHook`, `GrapplingHookReset`. State machine:
    `HOOK_FIRING`, `HOOK_PULLING`, `HOOK_WAITING_FOR_RELEASE`,
    `HOOK_REMOVING` (weapon-side constants in `hook.qc`).
  - `common/weapons/weapon/hook.qc` — the weapon side: primary
    fire spawns the hook (gated by `hook_refire`), latched
    state is `hook.state == 1`, `hooked_time_max` caps attach
    time, `hooked_ammo` burns fuel while attached, and the
    client-side `Draw_GrapplingHook` renders the rope as a
    `Draw_CylindricLine` (a textured cylinder, not a verlet
    chain — Xonotic's rope is a drawn line; DeBrief's rope is a
    verlet chain, and the two must not be confused: take the
    BEHAVIOR, keep DeBrief's rope).
  - `common/physics/player.qc` — hook-aware movement: crouch
    disabled while attached (`have_hook` forces
    `do_crouch = false`).
  - `server/weapons/common.qc` — the sky-grapple guard: a hook
    striking sky is detected and prevented ("Detected and
    prevented the sky-grapple bug"), a failure mode DeBrief's
    strike resolution must also handle (a strike with no world
    behind it must fail visibly, not latch the sky).
- **The two modes (the key discovery for DeBrief).** Xonotic's
  hook has a mode switch, `g_grappling_hook_tarzan`:
  - `tarzan = 0` — PULL mode: the chain reels the player to the
    anchor. In `GrapplingHookThink`, pull speed RAMPS with
    distance: `dist < 200 → spd = dist * (pullspeed / 200)`,
    else `spd = pullspeed`, and `spd < 50 → 0` (dead zone at
    arrival so the player does not orbit the anchor).
  - `tarzan ≥ 1` — SWING mode: the rope is a rubber band with
    stretch. If the rope is overstretched
    (`newlength < dist - ropestretch`), the rope shortens to
    `dist - ropestretch` and, when velocity opposes the rope
    direction, adds `frametime * dir * rubberforce_overstretch`
    — the restoring force that makes the swing. Air friction
    applies every think: `v *= 1 - frametime * ropeairfriction`.
  - DeBrief's brief specifies a verlet pendulum swing — that is
    the TARZAN branch's physics, not the pull branch's. The
    traversal build should port the tarzan restoring-force
    SHAPE (stretch clamp + opposing-velocity rubber force + air
    friction) onto DeBrief's verlet rope, and keep the pull
    branch's arrival dead zone for the climb-to-anchor case.
- **The parameters that define the feel (verified cvar names
  and published values — rescale Quake units to meters on port,
  keep the RELATIONSHIPS):**
  - `g_balance_grapplehook_speed_fly 5000` — hook projectile
    speed (the chain's flight).
  - `g_balance_grapplehook_speed_pull 1000` — pull-mode cap
    speed, with the linear ramp below 200 units above.
  - `g_balance_grapplehook_force_rubber 2000` and
    `force_rubber_overstretch` — the swing restoring forces.
  - `g_balance_grapplehook_airfriction 0.2` — per-think velocity
    retention loss on the rope.
  - `g_balance_grapplehook_stretch 1`,
    `g_balance_grapplehook_length_min 1` — rope stretch and
    minimum length.
  - `g_balance_grapplehook_health`, `damagedbycontents`,
    `gravity` — whether the hook entity itself can be damaged,
    is damaged by contents, or arcs under gravity.
  - Fuel/time caps (weapon side): `hooked_time_max`,
    `hooked_ammo`, `hook_refire` — DeBrief's design prices the
    strike's noise and exposure instead of fuel, so these caps
    are the SHAPE (a cost on holding), not the numbers.
- **The keybinding, verified:** Xonotic's wiki documents "the
  `hook` key (default: `E`)" as the shortcut for primary fire.
  Chase's E binding for DeBrief's grapple is the lineage's own
  default — the traversal build binds E with lineage behind it,
  not just preference.
- **What "match Nexuiz" means after this audit:** port the
  tarzan-branch restoring force + the pull-branch arrival ramp +
  the state machine + the E binding, onto DeBrief's verlet rope
  and capsule, rescaled to meters, proven by the traversal
  course. It does NOT mean copying Xonotic's reel-in pull as
  DeBrief's only mode — DeBrief's design (swing, drape, climb,
  #38's swing exit into a wall-run) is richer than Xonotic's
  hook, and the audit's recommendation is to MATCH the feel
  parameters while KEEPING DeBrief's rope model.

## 6. What NOT to take

- **BSP map pipelines (DarkPlaces, id Tech 4).** C35's world is
  procedural; there is no `.bsp`, no `.map` compile chain, no
  leaf/portal visibility to import — importing one would fight
  the generator at every step.
- **QuakeC VMs as a mod runtime (DarkPlaces).** C35's mod engine
  is already spec'd data-only → WASM; a QuakeC VM is a second
  language runtime solving a decided problem.
- **Network stacks (QuakeWorld protocol, master servers,
  DarkPlaces netcode).** Solo by design; the board excludes
  networking, and only Chase puts it back.
- **id Tech 4 MegaTexture.** Needs authored textures; fights
  zero image assets outright.
- **id Tech 4 GUI-on-surfaces.** Needs authored UI art; C35's
  readable papers and signage are procedural text.
- **DarkPlaces `.rtlights` authoring workflow.** Needs authored
  light files; C35's lamps are procedural with the block.
- **Any C/C++ scene graph (id Tech 4 entities, DarkPlaces
  server entities as a framework).** C35's ECS/job core already
  owns this; importing one is a rewrite, not a take.
- **The Dark Mod's assets (CC BY-NC-SA).** Non-commercial clause;
  also the wrong art (authored, not procedural). Code yes,
  assets never.
- **Bevy as an engine (not as patterns).** Adopting Bevy replaces
  C35's renderer, ECS, and scaffolding — a rewrite, not
  acceleration (§2.2).

## 7. Recommended order of extraction

1. **Grapple first (§5).** The traversal build's highest-value
   reference, and the only one with a deadline (the milestone
   is next in the engine phase). Header census on the four
   qcsrc files, then port the tarzan shape + pull ramp + state
   machine.
2. **Recast bake pipeline (§2.3).** The navigation milestone's
   crude board — port the pipeline shape in Rust against the
   block's real geometry.
3. **Movement model (§1.1).** Wishdir/wishspeed + physics sets
   for the capsule controller's feel pass.
4. **Stimulus/response (§2.4).** When the AI/perception engine
   starts — TDM's machinery, C35's field.
5. **Unified-lighting pass structure (§2.1) + particle tables
   (§1.4).** When the lighting and weather milestones start.
6. **Everything else on demand.** Bevy patterns, SoLoud mixer
   shape, MD5 clip shape — read when their subsystem starts,
   not before; this audit is the map, not the build.
