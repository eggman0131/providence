//! Port interfaces (docs/20-architecture.md §2.4).
//!
//! Ports are the trait boundary between the application and its adapters:
//! every side effect crosses one (I2/I4). This crate stays `no_std` and
//! depends on nothing — the DTOs a port hands across are defined *here*, so the
//! interface layer never imports `providence-core` and no adapter does either
//! (ADR 0020 §1). Adding or changing a port is an architectural change and
//! requires an ADR (docs/20-architecture.md §5 rule 5).
//!
//! Realised ports:
//! - [`RendererPort`] — present the terrain as a drawable [`TerrainFrame`]
//!   snapshot; the workbench renderer adapter implements it (ADR 0020). The
//!   remaining ports (`ConfigPort`, `LLMOpponentPort`, …) land with the
//!   subsystems that need them, each behind its own ADR.

#![no_std]
#![forbid(unsafe_code)]

/// A vertex's integer height in a [`TerrainFrame`]. Mirrors the core's `Height`
/// (ADR 0017) as a plain `i32` so `providence-ports` need not — and must not —
/// import `providence-core`: the frame is a *derived snapshot*, not core state.
pub type Height = i32;

/// A read-only, derived snapshot of the terrain height field handed to a
/// [`RendererPort`] to draw (ADR 0020 §1).
///
/// It carries only what a renderer needs — the grid dimensions and a borrow of
/// the row-major heights — and **no** simulation or camera/view state. The
/// application builds one from the core's height field and passes it in; the
/// renderer only ever sees this snapshot, never the core. Row-major: the vertex
/// at `(x, y)` is `heights[y * width + x]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainFrame<'a> {
    width: u32,
    height: u32,
    heights: &'a [Height],
}

impl<'a> TerrainFrame<'a> {
    /// Wrap a row-major height buffer as a drawable snapshot.
    ///
    /// `heights` is expected to be `width * height` long in row-major order;
    /// [`TerrainFrame::get`] bounds-checks every access, so a mismatched buffer
    /// yields `None` rather than a panic.
    #[must_use]
    pub const fn new(width: u32, height: u32, heights: &'a [Height]) -> Self {
        Self {
            width,
            height,
            heights,
        }
    }

    /// Grid width in vertices.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Grid height (depth) in vertices.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The backing row-major height buffer.
    #[must_use]
    pub const fn heights(&self) -> &[Height] {
        self.heights
    }

    /// The height at `(x, y)`, or `None` if the coordinate is out of bounds or
    /// the backing buffer is too short for the stated dimensions.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Option<Height> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = y as usize * self.width as usize + x as usize;
        self.heights.get(index).copied()
    }
}

/// Presents terrain as a drawable surface (ADR 0020 §1).
///
/// The composition root drives this to draw the world. Implementors own their
/// view/camera state — moving the camera is adapter-local and never crosses
/// this boundary (ADR 0020 §3), so nothing a renderer does can mutate the
/// simulation. The on-screen `wgpu`/`winit` renderer, a headless
/// render-to-PNG capture, and a no-op test double all realise it (ADR 0020 §2).
pub trait RendererPort {
    /// Present the given terrain snapshot as the current frame. Called by the
    /// window/redraw loop whenever a fresh frame should be drawn.
    fn present(&mut self, frame: TerrainFrame<'_>);
}
