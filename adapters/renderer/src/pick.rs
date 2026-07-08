//! Screen-ray vertex picking (ADR 0020 §3; issue #8 Phase 3).
//!
//! Pure and GPU-free. Turns a point on the screen into a world-space ray from
//! the camera, then finds the grid vertex that ray passes closest to — the
//! "which vertex is under the crosshair" question the Phase-3 readout answers,
//! and the exact resolve #9 reuses to turn a click into a raise/lower command.
//!
//! It is **read-only** view maths, entirely adapter-local (ADR 0020 §3): the
//! camera's floats resolve to a ray here, at the edge; nothing it computes ever
//! crosses back into the deterministic core. Kept here and unit-tested so the
//! ray/pick geometry is correct without any GPU.

use providence_ports::TerrainFrame;

use crate::camera::Camera;
use crate::math::{Vec3, cross, dot, normalize, sub};
use crate::mesh::vertex_position;

/// An owned copy of a presented [`TerrainFrame`]'s grid — dimensions plus the
/// row-major heights — kept by a renderer so it can pick a vertex every frame
/// as the camera moves (the borrowed [`TerrainFrame`] handed to `present` does
/// not outlive the call). Read-only presentation state (ADR 0020 §1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridSnapshot {
    /// Grid columns.
    pub width: u32,
    /// Grid rows.
    pub height: u32,
    /// Row-major heights, `width * height` of them.
    pub heights: Vec<i32>,
}

impl GridSnapshot {
    /// Take an owned snapshot of a presented frame.
    #[must_use]
    pub fn from_frame(frame: &TerrainFrame<'_>) -> Self {
        Self {
            width: frame.width(),
            height: frame.height(),
            heights: frame.heights().to_vec(),
        }
    }

    /// Borrow this snapshot back as a [`TerrainFrame`] for picking. Picking reads
    /// only heights, so this is a **heights-only** frame — its `types` slice is
    /// empty (ADR 0023) and its waterline is an unread `0` (ADR 0023, Phase 2);
    /// nothing here ever colours a vertex or draws water.
    #[must_use]
    pub fn frame(&self) -> TerrainFrame<'_> {
        TerrainFrame::new(self.width, self.height, &self.heights, &[], 0)
    }
}

/// A world-space ray: an `origin` and a **unit** `direction`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    /// Where the ray starts (the camera eye).
    pub origin: Vec3,
    /// Unit direction the ray travels.
    pub direction: Vec3,
}

/// The grid vertex a ray resolved to: its integer grid coordinate and the
/// height it carries. The `(x, y)` #9 will raise/lower; the `height` the
/// readout shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickedVertex {
    /// Grid column.
    pub x: u32,
    /// Grid row.
    pub y: u32,
    /// Integer height at `(x, y)`.
    pub height: i32,
}

/// The ray through the **screen centre** — the reticle the Phase-3 readout
/// identifies a vertex under. Just [`screen_ray`] at normalised device
/// coordinate `(0, 0)`; named because the reticle is the Phase-3 case.
#[must_use]
pub fn reticle_ray(camera: &Camera, aspect: f32) -> Ray {
    screen_ray(camera, aspect, [0.0, 0.0])
}

/// Convert a cursor position in **physical pixels** (origin top-left, y down —
/// `winit`'s convention) into the normalised device coordinate [`screen_ray`]
/// expects: `[0, 0]` is the screen centre, `[-1, -1]` the bottom-left, `[1, 1]`
/// the top-right (y up). This is the cursor-tracked generalisation of the
/// Phase-3 reticle: issue #9 picks the vertex under the *live cursor*, not just
/// the centre. A degenerate zero-sized surface maps to the centre.
#[must_use]
pub fn cursor_ndc(cursor_px: (f32, f32), size: (u32, u32)) -> [f32; 2] {
    let (px, py) = cursor_px;
    let (width, height) = size;
    if width == 0 || height == 0 {
        return [0.0, 0.0];
    }
    [
        2.0 * px / width as f32 - 1.0,
        // Flip y: pixels grow downward, NDC grows upward.
        1.0 - 2.0 * py / height as f32,
    ]
}

/// The world-space ray from the camera through a point given in normalised
/// device coordinates: `ndc = [0, 0]` is the screen centre, `[-1, -1]` the
/// bottom-left, `[1, 1]` the top-right (y up). This is the general form #9
/// drives from the cursor; Phase 3 only needs the centre ([`reticle_ray`]).
///
/// The pinhole construction avoids a matrix inverse: it offsets the view
/// direction across the camera's right/up axes by the half-extents of the
/// frustum at the given `ndc`, matching the `perspective_rh` lens
/// ([`crate::math`]).
#[must_use]
pub fn screen_ray(camera: &Camera, aspect: f32, ndc: [f32; 2]) -> Ray {
    let forward = normalize(sub(camera.target, camera.eye));
    let right = normalize(cross(forward, camera.up));
    let true_up = cross(right, forward);
    let half_height = (camera.fov_y_radians / 2.0).tan();
    let half_width = half_height * aspect;
    let direction = normalize([
        forward[0] + ndc[0] * half_width * right[0] + ndc[1] * half_height * true_up[0],
        forward[1] + ndc[0] * half_width * right[1] + ndc[1] * half_height * true_up[1],
        forward[2] + ndc[0] * half_width * right[2] + ndc[1] * half_height * true_up[2],
    ]);
    Ray {
        origin: camera.eye,
        direction,
    }
}

/// The grid vertex `ray` passes closest to, among vertices **in front of** the
/// ray origin. Returns `None` only when there is nothing to pick — an empty
/// frame, or every vertex behind the camera.
///
/// "Closest" is the smallest perpendicular distance from the vertex to the ray
/// line; ties (a vertex directly behind another along the ray) break toward the
/// one nearer the origin, so the crosshair reports the front face it points at.
/// `vertical_scale` matches the mesh's, so the picked world positions line up
/// with what is drawn ([`crate::mesh::vertex_position`]).
#[must_use]
pub fn pick_vertex(
    ray: &Ray,
    frame: &TerrainFrame<'_>,
    vertical_scale: f32,
) -> Option<PickedVertex> {
    let (width, depth) = (frame.width(), frame.height());
    let mut best: Option<(f32, f32, PickedVertex)> = None; // (perp², t, vertex)

    for y in 0..depth {
        for x in 0..width {
            let Some(height) = frame.get(x, y) else {
                continue;
            };
            let position = vertex_position(x, y, height, width, depth, vertical_scale);
            let offset = sub(position, ray.origin);
            let t = dot(offset, ray.direction); // signed distance along the ray
            if t <= 0.0 {
                continue; // at or behind the eye — not under the forward view
            }
            // Perpendicular distance², from |offset|² = t² + perp² (unit dir).
            let perp_sq = (dot(offset, offset) - t * t).max(0.0);
            let candidate = PickedVertex { x, y, height };
            let take = match best {
                None => true,
                Some((best_perp_sq, best_t, _)) => {
                    perp_sq < best_perp_sq
                        || ((perp_sq - best_perp_sq).abs() <= f32::EPSILON && t < best_t)
                }
            };
            if take {
                best = Some((perp_sq, t, candidate));
            }
        }
    }

    best.map(|(_, _, vertex)| vertex)
}

#[cfg(test)]
mod tests {
    use super::{PickedVertex, cursor_ndc, pick_vertex, reticle_ray, screen_ray};
    use crate::camera::Camera;
    use crate::math::{dot, normalize, sub};
    use providence_ports::TerrainFrame;

    /// A camera looking straight down the world −y axis at the origin, from
    /// height `eye_y`. `up` is horizontal (as it must be for a top-down view).
    fn top_down(eye_y: f32) -> Camera {
        Camera {
            eye: [0.0, eye_y, 0.0],
            target: [0.0, 0.0, 0.0],
            up: [0.0, 0.0, -1.0],
            fov_y_radians: std::f32::consts::FRAC_PI_4,
            near: 0.1,
            far: 1000.0,
        }
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-4
    }

    #[test]
    fn the_reticle_ray_points_from_the_eye_along_the_view_direction() {
        let camera = top_down(20.0);
        let ray = reticle_ray(&camera, 16.0 / 9.0);
        assert!(
            ray.origin
                .iter()
                .zip(camera.eye)
                .all(|(a, b)| approx(*a, b)),
            "the ray starts at the eye",
        );
        let forward = normalize(sub(camera.target, camera.eye));
        assert!(
            approx(ray.direction[0], forward[0])
                && approx(ray.direction[1], forward[1])
                && approx(ray.direction[2], forward[2]),
            "the centre ray is the view direction",
        );
        assert!(approx(dot(ray.direction, ray.direction), 1.0), "unit dir");
    }

    #[test]
    fn an_off_centre_ndc_tilts_the_ray_toward_that_side() {
        let camera = top_down(20.0);
        // Looking down −y with up = −z, the world +x axis is the camera's right.
        // An ndc to the right should tip the ray in +x.
        let right_ray = screen_ray(&camera, 1.0, [1.0, 0.0]);
        assert!(
            right_ray.direction[0] > 0.0,
            "a right-of-centre ray leans toward +x",
        );
        assert!(approx(dot(right_ray.direction, right_ray.direction), 1.0));
    }

    #[test]
    fn the_reticle_picks_the_vertex_under_the_crosshair() {
        // 3×3 grid, flat but for a raised centre. Looking straight down the
        // y-axis, the centre vertex sits exactly on the ray, so it is picked.
        let mut heights = [0; 9];
        heights[4] = 5; // centre vertex (1, 1)
        let frame = TerrainFrame::new(3, 3, &heights, &[], 0);
        let ray = reticle_ray(&top_down(20.0), 1.0);
        assert_eq!(
            pick_vertex(&ray, &frame, 1.0),
            Some(PickedVertex {
                x: 1,
                y: 1,
                height: 5
            }),
        );
    }

    #[test]
    fn nothing_in_front_of_the_camera_picks_nothing() {
        // Camera below the terrain looking further down: every vertex is behind
        // the ray, so there is nothing to pick.
        let camera = Camera {
            eye: [0.0, -50.0, 0.0],
            target: [0.0, -100.0, 0.0],
            up: [0.0, 0.0, -1.0],
            fov_y_radians: std::f32::consts::FRAC_PI_4,
            near: 0.1,
            far: 1000.0,
        };
        let heights = [0; 9];
        let frame = TerrainFrame::new(3, 3, &heights, &[], 0);
        let ray = reticle_ray(&camera, 1.0);
        assert_eq!(pick_vertex(&ray, &frame, 1.0), None);
    }

    #[test]
    fn an_empty_frame_has_no_pick() {
        let frame = TerrainFrame::new(0, 0, &[], &[], 0);
        let ray = reticle_ray(&top_down(20.0), 1.0);
        assert_eq!(pick_vertex(&ray, &frame, 1.0), None);
    }

    #[test]
    fn the_screen_centre_pixel_maps_to_the_ndc_origin() {
        // A cursor at the middle of an 800×600 surface is the reticle ([0, 0]),
        // so cursor picking through the centre agrees with reticle_ray.
        let ndc = cursor_ndc((400.0, 300.0), (800, 600));
        assert!(
            approx(ndc[0], 0.0) && approx(ndc[1], 0.0),
            "centre → origin"
        );
    }

    #[test]
    fn cursor_ndc_flips_y_and_spans_the_corners() {
        // Top-left pixel → NDC (-1, +1); bottom-right → (+1, -1) (y is flipped
        // because pixels grow downward while NDC grows upward).
        let top_left = cursor_ndc((0.0, 0.0), (800, 600));
        assert!(approx(top_left[0], -1.0) && approx(top_left[1], 1.0));
        let bottom_right = cursor_ndc((800.0, 600.0), (800, 600));
        assert!(approx(bottom_right[0], 1.0) && approx(bottom_right[1], -1.0));
    }

    #[test]
    fn a_zero_sized_surface_maps_to_the_centre() {
        let ndc = cursor_ndc((10.0, 10.0), (0, 0));
        assert!(
            approx(ndc[0], 0.0) && approx(ndc[1], 0.0),
            "degenerate → centre"
        );
    }

    #[test]
    fn a_cursor_left_of_centre_picks_a_left_vertex() {
        // Looking straight down with up = −z, world +x is the camera's right.
        // A cursor left of centre casts a ray leaning −x, so it picks a vertex
        // on the −x (left) side — the cursor-tracked pick issue #9 needs.
        let heights = [0; 9];
        let frame = TerrainFrame::new(3, 3, &heights, &[], 0);
        let camera = top_down(20.0);
        let ndc = cursor_ndc((100.0, 300.0), (800, 600)); // well left of centre
        let ray = screen_ray(&camera, 800.0 / 600.0, ndc);
        let picked = pick_vertex(&ray, &frame, 1.0).expect("a vertex is under the cursor");
        assert!(
            picked.x < 1,
            "a left-of-centre cursor resolves to a left column"
        );
    }
}
