//! UI/text: text, layout, and widgets for menus, HUD
//! chrome, and dialogue boxes — game-logic-free.
//!
//! The host passes content; UI lays it out and emits
//! geometry. The host's UI pass draws that geometry in
//! one indexed draw. The engine owns the scene pass; the
//! host owns the UI pass; [`UiMesh`] is the seam.
//!
//! # Space
//!
//! Layout happens in points. The reference frame is 480pt
//! tall (the PS1.5 internal frame's height); points scale
//! to pixels by `frame_height_px / 480`, so a screen laid
//! out once reads identically at 720p and 1080p, only
//! sharper. Vertices leave as NDC (y up), so the host pass
//! needs no transform: position straight into clip space,
//! colour straight through.
//!
//! # Text
//!
//! Glyphs come from a 5x7 dot-matrix defined as data in
//! [`font`] — no font file, no image asset, no borrowed
//! table. Each run of set pixels in a glyph row becomes
//! one solid quad, so text costs runs, not pixels. Sizes
//! scale font pixels into squares instead of blurring
//! them: the PS1.5 read.
//! Measurement counts the same pixels the emitter draws,
//! so a measured box never clips its text.
//!
//! # What UI is not
//!
//! UI holds no game state: no health, no alert tiers, no
//! damage numbers, no mission logic. A bar's fill fraction,
//! a list's selection, a dialogue box's lines are all host
//! decisions passed in. The dialogue SYSTEM (lines, cues,
//! advance input) comes later; UI provides the box it
//! will fill. Text input, caret editing, scrolling lists,
//! and animation are deliberately out of scope — they
//! need input ownership and time, which belong to the
//! host and the (future) dialogue/input systems, not to
//! a layout module.
//!
//! # Host API — five calls to a menu on screen
//!
//! 1. `UiScreen::new(frame_w_px, frame_h_px)` — the frame.
//! 2. `screen.panel(rect, fill, border, border_pt)` — boxes.
//! 3. `screen.text_in(text, rect, size, color, align)` —
//!    labels, or `text_wrapped` for blocks.
//! 4. `screen.list(rows, selected, rect, row_h, size, …)`
//!    — menus. `screen.bar(rect, frac, …)` — meters.
//! 5. `screen.finish()` — the [`UiMesh`] and [`UiLedger`].
//!    Draw the mesh in one indexed draw; `draws` is 1.
//!
//! # Ledger
//!
//! Every screen reports quads, triangles, vertices,
//! indices, and draws. A full menu frame is a few hundred
//! quads and one draw call — negligible next to the
//! scene. There is no new engine render pass: the UI pass
//! is host-owned (the demo carries one as scaffolding).

pub mod font;
pub mod layout;
pub mod mesh;
pub mod text;
pub mod widgets;

pub use layout::{hstack, vstack, Anchor, Rect};
pub use mesh::{Color, UiLedger, UiMesh, UiVertex};
pub use text::{advance, line_height, measure, measure_line, wrap};
pub use widgets::{Align, DialogueCue, UiScreen};
