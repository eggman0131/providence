//! Composition root (docs/20-architecture.md §2.3): wires adapters to ports
//! at startup and launches the application.
//!
//! Subcommands (dev tooling — the real game loop lands in later phases):
//! - *(none)* — a smoke run proving the config → params → session pipeline,
//!   plus a textual terrain demo (issue #6 §5).
//! - `workbench` — open the on-screen 3D terrain workbench (issue #8, ADR 0020):
//!   a lit height field the Director can look at. Needs a display.
//! - `capture [PATH]` — render the same scene headlessly to a PNG (ADR 0020 §2),
//!   the agents-only visual self-check used by `/verify`. No display required.
//!
//! The composition root is the only crate permitted to name concrete adapters
//! (docs/20-architecture.md §5.2): it projects `render.*` into `RenderParams`,
//! builds a [`TerrainFrame`] snapshot from a core height field, and hands it to
//! a [`RendererPort`]. The renderer only ever sees the derived snapshot, never
//! the core (ADR 0020 §1).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use providence_config::{Params, RenderParams, TerrainParams};
use providence_core::terrain::{HeightField, raise};
use providence_ports::{RendererPort, TerrainFrame};
use providence_renderer::{HeadlessRenderer, NoopRenderer, WindowRenderer};

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

/// Workbench scene (dev tooling): a larger field with a few overlapping hills so
/// the flat-shaded steps, height colouring, and lighting are all visible when
/// the land is seen in 3D.
const WORKBENCH_SIZE: u32 = 32;
/// Hills as `(x, y, raises)` — a tall central peak plus two smaller offset
/// rises, giving asymmetric relief to judge (issue #8; ADR 0019 "seen and felt").
const WORKBENCH_HILLS: &[(u32, u32, u32)] = &[(16, 16, 9), (8, 22, 5), (24, 9, 6)];
/// Default output path for a `capture` with no explicit path argument.
const DEFAULT_CAPTURE_PATH: &str = "target/workbench.png";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => smoke_run(),
        Some("workbench") => run_workbench(),
        Some("capture") => run_capture(args.get(1).map(PathBuf::from)),
        Some(other) => {
            eprintln!("providence: unknown subcommand `{other}` (try: workbench | capture [PATH])");
            ExitCode::FAILURE
        }
    }
}

/// The default smoke run: load config, print the textual terrain demo, prove
/// the `RendererPort` seam with the no-op renderer, then advance a session.
fn smoke_run() -> ExitCode {
    let params = match load_params() {
        Ok(params) => params,
        Err(code) => return code,
    };

    let field = print_terrain_demo(&params.sim.terrain);

    let render = match load_render() {
        Ok(render) => render,
        Err(code) => return code,
    };
    present_demo_frame(&field, &render);

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

/// Open the on-screen 3D workbench (issue #8 Phase 1). Builds the workbench
/// field, presents it through the windowed [`WindowRenderer`], and runs the
/// event loop until the window closes.
fn run_workbench() -> ExitCode {
    let (params, render) = match (load_params(), load_render()) {
        (Ok(params), Ok(render)) => (params, render),
        (Err(code), _) | (_, Err(code)) => return code,
    };

    let field = build_workbench_field(&params.sim.terrain);
    let heights = frame_heights(&field);
    let frame = TerrainFrame::new(field.width(), field.height(), &heights);

    println!("providence: opening the terrain workbench — close the window to exit.");
    let mut renderer = WindowRenderer::new(render);
    renderer.present(frame);
    match renderer.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("providence: workbench error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Render the workbench scene headlessly to a PNG (ADR 0020 §2) — the
/// display-free visual self-check for `/verify`.
fn run_capture(path: Option<PathBuf>) -> ExitCode {
    let (params, render) = match (load_params(), load_render()) {
        (Ok(params), Ok(render)) => (params, render),
        (Err(code), _) | (_, Err(code)) => return code,
    };

    let field = build_workbench_field(&params.sim.terrain);
    let heights = frame_heights(&field);
    let frame = TerrainFrame::new(field.width(), field.height(), &heights);
    let path = path.unwrap_or_else(|| PathBuf::from(DEFAULT_CAPTURE_PATH));

    let mut renderer = HeadlessRenderer::new(render);
    renderer.present(frame);
    match renderer.capture(&path) {
        Ok(()) => {
            println!(
                "providence: captured a {}×{} terrain workbench frame to {}",
                field.width(),
                field.height(),
                path.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("providence: capture error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Load and validate the core params from `config/`, mapping a failure to a
/// printed error and a failure exit code.
fn load_params() -> Result<Params, ExitCode> {
    providence_config_loader::load_dir(Path::new("config")).map_err(|error| {
        eprintln!("providence: config error: {error}");
        ExitCode::FAILURE
    })
}

/// Load and validate the presentation params (`render.*`) from `config/`.
fn load_render() -> Result<RenderParams, ExitCode> {
    providence_config_loader::load_render(Path::new("config")).map_err(|error| {
        eprintln!("providence: render config error: {error}");
        ExitCode::FAILURE
    })
}

/// Build the richer workbench field: raise each configured hill in turn on a
/// flat field, letting the core's cascade shape the stepped relief.
fn build_workbench_field(terrain: &TerrainParams) -> HeightField {
    let mut field = HeightField::flat(WORKBENCH_SIZE, WORKBENCH_SIZE, 0);
    for &(x, y, raises) in WORKBENCH_HILLS {
        for _ in 0..raises {
            raise(&mut field, x, y, terrain);
        }
    }
    field
}

/// Flatten a height field into the row-major buffer a [`TerrainFrame`] borrows.
fn frame_heights(field: &HeightField) -> Vec<i32> {
    let mut heights = Vec::with_capacity(field.width() as usize * field.height() as usize);
    for y in 0..field.height() {
        for x in 0..field.width() {
            heights.push(field.get(x, y).unwrap_or_default());
        }
    }
    heights
}

/// Build a flat field, raise its centre `DEMO_RAISES` times, and print the
/// resulting stepped plateau as an ASCII heightmap — the honest, textual
/// "verified" observation for issue #6 before the 3D workbench (§5).
/// Returns the built field so the workbench seam (below) can present it.
fn print_terrain_demo(terrain: &TerrainParams) -> HeightField {
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

    field
}

/// Present the demo field through the no-op renderer — the GPU-free proof that
/// the `RendererPort` seam (ADR 0020) is wired end-to-end. Builds a row-major
/// [`TerrainFrame`] snapshot from the field and hands it to a [`NoopRenderer`],
/// exactly as the on-screen adapter does. `render` is echoed to show the
/// presentation-config projection is live.
fn present_demo_frame(field: &HeightField, render: &RenderParams) {
    let heights = frame_heights(field);
    let frame = TerrainFrame::new(field.width(), field.height(), &heights);
    let mut renderer = NoopRenderer::new();
    renderer.present(frame);

    println!(
        "providence: workbench seam OK — presented a {w}×{d} frame via NoopRenderer \
         ({n} frame(s); palette low {low:?}, background {bg:?})",
        w = field.width(),
        d = field.height(),
        n = renderer.presented(),
        low = render.palette.low_rgb,
        bg = render.background.rgb,
    );
}

/// Map an integer height to its ASCII glyph, saturating at the tallest glyph.
fn glyph_for(height: i32) -> char {
    let index = usize::try_from(height.max(0)).unwrap_or(0);
    let last = HEIGHT_GLYPHS.len() - 1;
    char::from(HEIGHT_GLYPHS[index.min(last)])
}
