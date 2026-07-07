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

/// Per-vertex start delays (ms) for a **ripple** (ADR 0022 §5; issue #9/#10
/// Phase 4): each vertex's settle begins `delay` after the command, so outer
/// vertices lag and the change ripples *outward* from the shaped point rather
/// than the whole cascade rising at once.
///
/// `center_xz` is the shaped vertex's world `(x, z)`; a vertex's delay is its
/// horizontal distance from it times `ms_per_unit`. With unit grid spacing that
/// distance is the vertex's ring. `ms_per_unit == 0` yields all-zero delays —
/// the whole change settles together (the Phase-3 behaviour).
#[must_use]
pub fn ripple_delays(vertices: &[Vertex], center_xz: [f32; 2], ms_per_unit: f32) -> Vec<f32> {
    vertices
        .iter()
        .map(|v| {
            let dx = v.position[0] - center_xz[0];
            let dz = v.position[2] - center_xz[1];
            (dx * dx + dz * dz).sqrt() * ms_per_unit
        })
        .collect()
}

/// A render-only tween between two same-topology surfaces (ADR 0022 §5): the old
/// drawn [`Mesh`] and the new one after a shaping command, with a per-vertex
/// ripple **delay** so outer vertices settle later ([`ripple_delays`]).
/// [`at`](MeshTween::at) returns the eased in-between surface at an elapsed time.
///
/// If the two meshes ever differ in vertex count (they never do under shaping —
/// the grid is fixed — but a defensive guard beats a panic), the tween degrades
/// to showing the target immediately.
#[derive(Clone, Debug)]
pub struct MeshTween {
    from: Mesh,
    to: Mesh,
    /// Per-vertex start delay (ms), parallel to `to.vertices`.
    delays: Vec<f32>,
    /// The largest delay, so callers know when the whole ripple has finished.
    max_delay: f32,
}

impl MeshTween {
    /// Tween from the currently-drawn surface `from` to the post-command surface
    /// `to`, with per-vertex start `delays` (ms) — build them with
    /// [`ripple_delays`], or pass all-zero for a uniform settle. A `delays`
    /// length that does not match `to.vertices` is treated as no delay.
    #[must_use]
    pub fn new(from: Mesh, to: Mesh, mut delays: Vec<f32>) -> Self {
        // Only vertices whose *height* actually changes should ripple. A shaping
        // command can shift the whole surface's colour (the mesh colours by the
        // frame's global height range, which grows when the tallest point rises),
        // so unmoved far vertices differ in *colour* yet never move. Give them no
        // ripple delay — they ease their recolour immediately, avoiding a pop when
        // the animation ends — and exclude them from `max_delay`, so the length
        // spans the cascade that moved, not a ripple crossing stationary land.
        // Height is compared by bits: same integer height ⇒ identical float ⇒
        // identical bits (an exact test, not a fuzzy float compare).
        let mut max_delay = 0.0_f32;
        for (delay, (a, b)) in delays
            .iter_mut()
            .zip(from.vertices.iter().zip(to.vertices.iter()))
        {
            if a.position[1].to_bits() == b.position[1].to_bits() {
                *delay = 0.0; // height unchanged — no ripple, only a recolour
            } else {
                max_delay = max_delay.max(*delay);
            }
        }
        Self {
            from,
            to,
            delays,
            max_delay,
        }
    }

    /// The target surface — what the tween settles to.
    #[must_use]
    pub fn target(&self) -> &Mesh {
        &self.to
    }

    /// When the whole animation has finished: the last vertex to start (its
    /// `max_delay`) plus one `duration_ms` settle.
    #[must_use]
    pub fn total_ms(&self, duration_ms: f32) -> f32 {
        self.max_delay + duration_ms.max(0.0)
    }

    /// The eased in-between surface at `elapsed_ms` since the command, each vertex
    /// settling over `duration_ms` starting at its ripple `delay`. Before its
    /// delay a vertex sits at the old shape; past `delay + duration_ms` it rests
    /// at the new one. Smoothstep-eased ([`ease`]); each vertex's position,
    /// colour, and flat normal is lerped and the normal renormalised (safe:
    /// heightfield face normals all point up, so their lerp is never
    /// zero-length). Unchanged vertices (`from == to`) stay put regardless, so
    /// only the shaped region moves.
    #[must_use]
    pub fn at(&self, elapsed_ms: f32, duration_ms: f32) -> Mesh {
        if self.from.vertices.len() != self.to.vertices.len() {
            return self.to.clone(); // topology changed — snap rather than mangle
        }
        let vertices = self
            .from
            .vertices
            .iter()
            .zip(self.to.vertices.iter())
            .enumerate()
            .map(|(i, (a, b))| {
                let delay = self.delays.get(i).copied().unwrap_or(0.0);
                let t = ease(progress(elapsed_ms - delay, duration_ms));
                Vertex {
                    position: lerp3(a.position, b.position, t),
                    normal: normalize(lerp3(a.normal, b.normal, t)),
                    color: lerp3(a.color, b.color, t),
                }
            })
            .collect();
        Mesh { vertices }
    }
}

#[cfg(test)]
mod tests {
    use super::{MeshTween, ease, progress, ripple_delays};
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

    /// A mesh of vertices at `(x, height)` on the z = 0 line — so their world
    /// distance from a centre varies, exercising the ripple.
    fn line_mesh(points: &[(f32, f32)]) -> Mesh {
        Mesh {
            vertices: points
                .iter()
                .map(|&(x, y)| Vertex {
                    position: [x, y, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    color: [y, y, y],
                })
                .collect(),
        }
    }

    /// All-zero delays for an `n`-vertex tween — a uniform (no-ripple) settle.
    fn uniform(n: usize) -> Vec<f32> {
        vec![0.0; n]
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
        let tween = MeshTween::new(vert(0.0), vert(4.0), uniform(1));
        assert!(
            approx(tween.at(0.0, 200.0).vertices[0].position[1], 0.0),
            "elapsed 0 → from"
        );
        assert!(
            approx(tween.at(200.0, 200.0).vertices[0].position[1], 4.0),
            "elapsed == duration → to"
        );
    }

    #[test]
    fn the_midpoint_is_halfway_between_the_shapes() {
        // Smoothstep(0.5) == 0.5, so a linear rise reads its true midpoint at
        // half the (undelayed) duration — the intermediate a still captures.
        let tween = MeshTween::new(vert(0.0), vert(4.0), uniform(1));
        let mid = tween.at(100.0, 200.0);
        assert!(approx(mid.vertices[0].position[1], 2.0), "half height");
        assert!(approx(mid.vertices[0].color[0], 2.0), "colour eases too");
    }

    #[test]
    fn an_unchanged_vertex_stays_put_through_the_tween() {
        // from == to at a vertex → it never moves, so only the shaped region
        // animates when the tween runs over the whole mesh.
        let tween = MeshTween::new(vert(3.0), vert(3.0), uniform(1));
        assert!(approx(tween.at(60.0, 200.0).vertices[0].position[1], 3.0));
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
        let n = MeshTween::new(from, to, uniform(1))
            .at(100.0, 200.0)
            .vertices[0]
            .normal;
        let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!(approx(length, 1.0), "the eased normal is renormalised");
    }

    #[test]
    fn ripple_delays_grow_with_distance_from_the_centre() {
        // Centre vertex → no delay; a vertex 3 units out lags by 3 × ms_per_unit.
        let verts = line_mesh(&[(0.0, 0.0), (3.0, 0.0)]).vertices;
        let delays = ripple_delays(&verts, [0.0, 0.0], 10.0);
        assert!(approx(delays[0], 0.0), "the shaped vertex starts at once");
        assert!(approx(delays[1], 30.0), "distance 3 → 30ms of lag");
    }

    #[test]
    fn a_zero_ms_per_unit_gives_no_ripple() {
        let verts = line_mesh(&[(0.0, 0.0), (5.0, 0.0)]).vertices;
        let delays = ripple_delays(&verts, [0.0, 0.0], 0.0);
        assert!(delays.iter().all(|&d| approx(d, 0.0)), "no stagger");
    }

    #[test]
    fn the_ripple_makes_an_outer_vertex_lag_an_inner_one() {
        // Inner vertex at the centre (delay 0), outer 3 units out (delay 30ms).
        // At 20ms the inner has begun rising while the outer still waits — the
        // outward ripple (ADR 0022 §5; Phase 4).
        let from = line_mesh(&[(0.0, 0.0), (3.0, 0.0)]);
        let to = line_mesh(&[(0.0, 4.0), (3.0, 4.0)]);
        let delays = ripple_delays(&to.vertices, [0.0, 0.0], 10.0);
        let tween = MeshTween::new(from, to, delays);
        let mesh = tween.at(20.0, 100.0);
        assert!(
            mesh.vertices[0].position[1] > 0.0,
            "the inner vertex has begun rising",
        );
        assert!(
            approx(mesh.vertices[1].position[1], 0.0),
            "the outer vertex still waits out its ripple delay",
        );
    }

    #[test]
    fn total_ms_spans_the_last_delay_plus_a_full_settle() {
        let from = line_mesh(&[(0.0, 0.0), (3.0, 0.0)]);
        let to = line_mesh(&[(0.0, 4.0), (3.0, 4.0)]);
        let delays = ripple_delays(&to.vertices, [0.0, 0.0], 10.0); // max 30
        let tween = MeshTween::new(from, to, delays);
        assert!(
            approx(tween.total_ms(100.0), 130.0),
            "30ms lag + 100ms settle"
        );
        // Past the total, the outer (last) vertex has fully settled.
        let done = tween.at(tween.total_ms(100.0), 100.0);
        assert!(
            approx(done.vertices[1].position[1], 4.0),
            "outer vertex arrived"
        );
    }

    #[test]
    fn an_unchanged_far_vertex_does_not_stretch_the_animation() {
        // A near vertex that changes (delay 0) and a far vertex that does NOT
        // change (delay 500ms). Only the moving vertex bounds `total_ms`, so the
        // animation ends when the cascade settles — not when a ripple reaches
        // stationary far land (the far-field over-run this guards).
        let from = line_mesh(&[(0.0, 0.0), (50.0, 0.0)]);
        let to = line_mesh(&[(0.0, 4.0), (50.0, 0.0)]); // far vertex unchanged (0→0)
        let delays = ripple_delays(&to.vertices, [0.0, 0.0], 10.0); // [0, 500]
        let tween = MeshTween::new(from, to, delays);
        assert!(
            approx(tween.total_ms(100.0), 100.0),
            "only the moving near vertex bounds the length",
        );
    }

    #[test]
    fn a_recolour_without_a_height_change_does_not_ripple() {
        // A shaping op can recolour the whole surface (the global height range
        // grows when the tallest point rises). A far vertex whose HEIGHT is
        // unchanged but COLOUR differs must not ripple — else it stretches the
        // animation across stationary land. Movement is judged by height, so a
        // recolour-only vertex adds no lag.
        let from = Mesh {
            vertices: vec![Vertex {
                position: [50.0, 2.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                color: [0.5, 0.5, 0.5],
            }],
        };
        let to = Mesh {
            vertices: vec![Vertex {
                position: [50.0, 2.0, 0.0], // same height
                normal: [0.0, 1.0, 0.0],
                color: [0.3, 0.3, 0.3], // new colour
            }],
        };
        let delays = ripple_delays(&to.vertices, [0.0, 0.0], 10.0); // 500ms by distance
        let tween = MeshTween::new(from, to, delays);
        assert!(
            approx(tween.total_ms(100.0), 100.0),
            "a recolour-only vertex adds no ripple lag",
        );
    }

    #[test]
    fn a_topology_mismatch_snaps_to_the_target() {
        let from = vert(0.0);
        let to = Mesh {
            vertices: vec![vert(4.0).vertices[0], vert(4.0).vertices[0]],
        };
        let snapped = MeshTween::new(from, to, uniform(2)).at(100.0, 200.0);
        assert_eq!(snapped.vertices.len(), 2, "degrades to the target mesh");
    }
}
