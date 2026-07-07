//! The workbench view camera (ADR 0020 §3; issue #8 Phase 1).
//!
//! Pure and GPU-free: resolves the configured [`CameraParams`] into an eye
//! pose and a view/projection [`Mat4`] the renderer uploads. The camera is
//! **adapter-local view state** — it never crosses the determinism boundary and
//! moving it cannot change a single height (ADR 0020 §3). Phase 1 builds only
//! the *initial* fixed pose from config; orbit/pan/zoom mutation arrives in
//! Phase 2, reusing this same resolution.

use providence_config::CameraParams;

use crate::math::{Mat4, Vec3, look_at_rh, mul, perspective_rh};

/// A resolved camera: where it sits, what it looks at, and its lens. Built
/// from [`CameraParams`] via [`Camera::from_params`]; produces a view/projection
/// matrix for a given viewport aspect ratio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// World-space eye position.
    pub eye: Vec3,
    /// The point the camera looks at (the centre of the map, the origin).
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
    /// Resolve the initial camera pose from config. The camera orbits the
    /// centre of the map (the origin, since the mesh is centred there): its eye
    /// sits `initial_distance` away at the configured yaw (compass angle) and
    /// pitch (angle above the horizon).
    #[must_use]
    pub fn from_params(params: &CameraParams) -> Self {
        let yaw = params.initial_yaw_degrees.to_radians();
        let pitch = params.initial_pitch_degrees.to_radians();
        let distance = params.initial_distance;
        let eye = [
            distance * pitch.cos() * yaw.sin(),
            distance * pitch.sin(),
            distance * pitch.cos() * yaw.cos(),
        ];
        Self {
            eye,
            target: [0.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            fov_y_radians: params.fov_degrees.to_radians(),
            near: params.near,
            far: params.far,
        }
    }

    /// The combined view/projection matrix for a viewport of the given aspect
    /// ratio (`width / height`). Column-major, GPU-upload ready.
    #[must_use]
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        let view = look_at_rh(self.eye, self.target, self.up);
        let proj = perspective_rh(self.fov_y_radians, aspect, self.near, self.far);
        mul(proj, view)
    }
}

#[cfg(test)]
mod tests {
    use super::Camera;
    use crate::math::{dot, sub, transform_point};
    use providence_config::CameraParams;

    fn params() -> CameraParams {
        CameraParams {
            fov_degrees: 45.0,
            near: 0.1,
            far: 1000.0,
            initial_distance: 24.0,
            initial_yaw_degrees: 45.0,
            initial_pitch_degrees: 30.0,
        }
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-3
    }

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
        // The same off-centre world point lands nearer the centre in x on a
        // wider viewport — the projection divides x by the aspect ratio.
        let camera = Camera::from_params(&params());
        let point = [4.0, 0.0, 0.0];
        let square = transform_point(camera.view_projection(1.0), point);
        let wide = transform_point(camera.view_projection(2.0), point);
        assert!((wide[0] / wide[3]).abs() < (square[0] / square[3]).abs());
    }
}
