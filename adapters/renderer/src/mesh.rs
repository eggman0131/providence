//! Terrain surface geometry (issue #8 Phase 1 mesh).
//!
//! Pure and GPU-free. Builds the **flat-shaded stepped mesh** — per-face
//! vertices with per-face normals, so each integer step reads as a crisp facet
//! (issue #8 decision) — from a read-only [`TerrainFrame`] snapshot. The
//! coordinate kernel maps a vertex `(x, y, height)` to a centred world-space
//! position; [`build_mesh`] triangulates the grid into an unshared-vertex
//! triangle list ready to upload to the GPU.
//!
//! `vertical_scale` (how tall one integer height step is, in world units) is a
//! caller-supplied parameter here, not a constant: the renderer sources it from
//! the `render.mesh.vertical_scale` key (ADR 0020 §4).

use providence_config::PaletteParams;
use providence_ports::TerrainFrame;

use crate::color::height_color;
use crate::math::{cross, normalize, sub};

/// A world-space position, `[x, y, z]`, with y up.
pub type Position = [f32; 3];

/// A GPU-ready mesh vertex: a centred world position, the flat per-face normal
/// it belongs to, and its height colour. Vertices are **not** shared between
/// faces (each facet carries its own three), which is what makes the stepped
/// surface read crisply rather than smoothing across the steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    /// Centred world-space position, y up.
    pub position: Position,
    /// The flat normal of the face this vertex belongs to (unit, oriented up).
    pub normal: [f32; 3],
    /// Linear-RGB colour from the vertex's height (see [`height_color`]).
    pub color: [f32; 3],
}

/// The terrain surface as a flat triangle list (three vertices per triangle,
/// none shared) — the geometry the renderer uploads and draws (ADR 0020 §1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    /// Triangle-list vertices; `vertices.len()` is a multiple of three.
    pub vertices: Vec<Vertex>,
}

impl Mesh {
    /// Number of triangles in the mesh.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.vertices.len() / 3
    }

    /// Whether the mesh has no geometry (a frame smaller than one cell).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

/// Build the flat-shaded stepped surface mesh for `frame`.
///
/// Every 1×1 grid cell whose four corners are present becomes two triangles;
/// each triangle gets a single face normal (oriented upward so lighting reads
/// the top of the land regardless of winding) shared by its three unshared
/// vertices. Vertices are coloured by their own height across the frame's drawn
/// `[min, max]` range, so the height gradient shows through the facets. A frame
/// with fewer than two rows or columns yields an empty mesh (nothing to face).
#[must_use]
pub fn build_mesh(frame: &TerrainFrame<'_>, vertical_scale: f32, palette: &PaletteParams) -> Mesh {
    let (width, depth) = (frame.width(), frame.height());
    let (min_height, max_height) = height_bounds(frame);
    let mut vertices = Vec::new();

    for y in 0..depth.saturating_sub(1) {
        for x in 0..width.saturating_sub(1) {
            let (Some(h00), Some(h10), Some(h01), Some(h11)) = (
                frame.get(x, y),
                frame.get(x + 1, y),
                frame.get(x, y + 1),
                frame.get(x + 1, y + 1),
            ) else {
                continue; // a well-formed frame skips no cell
            };
            let c00 = Corner::new(x, y, h00, width, depth, vertical_scale);
            let c10 = Corner::new(x + 1, y, h10, width, depth, vertical_scale);
            let c01 = Corner::new(x, y + 1, h01, width, depth, vertical_scale);
            let c11 = Corner::new(x + 1, y + 1, h11, width, depth, vertical_scale);

            push_triangle(
                &mut vertices,
                [c00, c10, c11],
                min_height,
                max_height,
                palette,
            );
            push_triangle(
                &mut vertices,
                [c00, c11, c01],
                min_height,
                max_height,
                palette,
            );
        }
    }

    Mesh { vertices }
}

/// A cell corner during meshing: its world position and the integer height it
/// carries (kept so the vertex can be coloured by its own height).
#[derive(Clone, Copy)]
struct Corner {
    position: Position,
    height: i32,
}

impl Corner {
    fn new(x: u32, y: u32, height: i32, width: u32, depth: u32, vertical_scale: f32) -> Self {
        Self {
            position: vertex_position(x, y, height, width, depth, vertical_scale),
            height,
        }
    }
}

/// Emit one triangle: compute its flat face normal (oriented so it points up),
/// then push its three vertices, each coloured by its own height.
fn push_triangle(
    out: &mut Vec<Vertex>,
    corners: [Corner; 3],
    min_height: i32,
    max_height: i32,
    palette: &PaletteParams,
) {
    let normal = face_normal(
        corners[0].position,
        corners[1].position,
        corners[2].position,
    );
    for corner in corners {
        out.push(Vertex {
            position: corner.position,
            normal,
            color: height_color(corner.height, min_height, max_height, palette),
        });
    }
}

/// The unit normal of triangle `a→b→c`, flipped if needed so it points up
/// (`+y`). Orienting upward means the single directional light always shades
/// the visible top face, independent of triangle winding.
fn face_normal(a: Position, b: Position, c: Position) -> [f32; 3] {
    let normal = normalize(cross(sub(b, a), sub(c, a)));
    if normal[1] < 0.0 {
        [-normal[0], -normal[1], -normal[2]]
    } else {
        normal
    }
}

/// The lowest and highest heights present in `frame`, used to normalise the
/// height→colour ramp. An empty frame collapses to `(0, 0)`.
fn height_bounds(frame: &TerrainFrame<'_>) -> (i32, i32) {
    let mut bounds: Option<(i32, i32)> = None;
    for &height in frame.heights() {
        bounds = Some(match bounds {
            Some((lo, hi)) => (lo.min(height), hi.max(height)),
            None => (height, height),
        });
    }
    bounds.unwrap_or((0, 0))
}

/// World-space position of grid vertex `(x, y)` at `height`.
///
/// The grid lies on the world x/z plane and is **centred** on the origin so the
/// camera orbits the middle of the map; height becomes the up (y) axis, scaled
/// by `vertical_scale`.
#[must_use]
pub fn vertex_position(
    x: u32,
    y: u32,
    height: i32,
    width: u32,
    depth: u32,
    vertical_scale: f32,
) -> Position {
    [
        center_offset(x, width),
        height as f32 * vertical_scale,
        center_offset(y, depth),
    ]
}

/// Offset of index `i` from the centre of a `size`-wide axis, in units of one
/// vertex spacing, so a `size`-wide grid spans `[-(size-1)/2, (size-1)/2]`.
fn center_offset(i: u32, size: u32) -> f32 {
    i as f32 - (size.saturating_sub(1) as f32) / 2.0
}

/// The centred world positions of every vertex in `frame`, row-major — the
/// vertex grid the Phase-1 mesh builder facets. A vertex missing from the
/// snapshot is skipped (a well-formed frame skips none).
#[must_use]
pub fn vertex_positions(frame: &TerrainFrame<'_>, vertical_scale: f32) -> Vec<Position> {
    let (width, depth) = (frame.width(), frame.height());
    let mut positions = Vec::with_capacity(width as usize * depth as usize);
    for y in 0..depth {
        for x in 0..width {
            if let Some(height) = frame.get(x, y) {
                positions.push(vertex_position(x, y, height, width, depth, vertical_scale));
            }
        }
    }
    positions
}

#[cfg(test)]
mod tests {
    use super::{build_mesh, center_offset, height_bounds, vertex_position, vertex_positions};
    use providence_config::PaletteParams;
    use providence_ports::TerrainFrame;

    const PALETTE: PaletteParams = PaletteParams {
        low_rgb: [0.0, 0.0, 0.0],
        high_rgb: [1.0, 1.0, 1.0],
    };

    /// Floats compared within a tolerance (clippy forbids `==` on floats).
    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-5
    }

    /// Element-wise [`approx`] for a world-space position.
    fn approx3(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= 1e-5)
    }

    #[test]
    fn an_odd_axis_is_centred_on_the_origin() {
        assert!(approx(center_offset(1, 3), 0.0), "middle of a 3-wide axis");
        assert!(approx(center_offset(0, 3), -1.0));
        assert!(approx(center_offset(2, 3), 1.0));
    }

    #[test]
    fn height_becomes_the_scaled_up_axis() {
        let pos = vertex_position(1, 1, 4, 3, 3, 0.5);
        assert!(
            approx3(pos, [0.0, 2.0, 0.0]),
            "centre vertex, height 4 × scale 0.5"
        );
    }

    #[test]
    fn positions_cover_every_vertex_row_major() {
        let heights = [0, 1, 1, 2]; // 2×2
        let frame = TerrainFrame::new(2, 2, &heights);
        let positions = vertex_positions(&frame, 1.0);
        assert_eq!(positions.len(), 4);
        assert!(approx(positions[0][1], 0.0), "(0,0) height 0 → y 0");
        assert!(approx(positions[3][1], 2.0), "(1,1) height 2 → y 2");
    }

    #[test]
    fn a_grid_meshes_into_two_triangles_per_cell() {
        // 3×3 vertices → 2×2 cells → 4 cells × 2 triangles × 3 vertices = 24.
        let heights = [0; 9];
        let frame = TerrainFrame::new(3, 3, &heights);
        let mesh = build_mesh(&frame, 1.0, &PALETTE);
        assert_eq!(mesh.triangle_count(), 8);
        assert_eq!(mesh.vertices.len(), 24);
        assert!(!mesh.is_empty());
    }

    #[test]
    fn a_frame_too_small_for_a_cell_yields_no_geometry() {
        let heights = [3, 4];
        let frame = TerrainFrame::new(2, 1, &heights); // one row: no complete cell
        assert!(build_mesh(&frame, 1.0, &PALETTE).is_empty());
    }

    #[test]
    fn a_flat_field_has_upward_unit_normals() {
        let heights = [5; 4]; // 2×2, all equal → the surface is level
        let frame = TerrainFrame::new(2, 2, &heights);
        let mesh = build_mesh(&frame, 1.0, &PALETTE);
        for vertex in &mesh.vertices {
            assert!(
                approx3(vertex.normal, [0.0, 1.0, 0.0]),
                "flat land faces up"
            );
        }
    }

    #[test]
    fn every_face_normal_is_oriented_upward_and_unit_length() {
        // A tilted surface: normals must still point up (positive y) and be
        // unit length, so lighting reads the top face whatever the winding.
        let heights = [0, 1, 1, 2];
        let frame = TerrainFrame::new(2, 2, &heights);
        let mesh = build_mesh(&frame, 1.0, &PALETTE);
        for vertex in &mesh.vertices {
            let [nx, ny, nz] = vertex.normal;
            assert!(ny > 0.0, "normal points up");
            assert!(approx((nx * nx + ny * ny + nz * nz).sqrt(), 1.0), "unit");
        }
    }

    #[test]
    fn vertices_are_coloured_across_the_height_range() {
        // Lowest vertex → low palette anchor, highest → high anchor.
        let heights = [0, 0, 0, 10];
        let frame = TerrainFrame::new(2, 2, &heights);
        let mesh = build_mesh(&frame, 1.0, &PALETTE);
        assert!(
            mesh.vertices
                .iter()
                .any(|v| approx3(v.color, [0.0, 0.0, 0.0])),
            "the height-0 vertices take the low anchor"
        );
        assert!(
            mesh.vertices
                .iter()
                .any(|v| approx3(v.color, [1.0, 1.0, 1.0])),
            "the height-10 vertex takes the high anchor"
        );
    }

    #[test]
    fn height_bounds_span_the_frame() {
        let heights = [-2, 0, 5, 3];
        let frame = TerrainFrame::new(2, 2, &heights);
        assert_eq!(height_bounds(&frame), (-2, 5));
        let empty = TerrainFrame::new(0, 0, &[]);
        assert_eq!(height_bounds(&empty), (0, 0), "an empty frame collapses");
    }
}
