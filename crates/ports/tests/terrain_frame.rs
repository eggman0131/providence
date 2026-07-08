//! Contract tests for the `RendererPort` seam (ADR 0020, grown per ADR 0023):
//! the `TerrainFrame` snapshot behaves as a bounds-safe, read-only view over
//! heights, per-vertex terrain types, and the waterline datum, and the port is
//! implementable by a trivial test double. Integration tests live outside the
//! `no_std` crate, so they use `std` freely.

use providence_ports::{RendererPort, TerrainFrame, TerrainType};

/// A 2×3 field, row-major: row y is `[y*10 .. ]`.
const HEIGHTS: [i32; 6] = [0, 1, 10, 11, 20, 21];

/// The sea-level datum the frames carry (ADR 0023, Phase 2). A concrete,
/// non-trivial value so a verbatim read is unambiguous.
const WATERLINE: i32 = 5;

/// The per-vertex terrain types the app would derive for [`HEIGHTS`], row-major
/// (ADR 0023). Concrete values so the type reads are legible; the real
/// classification is the core's (`classify_vertex`).
const TYPES: [TerrainType; 6] = [
    TerrainType::Water,
    TerrainType::Shore,
    TerrainType::Land,
    TerrainType::Land,
    TerrainType::Mountain,
    TerrainType::Mountain,
];

#[test]
fn dimensions_and_buffers_are_reported_verbatim() {
    let frame = TerrainFrame::new(2, 3, &HEIGHTS, &TYPES, WATERLINE);
    assert_eq!(frame.width(), 2);
    assert_eq!(frame.height(), 3);
    assert_eq!(frame.heights(), &HEIGHTS);
    assert_eq!(frame.types(), &TYPES);
    assert_eq!(
        frame.waterline(),
        WATERLINE,
        "the sea-level datum is carried"
    );
}

#[test]
fn get_reads_row_major_in_bounds() {
    let frame = TerrainFrame::new(2, 3, &HEIGHTS, &TYPES, WATERLINE);
    assert_eq!(frame.get(0, 0), Some(0));
    assert_eq!(frame.get(1, 0), Some(1));
    assert_eq!(frame.get(0, 1), Some(10));
    assert_eq!(frame.get(1, 2), Some(21), "last vertex, row-major");
}

#[test]
fn type_at_reads_row_major_in_bounds() {
    let frame = TerrainFrame::new(2, 3, &HEIGHTS, &TYPES, WATERLINE);
    assert_eq!(frame.type_at(0, 0), Some(TerrainType::Water));
    assert_eq!(frame.type_at(1, 0), Some(TerrainType::Shore));
    assert_eq!(
        frame.type_at(1, 2),
        Some(TerrainType::Mountain),
        "last vertex"
    );
}

#[test]
fn get_and_type_at_are_none_past_either_edge() {
    let frame = TerrainFrame::new(2, 3, &HEIGHTS, &TYPES, WATERLINE);
    assert_eq!(frame.get(2, 0), None, "x past the right edge");
    assert_eq!(frame.get(0, 3), None, "y past the bottom edge");
    assert_eq!(frame.type_at(2, 0), None, "type past the right edge");
    assert_eq!(frame.type_at(0, 3), None, "type past the bottom edge");
}

#[test]
fn reads_are_none_when_a_buffer_is_too_short_for_the_dimensions() {
    // A frame whose stated size exceeds its buffer never panics — it reports
    // the missing vertex as absent (bounds-safe by construction).
    let short = [0, 1, 2];
    let frame = TerrainFrame::new(2, 3, &short, &TYPES, WATERLINE);
    assert_eq!(frame.get(1, 2), None, "index 5 is past a 3-long buffer");
    assert_eq!(frame.get(0, 0), Some(0), "present vertices still read");
}

#[test]
fn a_heights_only_frame_has_no_types() {
    // A frame built only to read heights (e.g. the picking snapshot) passes an
    // empty `types` slice; `type_at` then yields `None` everywhere (ADR 0023).
    let frame = TerrainFrame::new(2, 3, &HEIGHTS, &[], WATERLINE);
    assert_eq!(frame.get(0, 0), Some(0), "heights still read");
    assert!(frame.types().is_empty());
    assert_eq!(frame.type_at(0, 0), None, "no type without a types buffer");
}

#[test]
fn frame_is_a_cheap_copyable_value() {
    let frame = TerrainFrame::new(2, 3, &HEIGHTS, &TYPES, WATERLINE);
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
    renderer.present(TerrainFrame::new(2, 3, &HEIGHTS, &TYPES, WATERLINE));
    renderer.present(TerrainFrame::new(2, 3, &HEIGHTS, &TYPES, WATERLINE));
    assert_eq!(renderer.presented, 2, "each call presents one frame");
    assert_eq!(renderer.last_dims, Some((2, 3)));
}
