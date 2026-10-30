//! Widgets and the screen builder. The host builds a frame
//! by calling these in order; every call appends quads to
//! one batch. Nothing here reads game state — the host
//! passes content, selection, and fill fractions, and UI
//! only lays them out and colours them.
//!
//! Point space: the reference frame is 480pt tall. The
//! host constructs [`UiScreen`] with the real frame size
//! in pixels; points scale by `frame_height_px / 480`.
//! Vertices leave as NDC, so the host's UI pass needs no
//! transform of its own: position attribute straight into
//! clip space, colour straight through.

use super::layout::{Anchor, Rect};
use super::mesh::{Color, UiLedger, UiMesh};
use super::text;

/// Horizontal text alignment inside a rect.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// The advance indicator on a dialogue box: whether more
/// lines follow, this is the last line, or the box is a
/// static label with no advance at all.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DialogueCue {
    More,
    End,
    None,
}

/// A screen under construction. Build with the widget
/// calls, then `finish()` for the mesh and its ledger.
pub struct UiScreen {
    frame_w: f32,
    frame_h: f32,
    scale: f32,
    mesh: UiMesh,
}

impl UiScreen {
    /// New screen for a frame of `frame_w_px` x
    /// `frame_h_px` pixels. Reference height is 480pt, so
    /// a 1280x720 frame scales points by 1.5.
    pub fn new(frame_w_px: u32, frame_h_px: u32) -> UiScreen {
        let h = frame_h_px.max(1) as f32;
        UiScreen {
            frame_w: frame_w_px.max(1) as f32,
            frame_h: h,
            scale: h / 480.0,
            mesh: UiMesh::new(),
        }
    }

    /// The frame in points: width follows the aspect, so
    /// a 16:9 frame is 853.33pt wide at the 480pt height.
    pub fn frame(&self) -> Rect {
        Rect::new(0.0, 0.0, self.frame_w / self.scale, 480.0)
    }

    /// Points-to-NDC for one point-space x/y. NDC y is up;
    /// point y grows down, so the flip lives here and
    /// nowhere else.
    fn ndc(&self, x_pt: f32, y_pt: f32) -> [f32; 2] {
        let px = x_pt * self.scale;
        let py = y_pt * self.scale;
        [
            px / self.frame_w * 2.0 - 1.0,
            1.0 - py / self.frame_h * 2.0,
        ]
    }

    /// Append a point-space rect as one quad. The rect's
    /// (x, y) is its TOP-left corner; conversion flips y.
    pub fn rect_pt(&mut self, r: Rect, color: Color) {
        let bl = self.ndc(r.x, r.y + r.h);
        let br = self.ndc(r.x + r.w, r.y + r.h);
        let tr = self.ndc(r.x + r.w, r.y);
        let tl = self.ndc(r.x, r.y);
        self.mesh.quad(bl, br, tr, tl, color);
    }

    /// A filled panel with a border: one fill quad plus
    /// four border quads of `border_pt` thickness. A zero
    /// border draws fill only; a zero-size rect draws
    /// nothing (the mesh drops degenerate quads).
    pub fn panel(&mut self, r: Rect, fill: Color, border: Color, border_pt: f32) {
        self.rect_pt(r, fill);
        let b = border_pt.max(0.0);
        if b > 0.0 {
            // Top, bottom, left, right — corners covered by
            // the horizontal strips, so no double-draw.
            self.rect_pt(Rect::new(r.x, r.y, r.w, b), border);
            self.rect_pt(Rect::new(r.x, r.y + r.h - b, r.w, b), border);
            self.rect_pt(Rect::new(r.x, r.y + b, b, (r.h - 2.0 * b).max(0.0)), border);
            self.rect_pt(
                Rect::new(r.x + r.w - b, r.y + b, b, (r.h - 2.0 * b).max(0.0)),
                border,
            );
        }
    }

    /// One line of text at a point-space origin (top-left
    /// of the glyph cells). No wrapping — use
    /// [`UiScreen::text_wrapped`] for blocks. Each run of
    /// set pixels in a glyph row is one quad (see
    /// [`text::line_runs`]), so text costs runs, not pixels.
    pub fn text_at(&mut self, text: &str, x_pt: f32, y_pt: f32, size: f32, color: Color) {
        let px = text::pixel_size(size);
        if px <= 0.0 {
            return;
        }
        for (ox, oy, w) in text::line_runs(text, size) {
            self.rect_pt(Rect::new(x_pt + ox, y_pt + oy, w, px), color);
        }
    }

    /// One line of text, aligned inside `r` by `align`
    /// and vertically centred on the rect's height.
    pub fn text_in(&mut self, text: &str, r: Rect, size: f32, color: Color, align: Align) {
        let w = text::measure_line(text, size);
        let x = match align {
            Align::Left => r.x,
            Align::Center => r.x + (r.w - w) / 2.0,
            Align::Right => r.x + r.w - w,
        };
        let y = r.y + ((r.h - size) / 2.0).max(0.0);
        self.text_at(text, x, y, size, color);
    }

    /// A wrapped text block inside `r`, top-aligned, with
    /// `size`-cell text. Lines that would fall below the
    /// rect are clipped, never drawn outside the box.
    /// Returns the lines drawn, so the host can size a
    /// box around them afterwards.
    pub fn text_wrapped(&mut self, text: &str, r: Rect, size: f32, color: Color) -> Vec<String> {
        let lines = text::wrap(text, size, r.w);
        let lh = text::line_height(size);
        let mut drawn = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let y = r.y + i as f32 * lh;
            if y + lh > r.y + r.h {
                break;
            }
            self.text_at(line, r.x, y, size, color);
            drawn.push(line.clone());
        }
        drawn
    }

    /// A bar: track quad, fill quad at `frac` (clamped to
    /// 0..1), optional border via `panel`. The host owns
    /// what the fraction means; UI only draws it.
    pub fn bar(&mut self, r: Rect, frac: f32, track: Color, fill: Color, border: Color, border_pt: f32) {
        self.panel(r, track, border, border_pt);
        let f = frac.clamp(0.0, 1.0);
        if f > 0.0 {
            let inner = r.inset(border_pt.max(0.0));
            self.rect_pt(Rect::new(inner.x, inner.y, inner.w * f, inner.h), fill);
        }
    }

    /// A selectable list: one row per string, each
    /// `row_h` tall, stacked from `r`'s top. The selected
    /// row gets a highlight quad behind its text and the
    /// selected colour; other rows get the plain colour.
    /// A marker glyph (">") marks the selected row. Rows
    /// past the rect's bottom are clipped, never drawn
    /// off-frame.
    pub fn list(
        &mut self,
        rows: &[&str],
        selected: usize,
        r: Rect,
        row_h: f32,
        size: f32,
        plain: Color,
        selected_color: Color,
        highlight: Color,
    ) {
        for (i, row) in rows.iter().enumerate() {
            let y = r.y + i as f32 * row_h;
            if y + row_h > r.y + r.h {
                break;
            }
            let row_rect = Rect::new(r.x, y, r.w, row_h);
            if i == selected {
                self.rect_pt(row_rect, highlight);
                self.text_in("> ", row_rect, size, selected_color, Align::Left);
                let marker_w = text::measure_line("> ", size);
                let text_rect = Rect::new(r.x + marker_w, y, r.w - marker_w, row_h);
                self.text_in(row, text_rect, size, selected_color, Align::Left);
            } else {
                self.text_in(row, row_rect, size, plain, Align::Left);
            }
        }
    }

    /// A dialogue box: panel, speaker tag at the top-left
    /// (uppercase as given), wrapped body text, and an
    /// advance indicator at the bottom-right — a downward
    /// triangle for [`DialogueCue::More`], a rightward
    /// chevron for [`DialogueCue::End`], nothing for
    /// [`DialogueCue::None`]. The dialogue SYSTEM fills
    /// this box later; UI only provides the box.
    pub fn dialogue(
        &mut self,
        speaker: &str,
        body: &str,
        cue: DialogueCue,
        r: Rect,
        fill: Color,
        border: Color,
        border_pt: f32,
        speaker_color: Color,
        body_color: Color,
    ) {
        self.panel(r, fill, border, border_pt);
        let pad = 12.0f32;
        let inner = r.inset(pad);
        let tag_size = 14.0f32;
        self.text_at(speaker, inner.x, inner.y, tag_size, speaker_color);
        let body_rect = Rect::new(
            inner.x,
            inner.y + tag_size + 10.0,
            inner.w,
            (inner.h - tag_size - 10.0).max(0.0),
        );
        self.text_wrapped(body, body_rect, 16.0, body_color);
        match cue {
            DialogueCue::More => {
                // Downward triangle, 10pt wide, bottom-right.
                let cx = r.x + r.w - pad - 10.0;
                let cy = r.y + r.h - pad - 8.0;
                self.rect_pt(Rect::new(cx, cy, 10.0, 2.0), speaker_color);
                self.rect_pt(Rect::new(cx + 2.0, cy + 2.0, 6.0, 2.0), speaker_color);
                self.rect_pt(Rect::new(cx + 4.0, cy + 4.0, 2.0, 2.0), speaker_color);
            }
            DialogueCue::End => {
                // Rightward chevron, bottom-right.
                let cx = r.x + r.w - pad - 10.0;
                let cy = r.y + r.h - pad - 8.0;
                self.rect_pt(Rect::new(cx, cy, 2.0, 8.0), speaker_color);
                self.rect_pt(Rect::new(cx + 3.0, cy + 2.0, 2.0, 4.0), speaker_color);
                self.rect_pt(Rect::new(cx + 6.0, cy + 3.0, 2.0, 2.0), speaker_color);
            }
            DialogueCue::None => {}
        }
    }

    /// Anchor a box of `size` inside the frame. Shorthand
    /// for `frame().anchor(...)`, so hosts rarely touch
    /// point arithmetic at all.
    pub fn anchor(&self, size: (f32, f32), anchor: Anchor, margin: f32) -> Rect {
        self.frame().anchor(size, anchor, margin)
    }

    /// Finish: the batched mesh and its ledger. The mesh
    /// is one indexed draw in the host's UI pass.
    pub fn finish(self) -> (UiMesh, UiLedger) {
        let ledger = self.mesh.ledger();
        (self.mesh, ledger)
    }
}
