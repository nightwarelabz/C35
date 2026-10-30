# Example Street — the shipped example mod

Copy this folder, change the names, and you have a mod. One reused
shipped prop with overrides, one new prop, one placement rule each.
Run the validator until it says valid — the loop is seconds:

```sh
cargo run -p c35 --example check_mod -- crates/c35/example-mods/example-street
```

## mod.json

- `id` — the mod's namespace. Every prop this mod defines must start
  with `example-street:`. The engine's namespaces (`greyport:`,
  `debrief:`) cannot be claimed.
- `engine` — the engine version range this mod was built against
  (`>=0.1 <0.2`). Outside the range, the mod is refused — not
  loaded hopefully. A mod with no range is refused.
- `load_after` (optional) — other mod ids this mod loads after.
  A missing target or a cycle quarantines the mod.

## props/fruit-cart.json

One prop per file. Fields:

- `id` — namespace + name.
- `parts` — an array. Each part: `primitive` (the frozen vocabulary:
  `box`, `wedge`, `cylinder`, `quad`), `size` (metres; for a
  cylinder: diameter-x, height, diameter-z; for a quad: width,
  height, ignored third), `position` (the part's centre in
  prop-local space), `rotation` (Euler degrees, Rz * Ry * Rx),
  `material` (a name from the registry — never atlas coordinates).
  A size or position entry may be a `"$param"` reference to one of
  the prop's declared parameters (the awning's height below).
- `parameters` (optional) — knobs with defaults and legal ranges.
  An override outside the range is an error, never a silent clamp.
- `placement` — footprint, height, collision class (`solid`,
  `pass-through`, `climbable`), grapple-anchor flag, occluder flag,
  footstep surface (`wood`, `metal`, `concrete`, `wet-stone`,
  `none`). Omit it and the prop is dressing only.
- `tags` — placement vocabulary: `street`, `kerb`, `wall-face`,
  `rooftop`, `interior`, `dockside`.
- `budget` — declared triangles and atlas tiles, verified exactly.
  Count: box 12, wedge 8, cylinder 30, quad 4 (double-sided). The
  fruit cart: 1 box (12) + 4 cylinders (120) + 1 wedge (8) +
  2 boxes (24) + 1 quad (4) = 168. Atlas tiles are 0 in v1: mods
  reuse shipped regions and define no new surfaces.

## placements.json

- `placements` — each entry names a shipped prop or one of this
  mod's own props, plus exactly one of `anchor` (a city anchor,
  resolved at load) or `position` (absolute metres — allowed, but
  warned as fragile: it will not follow a rebuilt street).
  `variant` overrides parameters; `yaw` turns the placement;
  `repeat` stamps `count` instances `spacing` metres apart.
- `replacements` (absent here) — game-wide parameter replacements
  for shipped props, by id. Two mods replacing the same prop
  conflict; the later load wins and the report says so.

## What v1 refuses (not ignores)

- Image files anywhere in the folder (PNG, JPEG, WebP, ...):
  "image imports are not supported; define the surface as a
  painter recipe."
- `tuning.json`: tuning tables are v2.
- Fields claiming v2/v3 powers (`script`, `code`, `missions`,
  `dialogue`, `ai`, `enemies`): refused with the stage named.
- Unknown fields, unknown primitives, unknown materials, unknown
  tags, out-of-range overrides, wrong budgets: all errors with
  file:line locations. A mod with any error is quarantined whole —
  none of its props load, none of its placements apply, and the
  game still boots.

