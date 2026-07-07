//! Minimal linear algebra for the workbench renderer (issue #8 Phase 1).
//!
//! Pure and GPU-free: just enough vector and 4×4-matrix maths to build face
//! normals ([`mesh`](crate::mesh)) and a view/projection matrix
//! ([`camera`](crate::camera)) without pulling in a maths crate (I8 — minimal
//! deps). Matrices are **column-major** `[[f32; 4]; 4]` (`m[col][row]`), which
//! is exactly `wgpu`/WGSL's `mat4x4<f32>` memory order, so a [`Mat4`] uploads
//! to the GPU verbatim. Right-handed, with clip-space depth in `[0, 1]`
//! (`wgpu`'s convention), so this is `perspective_rh`/`look_at_rh`.

/// A 3-component vector — a position or direction in world space.
pub type Vec3 = [f32; 3];

/// A column-major 4×4 matrix (`m[col][row]`), GPU-upload compatible.
pub type Mat4 = [[f32; 4]; 4];

/// `a - b`, component-wise.
#[must_use]
pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// The cross product `a × b`.
#[must_use]
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The dot product `a · b`.
#[must_use]
pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `v` scaled to unit length. A (near-)zero vector is returned unchanged — a
/// degenerate triangle or direction yields a zero normal rather than a `NaN`.
#[must_use]
pub fn normalize(v: Vec3) -> Vec3 {
    let length = dot(v, v).sqrt();
    if length <= f32::EPSILON {
        return v;
    }
    [v[0] / length, v[1] / length, v[2] / length]
}

/// The matrix product `a * b` (both column-major).
#[must_use]
pub fn mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut out = [[0.0_f32; 4]; 4];
    for (col, out_col) in out.iter_mut().enumerate() {
        for (row, cell) in out_col.iter_mut().enumerate() {
            *cell = a[0][row] * b[col][0]
                + a[1][row] * b[col][1]
                + a[2][row] * b[col][2]
                + a[3][row] * b[col][3];
        }
    }
    out
}

/// Transform the homogeneous point `[p, 1]` by `m`, returning `[x, y, z, w]`.
/// Used by the camera tests to check where a world point lands in clip space.
#[must_use]
pub fn transform_point(m: Mat4, p: Vec3) -> [f32; 4] {
    let mut out = [0.0_f32; 4];
    for (row, cell) in out.iter_mut().enumerate() {
        *cell = m[0][row] * p[0] + m[1][row] * p[1] + m[2][row] * p[2] + m[3][row];
    }
    out
}

/// A right-handed perspective projection with clip-space depth in `[0, 1]`
/// (`wgpu`'s convention). `fov_y` is the vertical field of view in radians.
#[must_use]
pub fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let focal = 1.0 / (fov_y / 2.0).tan();
    let depth = far / (near - far);
    [
        [focal / aspect, 0.0, 0.0, 0.0],
        [0.0, focal, 0.0, 0.0],
        [0.0, 0.0, depth, -1.0],
        [0.0, 0.0, depth * near, 0.0],
    ]
}

/// A right-handed look-at view matrix: the camera sits at `eye`, looks at
/// `target`, with `up` the world-up hint.
#[must_use]
pub fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let forward = normalize(sub(target, eye));
    let right = normalize(cross(forward, up));
    let true_up = cross(right, forward);
    [
        [right[0], true_up[0], -forward[0], 0.0],
        [right[1], true_up[1], -forward[1], 0.0],
        [right[2], true_up[2], -forward[2], 0.0],
        [-dot(right, eye), -dot(true_up, eye), dot(forward, eye), 1.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        Vec3, cross, dot, look_at_rh, mul, normalize, perspective_rh, sub, transform_point,
    };

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-4
    }

    fn approx3(a: Vec3, b: Vec3) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= 1e-4)
    }

    #[test]
    fn cross_of_unit_axes_is_the_third_axis() {
        assert!(approx3(
            cross([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            [0.0, 0.0, 1.0]
        ));
    }

    #[test]
    fn normalize_scales_to_unit_length_and_leaves_zero_alone() {
        let n = normalize([0.0, 3.0, 4.0]);
        assert!(approx(dot(n, n).sqrt(), 1.0));
        assert!(approx3(normalize([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]));
    }

    #[test]
    fn look_at_places_the_target_on_the_negative_z_axis() {
        // A camera at (0,0,5) looking at the origin: the origin should map to
        // the view-space point (0, 0, -5) — straight ahead down -z.
        let view = look_at_rh([0.0, 0.0, 5.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let origin_in_view = transform_point(view, [0.0, 0.0, 0.0]);
        assert!(approx3(
            [origin_in_view[0], origin_in_view[1], origin_in_view[2]],
            [0.0, 0.0, -5.0]
        ));
    }

    #[test]
    fn perspective_keeps_a_point_ahead_inside_the_frustum() {
        // A point on the view axis between near and far projects to the centre
        // of the image (x=y=0) with 0 < z < w (in front of the camera).
        let proj = perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0);
        let clip = transform_point(proj, [0.0, 0.0, -10.0]);
        assert!(approx(clip[0], 0.0) && approx(clip[1], 0.0));
        assert!(clip[3] > 0.0, "w positive => in front of the camera");
        assert!(clip[2] > 0.0 && clip[2] < clip[3], "depth within [0, w]");
    }

    #[test]
    fn view_projection_frames_the_looked_at_point_at_the_image_centre() {
        let view = look_at_rh([6.0, 6.0, 6.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let proj = perspective_rh(std::f32::consts::FRAC_PI_4, 1.5, 0.1, 100.0);
        let view_proj = mul(proj, view);
        let clip = transform_point(view_proj, [0.0, 0.0, 0.0]);
        // The target is dead centre: x/w and y/w are ~0.
        assert!(approx(clip[0] / clip[3], 0.0));
        assert!(approx(clip[1] / clip[3], 0.0));
    }

    #[test]
    fn sub_is_componentwise() {
        assert!(approx3(
            sub([3.0, 5.0, 9.0], [1.0, 2.0, 4.0]),
            [2.0, 3.0, 5.0]
        ));
    }
}
