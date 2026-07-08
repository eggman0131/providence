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

use providence_config::MaterialParams;
use providence_ports::{TerrainFrame, TerrainType};

use crate::color::material_color;
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
    /// Linear-RGB colour from the vertex's terrain type and height (see
    /// [`material_color`]).
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
/// vertices. Vertices are coloured by their derived terrain **type** from the
/// `material` table (ADR 0023) — sand/grass/rock, mountains ramping to snow
/// across the frame's drawn mountain-height range — so the material bands trace
/// the terrain-type (and thus the step) boundaries. A frame built to draw
/// carries a full `types` slice; a well-formed frame skips no cell. A frame with
/// fewer than two rows or columns yields an empty mesh (nothing to face).
#[must_use]
pub fn build_mesh(
    frame: &TerrainFrame<'_>,
    vertical_scale: f32,
    material: &MaterialParams,
) -> Mesh {
    let (width, depth) = (frame.width(), frame.height());
    let (mountain_lo, mountain_hi) = mountain_bounds(frame);
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
            let c00 = Corner::new(x, y, h00, frame, vertical_scale);
            let c10 = Corner::new(x + 1, y, h10, frame, vertical_scale);
            let c01 = Corner::new(x, y + 1, h01, frame, vertical_scale);
            let c11 = Corner::new(x + 1, y + 1, h11, frame, vertical_scale);

            push_triangle(
                &mut vertices,
                [c00, c10, c11],
                mountain_lo,
                mountain_hi,
                material,
            );
            push_triangle(
                &mut vertices,
                [c00, c11, c01],
                mountain_lo,
                mountain_hi,
                material,
            );
        }
    }

    Mesh { vertices }
}

/// A cell corner during meshing: its world position, the integer height it
/// carries, and its derived terrain type — kept so the vertex can be coloured by
/// the material table. A vertex whose type is absent (only a heights-only frame,
/// never drawn) falls back to [`TerrainType::Land`].
#[derive(Clone, Copy)]
struct Corner {
    position: Position,
    height: i32,
    kind: TerrainType,
}

impl Corner {
    fn new(x: u32, y: u32, height: i32, frame: &TerrainFrame<'_>, vertical_scale: f32) -> Self {
        Self {
            position: vertex_position(x, y, height, frame.width(), frame.height(), vertical_scale),
            height,
            kind: frame.type_at(x, y).unwrap_or(TerrainType::Land),
        }
    }
}

/// Emit one triangle: compute its flat face normal (oriented so it points up),
/// then push its three vertices, each coloured by its terrain type (mountains
/// ramping across the frame's drawn mountain-height range `[lo, hi]`).
fn push_triangle(
    out: &mut Vec<Vertex>,
    corners: [Corner; 3],
    mountain_lo: i32,
    mountain_hi: i32,
    material: &MaterialParams,
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
            color: material_color(
                corner.kind,
                corner.height,
                mountain_lo,
                mountain_hi,
                material,
            ),
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

/// The lowest and highest heights among the frame's **mountain** vertices — the
/// drawn range the rock→snow ramp normalises over (ADR 0023). A frame with no
/// mountains collapses to `(0, 0)`; since only mountain vertices consult this
/// range, that fallback is never actually sampled.
fn mountain_bounds(frame: &TerrainFrame<'_>) -> (i32, i32) {
    let (width, depth) = (frame.width(), frame.height());
    let mut bounds: Option<(i32, i32)> = None;
    for y in 0..depth {
        for x in 0..width {
            if frame.type_at(x, y) != Some(TerrainType::Mountain) {
                continue;
            }
            if let Some(height) = frame.get(x, y) {
                bounds = Some(match bounds {
                    Some((lo, hi)) => (lo.min(height), hi.max(height)),
                    None => (height, height),
                });
            }
        }
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
    use super::{build_mesh, center_offset, mountain_bounds, vertex_position, vertex_positions};
    use providence_config::MaterialParams;
    use providence_ports::{TerrainFrame, TerrainType};

    /// A material table with a distinct base colour per type so a returned
    /// colour names the band; the mountain ramp runs rock (black) → snow (white).
    const MATERIAL: MaterialParams = MaterialParams {
        water_rgb: [0.0, 0.0, 1.0],
        shore_rgb: [1.0, 1.0, 0.0],
        land_rgb: [0.0, 1.0, 0.0],
        mountain_rgb: [0.0, 0.0, 0.0],
        peak_rgb: [1.0, 1.0, 1.0],
    };

    /// `n` copies of [`TerrainType::Land`] — the terrain types for a
    /// geometry-only test whose colours are not under test.
    fn land(n: usize) -> Vec<TerrainType> {
        vec![TerrainType::Land; n]
    }

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
        let frame = TerrainFrame::new(2, 2, &heights, &[], 0);
        let positions = vertex_positions(&frame, 1.0);
        assert_eq!(positions.len(), 4);
        assert!(approx(positions[0][1], 0.0), "(0,0) height 0 → y 0");
        assert!(approx(positions[3][1], 2.0), "(1,1) height 2 → y 2");
    }

    #[test]
    fn a_grid_meshes_into_two_triangles_per_cell() {
        // 3×3 vertices → 2×2 cells → 4 cells × 2 triangles × 3 vertices = 24.
        let heights = [0; 9];
        let types = land(9);
        let frame = TerrainFrame::new(3, 3, &heights, &types, 0);
        let mesh = build_mesh(&frame, 1.0, &MATERIAL);
        assert_eq!(mesh.triangle_count(), 8);
        assert_eq!(mesh.vertices.len(), 24);
        assert!(!mesh.is_empty());
    }

    #[test]
    fn a_frame_too_small_for_a_cell_yields_no_geometry() {
        let heights = [3, 4];
        let types = land(2);
        let frame = TerrainFrame::new(2, 1, &heights, &types, 0); // one row: no cell
        assert!(build_mesh(&frame, 1.0, &MATERIAL).is_empty());
    }

    #[test]
    fn a_flat_field_has_upward_unit_normals() {
        let heights = [5; 4]; // 2×2, all equal → the surface is level
        let types = land(4);
        let frame = TerrainFrame::new(2, 2, &heights, &types, 0);
        let mesh = build_mesh(&frame, 1.0, &MATERIAL);
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
        let types = land(4);
        let frame = TerrainFrame::new(2, 2, &heights, &types, 0);
        let mesh = build_mesh(&frame, 1.0, &MATERIAL);
        for vertex in &mesh.vertices {
            let [nx, ny, nz] = vertex.normal;
            assert!(ny > 0.0, "normal points up");
            assert!(approx((nx * nx + ny * ny + nz * nz).sqrt(), 1.0), "unit");
        }
    }

    #[test]
    fn vertices_are_coloured_by_their_terrain_type() {
        // A 2×2 cell with one vertex of each type: every band's base colour
        // appears in the mesh, keyed on the derived type (ADR 0023), not height.
        let heights = [0, 1, 4, 12];
        let types = [
            TerrainType::Water,
            TerrainType::Shore,
            TerrainType::Land,
            TerrainType::Mountain,
        ];
        let frame = TerrainFrame::new(2, 2, &heights, &types, 0);
        let mesh = build_mesh(&frame, 1.0, &MATERIAL);
        let has = |c: [f32; 3]| mesh.vertices.iter().any(|v| approx3(v.color, c));
        assert!(
            has([0.0, 0.0, 1.0]),
            "the water vertex is the seabed colour"
        );
        assert!(has([1.0, 1.0, 0.0]), "the shore vertex is sand");
        assert!(has([0.0, 1.0, 0.0]), "the land vertex is grass");
        // The sole mountain is both lo and hi of its band, so it takes the rock
        // anchor (a degenerate ramp).
        assert!(has([0.0, 0.0, 0.0]), "the mountain vertex is rock");
    }

    #[test]
    fn mountain_bounds_span_only_the_mountain_vertices() {
        // Heights vary, but only two vertices are mountains (at 8 and 14); the
        // ramp must normalise over those, ignoring the lower non-mountain land.
        let heights = [2, 8, 5, 14];
        let types = [
            TerrainType::Land,
            TerrainType::Mountain,
            TerrainType::Land,
            TerrainType::Mountain,
        ];
        let frame = TerrainFrame::new(2, 2, &heights, &types, 0);
        assert_eq!(mountain_bounds(&frame), (8, 14));
    }

    #[test]
    fn mountain_bounds_of_a_mountainless_frame_collapse() {
        let heights = [-2, 0, 5, 3];
        let types = land(4);
        let frame = TerrainFrame::new(2, 2, &heights, &types, 0);
        assert_eq!(
            mountain_bounds(&frame),
            (0, 0),
            "no mountains → an unsampled (0, 0)"
        );
    }
}
