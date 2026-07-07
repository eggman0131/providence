//! Terrain — the integer vertex height field and (from Phase 2) its shaping
//! operations (ADR 0017). The land is the game's primary substrate, built to
//! real depth first (ADR 0019).
//!
//! Phase 1 (issue #6) supplied the state type and the *step invariant*
//! predicate every terrain operation must preserve. Phase 2 adds [`raise`] /
//! [`lower`] with the bounded cascade; the randomised invariant property test
//! and the replay golden land in Phase 3.

mod field;
mod shape;

pub use field::{Height, HeightField};
pub use shape::{ShapeOutcome, lower, raise};
