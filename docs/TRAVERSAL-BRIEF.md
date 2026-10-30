# Traversal milestone — physics slice 1, the rope, wall-running, navigation

**Directive (Chase, from the founding design, DESIGN.md §4 + the
Traversal section, wall-running adopted 2026-10-29 21:51):**
"EVERYTHING grapples — the constraint is physical, never permission."
"WALL-RUNNING joins it as the second traversal verb... It lands with
the traversal milestone, after the operator — physics slice (capsule,
the rope, wall adhesion) and the animator's wall-run family together,
never one without the other." This brief is that milestone, whole:
physics, rope, wall-run, navmesh, and the animator family land as ONE
piece or not at all.

> **Status as of c158214:** traversal landed (26e5b6a: capsule physics,
> verlet grapple on E, wall-run, mantle, navmesh, poses) in
> `crates/c35/src/traversal/`. `hosts/debrief` is no longer a workspace
> member — the game moved to nightwarelabz/DeBrief (cb62da6). No
> `REPORT.md` from this milestone is in this repo.

**State you start from:** C35 main as the parent names it at dispatch
(operator perfection phase 1 landed; confirm the base hash before
building, as the windowed landing did). What exists to build on:

- `locomotion.rs` (M7): the pose-graph animator + movement controller
  on the PoseFrame seam; gaits proven on the fixed step (walk 1.50,
  run 3.40, sprint 5.80, crouch-walk 1.10 m/s).
- The Slice 1 Greyport block in `hosts/debrief` (procedural, zero
  image assets), the headless drive + windowed hosts, the movelog /
  ledger / light-read proof discipline.
- `slice1/anchors.csv`: the grapple anchor SURVEY — recorded test
  points. It is a fixture for proofs, **never a whitelist**: the law
  is no blessed anchor points, and any code path that consults it to
  permit a hook is a defect by definition.
- Physics today is ABSENT (SUBSYSTEMS.md §2): `height_at` terrain
  following and scripted drives only. Navigation is SEEDED: Recast
  design adopted, the patrol still walks a scripted loop.
- **Known terrain debt (M7 REPORT §1.2, fix-forward):** the height
  function carries a ~0.84 m discontinuity on the step-ring contour
  (|h| = 2.2). M7's movement is robust to it; the ring is NOT fixed.
  Traversal treats that contour as a **seam** (§1.4) — it does not
  get to discover it by falling through it.

## The standard (golden, in `~/workspace/c35-stage/`)

- `debrief-hook-reference.jpg` (#16) + `debrief-hook-reference-alt.jpg`:
  the grapple — a two-handed, drum-fed launcher; three-prong claw on
  a braided line. Hooking commits BOTH hands.
- `debrief-hud-swing-reference.jpg` (#38): the founding transition
  frame — the operator leaving a dockyard wall on the line, firing
  one-handed as he goes. The chain this milestone must be able to
  say: wall-run → swing → wall-run.
- `debrief-greyport-street-reference.jpg` (#9): the block's vertical
  truth — the course is run over THIS geometry's real engine twin.

## What to build

### 1. Physics slice 1 — capsule, gravity, the seam

1.1 **Capsule controller** for the operator (and shape-compatible
with the soldier's capsule later): capsule-vs-static-world against
the block's real geometry — walls, rooftops, interiors — with the
slope/step rules the M7 controller already assumes, so the existing
gaits do not change their contract. It ticks inside the sim's fixed
step on the ECS/job core, substepped as the rope and the seam
require; same inputs, same trajectory, byte for byte (SUBSYSTEMS §2:
ledger-provable like the gaits).

1.2 **Gravity and falling, priced honestly.** Falls report impact
velocity to the caller; what an impact MEANS (the death doctrine's
gib threshold, wound states) is doctrine's decision, not physics'.
Physics reports; it does not judge.

1.3 **Mantle/climb exit**: the fast-stealth doctrine names mantle
(§4) and M7 deliberately left it here. Scope it narrow: a ledge
within reach at capsule top, forward intent, climb to stand. It
exists so swings and wall-runs can END on rooftops, not as a
general climbing system.

1.4 **The seam rule.** The step-ring contour is handled explicitly:
the capsule sweep and the height query must cross it without
tunnelling, popping, or a one-tick penetration spike, and the
traversal course (§6) includes ring crossings on foot, on the wall,
and on the rope. If the honest fix is in the height function itself,
name it in the report as a separate, default-off change — do not
smuggle a terrain change into a physics milestone.

1.5 **Zero new dependencies.** Rapier stays PARKED (zero-dependency
rule; SUBSYSTEMS §2): design-adopt its solver shape, write the
narrow thing. Module placement: a physics module in the `c35` crate
beside `locomotion.rs` (the crew names it and defends the name in
the report); no physics code inside `operator.rs` — the mesh is
posed through PoseFrame, never simulated directly.

### 2. The rope — a verlet grapple constraint

2.1 The hook strike resolves against ANY surface point the launcher
can reach and the world presents — parapets, ledges, cranes, wire
masts, tower rails, sheer faces. There is no surface whitelist, no
anchor-point table, no permission flag anywhere in the path. The
constraints are the physical ones the design names: line length,
the strike's noise (§5), exposure while climbing, momentum on the
swing.

2.2 Model: a verlet rope — a chain of point masses with distance
constraints, the far end pinned at the strike point, the near end
coupled to the capsule. It must support, as one continuous system:
the swing (pendulum on a taut line), the climb (line shortens under
player intent, capsule rises), the drop (line pays out / releases),
and the reel-to-fire transition of #38 (swing exit straight into a
wall-run or a one-handed shot — the pose side is §4; the physics
owes it a clean release with momentum preserved, never zeroed).

2.3 Line behaviour against geometry: the rope drapes and catches on
edges (a line over a parapet shortens its free length at the contact
point — the pendulum pivot MOVES to the edge). A rope that passes
through buildings is a failed milestone, however good the swing
feels. Iteration count and substep rate are tuned constants, named
in the report with the determinism evidence that they are enough.

2.4 Misses and failures are physical too: a strike beyond line
length does not connect; a strike on a surface the claw cannot hold
(another physical property, read from the surface record — never a
per-surface permission bit authored per location) slides or fails
to bite, and the failure is visible and priced, not silent.

### 3. Wall-running — bought with speed, spent against gravity

3.1 **Entry price.** A wall-run is entered from the run/sprint band
of the existing gait table, moving roughly parallel to a wall
surface within the capsule's reach. The exact entry threshold is a
tuned constant: the course (§6) reports the slowest entry that
holds and the fastest that fails, and the constant ships with that
evidence, not as an assertion in this brief.

3.2 **The budget.** While running, gravity is spent against the
run: the arc decays and falls exactly as DESIGN.md prices it —
"the run decays, the arc falls, and the exit is a jump, a grapple,
or the ground." Wall angle prices it (a surface leaning away spends
faster; past a tuned overhang limit there is no run at all), and
approach speed is the deposit the decay draws down. No wall-run
may gain energy: exit speed ≤ entry speed minus the priced decay,
every tick, provable from the course telemetry.

3.3 **Surface wetness.** The run reads the wall's wetness from the
shared surface record — the same per-surface fields the perception
pricing reads footsteps from (SUBSYSTEMS: one field, every reader).
Wet concrete in rain shortens the run and slickens the exit; dry
brick holds. The weather engine itself is ABSENT and is NOT built
here: traversal consumes the wetness value through that field, and
the course exercises dry and wet values through the field itself —
never through a traversal-local override flag.

3.4 **Exits.** Jump (away from the wall, momentum kept), grapple
(the §2 strike fired mid-run — both hands commit, the run ends),
drop (budget exhausted or intent released — the arc falls to the
capsule's normal gravity), and wall transfer where two surfaces
meet at a corner the capsule can round. Every exit is a pose-graph
transition (§4), not a teleport of state.

### 4. Animator — the wall-run family, on the PoseFrame seam

4.1 New pose family in the existing pose graph, beside the M7
gaits, all of it through PoseFrame (joint transforms in, posed
geometry out — a mesh swap must not touch any of this):
- wall-run left-wall and right-wall (the lean INTO the wall, the
  wall-side arm trailing for balance, the coat streaming — the
  family's signature read, DESIGN.md Traversal section);
- the grapple stance: launcher raised two-handed, the strike, the
  hang, the climb, the swing tuck;
- the chain transitions: sprint → wall-run, wall-run → grapple
  swing, swing → wall-run on the far wall (#38 is the reference
  frame), wall-run → jump, any-traversal → fall → land.

4.2 **Both-hands law.** While the launcher is committed — from the
strike until release — the pose graph closes the one-handed weapon
layer: no firing, no aim pose, until the swing exit of #38 (line
released, one hand free, THEN the pistol may speak). The graph
enforces it as state, not as an input suggestion.

4.3 **Animation events are simulation output** (SUBSYSTEMS §1):
every wall-run footfall emits a footstep event with its tick,
position, and surface; the strike emits its event; landings emit
theirs. The animator is a sound source with perfect knowledge of
when feet land — traversal's events feed §5 from the same track
the gaits' footsteps already use, extended, not duplicated.

### 5. Stimulus pricing — traversal is LOUD in every channel

5.1 **Sound.** Wall-run footfalls are footstep stimuli priced as
footsteps on that surface (surface × speed, from the shared fields);
the hook strike is a stimulus at the strike point ("the strike's
noise" is a named physical constraint of the grapple, §4 of the
design). Events are written into the same stimulus fields the
perceivers read — the sound engine's stimulus layer is still
ABSENT as a system; traversal writes EVENTS in its shape and does
not build the playback layer here.

5.2 **Sight.** A wall-runner is a silhouette against sky and
lamplight. Traversal exposes the operator's traversal state
(position, velocity, posture class) to the same light-read path
Slice 1 proved — so a wall-run crossing can be priced for exposure
exactly as the street crossings were (open crossing 100% lit,
shadow crossing 18.1%): the course (§6) reports the wall-run's lit
fraction on the same CSV discipline. The witness economy reads the
same output: citizens look UP at a wall-runner, and the design
calls it the witness economy's easiest sentence. No traversal-local
visibility hack — if the light read says he is lit, he is seen.

5.3 Nothing in traversal may read perception state back. Physics
and the animator price their actions INTO the fields; who heard,
who saw, and what it costs him is perception's and the op engine's
judgement, in their milestones.

### 6. Navigation — the mesh below the verbs

6.1 Recast-design navmesh (SUBSYSTEMS: adopted) generated from the
block's real geometry — Greyport block first, proven there; the
generation pipeline must be district-shaped (the block is the TILE
every district varies — WORLD.md), so the bake takes a district of
blocks without redesign, but only the one block is owed as proof.

6.2 The patrol is the first customer: Slice 1's soldier walks his
loop on the mesh instead of the script, byte-identical route claims
checked against the scripted movelog. If the mesh cannot reproduce
the patrol, the mesh is wrong, not the patrol.

6.3 **Off-mesh links** are where navigation meets traversal, typed
by the verb that crosses them: grapple-swing links (anchor region
→ landing region), wall-run links (along a priced wall face),
climb/mantle links, and drop links (one-way, priced by the fall
report of §1.2). Links are generated from the same geometry and
surface records the physics reads — a link the physics cannot
traverse is a bake error the proof must catch, not a runtime
surprise. Links carry their stimulus/exposure price fields for the
perception layer to read later; navigation computes routes, it
does not decide whether a route is WISE.

## Proof (all of it, in `~/workspace/c35-stage/traversal/`)

- **The course.** A scripted traversal course over the Greyport
  block, driven from a recorded input log in the M7 movelog
  discipline: street sprint → wall-run (dry) → grapple swing across
  the square → wall-run on the far wall → rooftop mantle → drop;
  the same course wet; ring-contour crossings on foot, on the wall,
  and on the rope. Every leg present or the milestone is not done.
- **Determinism:** the full course replayed twice, headless — sim
  outputs (per-tick position / velocity / state) byte-identical
  across both runs, hashes filed. Any nondeterminism is reported
  as a defect with its first divergent tick, never smoothed over.
- **Before/after stills from fixed cameras**: the course's key
  frames (wall-run mid-arc, swing apex, the #38 transition, mantle)
  captured headless, PS1.5 and classic, against the same cameras'
  pre-milestone frames of the empty block.
- **Exposure + stimulus readouts**: the wall-run and swing legs
  priced on the light-read CSV discipline; the emitted stimulus
  events (footfalls, strikes, landings) listed with tick, position,
  surface — proving §5's events exist in the shared fields.
- **Navigation proof**: the bake's statistics (polygons, links by
  type), the patrol running on the mesh with its movelog compared
  to the scripted baseline, and at least one route the street
  cannot make (rooftop-to-rooftop via swing links) produced by the
  router, then verified traversable by the physics course.
- **Cost ledger before/after**, unchanged in shape: draws / tris /
  VRAM, plus the sim-side cost of the physics + rope + bake steps
  stated in the ledger's own terms.
- `REPORT.md`: what was built module by module, every tuned
  constant with the course evidence that set it, and the honest
  shortfalls section — where the swing still clips, where the run
  still reads wrong against #38, where the seam still shows. The
  beauty law applies to motion too: we do not claim it reads at
  the standard before it does.

## Laws

- Land NOTHING: no pushes, branches, or PRs. The parent reviews
  and lands via PR.
- One crew in the tree: you are it — and you start only when the
  parent dispatches you, after the operator phase has landed and
  freed the tree.
- Determinism: pinned runs byte-identical, twice. Physics ticks
  inside the fixed step; no wall-clock anywhere in the sim path.
- The reality layer is untouched: no time levers, no weather
  overrides outside the shared surface fields, nothing that reads
  the player's clock into traversal.
- PS1.5 presentation holds: traversal is judged in the PS1.5
  grade at gameplay distance, not in classic close-ups.
- Physics and the animator's wall-run family land together or
  not at all (the directive's own words). A swing without its
  poses, or poses without the rope, is half a milestone and will
  be sent back.
