//! The water-surface plane geometry (ADR 0023, Phase 2; issue #22).
//!
//! Pure and GPU-free. Builds the flat quad the renderer floats at the waterline
//! as a **living water surface**: a translucent sheet spanning the whole grid,
//! alpha-blended over the terrain so land rising above the waterline reveals the
//! coastline (the shoreline tracks a shaping edit *for free*, derived against the
//! live terrain every frame). The colour, translucency, lift, and shimmer are
//! all `render.water.*` config and live in the shader/uniforms
//! ([`crate::gpu`]); only the **plane geometry** lives here, so it is
//! unit-tested in the gate before any GPU code.
//!
//! The plane is centred on the origin exactly like the terrain mesh
//! ([`crate::mesh`]), so the two line up. Worldgen pins the sea floor **flat at
//! the waterline datum**, so the plane would be coplanar with the seabed; a small
//! `surface_lift` floats it just above (no z-fighting, and a hair of body),
//! deliberately kept below one height step so it never rises over the first dry
//! shore.

use crate::mesh::{Position, vertex_position};

/// The two-triangle water-surface quad: six unshared triangle-list vertex
/// positions spanning the grid at the (lifted) waterline height — the geometry
/// the renderer uploads for the alpha-blended water pass (ADR 0023, Phase 2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterPlane {
    /// Triangle-list vertices, two triangles (`p00,p10,p11` then `p00,p11,p01`).
    vertices: [Position; 6],
}

impl WaterPlane {
    /// Build the water plane for a `width × depth` grid whose sea-level datum is
    /// `waterline`, matching the terrain mesh's `vertical_scale` centring.
    ///
    /// The surface sits at `waterline * vertical_scale + surface_lift` in world
    /// Y — the small positive `surface_lift` floats it clear of the coplanar flat
    /// seabed (ADR 0023, Phase 2). It spans exactly the terrain's world extent
    /// (`vertex_position` at the grid corners), so the sea meets the land edge to
    /// edge. A grid narrower or shallower than two vertices yields a
    /// zero-area plane (nothing to draw), never a panic.
    #[must_use]
    pub fn new(
        width: u32,
        depth: u32,
        waterline: i32,
        vertical_scale: f32,
        surface_lift: f32,
    ) -> Self {
        let y = waterline as f32 * vertical_scale + surface_lift;
        // Reuse the mesh's own vertex placement for the grid corners so the sea
        // aligns with the drawn terrain; the height passed here is ignored (we
        // override Y with the water surface height below).
        let far_x = width.saturating_sub(1);
        let far_z = depth.saturating_sub(1);
        let min = vertex_position(0, 0, 0, width, depth, vertical_scale);
        let max = vertex_position(far_x, far_z, 0, width, depth, vertical_scale);
        let (min_x, min_z) = (min[0], min[2]);
        let (max_x, max_z) = (max[0], max[2]);

        let p00 = [min_x, y, min_z];
        let p10 = [max_x, y, min_z];
        let p11 = [max_x, y, max_z];
        let p01 = [min_x, y, max_z];
        Self {
            vertices: [p00, p10, p11, p00, p11, p01],
        }
    }

    /// The six triangle-list vertex positions to upload.
    #[must_use]
    pub fn vertices(&self) -> &[Position] {
        &self.vertices
    }

    /// The world-space Y the surface sits at (all six vertices share it) — the
    /// lifted waterline height. Handy for tests and callers that need the datum.
    #[must_use]
    pub fn surface_y(&self) -> f32 {
        self.vertices[0][1]
    }
}

#[cfg(test)]
mod tests {
    use super::WaterPlane;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-5
    }

    #[test]
    fn the_surface_sits_at_the_lifted_waterline() {
        // Waterline 3 at scale 2 → seabed Y 6; a 0.2 lift floats the sheet to 6.2.
        let plane = WaterPlane::new(4, 4, 3, 2.0, 0.2);
        assert!(approx(plane.surface_y(), 6.2), "waterline*scale + lift");
        // Every vertex shares that height — the sheet is level.
        for vertex in plane.vertices() {
            assert!(approx(vertex[1], 6.2), "the water plane is flat");
        }
    }

    #[test]
    fn the_plane_spans_the_centred_grid_extent() {
        // A 5-wide, 3-deep grid is centred on the origin, so it spans x in
        // [-2, 2] and z in [-1, 1] — exactly the terrain mesh's extent.
        let plane = WaterPlane::new(5, 3, 0, 1.0, 0.0);
        let xs: Vec<f32> = plane.vertices().iter().map(|v| v[0]).collect();
        let zs: Vec<f32> = plane.vertices().iter().map(|v| v[2]).collect();
        let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let min_z = zs.iter().copied().fold(f32::INFINITY, f32::min);
        let max_z = zs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(approx(min_x, -2.0) && approx(max_x, 2.0), "x spans [-2, 2]");
        assert!(approx(min_z, -1.0) && approx(max_z, 1.0), "z spans [-1, 1]");
    }

    #[test]
    fn it_is_two_triangles_of_six_vertices() {
        let plane = WaterPlane::new(4, 4, 0, 1.0, 0.1);
        assert_eq!(plane.vertices().len(), 6, "two triangles, unshared");
    }

    #[test]
    fn a_zero_lift_places_the_sheet_on_the_seabed_datum() {
        // With no lift the surface is exactly at waterline*scale — coplanar with
        // the flat seabed (the config default lifts it to avoid z-fighting).
        let plane = WaterPlane::new(4, 4, -2, 3.0, 0.0);
        assert!(approx(plane.surface_y(), -6.0), "waterline -2 × scale 3");
    }

    #[test]
    fn a_degenerate_grid_collapses_without_panicking() {
        // A single-column grid has no width; both x corners coincide → a
        // zero-area plane, but the call is total (no u32 underflow).
        let plane = WaterPlane::new(1, 1, 0, 1.0, 0.1);
        for vertex in plane.vertices() {
            assert!(
                approx(vertex[0], 0.0) && approx(vertex[2], 0.0),
                "collapsed"
            );
        }
    }
}
