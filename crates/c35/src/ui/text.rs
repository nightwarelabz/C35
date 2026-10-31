//! Text measurement and wrapping, over the 5x7 font.
//! Measurement is exact: it counts the same pixels the
//! emitter draws, so a measured box never clips its text.
//!
//! Metrics, in points, for a glyph cell height of `size`:
//! advance per character is `size * 6 / 7` (5 columns plus
//! 1 column of letterspacing), line height is
//! `size * 10 / 7` (7 rows plus 3 rows of leading). A size
//! of 7pt is the native cell: one point per font pixel.

use super::font::glyph_for;

/// Advance per character, in points, for cell height `size`.
pub fn advance(size: f32) -> f32 {
    size * 6.0 / 7.0
}

/// Line height, in points, for cell height `size`.
pub fn line_height(size: f32) -> f32 {
    size * 10.0 / 7.0
}

/// Width of one line (no newlines) in points.
pub fn measure_line(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * advance(size)
}

/// Width and height of possibly multi-line text (split on
/// '\n'), in points. Height counts every line, including
/// a trailing empty line after a final newline.
pub fn measure(text: &str, size: f32) -> (f32, f32) {
    let mut w = 0.0f32;
    let mut lines = 0usize;
    for line in text.split('\n') {
        w = w.max(measure_line(line, size));
        lines += 1;
    }
    (w, lines as f32 * line_height(size))
}

/// Word-wrap `text` to lines no wider than `max_w` points.
/// Words are split on spaces; a word longer than the line
/// is hard-split at the line edge. Explicit newlines force
/// breaks. Empty input yields one empty line, so a wrapped
/// block always has at least one line to place.
pub fn wrap(text: &str, size: f32, max_w: f32) -> Vec<String> {
    let adv = advance(size).max(1.0);
    let per_line = (max_w / adv).floor().max(1.0) as usize;
    let mut lines: Vec<String> = Vec::new();
    for paragraph in text.split('\n') {
        let mut cur = String::new();
        for word in paragraph.split(' ') {
            // Split overlong words first, so the loop below
            // only ever appends words that fit a fresh line.
            let mut rest = word.to_string();
            while rest.chars().count() > per_line {
                let (head, tail) = split_at_chars(&rest, per_line);
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
                lines.push(head);
                rest = tail;
            }
            if cur.is_empty() {
                cur.push_str(&rest);
            } else if cur.chars().count() + 1 + rest.chars().count() <= per_line {
                cur.push(' ');
                cur.push_str(&rest);
            } else {
                lines.push(std::mem::take(&mut cur));
                cur.push_str(&rest);
            }
        }
        lines.push(cur);
    }
    lines
}

fn split_at_chars(s: &str, n: usize) -> (String, String) {
    let mut head = String::new();
    let mut tail = String::new();
    for (i, c) in s.chars().enumerate() {
        if i < n {
            head.push(c);
        } else {
            tail.push(c);
        }
    }
    (head, tail)
}

/// Horizontal runs of set pixels in one line of text, as
/// (x, y, w) point offsets from the line's top-left origin,
/// for cell height `size`. Consecutive set pixels in a row
/// merge into one run, so a solid glyph row costs one quad
/// instead of five — a menu of text drops from ~2000 quads
/// to a few hundred. One point per font pixel at size 7;
/// larger sizes scale each font pixel into a `size / 7`
/// square, so glyphs stay chunky instead of blurring —
/// the PS1.5 text read.
pub fn line_runs(text: &str, size: f32) -> Vec<(f32, f32, f32)> {
    let px = (size / 7.0).max(0.0);
    let mut out = Vec::new();
    for (i, c) in text.chars().enumerate() {
        let g = glyph_for(c);
        let ox = i as f32 * advance(size);
        for (row, grow) in g.iter().enumerate() {
            let mut run_start: Option<u8> = None;
            let mut prev: u8 = 0;
            for col in 0..5 {
                let set = *grow & (1 << (4 - col)) != 0;
                if set && run_start.is_none() {
                    run_start = Some(col);
                }
                if !set {
                    if let Some(start) = run_start {
                        let w = (prev - start + 1) as f32 * px;
                        out.push((ox + start as f32 * px, row as f32 * px, w));
                        run_start = None;
                    }
                }
                prev = col;
            }
            if let Some(start) = run_start {
                let w = (5 - start) as f32 * px;
                out.push((ox + start as f32 * px, row as f32 * px, w));
            }
        }
    }
    out
}

/// Pixel size of one font pixel at cell height `size`.
pub fn pixel_size(size: f32) -> f32 {
    (size / 7.0).max(0.0)
}
