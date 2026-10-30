# Rain bed

Procedural rain for the sound mixer. No samples, no wall clock.
One sticky voice: lowpassed noise for the body of the rain, a
brighter hiss, and short ticks for drops near the ear.

Gain follows a 0–1 rain rate. At 1 the mixer gain is `PEAK_GAIN`
(0.55), so a downpour sits under a voice instead of replacing it.
At 0, before the bed has ever started, no slot is taken. After a
storm, 0 silences the same slot; one-shots cannot steal it.

The bed does not know about the weather module. Hosts pass the
rate. When weather is linked, that rate is `Atmosphere::rain`.

```sh
cargo test -p c35 rain
cargo run --release --example rain_probe -p c35
```

The probe writes three seconds (drizzle, downpour, dry) to
`c35-rain.wav` in the temp directory, prints RMS, and times one
second of the mix. Pass a path to write the file somewhere else.
The same command writes the same bytes.

A debug run of that probe: drizzle RMS 0.010, downpour RMS 0.042,
dry RMS 0, and one second of stereo mix in 3.64 ms of CPU. That
timing is the probe's printout, not a test: a wall-clock bound
does not belong in CI.

The drip clock is a `u64` sample counter. An f32 second-counter
runs fast after about 256 s and stops after about 512 s. If the
mixer is already full when the bed first asks for a slot, the
refusal is latched and later frames do not log another drop.

From a host that already owns a `Playback`:

```rust
playback.set_rain(rain_rate); // 0.0 dry … 1.0 downpour
let pcm = playback.render(frames);
```
