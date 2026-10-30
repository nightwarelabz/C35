# ENGINE LOGICS INVESTIGATION — what the full DeBrief game needs, and what to study

Read-only investigation. No tree edits, no repo writes, no code, no
benchmarks, no proof chains. Chase decides what gets built next.

> **Status as of c158214 (2026-10-30):** this snapshot predates the
> modules since landed in `crates/c35/src/`: `modding/` (mod engine v1),
> `traversal/` (capsule physics, grapple, wall-run, mantle, navmesh),
> `perception/` (perception + enemy AI), `sound/` (software mixer and
> synthesized voices) and `weather/` (seeded regime, rain/wind/fog,
> lightning). The "Has" lines and the module list below describe the
> tree when this report was written. The design docs it cites other
> than `ENGINE-REFERENCES.md`, `MOD-ENGINE.md`, `BEAUTY-PASS.md`,
> `TRAVERSAL-BRIEF.md` and `SUBSYSTEMS.md` live in the DeBrief repo's
> `docs/`, not here.

## Standing rule — references only, everything from scratch

Chase, 2026-10-30: open-source engines are REFERENCES ONLY. Every line
of C35 and DeBrief code is written from scratch in-house. This report
proposes LOGICS TO STUDY and reimplement — patterns, algorithms, and
architectures understood from the reference and re-expressed originally
in Rust on wgpu. It never proposes lifting, porting, vendoring, or
adapting reference code, even where an earlier audit found such reuse
legally combinable: that compatibility analysis is legal background and
is NOT exercised. Where a reference's license would forbid even reuse,
this report says so:

- The Dark Mod's ASSETS are CC BY-NC-SA — DO NOT REUSE, ever. Its code
  may be studied; its art, sounds, and maps may not be taken.
- Any file carrying a GPL-2.0-only header (no "or later") is
  study-only. A header census rides any future study that names a file.
- DarkPlaces, Xonotic, id Tech 4, RecastNavigation, SoLoud, and Bevy
  are studied for their logic, not taken for their code. Bevy in
  particular is patterns-only, never adoption: adopting it would
  replace C35's renderer, ECS, and scaffolding with Bevy's.

## Sources read for this report

- `SUBSYSTEMS.md` — the 20-entry board (4 working, 8 seeded, 7 absent,
  1 deliberately off: networking, solo by design).
- `ENGINE-REFERENCES.md` — the reference catalog (DarkPlaces, Xonotic
  grapple, id Tech 4, RecastNavigation, The Dark Mod, SoLoud, Bevy,
  Rapier parked). Its ADAPT/PORT language is superseded by the
  from-scratch rule above; its verified facts (grapple state machine,
  movement model, bake pipeline shape, stimulus architecture) are what
  this report reuses as study pointers.
- `FIRST-PERSON.md` (DeBrief repo) — the game is a first-person FPS stealth sim
  (decided 2026-10-30, superseding the third-person lines).
- `TRAVERSAL-BRIEF.md` — traversal physics/nav (predates FP and the
  E-binding; FIRST-PERSON.md governs where they differ).
- `DESIGN.md` (DeBrief repo; local copy is v0.1, third-person wording — perspective
  lines superseded by FIRST-PERSON.md; systems content stands),
  `WORLD.md`, `STORY-BIBLE.md` (campaign arc PROPOSED, mysteries
  binding), `COMPENDIUM.md` (Archive spec, PROPOSED schemas),
  `MOD-ENGINE.md` (mod spec v1/v2/v3), `BEAUTY-PASS.md` (materials and
  solidity direction).
- Engine state checked against the tree's crate module list
  (`crates/c35/src/`): ecs, governor, locomotion, noise, operator,
  props, scene, shaders, vista, world. There is no physics, sound, AI,
  navigation, UI, persistence, dialogue, or op-engine module in the
  crate today.

## State snapshot — what the full game needs vs what exists

Landed in C35 main: render engine, reality layer (real clock +
latitude, no time levers), ECS/job core, cost ledger, M7 pose-graph
animator + gaits, PS1.5 grade, one-block procedural Greyport, the
windowed host (free-fly debug camera only — NO player control yet),
Slice 1's light read (lit/unlit at a position, logged against a
patrol's facing).

Landed in C35 main (PR #13, merged 2026-10-30): thickened
operator masses, `Atlas::soften()`, city paint softening, FXAA retune,
and a viewmodel + body-awareness rig on the PoseFrame seam. The
DeBrief-side host wiring for the viewmodel is the owed follow-up.

Absent entirely: physics, sound, AI/perception brains, navigation
mesh, weather, UI/text, persistence, op engine, dialogue, mod engine,
save/load, the FP controller, and every game system in §§1–18 below.

The pattern this report keeps finding: the DESIGN already specifies
most systems in unusual depth (perception fields, component damage,
the witness economy, the Archive), and the references supply the
missing ALGORITHMIC shapes — not the design. Study fills gaps in
mechanism, never in intent.

---

## 1. First-person player controller — ground movement, posture, lean

**Need (grounded).** The operator moves at professional tempo: walk,
run, sprint (M7 gaits: 1.50 / 3.40 / 5.80 m/s, crouch-walk 1.10),
crouch, lean around corners and cover, and the fast-stealth doctrine
— sprint, slide, mantle, drop, vanish at full tempo, with speed
itself a stealth system (DESIGN §4). FIRST-PERSON §1.1 fixes the eye
at the operator rig's eye plane, FOV eased (narrow on aim, wide on
sprint, base while hiding), head-bob subtle, positional, and OFF
whenever stillness is the stealth. Lean translates the eye sideways,
never rolls the world, and exposes exactly the eye and no more;
leaning out of a lamp pool into shadow is priced by the light field
at the eye. Lean's key pair is Chase's call and cannot include E.
The both-hands law closes the weapon layer while the launcher or a
mantle commits both hands.

**Has.** M7's pose-graph animator + movement controller on the
PoseFrame seam (gaits proven on the fixed step), but it drives a
third-person figure on scripted drives — no capsule, no collision,
no input bindings, no posture state machine the game can own.

**Gap.** Everything between input and the capsule: a controller
state machine (ground / crouch / lean / sprint / air / traversal),
ground movement with collision, posture transitions that cost time,
and the eye rig (eye height derived from the model, FOV rules,
bob rules, landing dip).

**Study.** DarkPlaces/Xonotic's wishdir/wishspeed model: each tick,
input forms a desired direction and speed, velocity accelerates
toward it, ground friction bleeds speed when input stops, and air
movement keeps the same shape with a different acceleration constant
— velocity is steered, never zeroed, which is why the feel is crisp
and momentum survives. Study also its data-driven physics sets
(`g_physics_<set>_<option>` pattern): movement constants as a named,
swappable set, not literals in code. Study its hook-aware posture
override (crouch forced off while hooked) as the shape for
traversal-states-override-posture. For lean and eye discipline,
study the immersive-sim lineage (Thief, Deus Ex, Dishonored) only
for the FEEL contract — eye translation without roll, lean exposing
minimal silhouette — not for code.

**Shape.** A controller module beside `locomotion.rs` owning a
small state machine (ground, crouch, lean-left/right, sprint, air,
hook-committed, mantle) on the fixed step, driving the existing
pose-graph through PoseFrame; the wishdir/wishspeed velocity model
with C35's own meter constants and a named tuning set; the eye rig
as a pure function of capsule state (eye = capsule top minus the
model's eye offset, FOV eased by state, bob positional and gated
by the stealth states); posture and lean transitions priced in
time and exposed to the stimulus fields, so a lean out of shadow
is a perception event the moment the eye crosses the light field.

## 2. Traversal verbs — grapple rope, wall-run, mantle

**Need (grounded).** EVERYTHING grapples: no whitelists, no blessed
anchor points — the constraints are physical (line length, strike
noise, exposure, momentum). E press fires, E hold keeps the rope,
E release detaches instantly with momentum kept, and the
waiting-for-release guard prevents refire while held through a
detach. The rope is a braided line with parapet drape (the pivot
MOVES to the contact edge), movers are followed, sky never latches,
the line can be severed by enemy fire, and crouch on the rope stops
the reel, not the swing. Wall-running is bought with approach speed
and spent against gravity (dry brick holds, wet concrete shortens
the run and slickens the exit), entered from the run/sprint band,
exited by jump, grapple, drop, or corner transfer. Mantle is narrow:
a ledge within reach at capsule top, forward intent, climb to
stand. The chain wall-run → swing → wall-run (#38) must be one
continuous system, and the swing's energy never grows except by
reel-in paid for with hold time.

**Has.** The traversal brief (physics slice 1, verlet rope, wall-run
budget, navmesh with typed off-mesh links) and the M7 gaits; no
physics module, no rope, no wall adhesion, no mantle.

**Gap.** The whole traversal stack: strike resolution against any
surface point, the rope constraint, wall adhesion and budget,
mantle, and the pose-graph family (wall-run left/right, grapple
stance, hang, climb, swing tuck, chain transitions) with the
both-hands law enforced as graph state.

**Study.** Xonotic's grapple as the behavior reference: the state
machine (FIRING, PULLING, WAITING_FOR_RELEASE, REMOVING), the
tarzan branch (rope shortens at pull speed while a rubber-band
restoring force acts in proportion to stretch past current length,
damped by rope air friction, with a stronger overstretch clamp),
the pull branch's arrival ramp (speed scaled inside 200 units, cut
inside 50 — the dead zone that stops orbiting the anchor), the
sky-grapple guard (surface flagged NOIMPACT removes the hook),
mover-follow, and the E binding itself (Xonotic's own default hook
key). Study the verlet-rope shape from the traversal brief's own
model (chain of point masses, distance constraints, far end pinned,
near end coupled to the capsule) and edge-drape as a pivot-moving
rule, not a rendering trick. For wall-running, study the brief's
budget model directly — entry band, gravity decay, wall-angle and
wetness pricing — since no reference engine ships DeBrief's
priced wall-run; the study input is the budget's energy accounting,
not a borrowed implementation.

**Shape.** A traversal module owning strike resolution (ray/probe
against world geometry, surface record read for bite-ability,
mover attachment, sky rejection), a fixed-step verlet rope with a
named iteration/substep budget and drape contacts, a wall-run state
with an energy budget (entry deposit minus gravity decay minus
surface/wetness price, exit speed provably ≤ entry minus decay),
and a narrow mantle state; all of it emitting animator events
(footfalls, strike, landing) into the shared stimulus fields, and
all of it driving the pose-graph family through PoseFrame so a
swing without its poses — or poses without the rope — never ships
alone.

## 3. Camera — the eye, and motion sickness kept out by construction

**Need (grounded).** FIRST-PERSON §1 and §4.4 are a complete camera
law: eye at the rig's eye plane; FOV eased and capped (aim narrow,
sprint wide, swing follows speed capped at sprint widening,
wall-run holds base); bob subtle, positional, off during stealth
states and all traversal; landing a single eased dip, never a
bounce; no camera snap anywhere (latch, release, wall entry,
transfer, mantle, landing all eased over frames); roll capped and
rare (wall-run lean, capped swing bank only); the horizon sacred
during swings; shake reserved for damage and mecha tread, where it
is information. In interiors and the debrief room, the camera takes
composed, deliberate placed angles (the MGS1 lesson, kept).

**Has.** Follow/track rigs and scripted drive cameras; the windowed
host's free-fly debug camera (WASD/QE fly, F1–F4 placed views).
No game camera, no FOV/bob law, no placed-angle system.

**Gap.** The game camera as a system: eye rig, FOV state machine,
bob/dip/lean composition, swing-bank and wall-lean, placed-angle
sets for interiors and the debrief room, and the comfort fallback
question (a named transition's easing, not a global slider).

**Study.** DarkPlaces view-bob and roll only as TUNING reference
(the catalog is explicit: no engine ports the bible's camera law).
The real study material is the FIRST-PERSON note itself plus the
motion-sickness literature's standard mitigations (horizon
stability, eased transitions, capped roll, no FOV punches) — all
already stated as rules in §4.4. Study how immersive sims compose
camera state (eye + look + lean + bob + event impulses as additive,
eased layers with priority) rather than any one game's camera code.

**Shape.** A camera module that composes the frame from ordered,
eased layers — base eye (capsule-derived), lean translation, bob
(gated by state), FOV (gated by state, eased, capped), swing bank
(capped, proportional to lateral velocity), landing dip (single,
eased), damage/tread shake (informational only) — with a rule that
no layer may snap and no layer may roll the world except the two
named rolls; placed angles as data (position, look target, blend
rule) for interiors and the debrief room, driven by the same
composer.

## 4. Stealth perception — the one-field law and its channels

**Need (grounded).** ONE FIELD LAW: a single practical-light field
serves the renderer AND every perceiver — no separate "AI vision"
numbers; if a screenshot looks safe, it is safe. The soldier prices
light × motion × posture (crouched and still in shadow nearly
invisible; upright and running in a lamp pool seen street-length),
audio (footsteps priced by surface and speed, thrown weight,
gunfire means investigate), and memory (last known position, a
search that decays). States (unaware, suspicious, searching,
alarmed) are carried in posture and sound, never icons. The
soldier is a conscript: he hesitates, can be wrong, degrades in
rain and cold, and fears the mecha. The sense view (visor) renders
the world as signals — emitters bloom, attention by tone and
sound, sparse and analog. The Hound adds SCENT, the third channel
and the only one the operator cannot watch himself spend: his
trail persists, decays over minutes, freshens when crossed, washes
in rain, carries on wind, breaks at water and at the Wanderer's
raked gravel — and the visor never shows the player's own scent
picture (the debrief grades it after the fact). Grace is a field
like light: near the Wanderer, suspicion rises slower, scent
breaks, the Neighbour lowers his notebook, and the visor barely
registers her.

**Has.** Slice 1's light read: lit/unlit at a position, logged
against a patrol's facing. The design is written; no perceiver,
no stimulus fields, no memory, no scent, no sense view.

**Gap.** The field system itself (light field as shared data, not
a render artifact), the stimulus bus (events with position,
radius, surface, decay), the pricing functions (light × motion ×
posture, surface × speed audio, scent decay/wash/wind), last-known
position and search decay, and the sense-view presentation mode.

**Study.** The Dark Mod's stimulus/response architecture as the
machinery reference: stimuli with radius and falloff, AI senses,
alert states, search behavior — and its LIGHT GEM (player
visibility sampled from the light at the player) as the design
cousin of C35's light read. Study the one-field law's own
corollary in id Tech 4's unified lighting (every light affects
every surface through one pass structure) for the pass discipline
that keeps renderer and perceiver on the same field. Study Thief's
sound propagation lineage (room/portal-aware propagation, sound
occluded by the same geometry the light field uses) for the audio
channel's shape. The scent channel has no shipped reference in the
catalog — study it as a decaying ground-trail field (trail
segments with age, surface, weather wash, wind drift) modeled on
the stimulus/response shape, not on any engine's code.

**Shape.** A perception module owning three field layers on the
same spatial substrate the renderer reads — light (sampled at any
position, including the eye), sound (events propagated through
geometry with occlusion and decay), scent (ground trail segments
with age and weather state) — plus a pricing layer (light × motion
× posture, surface × speed, scent age/wind/wash) and a memory layer
(last known position, search with decay, hesitation and error
terms for the conscript); perceivers read only these fields, and
the visor's sense view is a presentation mode of the same fields,
never a second source of truth.

## 5. Enemy AI — soldier, Hound, Weathervane, Neighbour, mechas, alert budget

**Need (grounded).** The soldier: patrol routes, HALT at
checkpoints, graduated failure (question → detention → force),
body discovery (alarm, search, a report the Archive hears), fear
of the mecha (yields out of its arcs), degradation by rain, cold,
and fear. The Hound: scent-tracking shepherd + handler, posture
states (nose down pulling = tracking; head up still = scent held;
hackles, line straining = contact), handler grief priced in the
record. The Weathervane: rooftop observer, scope, field telephone
WIRED to the street — he REPORTS (a telephone call is a district
perception event), rarely shoots, scheduled rotation, poor hearing,
silhouette against the sky as his tell, contesting the vertical
layer the grapple crosses. The Neighbour: plain-clothes informer,
readable only by behaviour (watches, follows at a distance,
writes); his success state is a description entered into the
other archive, which changes checkpoint reads and blocks the
Wanderer. The Type 1: AUTONOMOUS analog governor — cams, relays,
gyroscopes, punched doctrine — fixed, learnable stimulus/response
tables, threshold machinery with fixed timers, no doubt; damage
compounds through the governor (a blinded machine leans on audio
in fixed, readable ways); telegraphs (servo noise, eye sweep,
ground tremor); the grab survivable at a cost. The Type 2: momentum
doctrine — charges in lines, overshoots, pays in street furniture
and recovery seconds, cannot climb or follow a grapple, contact
gib-capable, scream a district-level stimulus, voice apparatus a
silenceable component, nearly quiet between charges, escalation
bought by a risen alert budget, never population, never a standing
patrol. The alert budget itself: rises with witnessed violence,
hard mecha kills, failed crossings; falls with clean ops, quiet
crossings, time; buys harder patrols, then Weathervane coverage,
then a Type 2 event.

**Has.** The soldier's pose vocabulary rigged as data in Slice 1
and a scripted patrol loop; the design's stimulus/response tables
as prose. No brain, no states, no search, no budget.

**Gap.** Every brain: soldier state machine + search + body
discovery + checkpoint behavior; Hound tracking + handler pair;
Weathervane scheduling + reporting; Neighbour behaviour-reading
and description-writing; both mecha governors; the alert budget
as a city-level economy.

**Study.** The Dark Mod's stimulus/response architecture and alert
states again, as the general machinery — but the mecha governor is
best studied as a FIXED TABLE machine (finite-state tables with
thresholds and timers, no learning, no doubt), the opposite of a
modern utility-AI: study table-driven AI from the strategy/roguelike
lineage (fixed doctrine tables, readable compensation behaviors)
rather than any learning system. Study Recast/Detour's crowd
separation and avoidance for the citizen layer the Neighbour hides
in, and MGS1's alert/caution state discipline for the soldier's
graduated states (as design reference, not code). The alert budget
is an economy, not an AI: study it as a resource with income
(witnessed violence, hard kills, failed crossings) and decay (clean
ops, time), gated thresholds buying escalations.

**Shape.** An AI module of small, data-driven brains on the shared
fields: the soldier as a state machine (unaware, suspicious,
searching, alarmed, checkpoint, body-found) with posture/sound
outputs, memory, hesitation, and degradation terms; the Hound as a
scent-field follower with posture states and a handler tether; the
Weathervane as a scheduled sensor with a reporting action wired
into the district's perception bus; the Neighbour as a behaviour
scorer (watch/follow/write thresholds) whose output is a
description record, not a combat state; the mechas as fixed
governor tables (thresholds, timers, compensation rules, damage
compounding) with emission outputs the visor reads; the alert
budget as a city-level ledger with thresholds that buy patrol
density, Weathervane coverage, and Type 2 events — all of it
reading the same fields, all of it deterministic on the fixed step.

## 6. Combat and damage — weapons, component damage, visor readout, sword

**Need (grounded).** The USP is the standard sidearm, carried in
every op; the rack (golden #36) is eight weapons in progression
order, and the sheet reads top-to-bottom as a ladder of escalating
stimulus as much as escalating force (the suppressed SMG is the
only quiet firearm; the single-shot launcher's concussion blinds
and hobbles a mecha but the kill still belongs to the coolant
line). Small arms cannot hurt mecha plate; honesty is the design.
Damage is STATE, not a health bar: the mecha keeps NO health bar,
and the visor's numbers report what YOUR action did — component
struck (SENSOR, LOCOMOTION, CANNON, POWER-COOLANT) and component
integrity — so the kill ladder (avoid, blind/deafen, hobble, kill
through the coolant line from above) becomes measurable and
plannable. Wounds are specific: a shot soldier is down, crawling,
bleeding out, calling out — his sounds are stimuli; finishing him
is a choice the record sees. Gibbing has a threshold: heavy force
only (mecha, explosives, machinery, falls). The sword is the final
unlock: no report, no gunshot stimulus, arm's length reach, and
the coolant-kill's executioner from above. Ballistics have
consequences: one good bullet ends anyone, including the player.
Weapons are priced like everything else: a gunshot is a stimulus
the whole district hears, a slung weapon is a silhouette a
checkpoint reads, the grapple launcher commits both hands.

**Has.** The operator's USP and slung long gun as geometry; the
soldier's suppressed rifle as geometry. No ballistics, no damage
model, no hit reporting, no weapon states.

**Gap.** The whole combat layer: hit resolution, wound states,
component damage states and their visible system effects
(blinded eye → audio fallback with blind wedges; hobbled
locomotion → limp, wider turning, streets it cannot enter;
cannon baitable but not destroyable by the agent's weapons;
coolant exposed on the bare back), the visor's component readout,
weapon noise pricing, reload and swap states, the sword's melee
reach and coolant-line execution, and the player's own wound
state (the visor's body pictogram).

**Study.** The component-damage pattern from the immersive-sim and
mech lineages (damage as named subsystems with state transitions,
never a single pool) — study it as a design pattern, not a code
source: each component has states, each state has visible effects
and perception consequences, and the player's readout is the
component's state, not a number. Study DarkPlaces' weapon/attack
shapes only for the hit-resolution discipline (trace, damage type,
radius) and the noise-pricing idea that a shot is a stimulus
first and damage second. Study the visor readout as a presentation
problem of the perception fields: the numbers grade the player's
own hits because the visor reads the machine's emissions — the
same sense-view mechanism, pointed at damage.

**Shape.** A combat module where every damageable entity carries
named components with state machines (intact, degraded, disabled,
destroyed) and transition effects written into the shared fields
(a blinded sensor changes the governor's tables; a hobbled
locomotion changes its movement budget; a severed coolant line is
the kill); hit resolution as traces with damage types and falloff;
wound states for humans (down, crawling, bleeding out, calling
out) whose sounds are stimuli; weapons as data (noise price,
stimulus radius, hands committed, silhouette cost); the visor
readout as a presentation of component states the player has
caused, never of enemy intent; the sword as a melee weapon with
arm's-length reach, zero report, and a coolant-line execution
condition (from above, by grapple, onto the bare back).

## 7. Animation — viewmodel, body awareness, NPC pose-graph, clips, events

**Need (grounded).** Two rigs on one sim: the third-person
pose-graph STAYS for NPCs, enemies, mechas, and presentation
(debrief room, menu, mirrors, death evidence), and first person
needs a separate viewmodel rig — hands, sleeves, weapon, launcher,
mantle hands — parented to the camera with lag, at viewmodel
texel density, with its own pose set (low-ready, aim, reload per
weapon, launcher raise, strike, hang, swing tuck, wall-run
trailing arm at the frame edge, mantle reach, lean). Body
awareness: looking down shows legs, boots, coat skirt or carrier
and thigh rigs, moving with the gait, never detached, never a
headless-body gap at any pitch. Clip playback beside procedural
posing: authored/keyframed clips on the same skeleton seam,
blended with the procedural layer. Animation events are simulation
output: footstep, cloth, gear-rattle moments emitted by the
animator are stimuli — the animator is a sound source with
perfect knowledge of when feet land. Soldier vocabulary animated,
not posed: HALT, aim, reload, search sweep — transitions carrying
the perception law (state read from posture alone must survive
motion).

**Has.** M7's pose-graph animator + procedural gaits on the
PoseFrame seam (nodes, layers, masks, 240 Hz); the soldier's pose
vocabulary rigged as data; the beauty pass's
viewmodel + body-awareness rig, landed in C35 main (PR #13,
ViewmodelConfig Street/Assault,
ViewmodelPose set, body-awareness parts as a filtered subset of
the real figure, eye at 1.62) with a host placement contract
(spot = camera XZ, yaw, ground = camera height − 1.62) — engine
side done, DeBrief-side host wiring owed.

**Gap.** The pose graph formalized as an engine-level machine
(states, transitions, blend rules as data); clip playback and
blending; the event track as a first-class output; the NPC
animation families (soldier, Hound, Weathervane, Neighbour,
citizens, mechas); the viewmodel rig's host wiring in the DeBrief
repo; the guarantee that viewmodel and third-person figure never
disagree (the sim is single; the presentations are two).

**Study.** DarkPlaces' matrix-palette skinning: weight-based
skeletal data converted at load to a matrix palette (base mesh
rebuilt from weights and basepose, each weight storing influence
+ bone index only) — the skinning math and clip data shape for
clip playback, not a loader (C35's characters are procedural).
Study id Tech 4's MD5 clip shape (separate mesh + animation files,
named bones) as the clip data-shape reference. Study the
PoseFrame seam's own contract (joint transforms in, posed
geometry out) as the invariant the second rig must preserve.

**Shape.** An animator module formalizing the pose graph as data
(states, transitions with blend rules, layers, masks) on the
PoseFrame seam; clip playback as bone-weight data converted to a
matrix palette at load, blended with the procedural layer by the
same rules; an event track emitted per tick (footfalls with
surface, cloth, gear, strike, landing) written into the stimulus
fields; the viewmodel rig as a second pose set on the same seam,
parented to the camera with lag and driven by the same sim state
as the third-person figure, with the host wiring (placement
contract) living in the DeBrief repo; body awareness as a filtered
subset of the real figure's own parts, so it cannot detach.

## 8. Physics — capsule, collision, ragdoll and corpses that persist

**Need (grounded).** Capsule-vs-static-world collision for
characters (block, rooftops, interiors) with slope/step rules the
M7 controller already assumes; gravity and falling, priced
honestly (falls report impact velocity; the death doctrine's gib
threshold decides what it means); static-to-kinematic props later
(crates, doors); the mecha as a locomotion problem of its own.
Corpses persist for the whole op where they fell, in the state
they were left — no fades, no sinks, no despawn; blood pools
spread on wet ground and stay; heavy impacts and mecha violence
leave giblets; blood trails mark a wounded man's path; moving and
hiding bodies is core stealth craft, priced in time and exposure.
The operator's death ends the career; the city keeps the body
where it fell, and the successor can be sent to the same street
and find what the mecha left. Gore at Resident Evil weight,
rendered in PS1.5's language: blood as palette and pooling decal,
giblets as low-poly chunks with full surface craft.

**Has.** `height_at` terrain following and scripted drives. No
collision, no gravity, no bodies. The step-ring contour carries a
~0.84 m discontinuity the traversal brief treats as an explicit
seam.

**Gap.** The physics slice (capsule solver, gravity, seam-safe
sweeps), a body/corpse system (ragdoll or posed-fall bodies that
persist as evidence), blood pooling/trailing as persistent decals
on the shared fields, and prop physics for crates/doors.

**Study.** Rapier's solver shape, design-only and still PARKED:
study the narrow-solver architecture (broad phase, narrow phase,
contact resolution) to inform the capsule-vs-static-world solver,
without adopting the library — the zero-dependency rule stands,
and Quake's movement model (a controller, not a general solver)
is the closer shape for the capsule itself. Study ragdolls from
the rigid-body literature (impulse-based bodies, joint constraints,
settling to rest) as the corpse model: a body is a small set of
linked rigid bodies that falls, settles, and then becomes static
evidence — it does not need a full ragdoll simulation at all times,
only at the fall. Study blood as a decal/field system (pools as
persistent surface marks with spread over time on wet ground),
not as particles.

**Shape.** A physics module ticking inside the sim's fixed step:
capsule-vs-static-world solver against the block's real geometry
with slope/step rules and seam-safe sweeps; gravity with impact
reporting to the caller (doctrine judges, physics reports); a
corpse system where a killed body falls under the same solver
(ragdoll-lite: linked rigid bodies to rest, then frozen as
persistent evidence with blood pools and trails as surface marks
on the shared fields); bodies as first-class persistent objects
the op record and the witness economy can reference (found/unfound,
moved/hidden), so a body is never just a mesh.

## 9. Sound — stimulus layer, playback layer, music

**Need (grounded).** Sound is load-bearing and two coupled halves.
The STIMULUS layer (simulation): sound events with position,
radius, surface, and decay, written into the same fields the
perceivers read — one-field law extended to hearing: if it sounds
loud, it is loud, to everyone. Footsteps from the animator's events
(surface × speed); gunfire; hook strikes; servo whine; rain on
tin; the Hound's ears; the Type 2's scream as a district-level
stimulus; the nightjars' eerie churring as a sound event with NO
source marker (the visor does not tag it). The PLAYBACK layer
(presentation): positional mixing, occlusion by the same geometry
the light field uses, native output + WebAudio on the web host.
The music: an adaptive stem architecture (stealth / suspicion /
combat / conversation) composed originally against the Coupling
reference playlist — the playlist itself is never shippable.
Proof surface: standing in the street and hearing the patrol
before seeing him.

**Has.** Nothing. Every host is silent. The animator's event track
is specified (SUBSYSTEMS §1, traversal brief §4.3) but not built.

**Gap.** Both halves: the stimulus bus and its propagation, the
playback mixer and backends, the adaptive music system, and the
occlusion model shared with the light field.

**Study.** SoLoud's mixer architecture (voices, buses, filters,
3D positional sources, streams) as the playback layer's shape —
core permissive (zlib/libpng), modules checked individually, and
binding C++ would bend zero-dependency, so study the shape, not
the library. Study DarkPlaces' positional audio model (3D
positional + attenuation + occlusion hooks) for the same layer.
Study Thief's sound propagation lineage for the stimulus layer's
propagation (geometry-aware occlusion and decay). The stimulus
layer itself has no reference equivalent — it is C35's own
invention, and the study input is the field architecture from §4,
not an audio engine.

**Shape.** A sound module of two halves on one bus: the stimulus
half writes events (position, radius, surface, decay) into the
shared fields every perceiver reads, fed by the animator's event
track and by every noisy system (weapons, hook, mecha emissions,
rain, screams, churring); the playback half mixes those events
positionally with occlusion from the same geometry the light
field uses, native + WebAudio backends, plus an adaptive music
stem mixer (stealth/suspicion/combat/conversation) driven by the
op engine's alert state — music as a state output, never a
soundtrack pasted over the sim.

## 10. Weather and atmosphere — rain, fog, wet surfaces

**Need (grounded).** Greyport's RAIN does not exist yet: rain state,
its FX (sheet rain, splash, runoff brightening the streets), and
its simulation price — rain degrades the soldier's senses,
reprices every footstep, feeds the sound field, washes scent,
shortens wall-runs and slickens their exits, and prices traversal
through the shared surface fields. Fog exists as grade, not yet as
weather. The reality layer owns time and season; weather is a
system on top of it, not a time lever. Wetness is a material state
(BEAUTY-PASS direction): wet ground carries lamp streaks toward
the camera, and wet surfaces change footstep price, wall-run
budget, and blood pooling.

**Has.** Fog as a grade (PS1.5 fog ×2), the reality layer's solar
model and seasons, the shared surface fields as a concept (wetness
value the traversal course consumes). No rain state, no weather
FX, no sense repricing.

**Gap.** The weather engine: rain state machine, precipitation FX,
wetness as a field over surfaces, fog as weather (not just grade),
and the repricing hooks into perception, sound, traversal, and
scent.

**Study.** DarkPlaces' data-driven particles: effect definitions
(emitter shape, rate, size, velocity, life, gravity, texture) in
reloadable tables, with rain as one entry — study the TABLE shape
(effect definitions driven by weather state), not the textures
(C35's particles are procedural points/sprites from the painters,
under zero image assets). Study its height-fog model for fog that
fades with height. The wetness-as-field idea and the sense
repricing are C35's own — no reference ships them.

**Shape.** A weather module owning a small state machine (clear,
rain onset, rain, clearing, fog) driven by the reality layer's
season and clock but never by a time lever; a particle-effect table
(rain sheets, splash, runoff, smoke, sparks) rendered procedurally
through the painters; wetness as a per-surface field value that
every other system already reads (footstep price, wall-run budget,
scent wash, blood spread, lamp streaks); fog as a weather state
with height falloff, not only a grade; all of it deterministic on
the fixed step and priced in the cost ledger.

## 11. City generation and streaming — tiles, interiors, district scale

**Need (grounded).** Greyport is BIG and dense, not sprawling:
multiple districts (dockyard, old town, Administration quarter,
market streets and arcade, temple ground, tenements, financial
district), checkpoint crossings between them, rooftops as a second
city laid over the first. Slice 1's block is the TILE — the
generative pattern every district varies. The city exists whole as
DATA (every street, lamp, and door in the Archive's model) and
becomes geometry only around the operator; an op witnesses one
district deeply, a career walks the whole map. Density over
sprawl: acreage crossed without a decision is dilution. Every
district is a dungeon floor with its own light, patrol economy,
and way of being watched. Interiors matter (debrief room, cold
stores, kiosks, temples, arcades) and the camera takes composed
placed angles inside them. The checkpoint furniture (barriers, boom
gate, kiosk, queue) is the social stealth heart.

**Has.** One procedural Greyport block (Slice 1), the generator's
block builder, the tile concept, the governor pricing distance.
No districts, no streaming, no interiors system, no checkpoint
furniture as a system.

**Gap.** District generation (tile variation by district), the
streaming rule (geometry around the operator, data everywhere),
interiors as generated spaces with placed cameras, checkpoint
furniture as composable registry entries, and the district-level
patrol economies.

**Study.** The tile/variation pattern from procedural city
generation generally (a base tile with district parameter sets —
palette, density, furniture, light temperature, patrol economy —
rather than hand-built districts), and the data-vs-geometry split
from open-world streaming practice (world as data model, geometry
as a window around the player, with the ledger pricing the window).
Study Recast's bake shape again here: the navmesh bake consumes
the same geometry the renderer and physics read, so the city
generator's output must be bake-ready by construction. No engine
reference ships Greyport's dungeon-city; the study input is the
tile discipline and the streaming window, not a city generator to
copy.

**Shape.** A city module generating districts as parameterized
variations of the block tile (district parameter sets: architecture
kit, palette, furniture registry entries, lamp fields, patrol
economy, checkpoint furniture), emitting both the data model (the
Archive's whole-city model: streets, lamps, doors, crossings) and
the geometry window around the operator (streamed, culled,
ledger-priced); interiors as generated spaces with placed-angle
camera sets; every lamp, crate, banner, and bench a registry entry
with parameters, placement, and budget, so the city builder is
already the mod engine's first customer (MOD-ENGINE §7).

## 12. World persistence and the Archive — city state across dead agents

**Need (grounded).** Greyport must REMEMBER at city scale: bodies
where they fell, mecha damage welded over, the handler's kit one
stage further built, a dead agent's file read by his successor, a
closed temple, a checkpoint that learned your face, a destroyed
unit gone but the alert budget raised. The service record IS the
save file: clearances, reputation with factions, heat,
commendations, black marks persist across ops and shape what the
city offers and what Control trusts you with. Burned agents end
the career, not the game: a new agent inherits the same city with
its grudges and unfinished business. The Archive is literally a
file. The compendium auto-updates at debrief (and is readable in
the menu): updates computed during the op LAND only at debrief,
from the op record; entries gain paragraphs and never rewrite;
locked entries show nothing but a locked marker; the nightjars are
a margin count with NO entry; the six mysteries are never
resolved by any entry.

**Has.** Nothing: no persistence module, no save format, no
Archive file. The compendium spec (COMPENDIUM §6) proposes the
data shapes: an append-only journal per career (one op record at
op close, never edited) plus one Archive file per city (rewritten
only at debrief, from the journal, so a mid-op crash loses nothing
filed and a mid-debrief crash replays from the journal).

**Gap.** The persistence engine: serialization, save format,
versioning, integrity, the journal/Archive split, cross-agent
inheritance, and the compendium's filing function (op record →
paragraphs, deterministically).

**Study.** No engine reference — the catalog is explicit that
serialization, save format, versioning, and integrity are generic
engineering, not an engine take. Study event sourcing as a pattern
(append-only event journal + a derived state file rebuilt from it)
from the database lineage: the journal is the truth, the Archive
is a projection, and a crash replays the projection from the
journal. Study versioned serialization (format version + migration
path per version, integrity check on load) as generic practice.
The compendium's filing function is DeBrief's own: a pure function
from op record to paragraphs, with the mystery law and the
no-spoiler rule as its invariants.

**Shape.** A persistence module owning two files: the journal
(append-only, one op record per op close, with the COMPENDIUM §6
schema — op id, district, hour, weather, kit, objectives,
crossings, witness events, descriptions, bodies, blood, mecha and
Type 2 contacts, scent events, Wanderer events, nightjar counts,
grade, time, traces) and the Archive (entries, paragraphs,
statuses, Service Record, city state — mecha damage, temple state,
checkpoint memory, alert budget — rewritten only at debrief from
the journal); across agents the Archive persists and the journal
starts a new career section with the inheritance paragraph first;
the compendium's filing is a deterministic pure function of the
journal, so the same op record always files the same paragraphs.

## 13. Save/load — format, versioning, integrity, modded runs

**Need (grounded).** The service record is the save file; the
Archive persists across agents; a modded save loads only with the
same mod set (same ids, same versions) or with mods disabled and
mod props stripped — never with a silently different city than the
one the save was made in; a run with any mod loaded is marked
MODDED in the ledger and the service record's margin, and debrief
grades from modded runs carry that mark. A crash mid-op loses
nothing filed; a crash mid-debrief replays from the journal.
Determinism law holds: the same inputs reproduce the same world.

**Has.** Nothing. (See §12 — the journal/Archive split is the
proposed shape; no implementation exists.)

**Gap.** The save format itself, versioning and migration,
integrity checking, mod-set binding, and the load path (including
quarantined-mod behavior on load).

**Study.** Versioned serialization practice generally (format
version stamped on every file, a migration function per version
step, load refusing or repairing unknown versions explicitly rather
than silently) and content-addressed integrity (a checksum over
the journal so a tampered or truncated file is detected, not
played). Study the mod-binding rule as a manifest problem: the
save records the mod set (ids + versions + load order) and load
compares it exactly — the MOD-ENGINE §4 rules are the spec, not a
reference engine's.

**Shape.** A save/load layer on the persistence module: every file
carries a format version and a checksum; load verifies version,
integrity, and mod set before touching the world, and refuses
explicitly (with the reason) on mismatch; migration functions carry
old versions forward one step at a time; the journal's append-only
rule means load never rewrites history — it replays it; modded
runs are marked in the ledger and the Service Record at save time,
so a grade can never be laundered by unloading a mod.

## 14. UI/HUD — diegetic visor, damage numbers, menu, papers

**Need (grounded).** No in-engine text rendering exists today.
Owed: glyph atlas + layout, the debrief screens, menus, documents
and papers as readable objects, signage in the world, and the
visor's sense-view as a presentation mode. The HUD IS the visor
and the analog instruments — wound state, ammunition, op clock,
compass strip, body pictogram, SIG bars (golden #38/#39 HUD
frames) — not an overlay pasted on the world. Damage numbers come
through the visor (impact, penetration, component struck — the
player's own hits only; enemy state stays posture-only; the mecha
keeps NO health bar). Component tags (SENSOR, LOCOMOTION, CANNON,
POWER-COOLANT) appear through the sense view at the ranges the
sense mode grants. Papers are layered physical objects, presented
not clicked, inspected in real time under the lamp while the queue
watches. The compendium's menu presentation (PROPOSED) is a file
drawer: tabs by category, entries as folder pages, redaction bars
where knowledge stops, locked entries showing nothing but a locked
marker. The mod menu (v1) lists mods, conflicts and winners,
quarantined mods with reasons.

**Has.** Nothing: ledgers live in title bars and terminals. The
golden HUD frames (#38 swing, #39 dockside) are the visual
reference; the visor-frame rim light (FIRST-PERSON §3) is the cover
read.

**Gap.** The UI/text engine: glyph atlas, layout, world-space
readable surfaces, visor rendering, menu and debrief-room screens,
papers as inspectable objects, and the mod menu.

**Study.** The catalog's UI verdict is "no strong reference": id
Tech 4's GUI-on-surfaces is REJECTED (needs authored art — C35's
papers and signage are procedural text). Study only the CONCEPT
of world-space readable surfaces from it. Study text rendering as
generic practice (glyph atlas, signed-distance or bitmap glyphs
generated procedurally, layout as boxes with flow) — the engine
owns this outright. Study diegetic HUD practice from the
immersive-sim lineage (Dead Space's in-world health, Metro's
analog instruments) as design reference for the visor-as-instrument
idea, not for code.

**Shape.** A UI/text module owning a procedural glyph atlas and a
layout engine (boxes, flow, redaction bars as first-class layout
elements), rendering into two surfaces: world-space (papers,
signage, desk folders, the wall map's red op-circles) and
visor-space (the analog instruments, damage numbers, component
tags, the frame rim driven by the light field at the eye); the
menu and debrief-room screens as data-driven layouts over the
Archive and the Service Record, with the no-spoiler rule enforced
at the layout layer (locked entries render as a locked marker and
nothing else); the mod menu as a view over the load report.

## 15. Dialogue — handler, interrogations, Control's protocol

**Need (grounded).** Debrief conversations with the handler (the
man across the desk, gruff, harsh, slow, methodical — reading the
op back, grading the file, stamping the Event entry, spending
grades at the desk between ops, never in a field pause menu);
checkpoint interrogations (the inspector's questions, graduated
failure: question → detention → force); Control's protocol voice
(whether the handler IS Control is deliberately unstated — the
voice on the record and the man in the room may or may not be one
person, and the game does not answer it). Dialogue state is read
by the op engine: a detention, a description, a grade all feed the
record. Branching exchanges whose state the op engine reads.

**Has.** Nothing. The handler has a face (golden #18/#19) and a
voice direction; no dialogue system, no branching, no interrogation
states.

**Gap.** The dialogue engine: branching exchanges as data, state
the op engine can read, the handler's debrief script driven by the
op record, checkpoint interrogation as a state machine tied to the
papers system.

**Study.** The Dark Mod's conversation system as DESIGN reference:
branching-exchange state shape (exchanges as nodes with conditions
and effects, state readable by the mission system). Study the
debrief as a REPORT, not a conversation tree: the handler reads the
op record back (what you did, what you left, who saw you, what the
blood says) and the dialogue is generated from the record — the
study input is data-driven report generation (record → scripted
lines with slots), not a chat system.

**Shape.** A dialogue module owning exchanges as data (nodes,
conditions on op-record and Archive state, effects back into the
op record); the debrief as a generated reading (the op record's
fields filling the handler's script — crossings, bodies, grades,
traces — with the grade stamped on the Event entry and grades
spent at the desk); checkpoint interrogations as a state machine
(question, detention, force) driven by the papers system (face,
dress, papers, behaviour) and the inspector's state (quota, mood,
degradation by rain, cold, fear of the mecha); Control's protocol
as a voice-and-protocol layer, never resolved to a person.

## 16. Missions and the op engine — generation, triggers, grading, debrief

**Need (grounded).** The replayability core: faction doctrine
generates ops (tails, dead drops, extractions, eliminations,
burglaries of secrets, counter-surveillance sweeps); triggers and
conditions run them; the event record (every trace, every crossing,
every body found) feeds the debrief grading — Ghost, Surgeon,
Butcher, Ghost-That-Never-Was — and writes the next line of the
city's memory. Hand-built story ops (the spine, PROPOSED 13-op
arc in 3 acts ending with The Case Leaves the City — the spine
closes, the systemic city continues) interleave with systemic ops,
and the systemic city keeps score between them. No failure-state
reload: an op that goes wrong keeps going wrong, forward. Grades
pay progression at the desk (operator skills die with the agent;
service skills persist in the Archive). Replay is doctrine: every
level replays under new conditions — hour (real clock), season,
the alertness the city has learned about you, different kit,
different intel, different orders. Mastery is the content: grades,
ghost runs, no-trace runs, full-archive runs.

**Has.** Nothing: no op engine, no triggers, no grading, no
generation. The design (DESIGN §5–§6, STORY-BIBLE §9) and the
per-op record schema (COMPENDIUM §6) are the spec.

**Gap.** The op engine: op generation from faction doctrine,
trigger/condition evaluation, objective tracking, the event record,
the grading function, and the debrief loop that closes an op and
opens the next.

**Study.** The Dark Mod's mission/trigger logic as DESIGN
reference: trigger and condition shapes for op generation (the
grading itself — Ghost/Surgeon/Butcher — is DeBrief's own). Study
systemic mission generation from the simulation lineage (faction
dynamics composing ops from the world state: couriers moving,
assets flipping, stations rising and burning) as an architecture
pattern: the city simulates, the op engine reads the simulation
and composes objectives from it, rather than scripting ops
one by one. Study grading as a pure function of the event record
(traces, crossings, bodies, blood, grades as currency) — the
debrief grades what the record says, and the record is written by
the sim, not by the grader.

**Shape.** An op-engine module owning: a generator (faction
doctrine tables + city state → op definitions: type, district,
objectives, kit, intel, orders); a runtime (triggers and conditions
evaluated on the fixed step against the shared fields and the
event record); an event recorder (every trace, crossing, body,
witness event, description, mecha contact, scent event, nightjar
count, time, and trace left, in the COMPENDIUM §6 schema); a
grading function (event record → Ghost/Surgeon/Butcher/
Ghost-That-Never-Was, with grade-as-currency paid at the desk);
and the debrief loop (op close → journal append → handler reading
→ Archive filing → next op offered by doctrine and by the
Service Record's state).

## 17. Modding — registry props, placement contracts, validation, scripting

**Need (grounded).** Chase, 2026-10-30: "something people can use
to easily mod the game how they want and make it to where they can
reuse props we have made or make their own." A mod is a folder of
text files: it names props from the game's own registry, or defines
new props as recipes in the game's own part vocabulary, and places
them by rule on the game's own anchors. No Rust, no compiler, no
code in v1 or v2. Props are named registry entries (id, parts,
materials by painter name, parameters with ranges, placement
contract — footprint, height, collision class, grapple-anchor flag,
occluder flag, footstep surface — budget, tags); the city builder
itself composes registry entries from day one. Mod files are JSON
through the engine's own strict parser (unknown fields are errors,
nesting and size capped, duplicate keys are errors). No bitmap
imports in v1 (mods cannot break the PS1.5 look by construction).
Load order deterministic (base, then `load_after` topology, then
folder name); conflicts reported, never silent; broken mods
quarantined and the game still boots; modded runs marked MODDED
and grades carry the mark; modded saves bind to their mod set.
Staged: v1 props + placement, v2 tuning tables + behavior
selection (no code), v3 missions/maps + sandboxed WASM plugins
(Lua/Rhai rejected, native plugins disqualified).

**Has.** The spec (MOD-ENGINE.md, complete and verified). No
registry, no parser, no validator, no loader, no mod menu. The
asset pipeline is SEEDED (procedural generation, validators,
census tooling) and is the mod engine's home.

**Gap.** The whole mod engine, in its staged order — and, before
it, the prerequisites the spec names: the part-primitive vocabulary
frozen as the prop language, the material registry addressable by
name, atlas allocation that reserves mod space dynamically, and
the city built from registry entries from day one.

**Study.** DarkPlaces' QuakeC context split as the LESSON (not the
runtime): SSQC/CSQC/MenuQC — game logic, presentation, and menus
as separate sandboxes — which maps onto the mod engine's staged
sandbox (data-only, then WASM with a narrow deterministic host API:
spawn/despawn registry props, read field values, register triggers
on the op engine's vocabulary, write to the mod's own state).
Study DarkPlaces' PK3/ZIP packaging shape and effect-definition
tables only for the pack-and-table discipline. The strict parser,
validator, quarantine, and conflict rules are the spec's own — no
reference engine ships them as a set.

**Shape.** A mod module in the asset pipeline: the prop registry
(built at startup from the game's own builders, every shipped prop
registered); the strict JSON parser (one dialect for manifest,
props, placements); the validator (schema, namespace, vocabulary,
ranges, budget verification by generating the prop once and
counting real triangles and atlas share, placement anchors,
load-order cycles, engine range); the loader (deterministic order,
conflict reporting, quarantine before the world builds, load report
as a first-class output and a mod-menu view); the placement engine
(rules and anchors, never raw coordinates by default); v2 tuning
tables on the same machinery; v3's WASM host API deferred, not
paid early — and the shipped example mod (one reused prop, one
new prop, one placement rule) as the proof the system works.

## 18. Input and bindings — verbs, remapping, the fixed keys

**Need (grounded).** A real binding layer — remappable, the game's
verbs (aim, fire the hook, HALT answered, drag a body) — owed when
play begins. Fixed by decision: E = grapple (press fires, hold
keeps the rope, release detaches; the windowed host's free-fly
debug camera uses E as "down" today, and that is a dev binding
that yields when play mode owns the key). Lean's pair is Chase's
call and cannot include E. The both-hands law and the
waiting-for-release guard are controller law, not input
suggestions. The windowed host's current rig (click capture,
WASD/QE fly, Shift speed, Esc release, F1–F4 placed views, P
still, digits 0–4 governor re-pin, backquote release) is a debug
rig, not the game.

**Has.** The debug rig in the windowed host. No binding layer, no
verb table, no remapping.

**Gap.** The input engine: verb table, bind table, remapping,
play-mode vs debug-mode key ownership, and the input contract
(press-edge with guards, hold, release-always-instant) the
traversal build depends on.

**Study.** Xonotic/DarkPlaces' binding layer: the bind table shape
(action → keys, remappable) and the verified fact that E is the
lineage's own default hook key. Study the input contract shapes
from the traversal brief and FIRST-PERSON §4.0 (press-edge with
waiting-for-release guard, hold, release) as the spec the binding
layer must honor — the layer translates keys to verbs; the verbs'
guards live in the controller, not in input handling.

**Shape.** An input module owning a verb table (move, sprint,
crouch, lean pair, aim, fire, hook, mantle, HALT answer, drag
body, interact) and a bind table (verb → keys, remappable, saved
per player), with mode ownership (play mode owns E and the game
verbs; debug camera yields); input delivered to the controller as
press-edge/hold/release events on the fixed step, so the
waiting-for-release guard and the release-detaches-instantly rule
are enforceable in the controller rather than hoped for in the
key handler.

---

## Study order (reading order, not build order)

This is a reading list for understanding, not a build sequence —
Chase decides what gets built next, and the outside-in order
(engine → city → weathers → lighting → …) still governs building.

1. **Grapple first.** Xonotic's hook state machine + tarzan branch
   + pull arrival ramp + sky guard + mover follow. The traversal
   build's highest-value study, and the only one with a milestone
   already scoped.
2. **Stimulus/response.** The Dark Mod's architecture + light gem,
   read against C35's one-field law. This unlocks perception (§4)
   and every AI brain (§5).
3. **Recast bake pipeline.** Voxelize → watershed → contour →
   polygon mesh, Detour A* + string-pulling, off-mesh connections
   as typed links. This unlocks navigation and the crowd layer.
4. **Wishdir/wishspeed + physics sets.** The controller's feel
   model (§1) and the capsule's velocity contract.
5. **Matrix-palette skinning + MD5 clip shape.** Clip playback
   beside procedural posing (§7).
6. **Unified-lighting pass structure + particle tables.** The
   lighting milestone's pass discipline and the weather engine's
   effect tables (§10).
7. **SoLoud mixer shape + Thief propagation lineage.** The sound
   engine's two halves (§9).
8. **Event sourcing + versioned serialization.** Persistence and
   save/load (§§12–13) — generic patterns, no engine to study.
9. **Everything else on demand.** Bevy ECS patterns, QuakeC
   context split, diegetic HUD practice, systemic mission
   generation — read when their subsystem starts, not before.

## Areas with no reference engine to borrow from

Stated plainly so nobody hunts for one: the one-field law itself
(the field is C35's own; references supply machinery, not the
field); the scent channel (no shipped scent system in the catalog);
the witness economy and the Neighbour's description system
(DeBrief's own social stealth); the Archive and compendium filing
(DeBrief's own record-keeping, with event sourcing as a generic
pattern only); the alert budget as an economy (a resource with
income and decay, not an AI); the Type 1's fixed governor tables
and the Type 2's momentum doctrine (doctrine, not code); the
visor's component readout (a presentation of the perception
fields); the mod engine's validator/quarantine/conflict rules
(the spec's own). For these, the design documents ARE the
reference — study them, and build from scratch.

## Open questions this investigation does not answer

- Shipped Nexuiz balance numbers (`balance25.cfg` values) and the
  stock `g_grappling_hook_tarzan` default were NOT verified in the
  house tree — the traversal build re-derives them from its course
  rather than asserting them.
- Lean's key pair (cannot include E) — Chase's call.
- Base FOV and eye-height numbers — rules with proofs, not
  constants; the build crew reports them from the model.
- The 13-op campaign arc — PROPOSED, not settled; Chase's ruling
  owed.
- The compendium's category set, entry schema, event→entry mapping,
  Fable tier, and per-op record schema — all PROPOSED.
- The Type 2's field name (THE BRIDE / MOTHER) — undecided.
- Whether the in-game model wears the patch + balaclava at all
  times vs the identity governing presentation — rides to the
  model pass.
- #38's framing under FP (kept as compendium image or retired as
  gameplay reference) — Chase's call.
- Comfort fallback for the swing — a named transition's easing,
  not a global slider; whether a comfort option ships is Chase's
  call.
