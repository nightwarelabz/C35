//! Layout: points, anchors, and stacks. Nothing here draws
//! — it only answers "where does this box go", so widgets
//! stay dumb and the host stays in control of content.
//!
//! Units: points. The reference frame is 480pt tall (the
//! PS1.5 internal frame's height); the host scales points
//! to pixels by `frame_height_px / 480`. A screen laid out
//! at 480pt reads identically at 720p and 1080p, only
//! sharper — the layout never re-flows with resolution.

/// A box in point space: (x, y) is the TOP-left corner,
/// y growing downward, like text. Width and height are
/// non-negative by construction of the helpers below.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w: w.max(0.0), h: h.max(0.0) }
    }

    /// Inset by `pad` on every side (negative pad grows).
    pub fn inset(&self, pad: f32) -> Rect {
        Rect::new(self.x + pad, self.y + pad, self.w - 2.0 * pad, self.h - 2.0 * pad)
    }

    /// Shrink to a child box of `size`, placed by `anchor`
    /// inside this rect with `margin` clearance from the
    /// anchored edges.
    pub fn anchor(&self, size: (f32, f32), anchor: Anchor, margin: f32) -> Rect {
        let (w, h) = (size.0.max(0.0), size.1.max(0.0));
        let x = match anchor {
            Anchor::TopLeft | Anchor::CenterLeft | Anchor::BottomLeft => self.x + margin,
            Anchor::TopCenter | Anchor::Center | Anchor::BottomCenter => {
                self.x + (self.w - w) / 2.0
            }
            Anchor::TopRight | Anchor::CenterRight | Anchor::BottomRight => {
                self.x + self.w - w - margin
            }
        };
        let y = match anchor {
            Anchor::TopLeft | Anchor::TopCenter | Anchor::TopRight => self.y + margin,
            Anchor::CenterLeft | Anchor::Center | Anchor::CenterRight => {
                self.y + (self.h - h) / 2.0
            }
            Anchor::BottomLeft | Anchor::BottomCenter | Anchor::BottomRight => {
                self.y + self.h - h - margin
            }
        };
        Rect::new(x, y, w, h)
    }
}

/// Nine anchor points. Corners and edges pin to the frame
/// with a margin; Center ignores the margin.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// A vertical stack: children of given heights, laid top
/// to bottom inside `frame` with `gap` between them. A
/// child taller than the remaining space is clipped to
/// what remains, never pushed off the frame.
pub fn vstack(frame: Rect, heights: &[f32], gap: f32) -> Vec<Rect> {
    let mut out = Vec::with_capacity(heights.len());
    let mut y = frame.y;
    for &h in heights {
        let remain = (frame.y + frame.h - y).max(0.0);
        let hh = h.max(0.0).min(remain);
        out.push(Rect::new(frame.x, y, frame.w, hh));
        y += h.max(0.0) + gap;
    }
    out
}

/// A horizontal stack: same contract, left to right.
pub fn hstack(frame: Rect, widths: &[f32], gap: f32) -> Vec<Rect> {
    let mut out = Vec::with_capacity(widths.len());
    let mut x = frame.x;
    for &w in widths {
        let remain = (frame.x + frame.w - x).max(0.0);
        let ww = w.max(0.0).min(remain);
        out.push(Rect::new(x, frame.y, ww, frame.h));
        x += w.max(0.0) + gap;
    }
    out
}
