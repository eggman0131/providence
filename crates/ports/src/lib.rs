//! Port interfaces (docs/20-architecture.md §2.4).
//!
//! Ports are the trait boundary between the application and its adapters:
//! every side effect crosses one (I2/I4). This crate stays `no_std` and
//! depends on nothing — the DTOs a port hands across are defined *here*, so the
//! interface layer never imports `providence-core` and no adapter does either
//! (ADR 0020 §1). Adding or changing a port is an architectural change and
//! requires an ADR (docs/20-architecture.md §5 rule 5).
//!
//! Realised ports:
//! - [`RendererPort`] — present the terrain as a drawable [`TerrainFrame`]
//!   snapshot; the workbench renderer adapter implements it (ADR 0020).
//! - [`SimDriver`] — the interactive seam (ADR 0022): the renderer *holds* one
//!   to submit a discrete [`TerrainCommand`] and pull fresh snapshots to draw;
//!   the application implements it over a terrain world and its recorded log.
//!   The remaining ports (`ConfigPort`, `LLMOpponentPort`, …) land with the
//!   subsystems that need them, each behind its own ADR.

#![no_std]
#![forbid(unsafe_code)]

/// A vertex's integer height in a [`TerrainFrame`]. Mirrors the core's `Height`
/// (ADR 0017) as a plain `i32` so `providence-ports` need not — and must not —
/// import `providence-core`: the frame is a *derived snapshot*, not core state.
pub type Height = i32;

/// A vertex's derived terrain type in a [`TerrainFrame`] (ADR 0023). Mirrors the
/// core's `TerrainType` (ADR 0017 §1) as a plain `ports` enum for the same
/// reason [`Height`] mirrors the core height: it is a *derived* value the
/// renderer draws — the shared carrier for the look *and* future gameplay
/// (snow slows breeding, beaches forbid trees) — so defining it here keeps the
/// interface crate (and every adapter) free of a `providence-core` import.
///
/// The application classifies each vertex once (via the core's `classify_vertex`)
/// and hands the result across; the renderer keys the material band on it and
/// never re-derives the model's rules (ADR 0020 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainType {
    /// At or below the sea-level datum — underwater seabed.
    Water,
    /// Dry land within the shore band just above sea level — the coastline.
    Shore,
    /// Ordinary dry land between shore and mountain.
    Land,
    /// High ground at or above the mountain threshold.
    Mountain,
}

/// A read-only, derived snapshot of the terrain the application hands a
/// [`RendererPort`] to draw (ADR 0020 §1; grown per ADR 0023).
///
/// It carries only *derived* data a renderer needs — the grid dimensions, a
/// borrow of the row-major heights, a borrow of the row-major per-vertex
/// terrain [`types`](TerrainFrame::types), and the
/// [`waterline`](TerrainFrame::waterline) datum — and **no** simulation or
/// camera/view state. The application builds one from the core's height field,
/// its classification, and `sim.worldgen.sea_level`, and passes it in; the
/// renderer only ever sees this snapshot, never the core. Row-major: the vertex
/// at `(x, y)` is `heights[y * width + x]` (and likewise `types`).
///
/// The `types` slice may be **empty** for a frame built only to read heights —
/// e.g. the picking snapshot, which never colours anything ([`type_at`] then
/// yields `None`). A frame built to *draw* carries a full `width * height` slice.
/// The `waterline` is the sea-level datum a renderer floats its water surface at
/// (ADR 0023, Phase 2); a picking frame that never draws water passes any value
/// (conventionally `0`) since nothing reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainFrame<'a> {
    width: u32,
    height: u32,
    heights: &'a [Height],
    types: &'a [TerrainType],
    waterline: Height,
}

impl<'a> TerrainFrame<'a> {
    /// Wrap a row-major height buffer, its per-vertex terrain types, and the
    /// sea-level `waterline` datum as a drawable snapshot.
    ///
    /// `heights` and `types` are each expected to be `width * height` long in
    /// row-major order; [`get`](TerrainFrame::get) / [`type_at`](TerrainFrame::type_at)
    /// bounds-check every access, so a mismatched (or, for `types`, deliberately
    /// empty) buffer yields `None` rather than a panic. `waterline` is the
    /// `sim.worldgen.sea_level` datum the renderer draws its water surface at
    /// (ADR 0023, Phase 2); a heights-only picking frame passes `0` (unread).
    #[must_use]
    pub const fn new(
        width: u32,
        height: u32,
        heights: &'a [Height],
        types: &'a [TerrainType],
        waterline: Height,
    ) -> Self {
        Self {
            width,
            height,
            heights,
            types,
            waterline,
        }
    }

    /// Grid width in vertices.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Grid height (depth) in vertices.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The backing row-major height buffer.
    #[must_use]
    pub const fn heights(&self) -> &[Height] {
        self.heights
    }

    /// The backing row-major terrain-type buffer (ADR 0023). Empty for a
    /// heights-only frame (e.g. picking).
    #[must_use]
    pub const fn types(&self) -> &[TerrainType] {
        self.types
    }

    /// The sea-level datum (`sim.worldgen.sea_level`) the renderer floats its
    /// water surface at (ADR 0023, Phase 2). Vertices at or below it are
    /// underwater; land emerging above it reveals the coastline as the
    /// alpha-blended water plane is occluded — so a shaping edit moves the
    /// visible shoreline for free (the datum itself stays constant).
    #[must_use]
    pub const fn waterline(&self) -> Height {
        self.waterline
    }

    /// The height at `(x, y)`, or `None` if the coordinate is out of bounds or
    /// the backing buffer is too short for the stated dimensions.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Option<Height> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = y as usize * self.width as usize + x as usize;
        self.heights.get(index).copied()
    }

    /// The terrain type at `(x, y)`, or `None` if the coordinate is out of
    /// bounds or the `types` buffer is too short (including a heights-only frame
    /// whose `types` is empty). ADR 0023.
    #[must_use]
    pub fn type_at(&self, x: u32, y: u32) -> Option<TerrainType> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = y as usize * self.width as usize + x as usize;
        self.types.get(index).copied()
    }
}

/// Presents terrain as a drawable surface (ADR 0020 §1).
///
/// The composition root drives this to draw the world. Implementors own their
/// view/camera state — moving the camera is adapter-local and never crosses
/// this boundary (ADR 0020 §3), so nothing a renderer does can mutate the
/// simulation. The on-screen `wgpu`/`winit` renderer, a headless
/// render-to-PNG capture, and a no-op test double all realise it (ADR 0020 §2).
pub trait RendererPort {
    /// Present the given terrain snapshot as the current frame. Called by the
    /// window/redraw loop whenever a fresh frame should be drawn.
    fn present(&mut self, frame: TerrainFrame<'_>);
}

/// A single, discrete shaping command — the *one* vocabulary for "shape a
/// vertex" (ADR 0022 §1, ADR 0019 item 4).
///
/// It is produced at the input edge, consumed by the core, and recorded in a
/// session's replay log — every layer speaks this one type, so there is no
/// duplicate command type and no translation site. It carries **integer grid
/// coordinates only**: no float, no frame rate, no wall-clock. That is the
/// constraint that keeps a live, mutating sim replayable bit-for-bit (I3): a
/// gesture wanting finer control emits *more* commands, never a continuous one.
///
/// It lives in `providence-ports` for the same reason [`TerrainFrame`] does —
/// it is a plain value a port hands across a boundary, so defining it in the
/// interface crate keeps every adapter (and the core) free of a translation
/// type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainCommand {
    /// Raise the vertex at `(x, y)` by one step, cascading to restore the step
    /// invariant (ADR 0017 §3).
    Raise {
        /// Grid column of the target vertex.
        x: u32,
        /// Grid row of the target vertex.
        y: u32,
    },
    /// Lower the vertex at `(x, y)` by one step — the mirror of `Raise`.
    Lower {
        /// Grid column of the target vertex.
        x: u32,
        /// Grid row of the target vertex.
        y: u32,
    },
}

/// The interactive simulation seam (ADR 0022 §3): the renderer **holds** a
/// `SimDriver`, submitting shaping commands and pulling fresh snapshots to draw,
/// without ever importing the core.
///
/// The application implements it over a terrain world and its recorded command
/// log; the composition root passes `&mut dyn SimDriver` into the renderer's
/// run loop. It sits *alongside* [`RendererPort::present`], not replacing it, so
/// the static headless/no-op renderer adapters are unaffected (ADR 0022 §4).
pub trait SimDriver {
    /// The single input entry point (ADR 0022 §3): apply `command` to the sim
    /// and **record** it. Input reaches the sim *only* through here, as a
    /// discrete [`TerrainCommand`] — so a session is exactly `seed + params +
    /// log` and replays bit-for-bit (I3).
    fn submit(&mut self, command: TerrainCommand);

    /// Grid width in vertices — a frame-production read for the renderer.
    fn width(&self) -> u32;

    /// Grid height (depth) in vertices — a frame-production read.
    fn height(&self) -> u32;

    /// The current row-major height snapshot to draw. Row-major, mirroring
    /// [`TerrainFrame`]: the vertex at `(x, y)` is `heights()[y * width() + x]`.
    fn heights(&self) -> &[Height];

    /// The current row-major per-vertex terrain types to draw (ADR 0023) — the
    /// interactive twin of [`heights`](SimDriver::heights), so the renderer
    /// rebuilds the *material-banded* surface from the fresh snapshot after each
    /// shaping edit without ever re-deriving the model's classification rules.
    /// Same length and order as `heights`; the implementer recomputes it
    /// whenever a submitted command actually moves the field.
    fn types(&self) -> &[TerrainType];

    /// A revision that **bumps whenever the heights change**, so the renderer
    /// can tell a fresh frame from a repeat and animate the change (ADR 0022
    /// §3). A no-op command (out of bounds, at the ceiling, or refused by an
    /// immovable) leaves it unchanged.
    fn revision(&self) -> u64;
}

#[cfg(test)]
mod tests {
    use super::{Height, SimDriver, TerrainCommand, TerrainType};

    /// A minimal in-crate `SimDriver` proving the port is implementable — and
    /// object-safe — without importing the core: it serves a tiny fixed grid
    /// (heights and their derived types) and records the last command, bumping
    /// its revision on each submit.
    struct MockDriver {
        cells: [Height; 4],
        kinds: [TerrainType; 4],
        revision: u64,
        last: Option<TerrainCommand>,
    }

    impl SimDriver for MockDriver {
        fn submit(&mut self, command: TerrainCommand) {
            self.last = Some(command);
            self.revision += 1;
        }
        fn width(&self) -> u32 {
            2
        }
        fn height(&self) -> u32 {
            2
        }
        fn heights(&self) -> &[Height] {
            &self.cells
        }
        fn types(&self) -> &[TerrainType] {
            &self.kinds
        }
        fn revision(&self) -> u64 {
            self.revision
        }
    }

    #[test]
    fn a_mock_realises_the_sim_driver_port() {
        let mut driver = MockDriver {
            cells: [0, 1, 1, 2],
            kinds: [
                TerrainType::Water,
                TerrainType::Shore,
                TerrainType::Shore,
                TerrainType::Land,
            ],
            revision: 0,
            last: None,
        };
        // The snapshot reads are consistent (width × height == buffer length),
        // for both the heights and the per-vertex types (ADR 0023).
        assert_eq!(
            driver.width() as usize * driver.height() as usize,
            driver.heights().len()
        );
        assert_eq!(driver.heights().len(), driver.types().len());
        // Submitting drives the sim through the port and bumps the revision.
        driver.submit(TerrainCommand::Raise { x: 0, y: 0 });
        assert_eq!(driver.last, Some(TerrainCommand::Raise { x: 0, y: 0 }));
        assert_eq!(driver.revision(), 1);

        // The port is object-safe: usable behind a `&mut dyn` as the renderer
        // will hold it (ADR 0022 §4).
        let dynamic: &mut dyn SimDriver = &mut driver;
        dynamic.submit(TerrainCommand::Lower { x: 1, y: 1 });
        assert_eq!(dynamic.revision(), 2);
    }
}
