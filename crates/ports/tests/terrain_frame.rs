//! Contract tests for the `RendererPort` seam (ADR 0020): the `TerrainFrame`
//! snapshot behaves as a bounds-safe, read-only view, and the port is
//! implementable by a trivial test double. Integration tests live outside the
//! `no_std` crate, so they use `std` freely.

use providence_ports::{RendererPort, TerrainFrame};

/// A 2×3 field, row-major: row y is `[y*10 .. ]`.
const HEIGHTS: [i32; 6] = [0, 1, 10, 11, 20, 21];

#[test]
fn dimensions_and_buffer_are_reported_verbatim() {
    let frame = TerrainFrame::new(2, 3, &HEIGHTS);
    assert_eq!(frame.width(), 2);
    assert_eq!(frame.height(), 3);
    assert_eq!(frame.heights(), &HEIGHTS);
}

#[test]
fn get_reads_row_major_in_bounds() {
    let frame = TerrainFrame::new(2, 3, &HEIGHTS);
    assert_eq!(frame.get(0, 0), Some(0));
    assert_eq!(frame.get(1, 0), Some(1));
    assert_eq!(frame.get(0, 1), Some(10));
    assert_eq!(frame.get(1, 2), Some(21), "last vertex, row-major");
}

#[test]
fn get_is_none_past_either_edge() {
    let frame = TerrainFrame::new(2, 3, &HEIGHTS);
    assert_eq!(frame.get(2, 0), None, "x past the right edge");
    assert_eq!(frame.get(0, 3), None, "y past the bottom edge");
}

#[test]
fn get_is_none_when_the_buffer_is_too_short_for_the_dimensions() {
    // A frame whose stated size exceeds its buffer never panics — it reports
    // the missing vertex as absent (bounds-safe by construction).
    let short = [0, 1, 2];
    let frame = TerrainFrame::new(2, 3, &short);
    assert_eq!(frame.get(1, 2), None, "index 5 is past a 3-long buffer");
    assert_eq!(frame.get(0, 0), Some(0), "present vertices still read");
}

#[test]
fn frame_is_a_cheap_copyable_value() {
    let frame = TerrainFrame::new(2, 3, &HEIGHTS);
    let copy = frame; // Copy, not move
    assert_eq!(frame, copy, "PartialEq over the borrowed snapshot");
    assert!(
        format!("{frame:?}").contains("TerrainFrame"),
        "Debug renders"
    );
}

/// A minimal `RendererPort` test double, proving the contract is implementable
/// and that `present` dispatches — the shape the no-op/headless adapters take.
#[derive(Default)]
struct CountingRenderer {
    presented: u32,
    last_dims: Option<(u32, u32)>,
}

impl RendererPort for CountingRenderer {
    fn present(&mut self, frame: TerrainFrame<'_>) {
        self.presented += 1;
        self.last_dims = Some((frame.width(), frame.height()));
    }
}

#[test]
fn a_renderer_port_can_be_implemented_and_driven() {
    let mut renderer = CountingRenderer::default();
    renderer.present(TerrainFrame::new(2, 3, &HEIGHTS));
    renderer.present(TerrainFrame::new(2, 3, &HEIGHTS));
    assert_eq!(renderer.presented, 2, "each call presents one frame");
    assert_eq!(renderer.last_dims, Some((2, 3)));
}
