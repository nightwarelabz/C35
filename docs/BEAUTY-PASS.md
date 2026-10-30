# BEAUTY PASS — DeBrief / C35

Design only. No tree edits, no repo writes. Written 2026-10-30 after
Chase's fullscreen verdict on the G1: the build "feels and looks like
minecraft but crappier," "the operator looks nothing like what he is
supposed to," "the lines are static like." Performance is great and
must stay great. Build order is outside-in: engine, then city, then
weathers, then lighting, and so on.

Standing law this spec serves, not revises: the Beauty Law ("we are
not aiming for globby, minecraft shit. we are aiming for our own
unique beauty" — "we do not stop until the graphics are at the
standard"), the PS1.5 identity (low-poly, painted, chunky but
crafted), the advertised standard (C35 must match the concept-art
style it was advertised with), zero image assets, determinism, the
cost ledger, and the one-field light law (one practical-light field
serves renderer AND every perceiver — if a screenshot looks safe, it
is safe).

Reference facts the spec is measured against (all verified, none
invented):
- G1 fullscreen run, 2026-10-30: windowed host, GL/Mesa HD 4600,
  PS1.5, internal 854x467, output 1853x1013 from a 1920x1080 request,
  FXAA blit pass (TAA unavailable on this backend), palette 20 steps,
  fog x2.0, nearest presentation. Block: 9,692 verts / 4,846 tris,
  12 lamps, 10 occluders. Sessions ran clean (3,944 and 5,970 frames)
  and segfaulted on exit only (separate fix, already running).
- Both judged runs rendered in DAYLIGHT: no latitude/longitude passed,
  equator fallback in force, sun near 40 degrees up at 03:27 local.
  The night grade never ran in the test Chase judged.
- Character build as landed (phase 1): operator 1,310 tris, 110 parts,
  one shared 256² procedural atlas with the soldier (soldier regions
  frozen), face as a painted quad on a box head, boots/gloves/holster
  near-black-brown on the soldier's shared LEATHER, shirt sharing the
  TROUSER paint, chest pockets reading by geometry shadow only.

## 1. Why the build reads as Minecraft — the concrete diagnosis

Not a vibe. Five mechanisms, each checkable on a fullscreen grab:

1. **Flat unbroken boxes.** The Slice 1 block is mostly axis-aligned
   masses with one paint per surface. The golden street (#9) has
   cornices, window frames, sills, lintels, stone trims, a shuttered
   storefront, bollards, crates, wires, signs — every wall is broken
   by something. Minecraft's signature is a flat textured cube; the
   block currently rhymes with it at every level: cube mass + one
   repeated texture + hard edge.
2. **One texture per surface, no variation.** The street reference
   shows per-brick tone shifts, mortar lines, grime at the base,
   concrete panels, stone trims as a different material. The block's
   brick is one paint tiled. At the block's texel density the upscale
   smears it into a flat color with noise — a colored block, not a
   wall.
3. **Nearest upscale at ~2.2x.** Internal 854x467 presented at
   1853x1013 with nearest sampling turns every texel into a 2x2 block
   and every edge into stairs. That is the Minecraft pixel look,
   literally. The PS1.5 doctrine says the upscale is served "clean"
   by the temporal reconstruction line — but the hardware run shows
   FXAA blit only, TAA unavailable on the GL backend. The cleaning
   stage the doctrine promises is not active on the path Chase
   tested.
4. **Static lines, static frame.** FXAA smears edges but does not
   stabilize them; with no temporal accumulation on GL, edges crawl
   in motion and sit dead in stills. Nothing else moves either: no
   idle motion on the figures (stand/crouch/aim only), no flickering
   lamps, no drifting fog, no dripping water. The frame is a diorama,
   and a diorama of boxes reads as Minecraft.
5. **Daylight.** The night grade — deep blue, sodium pools, fog
   depth — is what makes low-poly read as crafted; it gives flat
   facets shadow, separation, and mystery. Even illumination on flat
   facets gives colored blocks. The test conditions maximized the
   Minecraft read before a single triangle was judged. Any beauty
   verdict must be re-taken at night, with coordinates passed, before
   geometry is blamed for what light caused.

## 2. Materials and surfaces — engine phase

Goal: surfaces stop reading as flat colored blocks, with zero image
assets and the ledger held.

- **Split the character atlas.** One 256² atlas shared by operator
  and soldier, with the soldier's regions frozen, is what forced the
  phase-1 compromises (shirt = trousers, boots/gloves = soldier's
  leather, face = one borrowed rect). The engine phase gives the
  operator his own atlas budget and the soldier his own; frozen
  regions stop being a tax on every future figure. Ledger: report
  atlas sizes and VRAM before/after; the G1 windowed session must
  hold its vsync cap (it ran at the 60 Hz cap on the G1) — that cap,
  not a new number, is the budget.
- **Procedural paint with variation, not flat fills.** Brick becomes
  courses: per-brick tone jitter, mortar lines, a grime gradient at
  the base, stone trim as a separate material with its own paint.
  Waxed canvas becomes mottle + seam lines + wear pale at strap
  lines (already the #31/#32 language, at higher texel density).
  Concrete becomes panels + stains + expansion joints. Rule, not a
  number: at the distances the game plays (checkpoint ~5 m, patrol
  ~20 m), brick courses, mortar, and trim must each be separately
  readable on a fullscreen grab. If they smear into one color, the
  texel density is still too low.
- **Wet surfaces as a material state, not a decal.** The street
  reference's whole read is wet concrete + sodium reflection. The
  engine phase adds a wetness channel to ground materials (puddle
  response, reflection streaks under lamps) that the weather phase
  later drives. This is the single highest-value material change for
  Greyport: without it, no night street will ever read as #9.
- **Detail layer for close range.** A second, finer procedural
  detail pass (grain, weave, scratches) that fades in under ~5 m and
  costs nothing at distance. This is what separates "painted" from
  "flat" in the golden art: the references hold detail at every
  distance because they were painted that way.
- **Ledger law, unchanged.** Every tri-count or texture-budget
  change ships with before/after numbers: scene tris, draws, atlas
  sizes, VRAM, G1 windowed frame times. Performance is great; the
  pass proves it stayed great. No invented targets — the measured
  envelope (G1 windowed at the vsync cap, headless drives ~139 fps
  on the G1's earlier runs) is the bar, and any regression is a
  failed phase.

## 3. Silhouettes and shapes — city phase art kit

Plain boxes read as Minecraft because their silhouettes are
rectangles. The fix is not more triangles in general; it is
deliberate silhouette breaks. The city phase builds an architecture
kit, and the block is rebuilt from it (the block is the TILE every
district varies, so the kit pays off district-wide):

- **Roofline rules.** No flat unbroken roofline longer than a
  shopfront: parapets step, cornices project, chimneys and vents
  break the line, rooflines vary in height between neighbours. The
  banner's skyline works because every mass has a different top.
- **Wall rules.** No unbroken flat wall plane wider than a doorway
  without trim, a frame, a sign, a pipe, or a prop. Window frames,
  sills, lintels, shutters, stone base courses, and door surrounds
  are kit parts, not one-offs. Brick variation (section 2) does the
  rest.
- **Mass rules.** Buildings get setbacks, bays, and attached masses
  (stair towers, loading docks, crane bases, watchtowers) so no
  building is one box. Street furniture is mandatory, not dressing:
  lamp posts, bollards, crates, barrels, wires on catenaries, the
  three-bars banner as street furniture (golden #9), signs.
- **Character silhouettes obey the same law.** A figure's
  pure-black silhouette at 20 m must still read its kit: coat +
  radio + whips, or mask + visor + raised USP triangle. If the
  silhouette is a post with legs, the geometry failed before any
  texture is judged. (Full treatment: section 5.)
- **Low-poly is kept.** The kit is still boxes — but boxes with
  bevels, steps, and attachments. Chunky, not blocky: the difference
  between PS1.5 and Minecraft is craft per triangle, and the kit
  exists to force craft per triangle.

## 4. Smoothness — the "static lines" complaint, engine phase

Three separate problems hide in "static like," and each gets its own
fix:

1. **Jagged edges (spatial).** The GL backend currently presents via
   an FXAA blit pass; TAA is unavailable on this backend per the run
   log. Engine-phase work, in order: (a) tune the FXAA pass
   (subpixel and edge thresholds are shader constants — cheap, and
   the current pass is visibly under-tuned for a 2.2x upscale);
   (b) prove MSAA 2x/4x on the G1 GL path as an experiment with a
   ledger (frame time, VRAM) — wgpu's GL backend may or may not
   grant multisample targets on Haswell Mesa, so this is stated as
   an experiment with a measured verdict, not a promise; if the
   backend refuses it or the cost breaks the vsync cap, it is
   refused honestly and the fallback is (a) plus (c); (c) port or
   activate the temporal reconstruction line (M3/M4, built for the
   island host) on the GL presentation path, or document precisely
   why it cannot run there. "TAA unavailable" is a run-log fact, not
   a law of nature — the phase must say which of these three
   actually ships.
2. **Unstable edges (temporal).** With no accumulation, edges crawl
   when the camera moves and sit dead when it stops. The fix is the
   same as (1c): a temporal stage, or at minimum a stable upscale
   (a crafted upscale filter: nearest + FXAA + dither, tuned so a
   still frame and a moving frame show the same edges). Proof: a
   slow camera pan across the block, grabbed at intervals — edges
   must not crawl or shimmer between grabs.
3. **Dead frame (motion).** Nothing in the frame moves. Engine phase
   adds idle life as a system, not a decoration: figure idle motion
   (breathing, weight shift, head turns) through the animator;
   lamp flicker (subtle, irregular, never strobing); fog drift;
   rain streaks when the weather phase lands. A frame with moving
   light and breathing figures stops reading as a diorama even
   before a single edge is fixed.
4. **Internal resolution is an open experiment, not a decision.**
   The 854x467 internal frame is the PS1.5 class, and at a
   1920x1080 window the 2.2x nearest upscale is a large share of the
   pixel-block look. The engine phase runs one measured experiment:
   internal resolution scaled with window size (palette grade and
   fog kept) versus the fixed 480p-class internal frame with the
   improved upscale from (1). Ledger decides. The PS1.5 identity
   (low internal resolution, painted, chunky) is not abandoned by
   asking the question — but no resolution change ships without the
   before/after grabs proving it reads better against #9.

## 5. OPERATOR FIDELITY — engine phase, not a later character pass

Chase's verdict: "the operator looks nothing like what he is
supposed to." He is right, and the reason is structural: the
phase-1 figure fails BOTH references at once, for different reasons.

### The two references, and what each one reads as

- **Golden #31/#32 (street configuration): a MAN.** Bare weathered
  face — grey at the temples, scar, once-broken nose — waxed brown
  coat with collar up, dark sweater, belt with brass buckles and a
  front pouch, leather USP holster, watch face-inward on the left
  wrist, lug boots, radio pack high-left with twin whips behind the
  shoulder. The read is face + coat silhouette + belt furniture.
  The figure is recognizable because his face is.
- **Golden #40 (main identity): a MASK.** Black-camo balaclava +
  headset, red visor as the single accent, single-eagle NSA patch,
  olive webbing, USP up in both hands, radio with twin whips. The
  read is mask + visor + raised-pistol triangle. The figure is
  recognizable because his face is absent and the mask is total.
  (Model law, binding: the in-game visor is dark and never glowing
  — red is the mecha's language — and the patch is deleted outright.
  No labels on him in either configuration. The portrait's glowing
  visor and eagle patch do NOT transfer to the model.)

### Why the in-game figure reads as neither

1. **It has the coat's bare head without the coat's readable face.**
   The face is a painted quad on a box head. At study-close range
   it holds (features upright, scar placed, stubble painted). At
   play distance — 20 m, free camera, the distances the game
   actually uses — it is a blank box. #31's whole identity is a
   face the player can read; the figure has no face at any distance
   that matters.
2. **It lacks the identity's mask entirely.** No balaclava, no
   headset mass, no visor accent, no raised-pistol triangle at
   stand-ready (arms at sides). The silhouette is a post. #40's
   whole identity is a mask the player can read at any distance;
   the figure has no mask at all.
3. **The coat silhouette collapses at distance.** #31's coat reads
   through collar height, skirt break at the thighs, and brass belt
   furniture catching light. The in-game coat is torso box + skirt
   boxes + small pouch boxes; at 20 m the bumps vanish into the
   torso mass and the silhouette is a rectangle with legs. The
   phase-1 report admitted the same failure in its own words: on
   backlit island studies the figure "still reads close to
   silhouette," and the rear three-quarter close study was "the
   weakest frame in the packet."
4. **Daylight removed the only thing hiding it.** Under lamps, the
   phase-1 figure separates (the report proves it at hero-street
   and echo-patrol). Under the daylight of Chase's test, with even
   illumination and no lamp pools or fog depth, the flat facets and
   flat colors had nowhere to hide. The figure was judged in the
   worst possible light — and the worst possible light is also the
   light that exposes that the fidelity is only proven under lamps.
5. **It does not move.** Stand-ready, crouch, aim — no idle motion.
   A statue made of boxes, seen at distance, in daylight, is
   exactly what "nothing like what he is supposed to" describes.

### What the ENGINE must do for character fidelity

This is engine-phase work because it is material, mesh, and
animator capability, not a repaint:

- **A character material system, separate from the block.** The
  operator gets his own atlas budget (section 2) with dedicated
  rects for face, hands, and kit — no more borrowed rects. Cloth
  treatment: waxed canvas (mottle + seams + wear pale at strap
  lines), webbing (weave + edge stitching), leather (grain + edge
  wear), gunmetal (edge highlight + wear). Skin treatment: skin is
  never a flat fill — gradient plus shaped shadow (brow, cheek,
  jaw) painted at a dedicated face resolution.
- **A face beyond the painted quad.** Two changes, both required:
  (a) a head mesh with modeled planes — brow shelf, nose bridge,
  jaw line — so light shapes the face instead of paint faking it;
  (b) a dedicated face texel budget, so the painted detail survives
  to 5 m. Rule: the face must read (brow, nose set, scar side) at
  checkpoint distance (~5 m) on a fullscreen grab, not only at
  study-close range.
- **Proportion and silhouette rules, pinned to the references.**
  Head-to-body ratio, shoulder width, collar height, coat skirt
  break point, radio height (top at the shoulder line), whip
  heights — all pinned to #31/#32 (and, if the ruling goes the other
  way, to #40). Proof: pure-black silhouette renders at 5 m, 10 m,
  and 20 m; the kit map must survive all three distances.
- **Idle life through the animator.** Breathing, weight shift, head
  turns, coat skirt swing in walk. A figure that breathes stops
  reading as a statue, and the animator work is engine capability
  the traversal phase reuses.
- **Per-distance read requirements.** The engine phase states them
  and the proofs check them: at 20 m (patrol distance) the
  silhouette + coat color must identify him; at 5 m (checkpoint)
  the belt furniture, radio, and whips must read; at 2 m (debrief)
  the face must read. A figure that only reads at one distance has
  failed.

### OPEN QUESTION — for Chase's ruling, not decided here

**Which reference must the in-game street figure read as — the coat
(#31/#32) or the identity portrait (#40)?**

- **If the coat (#31/#32):** the figure keeps the bare weathered
  face. The work is face modeling + coat silhouette + belt
  furniture at distance (above). The identity portrait governs
  presentation and marketing only. The in-game man is recognizable
  by his face and his coat, and the debrief-room scenes keep a face
  across the desk.
- **If the identity (#40):** the in-game figure wears the
  black-camo balaclava + headset + dark (never glowing) visor in
  play. The face problem disappears — a mask is one readable shape
  at any distance — but the street configuration must be
  re-decided (does the waxed coat pair with the balaclava, or does
  the identity replace the street configuration outright?), the
  eagle patch stays deleted under the no-label law, and the
  Operator becomes faceless in play — which changes every
  debrief-room scene, where his face is currently the other side of
  the desk.
- **A third state is also open, not decided:** street coat +
  balaclava as a combined configuration. That is a new configuration,
  not a reading of either reference, and it needs its own ruling.

The spec does not pick. The engine work above (materials, modeled
head planes, silhouette rules, idle life) is required under every
answer; only the head treatment and the configuration map change.

## 6. The outside-in sequence — phases and their proofs

Each phase states its proof before it starts. A phase is done when
its grabs read at the standard AND its ledger holds — not when its
code lands.

### Phase 0 — Engine (now)

Renderer, materials, smoothness (sections 2, 4), character-fidelity
engine capability (section 5), and the exit-crash fix already
running. The segfault is engine work and stays first: a crashing
host cannot be judged for beauty.
- **Proof:** fullscreen night grabs (coordinates passed) at four
  fixed cameras — F1 hero-street, F2 patrol-echo, F3 open crossing,
  F4 shadow crossing — judged against #9 (street) and #11
  (patrol). "At the standard" for this phase: no flat unbroken wall
  in frame, brick courses and trim separately readable, edges
  stable across a slow pan, the figure's kit map readable at 20 m.
  Ledger: G1 windowed session holds the vsync cap; scene tris,
  draws, atlas sizes, VRAM reported before/after.
- **Also owed in this phase:** the OPEN QUESTION in section 5 goes
  to Chase before the character-fidelity build starts, so the build
  aims at the right reference.

### Phase 1 — City

The architecture kit (section 3) + the block rebuilt from it. The
block is the TILE: this phase's kit is what every district varies.
- **Proof:** the same four cameras, night, judged against #9 and
  the banner's skyline. "At the standard": rooflines broken, walls
  trimmed, street furniture present, no doorway-wide flat plane,
  the block reads as a place rather than a mass. Ledger: block tris
  before/after (the current 4,846 is the floor, not a ceiling — the
  ledger, not a number, decides what the kit costs).

### Phase 2 — Weathers

Rain, fog, and wet surfaces as atmosphere systems, not particles
sprinkled on top. Rain as a system: streaks, splashes, dripping,
runoff; fog as volumes with depth, not a flat tint; wetness as the
material state from section 2, driven by rain history (recent rain
= wet; dry spell = dry). Weather prices traversal and hearing
already by design — this phase makes it visible.
- **Proof:** the same four cameras in rain and in clear night,
  judged against #9 (wet concrete + sodium reflection is the
  standard's whole read). "At the standard": wet streets reflect
  lamps in streaks, fog gives depth between near and far masses,
  rain is visible in lamp light without obscuring the scene.

### Phase 3 — Lighting

Sodium lamp quality, light-field quality, shadows. The 2048²
shadows exist (4096² as of c158214, graphics push 7c4b499); this phase is falloff, pools, and the light field's
resolution: lamp pools with soft edges and hot cores, falloff that
separates near from far, shadow quality that holds at fullscreen.
- **Proof:** the same four cameras, judged against #9 and #11 —
  and, critically, the F3/F4 crossings re-proven, because the
  one-field law means any lighting change is also a perception
  change. "At the standard": a screenshot that looks safe IS safe
  (crossing numbers unchanged or explicitly re-adjudicated), lamp
  pools read as geography, the figure separates from the street
  under lamps at patrol distance.
- **Note:** lighting comes AFTER weathers in Chase's order, and the
  order matters: wet surfaces (phase 2) are what lamps reflect, so
  lighting quality cannot be judged before wetness exists.

### Phase 4 — After the engine is finished

Interiors (the debrief room to golden #10: sodium through blinds,
wall map with red op-circles, scratched metal), characters at full
fidelity under the section-5 ruling, and then — only then — DeBrief
content resumes (checkpoint slice, citizens, enemies, RPG layer).
The engine is "finished" when phases 0–3 have passed their proofs
and the v1.0 report is written. Content built on an unfinished
engine inherits the engine's look; that is the mistake this pass
exists to stop repeating.

## 7. What does NOT change

- **The Beauty Law.** "Our own unique beauty," never globby
  Minecraft. This pass exists because the current build violates
  it, not because the law moved.
- **The PS1.5 identity.** Low-poly, painted, chunky — but crafted.
  Chunky is not blocky. Nothing in this spec raises triangle counts
  toward photorealism or imports high-poly assets; the kit, the
  paints, and the presentation do the work.
- **Zero image assets.** Every surface stays procedural. If a
  future phase ever proposes a loaded texture, it says why the
  zero-asset law bends for it — this spec proposes none.
- **Determinism.** Byte-identical replays across runs, as now. No
  presentation change (MSAA, temporal stages, resolution
  experiments) ships without re-proving determinism.
- **The cost ledger.** Performance is great and stays great. Every
  phase reports before/after on the G1; a phase that regresses the
  vsync cap has failed, however pretty its grabs.
- **The one-field law.** Light serves renderer and perceiver alike.
  Lighting and weather changes re-prove the crossings; a prettier
  frame that lies about safety is a regression, not a pass.
- **The no-label law and the visor law.** No labels on the operator
  in either configuration; his visor dark, never glowing. The
  identity portrait's patch and glow stay in the portrait register.
- **The advertised standard.** The banner binds visual temperature;
  the renders bind kit map and silhouette. The pass is judged
  against those images, not against taste.
