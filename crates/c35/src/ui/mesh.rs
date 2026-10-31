//! The UI mesh: screen-space quads a host pass draws in one
//! indexed draw. No texture, no atlas, no lighting — every quad
//! is a solid colour, which is the whole PS1.5 UI read: flat
//! panels, chunky glyphs, hard selection bars.
//!
//! Coordinates are NDC (-1..1, y up), converted from the
//! host's point space by [`UiScreen`]. The host's UI pass is
//! trivial by design: one pipeline, one vertex buffer, one
//! indexed draw call. The engine owns the scene pass; the
//! host owns the UI pass; this mesh is the seam between them.
//!
//! Determinism: vertices are pure f32 arithmetic over the
//! host's inputs, emitted in widget order, with no hash
//! iteration and no randomness. The same screen built twice
//! yields byte-identical vertex and index buffers.

/// One UI vertex: NDC position + RGBA colour. Colours are
/// used as-is by the host pass (the demo writes into an
/// sRGB target, so demo colours are authored in sRGB).
/// Plain floats throughout, so the vertex buffer uploads
/// with a single `bytemuck::cast_slice`.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

/// A colour, as four floats. Hosts author these; the engine
/// never invents a palette — game colour is game content.
pub type Color = [f32; 4];

/// The batched UI geometry for one frame, plus its cost.
#[derive(Default, Clone, Debug)]
pub struct UiMesh {
    pub vertices: Vec<UiVertex>,
    pub indices: Vec<u32>,
}

impl UiMesh {
    pub fn new() -> UiMesh {
        UiMesh::default()
    }

    /// Append one solid quad. Corners are given in NDC, in
    /// counter-clockwise order starting at the bottom-left:
    /// (bl, br, tr, tl). Degenerate quads (zero area) are
    /// dropped — a zero-size widget costs nothing and
    /// draws nothing, instead of smearing a pixel.
    pub fn quad(&mut self, bl: [f32; 2], br: [f32; 2], tr: [f32; 2], tl: [f32; 2], color: Color) {
        if bl == br || bl == tl {
            return;
        }
        let base = self.vertices.len() as u32;
        self.vertices.push(UiVertex {
            position: bl,
            color,
        });
        self.vertices.push(UiVertex {
            position: br,
            color,
        });
        self.vertices.push(UiVertex {
            position: tr,
            color,
        });
        self.vertices.push(UiVertex {
            position: tl,
            color,
        });
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Append an axis-aligned NDC rect: (x0, y0) bottom-left
    /// corner, (x1, y1) top-right corner.
    pub fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Color) {
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        self.quad([x0, y0], [x1, y0], [x1, y1], [x0, y1], color);
    }

    pub fn quad_count(&self) -> usize {
        self.indices.len() / 6
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Cost of this mesh, in the engine's ledger shape.
    /// `draws` is always 1: the whole mesh is one indexed
    /// draw in the host's UI pass, whatever it contains.
    pub fn ledger(&self) -> UiLedger {
        UiLedger {
            quads: self.quad_count(),
            triangles: self.triangle_count(),
            vertices: self.vertices.len(),
            indices: self.indices.len(),
            draws: 1,
        }
    }
}

/// Per-frame UI cost, reported by every screen build.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct UiLedger {
    pub quads: usize,
    pub triangles: usize,
    pub vertices: usize,
    pub indices: usize,
    pub draws: usize,
}
