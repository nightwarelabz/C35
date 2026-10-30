//! Weather & atmosphere: the sky as a system, beside
//! the reality layer, never instead of it.
//!
//! The reality layer owns sun, seasons, and the base fog
//! from the real clock and latitude. Weather owns what
//! changes over hours: rain, wind, fog thickness, surface
//! wetness, lightning. It is seeded and deterministic —
//! the same seed and clock read the same sky — and it
//! drives parameters other systems already price from,
//! instead of re-implementing their judgments:
//!
//! - Scent: copy `wind_vec()` into `ScentField::wind`
//!   and `rain()` into `ScentField::rain`. Drift (2% of
//!   wind, horizontal only), rain wash, and the 3-minute
//!   half-life are already priced in scent.rs.
//! - Surfaces: feed `wetness()` into
//!   `SurfaceRecord::wet(kind, wetness)`. Wall-run slip
//!   (budget ratio falls with wetness) and footstep
//!   splash are already priced in traversal/ and sound/.
//! - Fog: bind `world.fog_density + fog_add()`. The
//!   renderer's ground-haze term pools the result in
//!   streets and hollows; weather never touches a shader.
//! - Lightning: drain `last_strike` after each tick.
//!   Thunder is NOT a DistrictStimulus — that enum is
//!   closed (Scream / Report / Tremor) and weather does
//!   not edit perception. The strike carries position,
//!   flash, magnitude, and `thunder_delay(ear)`; the
//!   host routes it when a sound backend exists.
//!
//! What weather does NOT do, by design: it does not
//! render rain particles, does not pick strike geography
//! (the host supplies strike points), does not drive the
//! alert budget, and does not read the op engine — the
//! sky is indifferent to the mission, as it should be.

pub mod lightning;
pub mod regime;
pub mod state;

pub use lightning::{LightningStrike, StrikeClock, SOUND_SPEED_M_S};
pub use regime::Regime;
pub use state::{Precip, WeatherState};
