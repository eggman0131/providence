//! Workbench renderer adapter (ADR 0020).
//!
//! Realises [`providence_ports::RendererPort`] — it presents the terrain as a
//! lit 3D surface the Director can move around (issue #8). It depends only on
//! `providence-ports` (the port and its [`TerrainFrame`] snapshot) and
//! `providence-config` ([`providence_config::RenderParams`]); it **never**
//! imports the core, so it can only ever read a derived snapshot, never
//! simulation state (ADR 0020 §1).
//!
//! Three adapters realise [`RendererPort`] (ADR 0020 §2): the on-screen
//! [`WindowRenderer`] (windowed `wgpu`/`winit`), the [`HeadlessRenderer`]
//! (render-to-PNG capture — the agents-only visual self-check), and the
//! GPU-free [`NoopRenderer`] test double. They all draw the same pure geometry
//! ([`mesh`]), camera ([`camera`]), light ([`light`]), and palette ([`color`]),
//! which are unit-tested in the gate; the `wgpu`/`winit` glue is confined here
//! and exercised only through the capture path (I9).

#![forbid(unsafe_code)]
// This adapter does floating-point presentation math: small-magnitude integer
// grid coordinates and heights are cast to `f32` for world-space geometry and
// colour. Those casts are intentional and effectively lossless here, so the
// pedantic precision-loss lint carries no signal for this crate.
#![allow(clippy::cast_precision_loss)]

pub mod camera;
pub mod color;
pub mod context;
pub mod error;
pub mod gpu;
pub mod headless;
pub mod light;
pub mod math;
pub mod mesh;
pub mod window;

pub use error::RendererError;
pub use headless::HeadlessRenderer;
pub use window::WindowRenderer;

use providence_ports::{RendererPort, TerrainFrame};

/// A [`RendererPort`] that draws nothing — the GPU-free test double (ADR 0020
/// §2) for tests and for any run without a display. It records how many frames
/// it has been handed so callers can prove the seam was exercised.
#[derive(Debug, Default)]
pub struct NoopRenderer {
    presented: u64,
}

impl NoopRenderer {
    /// A fresh no-op renderer that has presented nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many frames have been presented so far.
    #[must_use]
    pub fn presented(&self) -> u64 {
        self.presented
    }
}

impl RendererPort for NoopRenderer {
    fn present(&mut self, _frame: TerrainFrame<'_>) {
        self.presented += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::NoopRenderer;
    use providence_ports::{RendererPort, TerrainFrame};

    #[test]
    fn noop_renderer_counts_presents_but_draws_nothing() {
        let heights = [0, 1, 1, 2];
        let frame = TerrainFrame::new(2, 2, &heights);
        let mut renderer = NoopRenderer::new();
        assert_eq!(renderer.presented(), 0);
        renderer.present(frame);
        renderer.present(frame);
        assert_eq!(renderer.presented(), 2, "each present counts one frame");
    }
}
