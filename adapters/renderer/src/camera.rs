//! The workbench view camera (ADR 0020 §3; issue #8 Phases 1–2).
//!
//! Pure and GPU-free. Two layers, both **adapter-local view state** that never
//! crosses the determinism boundary — moving the camera cannot change a single
//! height (ADR 0020 §3):
//!
//! - [`Camera`] — a resolved pose plus lens, which produces the view/projection
//!   [`Mat4`] the renderer uploads.
//! - [`OrbitController`] — the interactive orbit/pan/zoom state (issue #8
//!   Phase 2). It holds yaw/pitch/distance around a look-at target, applies
//!   mouse-drag and scroll deltas **clamped** to the configured envelope
//!   (`render.camera.{min,max}_*`, scaled by `render.camera.*_speed`), and
//!   resolves to a [`Camera`] each frame. The window drives it from raw `winit`
//!   events; the maths lives here so the transforms are unit-tested in the gate.

use providence_config::CameraParams;

use crate::math::{Mat4, Vec3, cross, dot, look_at_rh, mul, normalize, perspective_rh, sub};

/// A resolved camera: where it sits, what it looks at, and its lens. Produced by
/// [`OrbitController::camera`] (or, for the fixed initial pose,
/// [`Camera::from_params`]); yields a view/projection matrix for a viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// World-space eye position.
    pub eye: Vec3,
    /// The point the camera looks at.
    pub target: Vec3,
    /// World-up hint.
    pub up: Vec3,
    /// Vertical field of view, in radians.
    pub fov_y_radians: f32,
    /// Near clip-plane distance.
    pub near: f32,
    /// Far clip-plane distance.
    pub far: f32,
}

impl Camera {
    /// Resolve the initial camera pose from config — the fixed pose Phase 1
    /// draws and the starting point Phase 2's controller orbits from. Delegates
    /// to [`OrbitController`] so the eye-from-yaw/pitch/distance maths lives in
    /// one place; the initial pose is clamped into the configured envelope.
    #[must_use]
    pub fn from_params(params: &CameraParams) -> Self {
        OrbitController::from_params(params).camera()
    }

    /// The combined view/projection matrix for a viewport of the given aspect
    /// ratio (`width / height`). Column-major, GPU-upload ready.
    #[must_use]
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        let view = look_at_rh(self.eye, self.target, self.up);
        let proj = perspective_rh(self.fov_y_radians, aspect, self.near, self.far);
        mul(proj, view)
    }

    /// Recover the orbit pose — yaw and pitch in **degrees** plus the orbit
    /// distance — from this resolved eye/target. The inverse of the
    /// eye-from-pose maths [`OrbitController`] applies, so the HUD readout can
    /// show the pose behind *any* camera the same way, windowed or headless
    /// (issue #8 Phase 3). A degenerate (eye == target) camera reports zeros.
    #[must_use]
    pub fn orbit_pose(&self) -> (f32, f32, f32) {
        let offset = sub(self.eye, self.target);
        let distance = dot(offset, offset).sqrt();
        if distance <= f32::EPSILON {
            return (0.0, 0.0, 0.0);
        }
        let pitch = (offset[1] / distance).asin();
        let yaw = offset[0].atan2(offset[2]);
        (yaw.to_degrees(), pitch.to_degrees(), distance)
    }
}

/// The interactive orbit/pan/zoom camera (issue #8 Phase 2).
///
/// Holds the view as an orbit around a look-at `target`: a compass `yaw`, a
/// `pitch` above the horizon, and a `distance`. Drag and scroll deltas mutate
/// those, each **clamped** to the configured band so the view can never dive
/// under the land, flip over the pole, or dolly through the target. All state
/// is adapter-local (ADR 0020 §3): none of it is a `TerrainFrame` field or a
/// command, so it cannot reach the sim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitController {
    /// Compass angle, radians.
    yaw: f32,
    /// Angle above the horizon, radians, kept in `[min_pitch, max_pitch]`.
    pitch: f32,
    /// Orbit radius, kept in `[min_distance, max_distance]`.
    distance: f32,
    /// The look-at point the eye orbits.
    target: Vec3,
    /// World-up hint.
    up: Vec3,
    /// Vertical field of view, radians (carried into [`Camera`]).
    fov_y_radians: f32,
    /// Near clip-plane distance.
    near: f32,
    /// Far clip-plane distance.
    far: f32,
    /// Closest permitted `distance`.
    min_distance: f32,
    /// Farthest permitted `distance`.
    max_distance: f32,
    /// Lowest permitted `pitch`, radians.
    min_pitch: f32,
    /// Highest permitted `pitch`, radians.
    max_pitch: f32,
    /// Orbit rotation per pixel of drag, radians.
    orbit_speed: f32,
    /// Look-at translation per pixel of drag, world units.
    pan_speed: f32,
    /// Fraction of `distance` changed per unit of scroll.
    zoom_speed: f32,
}

impl OrbitController {
    /// Build the controller from config, orbiting the map centre (the origin,
    /// where the mesh is centred). The initial pose is clamped into the
    /// configured envelope so a mis-set `initial_*` can never start out of
    /// bounds.
    #[must_use]
    pub fn from_params(params: &CameraParams) -> Self {
        let min_pitch = params.min_pitch_degrees.to_radians();
        let max_pitch = params.max_pitch_degrees.to_radians();
        Self {
            yaw: params.initial_yaw_degrees.to_radians(),
            pitch: clamp_range(
                params.initial_pitch_degrees.to_radians(),
                min_pitch,
                max_pitch,
            ),
            distance: clamp_range(
                params.initial_distance,
                params.min_distance,
                params.max_distance,
            ),
            target: [0.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            fov_y_radians: params.fov_degrees.to_radians(),
            near: params.near,
            far: params.far,
            min_distance: params.min_distance,
            max_distance: params.max_distance,
            min_pitch,
            max_pitch,
            orbit_speed: params.orbit_speed.to_radians(),
            pan_speed: params.pan_speed,
            zoom_speed: params.zoom_speed,
        }
    }

    /// Orbit by a mouse drag: `dx`/`dy` are pixel deltas. Horizontal drag spins
    /// the compass (yaw); vertical drag tilts (pitch), clamped to the configured
    /// band so the view stays above the horizon and below the pole.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * self.orbit_speed;
        self.pitch = clamp_range(
            self.pitch + dy * self.orbit_speed,
            self.min_pitch,
            self.max_pitch,
        );
    }

    /// Zoom by a scroll `amount` (positive dollies in). Multiplicative in the
    /// current distance, clamped to `[min_distance, max_distance]`.
    pub fn zoom(&mut self, amount: f32) {
        let factor = 1.0 - amount * self.zoom_speed;
        self.distance = clamp_range(self.distance * factor, self.min_distance, self.max_distance);
    }

    /// Pan the look-at point by a mouse drag: slide `target` across the camera's
    /// screen plane — `dx` along the view's right axis, `dy` along its up axis —
    /// so grabbing and dragging moves the world under a fixed view direction.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let (right, screen_up) = self.screen_axes();
        self.target = [
            self.target[0] + (-dx * right[0] + dy * screen_up[0]) * self.pan_speed,
            self.target[1] + (-dx * right[1] + dy * screen_up[1]) * self.pan_speed,
            self.target[2] + (-dx * right[2] + dy * screen_up[2]) * self.pan_speed,
        ];
    }

    /// Set an absolute pose (degrees / world units), clamped to the envelope.
    /// The composition root uses this to render the workbench from a chosen
    /// orbit for the headless multi-angle self-check (issue #8 Phase 2 verify).
    pub fn set_pose(&mut self, yaw_degrees: f32, pitch_degrees: f32, distance: f32) {
        self.yaw = yaw_degrees.to_radians();
        self.pitch = clamp_range(pitch_degrees.to_radians(), self.min_pitch, self.max_pitch);
        self.distance = clamp_range(distance, self.min_distance, self.max_distance);
    }

    /// Resolve the current orbit state to a [`Camera`] the renderer can upload.
    #[must_use]
    pub fn camera(&self) -> Camera {
        Camera {
            eye: self.eye(),
            target: self.target,
            up: self.up,
            fov_y_radians: self.fov_y_radians,
            near: self.near,
            far: self.far,
        }
    }

    /// The eye position for the current yaw/pitch/distance, offset from the
    /// look-at target.
    fn eye(&self) -> Vec3 {
        let (sp, cp) = (self.pitch.sin(), self.pitch.cos());
        let (sy, cy) = (self.yaw.sin(), self.yaw.cos());
        [
            self.target[0] + self.distance * cp * sy,
            self.target[1] + self.distance * sp,
            self.target[2] + self.distance * cp * cy,
        ]
    }

    /// The camera's screen-plane basis: the unit right and up axes used for
    /// panning. Independent of `target` (they depend only on yaw/pitch), so a
    /// pan does not rotate the frame it slides within.
    fn screen_axes(&self) -> (Vec3, Vec3) {
        let forward = normalize(sub(self.target, self.eye()));
        let right = normalize(cross(forward, self.up));
        let screen_up = cross(right, forward);
        (right, screen_up)
    }
}

/// Clamp `value` into `[lo, hi]` **without** panicking if a mis-ordered config
/// gives `lo > hi` (unlike [`f32::clamp`]): a bad band yields a stable value at
/// the bound, never a crash.
fn clamp_range(value: f32, lo: f32, hi: f32) -> f32 {
    value.max(lo).min(hi)
}

#[cfg(test)]
mod tests {
    use super::{Camera, OrbitController};
    use crate::math::{cross, dot, normalize, sub, transform_point};
    use providence_config::CameraParams;

    fn params() -> CameraParams {
        CameraParams {
            fov_degrees: 45.0,
            near: 0.1,
            far: 1000.0,
            initial_distance: 24.0,
            initial_yaw_degrees: 45.0,
            initial_pitch_degrees: 30.0,
            min_distance: 6.0,
            max_distance: 120.0,
            min_pitch_degrees: 5.0,
            max_pitch_degrees: 85.0,
            orbit_speed: 0.4,
            pan_speed: 0.05,
            zoom_speed: 0.1,
        }
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-3
    }

    fn approx3(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= 1e-3)
    }

    // --- Camera (Phase 1 pose + projection) --------------------------------

    #[test]
    fn the_eye_sits_at_the_configured_distance_from_the_target() {
        let camera = Camera::from_params(&params());
        let offset = sub(camera.eye, camera.target);
        assert!(
            approx(dot(offset, offset).sqrt(), 24.0),
            "distance honoured"
        );
    }

    #[test]
    fn a_positive_pitch_lifts_the_eye_above_the_ground() {
        let camera = Camera::from_params(&params());
        assert!(camera.eye[1] > 0.0, "30° pitch puts the camera above y=0");
    }

    #[test]
    fn the_target_projects_to_the_centre_of_the_image() {
        let camera = Camera::from_params(&params());
        let clip = transform_point(camera.view_projection(16.0 / 9.0), camera.target);
        assert!(clip[3] > 0.0, "target is in front of the camera");
        assert!(approx(clip[0] / clip[3], 0.0), "centred horizontally");
        assert!(approx(clip[1] / clip[3], 0.0), "centred vertically");
    }

    #[test]
    fn a_wider_aspect_compresses_x_relative_to_a_square() {
        let camera = Camera::from_params(&params());
        let point = [4.0, 0.0, 0.0];
        let square = transform_point(camera.view_projection(1.0), point);
        let wide = transform_point(camera.view_projection(2.0), point);
        assert!((wide[0] / wide[3]).abs() < (square[0] / square[3]).abs());
    }

    // --- OrbitController (Phase 2 orbit/pan/zoom + clamping) ----------------

    #[test]
    fn the_initial_controller_matches_the_fixed_camera_pose() {
        // Camera::from_params delegates to the controller, so the starting pose
        // is identical — Phase 2 begins exactly where Phase 1's view stood.
        let controller = OrbitController::from_params(&params());
        let camera = Camera::from_params(&params());
        assert!(approx3(controller.camera().eye, camera.eye));
        assert!(approx3(controller.camera().target, camera.target));
    }

    #[test]
    fn from_params_clamps_an_out_of_band_initial_pose() {
        let mut wild = params();
        wild.initial_pitch_degrees = 200.0; // past the pole
        wild.initial_distance = 1_000.0; // past the far dolly limit
        let camera = OrbitController::from_params(&wild).camera();
        // Pitch clamped to 85° → eye height = distance·sin(85°); distance
        // clamped to max_distance (120). Both bounds honoured at start.
        let offset = sub(camera.eye, camera.target);
        assert!(
            approx(dot(offset, offset).sqrt(), 120.0),
            "distance clamped"
        );
        assert!(
            camera.eye[1] > 0.0 && camera.eye[1] < 120.0,
            "pitch clamped below the pole, not overhead",
        );
    }

    #[test]
    fn orbit_turns_yaw_and_preserves_the_radius() {
        let mut controller = OrbitController::from_params(&params());
        let before = controller.camera().eye;
        let radius_before = radius(&controller);
        controller.orbit(50.0, 0.0); // purely horizontal drag
        assert!(
            !approx3(before, controller.camera().eye),
            "a horizontal drag rotates the view",
        );
        // A pure orbit spins the eye around the target without changing its
        // distance — it stays on the orbit sphere.
        assert!(
            approx(radius(&controller), radius_before),
            "orbit preserves the radius",
        );
    }

    #[test]
    fn a_vertical_drag_is_clamped_to_the_max_pitch() {
        let mut controller = OrbitController::from_params(&params());
        // A large downward drag would tilt far past the pole; it must stop at
        // max_pitch (85°), never flip over.
        controller.orbit(0.0, 10_000.0);
        let camera = controller.camera();
        let offset = sub(camera.eye, camera.target);
        let height_fraction = offset[1] / dot(offset, offset).sqrt();
        assert!(
            height_fraction < 85.5_f32.to_radians().sin(),
            "pitch never exceeds the configured max",
        );
        assert!(height_fraction > 0.0, "still above the horizon");
    }

    #[test]
    fn zoom_dollies_in_and_clamps_to_the_distance_band() {
        let mut controller = OrbitController::from_params(&params());
        let start = radius(&controller);
        controller.zoom(5.0); // scroll in
        assert!(
            radius(&controller) < start,
            "scrolling in shortens the orbit"
        );

        // Zoom hard in and out; the radius stays inside [min, max].
        for _ in 0..100 {
            controller.zoom(50.0);
        }
        assert!(approx(radius(&controller), 6.0), "clamped at min_distance");
        for _ in 0..100 {
            controller.zoom(-50.0);
        }
        assert!(
            approx(radius(&controller), 120.0),
            "clamped at max_distance"
        );
    }

    #[test]
    fn pan_slides_the_target_along_the_view_plane() {
        let mut controller = OrbitController::from_params(&params());
        let camera = controller.camera();
        let forward = normalize(sub(camera.target, camera.eye));
        let right = normalize(cross(forward, camera.up));

        let before = controller.camera().target;
        controller.pan(10.0, 0.0); // purely horizontal drag
        let delta = sub(controller.camera().target, before);

        assert!(dot(delta, delta) > 0.0, "the look-at point actually moved");
        // A horizontal-only drag moves the target purely along the right axis:
        // its component out of that direction (the cross product) is ~zero.
        assert!(
            approx3(cross(delta, right), [0.0, 0.0, 0.0]),
            "horizontal pan stays on the camera's right axis",
        );
    }

    #[test]
    fn set_pose_places_an_absolute_clamped_orbit() {
        let mut controller = OrbitController::from_params(&params());
        controller.set_pose(90.0, 200.0, 1.0); // pitch + distance out of band
        assert!(approx(radius(&controller), 6.0), "distance clamped to min");
        let camera = controller.camera();
        // yaw 90° looks down +x: the eye's x offset dominates.
        assert!(camera.eye[0] > 0.0, "yaw 90° swings the eye onto +x");
    }

    #[test]
    fn the_lens_is_carried_through_to_the_camera() {
        let camera = OrbitController::from_params(&params()).camera();
        assert!(approx(camera.fov_y_radians, 45.0_f32.to_radians()));
        assert!(approx(camera.near, 0.1));
        assert!(approx(camera.far, 1000.0));
    }

    #[test]
    fn orbit_pose_inverts_the_eye_maths() {
        // The pose recovered from a resolved camera matches the yaw/pitch/
        // distance it was built from — what the HUD readout reports (Phase 3).
        let (yaw, pitch, distance) = OrbitController::from_params(&params())
            .camera()
            .orbit_pose();
        assert!(approx(yaw, 45.0), "yaw recovered");
        assert!(approx(pitch, 30.0), "pitch recovered");
        assert!(approx(distance, 24.0), "distance recovered");
    }

    /// The current orbit radius (eye→target distance).
    fn radius(controller: &OrbitController) -> f32 {
        let camera = controller.camera();
        let offset = sub(camera.eye, camera.target);
        dot(offset, offset).sqrt()
    }
}
