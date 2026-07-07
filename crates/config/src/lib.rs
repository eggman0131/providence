//! Validated, immutable simulation parameters — the plain data types the
//! deterministic core consumes (docs/20-architecture.md §2.1, ADR 0008).
//!
//! `no_std`: these types cross into the core, which cannot touch `std`
//! (ADR 0009). The std-side authoring/validation structs (`serde`/`garde`/
//! `schemars`) live in the `config-loader` adapter and map into these types
//! (ADR 0008 as refined by ADR 0009). Field docs name the config key each
//! field carries (docs/40-parameterisation.md §2).

#![no_std]
#![forbid(unsafe_code)]

/// Root of all parameters injected into the deterministic core.
///
/// Constructed only by the `config-loader` adapter after full validation;
/// the core treats it as immutable data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    /// `sim.*` — deterministic-simulation parameters.
    pub sim: SimParams,
}

/// `sim.*` — parameters governing the deterministic core.
///
/// Organised as **one disjoint subtree per simulation subsystem** so that a
/// knob in one subsystem cannot reach another — the structural fix for the
/// coupling cascade that forced the fresh start ([ADR 0016](../../docs/decisions/0016-exploration-lane-and-subsystem-isolation.md)).
/// Every subsystem carries an on/off seam (`sim.<subsystem>.enabled`); a
/// subsystem reads its *own* state, never another's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimParams {
    /// `sim.opponent.*` — the rival deity subsystem. Disable it and the loop
    /// still runs; nothing casts against the player (ADR 0016 §3).
    pub opponent: OpponentParams,
    /// `sim.economy.*` — the faith/mana economy subsystem.
    pub economy: EconomyParams,
    /// `sim.winloss.*` — the win/loss evaluation subsystem.
    pub winloss: WinLossParams,
    /// `sim.terrain.*` — the vertex height-field subsystem (ADR 0017). The
    /// game's foundation substrate; always on, so it carries no `enabled`
    /// seam (unlike the toggleable peers above).
    pub terrain: TerrainParams,
    /// `sim.placeholder.*` — Phase-1 gate-scaffolding parameters; the core's
    /// sole consumed value until Phase 3 gives it real subsystem state, then
    /// deleted (prefer deletion, contract §4.1).
    pub placeholder: PlaceholderParams,
}

/// `sim.opponent.*` — the rival-deity subsystem (ADR 0016 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpponentParams {
    /// `sim.opponent.enabled` — `false` ⇒ no rival deity; the loop runs but
    /// nothing casts against the player. The general isolation seam.
    pub enabled: bool,
}

/// `sim.economy.*` — the faith/mana economy subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EconomyParams {
    /// `sim.economy.mana.*` — the mana resource.
    pub mana: ManaParams,
}

/// `sim.economy.mana.*` — the mana resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManaParams {
    /// `sim.economy.mana.mode` — mana generation mode; first-class god-mode,
    /// not a hack (ADR 0016 §3). `Unlimited` is the sandbox exploration knob.
    pub mode: ManaMode,
}

/// `sim.economy.mana.mode` — how mana is generated for the player's economy.
///
/// A subsystem reads its *own* budget (ADR 0016 §3): flipping this to
/// [`ManaMode::Unlimited`] for exploration must not alter what the opponent
/// subsystem owns — the core routes each deity's spend through its own state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManaMode {
    /// Ordinary metered mana — the governed default.
    Normal,
    /// Accelerated regeneration for quicker iteration.
    Fast,
    /// Effectively infinite mana; the sandbox god-mode value.
    Unlimited,
}

/// `sim.winloss.*` — the win/loss evaluation subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinLossParams {
    /// `sim.winloss.enabled` — `false` ⇒ no win/loss evaluation during free
    /// play. The general isolation seam.
    pub enabled: bool,
}

/// `sim.terrain.*` — the vertex height-field subsystem (ADR 0017).
///
/// Governs the integer height field the land is built on. `max_step` and
/// `max_height` are **structural** (load-time, not hot-reloadable): the
/// model, mesh, and cascade are written assuming `max_step == 1`, and a
/// value ≠ 1 is not a supported configuration until proven otherwise
/// (ADR 0017 consequences).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainParams {
    /// `sim.terrain.max_step` — the maximum height difference permitted
    /// between two orthogonally-adjacent vertices (the *step invariant*,
    /// ADR 0017 §2). Default 1.
    pub max_step: u32,
    /// `sim.terrain.max_height` — the world height ceiling a raise cannot
    /// exceed; it also bounds the cascade radius, so no separate radius limit
    /// is needed for termination (ADR 0017 §3).
    pub max_height: i32,
    /// `sim.terrain.raise.*` — the raise/lower shaping operation.
    pub raise: RaiseParams,
}

/// `sim.terrain.raise.*` — the raise/lower shaping operation (ADR 0017 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaiseParams {
    /// `sim.terrain.raise.mana_cost` — mana charged per vertex **actually
    /// moved** by a raise/lower. The cost model falls out of the cascade
    /// (ADR 0017 §3). Wired now; spent once the economy subsystem is unparked
    /// (Phase 2 returns it without deducting — the economy is parked).
    pub mana_cost: u32,
}

/// `sim.placeholder.*` — placeholder parameters proving the config → core
/// wiring end-to-end (contract §7.2). Deleted when the Phase-3 core consumes
/// real subsystem state (prefer deletion, contract §4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceholderParams {
    /// `sim.placeholder.tick_increment` — ticks the placeholder state
    /// advances per step.
    pub tick_increment: u64,
}

/// `render.*` — presentation parameters for the workbench renderer (ADR 0020
/// §4).
///
/// Deliberately **not** a field of [`Params`]: presentation is
/// non-deterministic and must never cross into the core
/// (docs/40-parameterisation.md §2.2). The `config-loader` projects `render.*`
/// into this standalone type and hands it to the renderer adapter, never to
/// the core. Because its values are floats it derives no `Eq` (unlike the core
/// [`Params`]).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderParams {
    /// `render.camera.*` — the view camera.
    pub camera: CameraParams,
    /// `render.lighting.*` — the directional light shading the surface.
    pub lighting: LightingParams,
    /// `render.palette.*` — how vertex height maps to colour.
    pub palette: PaletteParams,
    /// `render.background.*` — the surface the world is drawn against.
    pub background: BackgroundParams,
    /// `render.mesh.*` — how the height field becomes a drawable surface.
    pub mesh: MeshParams,
    /// `render.window.*` — the on-screen surface (and headless-capture size).
    pub window: WindowParams,
}

/// `render.camera.*` — the workbench view camera (ADR 0020 §3). The camera is
/// adapter-local view state; these are only its **initial** pose and its
/// projection limits (orbit/pan/zoom bounds arrive in issue #8 Phase 2).
#[derive(Debug, Clone, PartialEq)]
pub struct CameraParams {
    /// `render.camera.fov_degrees` — vertical field of view, in degrees.
    pub fov_degrees: f32,
    /// `render.camera.near` — near clip-plane distance.
    pub near: f32,
    /// `render.camera.far` — far clip-plane distance.
    pub far: f32,
    /// `render.camera.initial_distance` — starting orbit distance from target.
    pub initial_distance: f32,
    /// `render.camera.initial_yaw_degrees` — starting orbit yaw, in degrees.
    pub initial_yaw_degrees: f32,
    /// `render.camera.initial_pitch_degrees` — starting orbit pitch, in degrees.
    pub initial_pitch_degrees: f32,
}

/// `render.lighting.*` — a single directional light plus ambient fill, enough
/// to read the flat-shaded stepped faces (ADR 0020; issue #8 Phase 1).
#[derive(Debug, Clone, PartialEq)]
pub struct LightingParams {
    /// `render.lighting.azimuth_degrees` — light compass direction, in degrees.
    pub azimuth_degrees: f32,
    /// `render.lighting.elevation_degrees` — light angle above the horizon.
    pub elevation_degrees: f32,
    /// `render.lighting.ambient` — ambient light fraction in `[0, 1]`.
    pub ambient: f32,
    /// `render.lighting.diffuse` — diffuse light fraction in `[0, 1]`.
    pub diffuse: f32,
}

/// `render.palette.*` — vertex height → colour, lerped from `low_rgb` at the
/// lowest drawn height to `high_rgb` at the highest.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteParams {
    /// `render.palette.low_rgb` — colour at the lowest drawn height, linear RGB.
    pub low_rgb: [f32; 3],
    /// `render.palette.high_rgb` — colour at the highest drawn height, linear RGB.
    pub high_rgb: [f32; 3],
}

/// `render.background.*` — the clear colour the world is drawn against.
#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundParams {
    /// `render.background.rgb` — clear colour, linear RGB.
    pub rgb: [f32; 3],
}

/// `render.mesh.*` — how the integer height field is turned into the drawable
/// flat-shaded stepped surface (ADR 0020; issue #8 Phase 1). Purely
/// presentation: it scales the *look* of the relief and never touches a height.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshParams {
    /// `render.mesh.vertical_scale` — world-space height of one integer height
    /// step. Larger values exaggerate the relief; the core heights are
    /// unchanged (the renderer only reads a snapshot, ADR 0020 §1).
    pub vertical_scale: f32,
}

/// `render.window.*` — the on-screen surface the workbench opens (ADR 0020 §2),
/// and the resolution of the headless render-to-PNG capture used by `/verify`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowParams {
    /// `render.window.width` — initial surface width, in physical pixels.
    pub width: u32,
    /// `render.window.height` — initial surface height, in physical pixels.
    pub height: u32,
}
