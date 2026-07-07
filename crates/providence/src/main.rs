//! Composition root (docs/20-architecture.md §2.3): wires adapters to ports
//! at startup and launches the application.
//!
//! Phase-1 scope: a smoke binary proving the config → params → session
//! pipeline end-to-end (contract §3 "Verified"). It also renders a small
//! terrain shaping demo (issue #6 §5) — the pre-workbench, textual surrogate
//! for "seen and felt": a stepped plateau you can eyeball until the 3D
//! workbench (#8/#9) lets the land be seen in motion. Renderer/input/
//! persistence wiring lands in Phase 4+.

use std::path::Path;
use std::process::ExitCode;

use providence_config::TerrainParams;
use providence_core::terrain::{HeightField, raise};

/// Fixed demo values for the smoke run — not behavioural config (the smoke
/// run is dev tooling, not gameplay; real sessions take seed and length
/// from scenario config in later phases).
const SMOKE_SEED: u64 = 0xD1CE;
const SMOKE_STEPS: u64 = 100;

/// Terrain demo dimensions and shaping (dev tooling, not gameplay): an odd
/// side so the field has a true centre vertex, raised a few times to build a
/// visible stepped cone.
const DEMO_SIZE: u32 = 11;
const DEMO_RAISES: u32 = 3;
/// Glyph per integer height for the ASCII heightmap, `0` first. Heights in the
/// demo stay within its length; taller vertices reuse the last glyph.
const HEIGHT_GLYPHS: &[u8] = b".:-=+*#%@";

fn main() -> ExitCode {
    let params = match providence_config_loader::load_dir(Path::new("config")) {
        Ok(params) => params,
        Err(error) => {
            eprintln!("providence: config error: {error}");
            return ExitCode::FAILURE;
        }
    };

    print_terrain_demo(&params.sim.terrain);

    let mut session = providence_app::Session::new(params, SMOKE_SEED);
    for _ in 0..SMOKE_STEPS {
        session.advance();
    }

    println!(
        "providence: gate scaffold OK — tick {} after {} steps (seed {SMOKE_SEED:#x})",
        session.state().tick,
        SMOKE_STEPS
    );
    ExitCode::SUCCESS
}

/// Build a flat field, raise its centre `DEMO_RAISES` times, and print the
/// resulting stepped plateau as an ASCII heightmap — the honest, textual
/// "verified" observation for issue #6 before the 3D workbench exists (§5).
fn print_terrain_demo(terrain: &TerrainParams) {
    let mid = DEMO_SIZE / 2;
    let mut field = HeightField::flat(DEMO_SIZE, DEMO_SIZE, 0);

    let mut total_moved: u32 = 0;
    let mut total_cost: u64 = 0;
    for _ in 0..DEMO_RAISES {
        let outcome = raise(&mut field, mid, mid, terrain);
        total_moved += outcome.moved;
        total_cost += outcome.cost;
    }

    println!(
        "providence: terrain demo — {size}×{size} field, centre raised {n}× \
         (max_step {step}, ceiling {ceiling}):",
        size = DEMO_SIZE,
        n = DEMO_RAISES,
        step = terrain.max_step,
        ceiling = terrain.max_height,
    );
    for y in 0..field.height() {
        let mut row = String::new();
        for x in 0..field.width() {
            let height = field.get(x, y).unwrap_or_default();
            row.push(glyph_for(height));
        }
        println!("  {row}");
    }
    println!(
        "  moved {total_moved} vertices, cost {total_cost}, invariant held = {}",
        field.satisfies_step_invariant(terrain.max_step),
    );
}

/// Map an integer height to its ASCII glyph, saturating at the tallest glyph.
fn glyph_for(height: i32) -> char {
    let index = usize::try_from(height.max(0)).unwrap_or(0);
    let last = HEIGHT_GLYPHS.len() - 1;
    char::from(HEIGHT_GLYPHS[index.min(last)])
}
