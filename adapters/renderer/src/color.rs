//! Height → colour for the terrain surface (issue #8 Phase 1 palette).
//!
//! Pure and GPU-free: the flat-shaded renderer colours the surface by height,
//! lerping the configured palette from `low_rgb` at the lowest drawn height to
//! `high_rgb` at the highest. Kept here and unit-tested so the palette is
//! correct before any GPU code exists.

use providence_config::PaletteParams;

/// Linear-RGB colour for `height`, interpolating `palette` over the drawn
/// height range `[min_height, max_height]`.
///
/// Heights are clamped into the range, so out-of-range values saturate at the
/// nearest palette anchor rather than extrapolating. A degenerate range
/// (`min_height >= max_height`) collapses to `low_rgb`.
#[must_use]
pub fn height_color(
    height: i32,
    min_height: i32,
    max_height: i32,
    palette: &PaletteParams,
) -> [f32; 3] {
    let t = normalized_height(height, min_height, max_height);
    lerp_rgb(palette.low_rgb, palette.high_rgb, t)
}

/// Position of `height` within `[min, max]` as a fraction in `[0, 1]`.
fn normalized_height(height: i32, min: i32, max: i32) -> f32 {
    if max <= min {
        return 0.0;
    }
    let span = (max - min) as f32;
    let offset = (height.clamp(min, max) - min) as f32;
    offset / span
}

/// Component-wise linear interpolation between two RGB colours.
fn lerp_rgb(low: [f32; 3], high: [f32; 3], t: f32) -> [f32; 3] {
    [
        low[0] + (high[0] - low[0]) * t,
        low[1] + (high[1] - low[1]) * t,
        low[2] + (high[2] - low[2]) * t,
    ]
}

#[cfg(test)]
mod tests {
    use super::{height_color, normalized_height};
    use providence_config::PaletteParams;

    const PALETTE: PaletteParams = PaletteParams {
        low_rgb: [0.0, 0.0, 0.0],
        high_rgb: [1.0, 1.0, 1.0],
    };

    /// Floats compared within a tolerance (clippy forbids `==` on floats).
    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-5
    }

    /// Element-wise [`approx`] for an RGB triple.
    fn approx3(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= 1e-5)
    }

    #[test]
    fn endpoints_map_to_the_palette_anchors() {
        assert!(approx3(height_color(0, 0, 10, &PALETTE), [0.0, 0.0, 0.0]));
        assert!(approx3(height_color(10, 0, 10, &PALETTE), [1.0, 1.0, 1.0]));
    }

    #[test]
    fn midpoint_is_the_halfway_colour() {
        assert!(approx3(height_color(5, 0, 10, &PALETTE), [0.5, 0.5, 0.5]));
    }

    #[test]
    fn out_of_range_heights_saturate_at_the_anchors() {
        assert!(approx3(height_color(-4, 0, 10, &PALETTE), [0.0, 0.0, 0.0]));
        assert!(approx3(height_color(99, 0, 10, &PALETTE), [1.0, 1.0, 1.0]));
    }

    #[test]
    fn a_degenerate_range_collapses_to_low() {
        assert!(approx(normalized_height(7, 5, 5), 0.0));
        assert!(approx3(height_color(7, 5, 5, &PALETTE), [0.0, 0.0, 0.0]));
    }
}
