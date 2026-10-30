# Weather

Clock-seeded rain, ground wetness, wind, fog and cloud for Comigen.
Same UTC instant, same storm. No network, no slider, no image assets.

The renderer defaults to clear weather: fog multiplier exactly 1, and
no extra draws. A proof run that does not opt in is unchanged.

## What a frame costs when it is on

| Piece | When it draws | Budget |
| --- | --- | --- |
| Rain sheet | `rain > 0.02` | 1 draw, 2 triangles, one camera-facing quad 8 m in front of the eye |
| Cloud veil | `cloud > 0.02` | 1 draw, 1 triangle, blended over the sky |
| Fog | whenever the multiplier is not 1 | a multiply on the existing fog density, no pass |
| Wetness | CPU, once per reading | 64 rain samples over the previous 6 hours |

Streaks are a column hash and a falling window in the fragment
shader. Columns with `hash > rain` are empty, so drizzle is a few
lines and a downpour fills the sheet. The sheet depth-tests and does
not write depth, so a wall closer than 8 m occludes it. Night streaks
take a warm cast from the reality layer's daylight term and from how
far the drop sits below the eye. The shader does not read a lamp list.

Clouds are value noise on the view ray, scrolled by the wind. Terrain
drawn after the veil covers the ground, so the veil stays in the sky.

## What else reads it

- `Atmosphere::paint_surface` writes wetness onto the shared surface
  record. Wall grip and footstep gain already price that field. A
  wetter record (a puddle) is kept.
- `Atmosphere::vision_gain` is the conscript's rain degradation
  (`1 - 0.30 * rain`, floored at 0.70). Clear weather returns 1.
- `Atmosphere::apply_scent` writes rain and horizontal wind onto the
  Hound's trail, which already washes and drifts.
- A soldier's input `rain` is this reading's `rain`. The brain already
  multiplies attention by it. Copy the field; do not invent a second
  rain.

`ripple_offset(x, z, time, rain)` is a bounded puddle-normal offset
for a wet surface to sample later. The rain sheet does not use it.

## Run it

From the repo root, no GPU:

```sh
cargo test -p c35 weather
cargo run --example weather_probe -p c35
```

The probe prints the four reality-layer proof instants and how long
`Atmosphere::at` takes.

The vista, opt-in (needs a Vulkan or GL adapter; Mesa's software
driver is enough on a GPU-less Linux box):

```sh
cargo run --release -p comigen-native -- --weather --frames 30 --out weather.png
COMIGEN_WEATHER=1 cargo run --release -p comigen-windowed
```

`--weather` and `COMIGEN_WEATHER=1` use the same unix instant the host
already feeds the reality layer, including `COMIGEN_PIN_ISO` on the
native host. The native ledger prints a `weather` line only when the
flag is set.

## Measured (software renderer)

Pinned autumn dusk (`2026-10-19T18:30:00-05:00`, Chicago), 320×180,
classic, governor pinned at level 0, lavapipe. The sun is down, so
this is the scene pass only. Debug build.

| | draws | triangles | GPU scene pass | frame loop |
| --- | --- | --- | --- | --- |
| clear (default) | 23 | 45572 | 3.89 ms | 6.31 ms |
| `--weather` (rain 0.45, cloud 0.63) | 25 | 45575 | 4.33 ms | 6.85 ms |

The storm added the two draws and three triangles the budget names,
and about 0.4 ms of scene time on this CPU rasterizer. A clear opt-in
hour that stays under both thresholds adds neither draw. Hardware
numbers belong on a real GPU; this is the ledger, not a claim about one.
