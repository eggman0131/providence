//! The workbench's single directional light (ADR 0020; issue #8 Phase 1).
//!
//! Pure and GPU-free: turns the configured compass azimuth and elevation into
//! a unit direction vector pointing **toward** the light, which the shader dots
//! against each face normal for Lambert diffuse shading. Kept here and tested so
//! the light geometry is correct before any GPU code runs.

use crate::math::{Vec3, normalize};

/// Unit direction pointing **toward** the light, from `azimuth` (compass
/// direction, degrees clockwise from `+z`) and `elevation` (degrees above the
/// horizon). Elevation `90°` is straight overhead; `0°` grazes the horizon.
#[must_use]
pub fn direction(azimuth_degrees: f32, elevation_degrees: f32) -> Vec3 {
    let azimuth = azimuth_degrees.to_radians();
    let elevation = elevation_degrees.to_radians();
    normalize([
        elevation.cos() * azimuth.sin(),
        elevation.sin(),
        elevation.cos() * azimuth.cos(),
    ])
}

#[cfg(test)]
mod tests {
    use super::direction;
    use crate::math::dot;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-4
    }

    #[test]
    fn overhead_light_points_straight_up() {
        let dir = direction(0.0, 90.0);
        assert!(approx(dir[0], 0.0) && approx(dir[1], 1.0) && approx(dir[2], 0.0));
    }

    #[test]
    fn the_direction_is_always_unit_length() {
        for (az, el) in [(0.0, 0.0), (135.0, 45.0), (270.0, 10.0)] {
            let dir = direction(az, el);
            assert!(approx(dot(dir, dir).sqrt(), 1.0));
        }
    }

    #[test]
    fn elevation_sets_how_high_the_light_sits() {
        // A higher elevation raises the light's y component.
        assert!(direction(90.0, 60.0)[1] > direction(90.0, 20.0)[1]);
    }
}
