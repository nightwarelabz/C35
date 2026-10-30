# C35 — the three subsystem engines

Owner direction, 2026-10-29 (Chase): "You will probably also need to build
an animator engine coupled with C35 and a physics engine and a sound
engine." Correct — assessed against what exists, scoped below. All three
live in the one Rust tree, native + browser in lockstep, zero-dependency
doctrine as ever: design-adopt from the field, build narrow for the games
we actually ship.

## 1. Animator — SEEDED, not built

What exists: M7's `locomotion.rs` — a pose-graph animator + movement
controller on the PoseFrame seam (nodes / layers / masks), procedural
gaits proven at 240 Hz (walk 1.50, run 3.40, sprint 5.80 m/s), the soldier's
pose vocabulary rigged as data in Slice 1.

What the engine still owes:
- The pose graph formalized as an engine-level machine (already scoped
  inside THE REBUILD) — states, transitions, blend rules as data.
- Clip playback beside procedural posing: authored/keyframed clips on the
  same skeleton seam, blended with the procedural layer.
- **Animation events as simulation output** — footstep, cloth, gear-rattle
  moments emitted by the animator are stimuli (see §3): the animator is a
  sound source with perfect knowledge of when feet land.
- Soldier vocabulary animated, not posed: HALT, aim, reload, the search
  sweep — transitions carry the perception law (his state read from
  posture alone must survive motion).

Sequence: formalization rides THE REBUILD (already its scope); clips +
events land with the first milestone that needs a man doing something the
gaits can't say.

## 2. Physics — DOES NOT EXIST

What exists: `height_at` terrain following and scripted drives. No
collision, no gravity, no bodies. M7's camera debt (no terrain collision)
and the step-ring workaround are this absence showing.

Scope — game-shaped, deliberately not a general solver:
- Capsule-vs-static-world collision for characters (the block, rooftops,
  interiors) with slope/step rules the movement controller already assumes.
- Gravity and falling, priced honestly (falls are a gib-threshold input in
  the death doctrine — the physics engine reports impact, the doctrine
  decides what it means).
- **The rope**: a distance-constraint pendulum for the grapple — line
  length, swing momentum, anchor release. This is the traversal
  milestone's heart; physics slice 1 exists to serve it.
- Static-to-kinematic props later (crates, doors); the mecha is a
  locomotion problem of its own when it comes.
- Rapier stays PARKED (zero-dependency rule, engine research 2026-10-29):
  design-adopt its solver shape, write the narrow thing.

Determinism: physics ticks inside the sim's fixed step, ledger-provable
like the gaits — same inputs, same swing, byte for byte.

Sequence: slice 1 lands WITH the traversal/grapple milestone (collision +
rope); nothing in Slice 1 or the Rebuild blocks on it.

## 3. Sound — DOES NOT EXIST, and is a simulation system

What exists: nothing. Every host is silent.

The DeBrief design makes sound load-bearing in a way playback alone cannot
serve: perception prices audio (surface × speed), the mecha's emissions
are data the visor reads, a wounded man calling out is a stimulus. So the
sound engine is two coupled halves:

- **The stimulus layer (simulation).** Sound events with position, radius,
  surface and decay, written into the same fields the perceivers read —
  ONE FIELD law extended to hearing: if it sounds loud, it is loud, to
  everyone. Footsteps come from the animator's events (§1); gunfire,
  hook-strikes, servo whine, rain on tin — all events first, audio second.
- **The playback layer (presentation).** Positional mixing, occlusion by
  the same geometry the light field uses, native output + WebAudio on the
  web host. Plus the music: the Coupling's adaptive stem architecture
  (stealth / suspicion / combat / conversation), original score composed
  against the reference playlist — the playlist itself is never shippable.

Proof surface: the windowed DeBrief host — sound is proven by standing in
the street and hearing the patrol before you see him.

Sequence: its own milestone after the windowed host lands (the window is
its proof stage); the stimulus layer's shape is fixed NOW so the animator's
event track and the perception work don't have to be reopened for it.

## The coupling in one line

Animator emits events → physics gives them bodies and surfaces → sound
carries them to every ear in the field → perception reads the field. Four
systems, one ledger. That is why these are engines of C35 and not
libraries bolted to a game.

## The full board — every other engine, censused

Ordered 2026-10-29 (Chase: "Take note of all other engines we will need").
Twenty entries total on the board. The three above are counted in their
groups; statuses are as of Slice 1 building.

### Working (4)
- **Render engine** — the core: forward renderer, sun shadows, temporal
  reconstruction, PS1.5 palette grade + dither, the quality governor.
- **Reality layer** — solar model, hemisphere seasons, the player's real
  clock and latitude; no time levers in shipped hosts.
- **ECS / job core** — entities, systems, the parallel job pool (M3).
- **Proof & telemetry tooling** — cost ledgers, proof harnesses, movelogs,
  the light-read CSVs. House tooling, not shipped — but it is how every
  other engine on this board gets believed.

### Seeded (8)
- **Animator engine** — §1 above.
- **Lighting engine** — the sun + the block's practicals already feed one
  field the renderer and the light read share; the FULL unified practical
  lighting (Dark Mod design-adopt: one field renders the operator AND
  decides whether he is seen) lands in the Rebuild.
- **Materials / painter engine** — the procedural painters and atlas
  generators (operator atlas, terrain surfaces). The golden standards'
  surface craft becomes a system here, not per-model heroics; the Rebuild's
  surface pass grows it.
- **Camera engine** — follow/track rigs and scripted drive cameras exist;
  the bible's camera law (free low shoulder in open ground, COMPOSED
  placed angles in interiors) needs the real system: shots, blends,
  interior placed-angle sets.
- **AI / perception engine** — the design is written (one-field law,
  stimulus/response, states carried in posture) and the light read is its
  first working sense. The soldier's brain, the mecha's fixed doctrine
  tables, search and memory: owed. Dark Mod stimulus/response is the
  adopted shape.
- **Navigation engine** — Recast-design adopted (navmesh + off-mesh
  links); Slice 1's patrol walks a scripted loop and the grapple anchor
  survey is recorded. The mesh, the links, and the crowd layer land with
  traversal.
- **Asset pipeline** — zero-image-asset procedural generation, validators,
  census tooling; meshoptimizer enters with the Rebuild. This is the
  factory the golden-standard models get built in.
- **Input engine** — the windowed host's rig works (and Chase's hands know
  it); a real binding layer — remappable, the game's verbs (aim, fire the
  hook, HALT answered, drag a body) — is owed when play begins.

### Absent (7)
- **Physics engine** — §2 above.
- **Sound engine** — §3 above.
- **Weather & atmosphere engine** — the reality layer owns time and
  season; Greyport's RAIN does not exist yet: rain state, its FX (sheet
  rain, splash, runoff brightening the streets), and its simulation price
  (rain degrades the soldier's senses, reprices every footstep, feeds the
  sound field). Fog exists as grade, not yet as weather.
- **UI / text engine** — no in-engine text rendering exists (ledgers live
  in title bars and terminals). Owed: glyph atlas + layout, the debrief
  screens, menus, documents and papers as readable objects, signage in the
  world, and the visor's sense-view as a presentation mode of this engine.
- **World persistence engine** — Greyport must REMEMBER: bodies where
  they fell, mecha damage welded over, the handler's kit one stage
  further built, a dead agent's file read by his successor. City state,
  the service record, career inheritance — serialization, save format,
  versioning, integrity. The Archive is literally a file; this engine
  is its keeper.
- **Op engine (mission logic + grading)** — the replayability core:
  faction doctrine generates ops; triggers and conditions run them; the
  event record (every trace, every crossing, every body found) feeds the
  debrief grading — Ghost / Surgeon / Butcher — and writes the next line
  of the city's memory. Rides on persistence + AI.
- **Dialogue engine** — debrief conversations with the handler, checkpoint
  interrogations, Control's protocol voice. Branching exchanges whose
  state the op engine reads. Rides on UI/text + op engine.

### Deliberately off the board (1)
- **Networking** — DeBrief is designed as a solo immersive sim; no
  multiplayer is scoped. Not forgotten — excluded, and only the owner
  puts it back.
