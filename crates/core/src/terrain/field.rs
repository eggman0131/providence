//! The vertex height field — terrain state (ADR 0017 §1).
//!
//! Height is an **integer** sampled at each grid **vertex** (corner); the
//! value at `(x, y)` is that corner's elevation, and a *face* (the square
//! spanning four corners) is derived from them, holding no state of its own.
//! Because heights are integers the field carries no floating-point state and
//! every operation over it is exact — a determinism benefit (I3).
//!
//! The governing rule is the *step invariant*
//! ([`HeightField::satisfies_step_invariant`]): orthogonally-adjacent vertices
//! differ in height by at most `sim.terrain.max_step`. This module supplies
//! the state and this predicate; the shaping operations that must preserve it
//! (raise/lower with a bounded cascade) live in [`super::shape`].

use alloc::vec::Vec;

/// A vertex's integer height. Signed so terrain can sit below the sea datum;
/// the absolute floor (sea level) arrives with worldgen (issue #7). The step
/// invariant needs no absolute floor — it constrains only differences.
pub type Height = i32;

/// An integer height field sampled at grid vertices (ADR 0017 §1).
///
/// Row-major and heap-backed: the vertex at `(x, y)` lives at
/// `y * width + x`. The grid does not wrap — edge vertices simply have fewer
/// in-grid neighbours, which is what "world-edge bounding" means here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeightField {
    width: u32,
    height: u32,
    /// Row-major heights; `cells.len() == width * height`, upheld by every
    /// constructor so indexing an in-bounds `(x, y)` never panics.
    cells: Vec<Height>,
}

impl HeightField {
    /// A field of `width × height` vertices all at `level`.
    ///
    /// The trusted constructor for tests and worldgen seeding; a flat field
    /// satisfies the step invariant for any `max_step`.
    #[must_use]
    pub fn flat(width: u32, height: u32, level: Height) -> Self {
        let count = width as usize * height as usize;
        Self {
            width,
            height,
            cells: alloc::vec::from_elem(level, count),
        }
    }

    /// A field from an explicit row-major buffer.
    ///
    /// Returns `None` unless both dimensions are non-zero **and**
    /// `cells.len()` equals `width × height`, so a `HeightField` can never
    /// hold a ragged or misaligned buffer.
    #[must_use]
    pub fn from_cells(width: u32, height: u32, cells: Vec<Height>) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        if cells.len() != width as usize * height as usize {
            return None;
        }
        Some(Self {
            width,
            height,
            cells,
        })
    }

    /// Width in vertices.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height (depth) in vertices.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The height at `(x, y)`, or `None` if the coordinate is out of bounds.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Option<Height> {
        if x < self.width && y < self.height {
            Some(self.cells[self.index(x, y)])
        } else {
            None
        }
    }

    /// Overwrite the height at `(x, y)`, returning the previous value, or
    /// `None` if the coordinate is out of bounds.
    ///
    /// Crate-internal: the shaping operations in [`super::shape`] are the only
    /// writers, so every mutation runs through the step-invariant-preserving
    /// cascade rather than poking cells directly.
    pub(crate) fn set(&mut self, x: u32, y: u32, value: Height) -> Option<Height> {
        if x < self.width && y < self.height {
            let index = self.index(x, y);
            let previous = self.cells[index];
            self.cells[index] = value;
            Some(previous)
        } else {
            None
        }
    }

    /// Row-major index of an in-bounds `(x, y)`.
    const fn index(&self, x: u32, y: u32) -> usize {
        y as usize * self.width as usize + x as usize
    }

    /// True iff every orthogonally-adjacent vertex pair differs in height by
    /// at most `max_step` — the step invariant every terrain operation must
    /// preserve (ADR 0017 §2).
    ///
    /// Only orthogonal (horizontal/vertical) pairs are bounded; diagonal
    /// pairs may differ by up to `2 × max_step`, giving the intended stepped
    /// look. Scanning each vertex's right and down neighbour visits every
    /// orthogonal pair exactly once.
    #[must_use]
    pub fn satisfies_step_invariant(&self, max_step: u32) -> bool {
        for y in 0..self.height {
            for x in 0..self.width {
                let here = self.cells[self.index(x, y)];
                if x + 1 < self.width {
                    let right = self.cells[self.index(x + 1, y)];
                    if here.abs_diff(right) > max_step {
                        return false;
                    }
                }
                if y + 1 < self.height {
                    let down = self.cells[self.index(x, y + 1)];
                    if here.abs_diff(down) > max_step {
                        return false;
                    }
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::HeightField;
    use alloc::vec;

    // The governed default (config/default.toml); a value ≠ 1 is untested
    // terrain per ADR 0017, so the unit tests pin the shipped step.
    const MAX_STEP: u32 = 1;

    #[test]
    fn flat_field_has_the_requested_dimensions_and_level() {
        let field = HeightField::flat(4, 3, 7);
        assert_eq!(field.width(), 4);
        assert_eq!(field.height(), 3);
        for y in 0..3 {
            for x in 0..4 {
                assert_eq!(
                    field.get(x, y),
                    Some(7),
                    "every vertex holds the fill level"
                );
            }
        }
    }

    #[test]
    fn get_is_none_outside_the_grid() {
        let field = HeightField::flat(2, 2, 0);
        assert_eq!(field.get(2, 0), None, "x past the right edge");
        assert_eq!(field.get(0, 2), None, "y past the bottom edge");
    }

    #[test]
    fn clone_equals_the_original() {
        let field = HeightField::flat(3, 3, 2);
        assert_eq!(
            field.clone(),
            field,
            "a clone is bit-equal (I3-friendly state)"
        );
    }

    #[test]
    fn from_cells_accepts_a_well_sized_buffer_row_major() {
        let field =
            HeightField::from_cells(2, 2, vec![0, 1, 1, 2]).expect("a 2×2 buffer must build");
        assert_eq!(field.get(0, 0), Some(0));
        assert_eq!(field.get(1, 0), Some(1));
        assert_eq!(field.get(0, 1), Some(1));
        assert_eq!(field.get(1, 1), Some(2));
    }

    #[test]
    fn from_cells_rejects_a_mismatched_length() {
        assert!(
            HeightField::from_cells(2, 2, vec![0, 1, 2]).is_none(),
            "3 cells cannot fill a 2×2 grid"
        );
    }

    #[test]
    fn from_cells_rejects_zero_dimensions() {
        assert!(
            HeightField::from_cells(0, 3, vec![]).is_none(),
            "zero width"
        );
        assert!(
            HeightField::from_cells(3, 0, vec![]).is_none(),
            "zero height"
        );
    }

    #[test]
    fn a_flat_field_always_satisfies_the_invariant() {
        assert!(HeightField::flat(5, 5, 3).satisfies_step_invariant(MAX_STEP));
    }

    #[test]
    fn a_single_step_slope_satisfies_the_invariant() {
        // Orthogonal neighbours differ by exactly max_step; the diagonal
        // (0,0)->(1,1) differs by 2, which the invariant deliberately allows.
        let field = HeightField::from_cells(2, 2, vec![0, 1, 1, 2]).unwrap();
        assert!(field.satisfies_step_invariant(MAX_STEP));
    }

    #[test]
    fn a_horizontal_cliff_violates_the_invariant() {
        // (0,0)->(1,0) differs by 2 > max_step.
        let field = HeightField::from_cells(2, 2, vec![0, 2, 0, 1]).unwrap();
        assert!(!field.satisfies_step_invariant(MAX_STEP));
    }

    #[test]
    fn a_vertical_cliff_violates_the_invariant() {
        // (0,0)->(0,1) differs by 3 > max_step.
        let field = HeightField::from_cells(2, 2, vec![0, 1, 3, 1]).unwrap();
        assert!(!field.satisfies_step_invariant(MAX_STEP));
    }

    #[test]
    fn max_step_admits_correspondingly_steeper_fields() {
        // The same field is illegal at max_step 1 but legal at 2 — the knob
        // does what it says, even though ≠ 1 is not a supported map (ADR 0017).
        let field = HeightField::from_cells(2, 2, vec![0, 2, 2, 4]).unwrap();
        assert!(!field.satisfies_step_invariant(1));
        assert!(field.satisfies_step_invariant(2));
    }
}
