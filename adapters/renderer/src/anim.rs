//! Render-only shaping animation (ADR 0022 §5; issue #9/#10 Phase 3).
//!
//! Pure and GPU-free. When a shaping command changes the height field, the
//! renderer eases the *drawn* surface from its old shape to the new one instead
//! of snapping. This is the **only** real-time input in the whole path, and it
//! is deliberately confined to presentation: it interpolates a vertex's visual
//! height, exactly like the camera moves (ADR 0020 §3), so no wall-clock, float,
//! or frame-rate value ever reaches the deterministic core (I3).
//!
//! The interpolation is a **mesh tween**: because the grid dimensions never
//! change under shaping, the before and after [`Mesh`]es share topology (same
//! vertex count and order), so easing between them is a per-vertex lerp of
//! position, colour, and normal. Keeping the maths here — free of `winit`, GPU,
//! and any clock — is what lets the gate unit-test the animation without a
//! window: the window drives the progress fraction from wall-clock, the headless
//! capture drives it explicitly (mid-animation stills), and both share this
//! code.

use crate::math::normalize;
use crate::mesh::{Mesh, Vertex};

/// The fraction of an animation elapsed: `elapsed_ms / duration_ms`, clamped to
/// `[0, 1]`. A non-positive `duration_ms` yields `1.0` — an instant snap (the
/// Phase-2 behaviour, `render.animation.duration_ms = 0`).
#[must_use]
pub fn progress(elapsed_ms: f32, duration_ms: f32) -> f32 {
    if duration_ms <= 0.0 {
        1.0
    } else {
        (elapsed_ms / duration_ms).clamp(0.0, 1.0)
    }
}

/// Smoothstep ease-in-out on a fraction `t` (clamped to `[0, 1]`): `3t² − 2t³`.
/// Starts and ends gently so a shaping change accelerates out of its old shape
/// and settles softly into the new one, rather than moving at a constant rate.
#[must_use]
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Linear interpolation `a + (b − a) · t`.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Componentwise [`lerp`] of two 3-vectors.
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
}

/// A render-only tween between two same-topology surfaces (ADR 0022 §5): the old
/// drawn [`Mesh`] and the new one after a shaping command. [`at`](MeshTween::at)
/// returns the eased in-between surface for a given elapsed fraction.
///
/// If the two meshes ever differ in vertex count (they never do under shaping —
/// the grid is fixed — but a defensive guard beats a panic), the tween degrades
/// to showing the target immediately.
#[derive(Clone, Debug)]
pub struct MeshTween {
    from: Mesh,
    to: Mesh,
}

impl MeshTween {
    /// Tween from the currently-drawn surface `from` to the post-command surface
    /// `to`.
    #[must_use]
    pub fn new(from: Mesh, to: Mesh) -> Self {
        Self { from, to }
    }

    /// The target surface — what the tween settles to (its `at(1.0)`).
    #[must_use]
    pub fn target(&self) -> &Mesh {
        &self.to
    }

    /// The eased in-between surface at elapsed fraction `fraction` (0 = the old
    /// shape, 1 = the new one). Smoothstep-eased ([`ease`]); each vertex's
    /// position, colour, and flat normal is lerped, and the normal renormalised
    /// (safe: heightfield face normals all point up, so their lerp is never
    /// zero-length). Unchanged vertices have `from == to` and stay put, so only
    /// the shaped region moves.
    #[must_use]
    pub fn at(&self, fraction: f32) -> Mesh {
        if self.from.vertices.len() != self.to.vertices.len() {
            return self.to.clone(); // topology changed — snap rather than mangle
        }
        let t = ease(fraction);
        let vertices = self
            .from
            .vertices
            .iter()
            .zip(self.to.vertices.iter())
            .map(|(a, b)| Vertex {
                position: lerp3(a.position, b.position, t),
                normal: normalize(lerp3(a.normal, b.normal, t)),
                color: lerp3(a.color, b.color, t),
            })
            .collect();
        Mesh { vertices }
    }
}

#[cfg(test)]
mod tests {
    use super::{MeshTween, ease, progress};
    use crate::mesh::{Mesh, Vertex};

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-5
    }

    /// A one-vertex mesh at height `y` (flat-up normal, greyscale by `y`).
    fn vert(y: f32) -> Mesh {
        Mesh {
            vertices: vec![Vertex {
                position: [0.0, y, 0.0],
                normal: [0.0, 1.0, 0.0],
                color: [y, y, y],
            }],
        }
    }

    #[test]
    fn progress_is_the_clamped_elapsed_fraction() {
        assert!(approx(progress(0.0, 200.0), 0.0));
        assert!(approx(progress(100.0, 200.0), 0.5));
        assert!(approx(progress(200.0, 200.0), 1.0));
        assert!(approx(progress(500.0, 200.0), 1.0), "clamped past the end");
    }

    #[test]
    fn a_zero_duration_is_an_instant_snap() {
        assert!(approx(progress(0.0, 0.0), 1.0), "0ms → already done");
    }

    #[test]
    fn ease_pins_the_endpoints_and_is_symmetric_at_the_midpoint() {
        assert!(approx(ease(0.0), 0.0));
        assert!(approx(ease(1.0), 1.0));
        assert!(approx(ease(0.5), 0.5), "smoothstep is symmetric at 0.5");
        assert!(approx(ease(-1.0), 0.0), "clamped below 0");
        assert!(approx(ease(2.0), 1.0), "clamped above 1");
    }

    #[test]
    fn the_tween_endpoints_are_the_two_meshes() {
        let tween = MeshTween::new(vert(0.0), vert(4.0));
        assert!(
            approx(tween.at(0.0).vertices[0].position[1], 0.0),
            "at 0 → from"
        );
        assert!(
            approx(tween.at(1.0).vertices[0].position[1], 4.0),
            "at 1 → to"
        );
    }

    #[test]
    fn the_midpoint_is_halfway_between_the_shapes() {
        // Smoothstep(0.5) == 0.5, so a linear rise reads its true midpoint —
        // the intermediate the headless mid-animation still captures.
        let tween = MeshTween::new(vert(0.0), vert(4.0));
        let mid = tween.at(0.5);
        assert!(approx(mid.vertices[0].position[1], 2.0), "half height");
        assert!(approx(mid.vertices[0].color[0], 2.0), "colour eases too");
    }

    #[test]
    fn an_unchanged_vertex_stays_put_through_the_tween() {
        // from == to at a vertex → it never moves, so only the shaped region
        // animates when the tween runs over the whole mesh.
        let tween = MeshTween::new(vert(3.0), vert(3.0));
        assert!(approx(tween.at(0.3).vertices[0].position[1], 3.0));
    }

    #[test]
    fn interpolated_normals_stay_unit_length() {
        let from = Mesh {
            vertices: vec![Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                color: [0.0; 3],
            }],
        };
        let to = Mesh {
            vertices: vec![Vertex {
                position: [0.0, 1.0, 0.0],
                // A tilted (but still upward) face normal, as a raised step makes.
                normal: crate::math::normalize([0.7, 0.7, 0.0]),
                color: [1.0; 3],
            }],
        };
        let n = MeshTween::new(from, to).at(0.5).vertices[0].normal;
        let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!(approx(length, 1.0), "the eased normal is renormalised");
    }

    #[test]
    fn a_topology_mismatch_snaps_to_the_target() {
        let from = vert(0.0);
        let to = Mesh {
            vertices: vec![vert(4.0).vertices[0], vert(4.0).vertices[0]],
        };
        let snapped = MeshTween::new(from, to).at(0.5);
        assert_eq!(snapped.vertices.len(), 2, "degrades to the target mesh");
    }
}
