//! Raise/lower shaping operations over the vertex height field (ADR 0017 §3).
//!
//! A shaping op nudges one vertex up or down by a single `max_step`, then
//! restores the step invariant by cascading the change outward: any neighbour
//! that would now sit more than `max_step` from a moved vertex is pulled to
//! the boundary value, and so on. The result is the *stepped plateau* the
//! design calls for — land reads as crafted, never a sheer cliff.
//!
//! The cascade is a FIFO relaxation from the target: a single source, uniform
//! step, moving heights monotonically in one direction, so it terminates on a
//! finite grid (the reach is bounded by `max_height / max_step`) and visits
//! each affected vertex once. Because the outcome is the *unique* minimal
//! (raise) or maximal (lower) field satisfying the invariant, it is
//! order-independent — hence trivially deterministic (I3). The functions are
//! pure and mutate in place: no clock, no I/O, no ambient randomness.
//!
//! **Precondition:** the field satisfies the step invariant on entry. Under
//! that precondition every op restores it; ADR 0017's cost model then falls
//! straight out — the price is the count of vertices actually moved.

use alloc::collections::VecDeque;

use providence_config::TerrainParams;

use super::field::{Height, HeightField};

/// The four orthogonal neighbour offsets — the only adjacency the step
/// invariant bounds (ADR 0017 §2). Declared as a slice, not a fixed-size
/// array, so no array-length literal appears; the offset components are only
/// `0`/`±1`.
const ORTHOGONAL: &[(i32, i32)] = &[(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The result of a [`raise`] / [`lower`]: how many vertices actually moved and
/// what shaping them costs.
///
/// The cost is **returned, not spent**: the follower economy is parked
/// (ADR 0019), so #6 reports the price a raise would charge without deducting
/// any mana.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapeOutcome {
    /// Vertices whose height actually changed — the target plus every vertex
    /// the cascade pulled with it. `0` for a no-op.
    pub moved: u32,
    /// `moved × sim.terrain.raise.mana_cost`: the mana a follower economy
    /// would charge for this shaping. `0` for a no-op.
    pub cost: u64,
}

impl ShapeOutcome {
    /// The no-op outcome — nothing moved, nothing owed. Returned when the
    /// target is out of bounds or already at the ceiling (ADR 0017 §3).
    pub const UNCHANGED: Self = Self { moved: 0, cost: 0 };
}

/// Which way a shaping op pushes the target vertex.
enum Direction {
    /// Push the target up one `max_step`, clamped at `max_height`.
    Raise,
    /// Push the target down one `max_step` (no floor until sea level, #7).
    Lower,
}

/// Raise the vertex at `(x, y)` by one `max_step`, cascading outward so the
/// step invariant is restored, and report what it moved and cost.
///
/// The target rises to `min(original + max_step, max_height)` and the change
/// descends outward as a stepped cone — the unique *minimal* field `≥` the
/// original that still satisfies the invariant, so the `max_height` ceiling is
/// respected automatically (every cascaded value is below the target). A
/// target already at the ceiling, or an out-of-bounds `(x, y)`, is a no-op
/// ([`ShapeOutcome::UNCHANGED`]).
///
/// Preserves the invariant iff the field satisfied it on entry (the operation
/// precondition). Pure and in place (I3).
pub fn raise(field: &mut HeightField, x: u32, y: u32, params: &TerrainParams) -> ShapeOutcome {
    shape(field, x, y, params, &Direction::Raise)
}

/// Lower the vertex at `(x, y)` by one `max_step`, cascading outward so the
/// step invariant is restored — the mirror of [`raise`].
///
/// The target drops to `original − max_step` and the change rises outward as
/// the unique *maximal* field `≤` the original that still satisfies the
/// invariant. Lowering has no floor in #6 (sea level arrives with worldgen,
/// #7); an out-of-bounds `(x, y)` is a no-op ([`ShapeOutcome::UNCHANGED`]).
///
/// Preserves the invariant iff the field satisfied it on entry. Pure and in
/// place (I3).
pub fn lower(field: &mut HeightField, x: u32, y: u32, params: &TerrainParams) -> ShapeOutcome {
    shape(field, x, y, params, &Direction::Lower)
}

/// Shared engine for [`raise`] / [`lower`]: pin the target, then relax the
/// grid back into the step invariant, counting the vertices that moved.
fn shape(
    field: &mut HeightField,
    x: u32,
    y: u32,
    params: &TerrainParams,
    direction: &Direction,
) -> ShapeOutcome {
    let Some(current) = field.get(x, y) else {
        return ShapeOutcome::UNCHANGED; // out of bounds — nothing to shape
    };

    // Bounds are computed in i64 so the extremes never panic on overflow;
    // heights are pinned back into range on write.
    let step = i64::from(params.max_step);
    let target = match direction {
        Direction::Raise => (i64::from(current) + step).min(i64::from(params.max_height)),
        Direction::Lower => i64::from(current) - step,
    };
    let target = as_height(target);

    let moves_target = match direction {
        Direction::Raise => target > current, // false at the ceiling → no-op
        Direction::Lower => target < current,
    };
    if !moves_target {
        return ShapeOutcome::UNCHANGED;
    }

    field.set(x, y, target);
    let mut moved: u32 = 1;
    let mut frontier = VecDeque::new();
    frontier.push_back((x, y));

    // FIFO relaxation: each dequeued vertex constrains its neighbours; any
    // that now violate the invariant are pulled to the boundary and enqueued.
    // Heights move one direction only, so a vertex is pulled at most once.
    while let Some((cx, cy)) = frontier.pop_front() {
        let here = field.get(cx, cy).expect("a queued vertex is in bounds");
        for &(dx, dy) in ORTHOGONAL {
            let Some((nx, ny)) = in_bounds_neighbour(field, cx, cy, dx, dy) else {
                continue; // off the grid — world-edge bounding, no wrap
            };
            let neighbour = field
                .get(nx, ny)
                .expect("an in-bounds neighbour is readable");
            // The value the neighbour must reach to sit within one step of
            // `here`: a floor when raising, a ceiling when lowering.
            let bound = match direction {
                Direction::Raise => i64::from(here) - step,
                Direction::Lower => i64::from(here) + step,
            };
            let violates = match direction {
                Direction::Raise => i64::from(neighbour) < bound,
                Direction::Lower => i64::from(neighbour) > bound,
            };
            if violates {
                field.set(nx, ny, as_height(bound));
                moved += 1;
                frontier.push_back((nx, ny));
            }
        }
    }

    ShapeOutcome {
        moved,
        cost: u64::from(moved) * u64::from(params.raise.mana_cost),
    }
}

/// The in-bounds orthogonal neighbour of `(x, y)` at offset `(dx, dy)`, or
/// `None` if it falls off the grid. `checked_add_signed` folds both an
/// underflow (`x == 0`, `dx == -1`) and an overflow into `None`, so the world
/// edge bounds the cascade with no wrap and no cast.
fn in_bounds_neighbour(
    field: &HeightField,
    x: u32,
    y: u32,
    dx: i32,
    dy: i32,
) -> Option<(u32, u32)> {
    let nx = x.checked_add_signed(dx)?;
    let ny = y.checked_add_signed(dy)?;
    (nx < field.width() && ny < field.height()).then_some((nx, ny))
}

/// Pin an i64 bound back into the [`Height`] range without panicking (a
/// saturating conversion). Ordinary fields sit far from the range ends, so
/// this only bites a pathologically deep stack of lowers.
fn as_height(value: i64) -> Height {
    Height::try_from(value).unwrap_or(if value < 0 { Height::MIN } else { Height::MAX })
}

#[cfg(test)]
mod tests {
    use super::{ShapeOutcome, lower, raise};
    use crate::terrain::HeightField;
    use providence_config::{RaiseParams, TerrainParams};

    // The governed default (config/default.toml): the model is written for a
    // unit step (ADR 0017), so the tests pin the shipped value.
    const MAX_STEP: u32 = 1;

    /// Params with an explicit ceiling and per-vertex cost, so the tests can
    /// assert both the clamp and the cost multiplier.
    fn params(max_height: i32, mana_cost: u32) -> TerrainParams {
        TerrainParams {
            max_step: MAX_STEP,
            max_height,
            raise: RaiseParams { mana_cost },
        }
    }

    /// Count vertices differing from `before`, an independent check on the
    /// `moved` counter the cascade maintains.
    fn changed_cells(before: &HeightField, after: &HeightField) -> u32 {
        let mut count = 0;
        for y in 0..after.height() {
            for x in 0..after.width() {
                if before.get(x, y) != after.get(x, y) {
                    count += 1;
                }
            }
        }
        count
    }

    #[test]
    fn a_single_raise_on_flat_ground_moves_only_the_target() {
        // Flat field: bumping one vertex by one step leaves it exactly one
        // step above its neighbours, so nothing cascades.
        let mut field = HeightField::flat(5, 5, 0);
        let outcome = raise(&mut field, 2, 2, &params(64, 1));
        assert_eq!(outcome.moved, 1, "only the target moves on flat ground");
        assert_eq!(outcome.cost, 1, "cost = moved × mana_cost");
        assert_eq!(field.get(2, 2), Some(1));
        assert_eq!(field.get(2, 1), Some(0), "neighbours untouched");
        assert!(field.satisfies_step_invariant(MAX_STEP));
    }

    #[test]
    fn a_second_raise_cascades_into_a_stepped_cone() {
        // Raising the same vertex twice forces the neighbours up a step: a
        // Manhattan-diamond plateau, height = target − distance.
        let mut field = HeightField::flat(7, 7, 0);
        raise(&mut field, 3, 3, &params(64, 1));
        let before = field.clone();
        let outcome = raise(&mut field, 3, 3, &params(64, 1));

        assert_eq!(field.get(3, 3), Some(2), "target rose to two steps");
        // The four orthogonal neighbours were pulled up one step.
        for (x, y) in [(2, 3), (4, 3), (3, 2), (3, 4)] {
            assert_eq!(field.get(x, y), Some(1), "ring pulled up to one");
        }
        assert_eq!(field.get(1, 3), Some(0), "two cells out stays flat");
        assert_eq!(outcome.moved, 5, "target + four neighbours moved");
        assert_eq!(outcome.cost, 5);
        assert_eq!(
            outcome.moved,
            changed_cells(&before, &field),
            "the counter matches an independent diff"
        );
        assert!(field.satisfies_step_invariant(MAX_STEP));
    }

    #[test]
    fn raising_at_the_ceiling_is_a_no_op() {
        // Every vertex already sits at max_height, so a raise cannot lift the
        // target and nothing moves.
        let ceiling = 3;
        let mut field = HeightField::flat(4, 4, ceiling);
        let before = field.clone();
        let outcome = raise(&mut field, 1, 1, &params(ceiling, 1));
        assert_eq!(outcome, ShapeOutcome::UNCHANGED, "clamped raise is a no-op");
        assert_eq!(field, before, "field untouched at the ceiling");
    }

    #[test]
    fn a_raise_never_exceeds_the_ceiling() {
        // A vertex one below the ceiling rises exactly to it, not past it.
        let ceiling = 2;
        let mut field = HeightField::flat(3, 3, ceiling - 1);
        let outcome = raise(&mut field, 1, 1, &params(ceiling, 1));
        assert_eq!(outcome.moved, 1);
        assert_eq!(field.get(1, 1), Some(ceiling), "clamped to the ceiling");
        assert!(field.satisfies_step_invariant(MAX_STEP));
    }

    #[test]
    fn raising_out_of_bounds_is_a_no_op() {
        let mut field = HeightField::flat(3, 3, 0);
        let before = field.clone();
        let outcome = raise(&mut field, 3, 0, &params(64, 1));
        assert_eq!(outcome, ShapeOutcome::UNCHANGED);
        assert_eq!(field, before);
    }

    #[test]
    fn cost_scales_with_the_configured_mana_cost() {
        // Same cascade (moved = 5), but a mana_cost of two doubles the price.
        let mut field = HeightField::flat(7, 7, 0);
        raise(&mut field, 3, 3, &params(64, 2));
        let outcome = raise(&mut field, 3, 3, &params(64, 2));
        assert_eq!(outcome.moved, 5);
        assert_eq!(outcome.cost, 10, "cost = 5 moved × 2 mana each");
    }

    #[test]
    fn raising_a_corner_cascades_only_inward() {
        // The corner has just two in-grid neighbours; the world edge bounds
        // the cascade (no wrap-around).
        let mut field = HeightField::flat(5, 5, 0);
        raise(&mut field, 0, 0, &params(64, 1));
        let outcome = raise(&mut field, 0, 0, &params(64, 1));
        assert_eq!(field.get(0, 0), Some(2), "corner target rose two steps");
        assert_eq!(field.get(1, 0), Some(1), "east neighbour pulled up");
        assert_eq!(field.get(0, 1), Some(1), "south neighbour pulled up");
        assert_eq!(outcome.moved, 3, "corner + its two in-grid neighbours");
        assert!(field.satisfies_step_invariant(MAX_STEP));
    }

    #[test]
    fn lower_is_the_mirror_of_raise() {
        // Dropping a vertex twice pulls its neighbours down a step — the same
        // diamond, inverted.
        let mut field = HeightField::flat(7, 7, 5);
        lower(&mut field, 3, 3, &params(64, 1));
        let outcome = lower(&mut field, 3, 3, &params(64, 1));
        assert_eq!(field.get(3, 3), Some(3), "target dropped two steps");
        for (x, y) in [(2, 3), (4, 3), (3, 2), (3, 4)] {
            assert_eq!(field.get(x, y), Some(4), "ring pulled down to four");
        }
        assert_eq!(outcome.moved, 5, "target + four neighbours moved");
        assert_eq!(outcome.cost, 5);
        assert!(field.satisfies_step_invariant(MAX_STEP));
    }

    #[test]
    fn lowering_ignores_the_ceiling_and_needs_no_floor() {
        // A single lower always moves the target (there is no floor in #6).
        let mut field = HeightField::flat(3, 3, 0);
        let outcome = lower(&mut field, 1, 1, &params(64, 1));
        assert_eq!(outcome.moved, 1);
        assert_eq!(field.get(1, 1), Some(-1), "target dropped below the datum");
        assert!(field.satisfies_step_invariant(MAX_STEP));
    }
}
