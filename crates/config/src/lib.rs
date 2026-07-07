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
    /// `content.*` — content **definitions** the core reads as data
    /// (docs/40-parameterisation.md §2.2): the terrain type catalogue today,
    /// the powers/followers/scenarios catalogues later. Deterministic — inside
    /// the boundary — but organised as content, not tuning.
    pub content: ContentParams,
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
    /// `sim.worldgen.*` — the seeded world generator (ADR 0021). Like
    /// `sim.terrain.*`, an always-on foundation: it produces the substrate the
    /// whole game stands on, so it carries no `enabled` seam (ADR 0016).
    pub worldgen: WorldgenParams,
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

/// `sim.worldgen.*` — the seeded, parameterised world generator (ADR 0021).
///
/// Worldgen is a **pure function of `seed`** producing an integer height field
/// that already satisfies the terrain step invariant (ADR 0017). Its knobs span
/// a *shape × relief* space — never a single baked-in world — so the same code
/// yields an island, a coastline, an archipelago, or a lake-dotted interior
/// (the Director's steer, ADR 0021 §2). All fields are **structural /
/// load-time** (docs/40-parameterisation.md §4) and integer-valued, keeping the
/// core float-free and reproducible (I3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldgenParams {
    /// `sim.worldgen.width` — map width in vertices (east–west extent).
    pub width: u32,
    /// `sim.worldgen.height` — map height in vertices (north–south extent).
    /// The grid's *depth*, not an elevation (heights live in the field).
    pub height: u32,
    /// `sim.worldgen.seed` — the `u64` the generator draws from. Same seed +
    /// same knobs ⇒ the same world, forever (ADR 0021 §1).
    pub seed: u64,
    /// `sim.worldgen.sea_level` — the waterline datum: vertices at or below it
    /// are water, above it are land (ADR 0017 §1). A signed height, so the sea
    /// can sit at, above, or below zero.
    pub sea_level: i32,
    /// `sim.worldgen.land_percent` — target percentage of the map above sea
    /// level. The generator picks the elevation threshold that lands *about*
    /// this fraction dry; the conform pass may shift it slightly (ADR 0021 §3).
    /// The ADR's `land_ratio`, expressed as an integer percent so the core
    /// stays integer-exact (I3).
    pub land_percent: u32,
    /// `sim.worldgen.shape` — how land is arranged (ADR 0021 §2, sub-choice a):
    /// a named mode, not a blend scalar.
    pub shape: Shape,
    /// `sim.worldgen.relief` — the vertical drama: the maximum number of height
    /// steps land rises above sea level (and water falls below it). Small is
    /// gentle, large is dramatic; the step invariant still caps what a given
    /// map size can actually express (ADR 0021 §3).
    pub relief: i32,
    /// `sim.worldgen.feature_size` — the base noise wavelength in vertices: how
    /// broad the primary hills and bays are. Larger is smoother/broader.
    pub feature_size: u32,
    /// `sim.worldgen.detail` — how many noise octaves are summed. More octaves
    /// add finer texture atop the base features (ADR 0021 §3).
    pub detail: u32,
}

/// `sim.worldgen.shape` — the named land-arrangement modes (ADR 0021 §2a). Each
/// selects a distinct generator mask; the seed varies the instance *within* the
/// chosen flavour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// One landmass ringed by sea — the classic inspiration and the shipped
    /// default. A radial falloff pulls the coast inward on every side.
    Island,
    /// Land fills most of the map but recedes from the edges, guaranteeing a
    /// coastline without isolating the land.
    Continent,
    /// Several scattered islands — a coarse mask breaks the land into clusters.
    Archipelago,
    /// Mostly land with interior lakes — the mask stays full and only the
    /// lowest ground dips below the waterline.
    Inland,
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

/// `content.*` — content **definitions** consumed by the core as data
/// (docs/40-parameterisation.md §2.2, §3). The first `content.*` table to land
/// (ADR 0021): the terrain type catalogue. Powers, followers, and scenarios
/// join it as their subsystems unpark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentParams {
    /// `content.terrain.*` — the terrain type catalogue: the thresholds that
    /// name *shore* and *mountain* over the height field (ADR 0017 §1).
    pub terrain: TerrainContent,
}

/// `content.terrain.*` — the terrain type catalogue (ADR 0017 §1, ADR 0021 §4).
///
/// These thresholds turn a bare height into a named [terrain type](../../crates/core/src/terrain/derive.rs):
/// they are **content**, not simulation tuning — what "shore" and "mountain"
/// *mean* over the field the generator produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainContent {
    /// `content.terrain.shore.*` — the coastal band just above the waterline.
    pub shore: ShoreContent,
    /// `content.terrain.mountain.*` — the high-ground band.
    pub mountain: MountainContent,
    /// `content.terrain.tree.*` — trees, a terrain-owned immovable scattered on
    /// land (ADR 0017 §5, ADR 0021 §5).
    pub tree: TreeContent,
    /// `content.terrain.rock.*` — rock, a terrain-owned immovable scattered on
    /// mountains.
    pub rock: RockContent,
}

/// `content.terrain.shore.*` — the shore band (ADR 0017 §1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShoreContent {
    /// `content.terrain.shore.band` — how many height steps above sea level
    /// still count as shore rather than ordinary land. `0` means no shore band.
    pub band: u32,
}

/// `content.terrain.mountain.*` — the mountain band (ADR 0017 §1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountainContent {
    /// `content.terrain.mountain.min_height` — the height at or above which a
    /// vertex is mountain, taking precedence over the shore band.
    pub min_height: i32,
}

/// `content.terrain.tree.*` — trees, a terrain-owned immovable (ADR 0021 §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeContent {
    /// `content.terrain.tree.density_permille` — how many of every 1000
    /// eligible **land** vertices worldgen plants a tree on (0–1000). Seeded,
    /// so placement is reproducible.
    pub density_permille: u32,
}

/// `content.terrain.rock.*` — rock, a terrain-owned immovable (ADR 0021 §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RockContent {
    /// `content.terrain.rock.density_permille` — how many of every 1000
    /// eligible **mountain** vertices worldgen sets rock on (0–1000). Seeded.
    pub density_permille: u32,
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
    /// `render.hud.*` — the read-only debug/HUD overlay (ADR 0015; issue #8
    /// Phase 3). Config is always present; the overlay itself compiles only
    /// under the renderer's `debug-hud` feature and draws only when `enabled`.
    pub hud: HudParams,
    /// `render.animation.*` — how a shaping change is animated on screen
    /// (ADR 0022 §5; issue #9/#10 Phase 3). Render-only: the interpolation is
    /// pure presentation, adapter-local, and never touches a core height.
    pub animation: AnimationParams,
}

/// `render.camera.*` — the workbench view camera (ADR 0020 §3). The camera is
/// adapter-local view state: these are its **initial** pose, its projection
/// lens, and the bounds/sensitivities of the orbit/pan/zoom controller
/// (issue #8 Phase 2). The live pose itself never leaves the renderer adapter.
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
    /// `render.camera.min_distance` — closest the zoom may dolly to the target.
    pub min_distance: f32,
    /// `render.camera.max_distance` — farthest the zoom may pull back.
    pub max_distance: f32,
    /// `render.camera.min_pitch_degrees` — lowest tilt (kept above the horizon
    /// so the view never dives under the land).
    pub min_pitch_degrees: f32,
    /// `render.camera.max_pitch_degrees` — highest tilt (kept below the pole so
    /// the look-at framing never degenerates).
    pub max_pitch_degrees: f32,
    /// `render.camera.orbit_speed` — orbit rotation per pixel of drag, in
    /// degrees.
    pub orbit_speed: f32,
    /// `render.camera.pan_speed` — look-at translation per pixel of drag, in
    /// world units.
    pub pan_speed: f32,
    /// `render.camera.zoom_speed` — fraction of the current distance changed per
    /// unit of scroll.
    pub zoom_speed: f32,
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

/// `render.hud.*` — the read-only developer HUD overlay (ADR 0015; issue #8
/// Phase 3): an on-screen readout of the grid dimensions, the live camera pose,
/// and the vertex under the screen-centre reticle (the "identify a vertex" step
/// that sets up picking, #9).
///
/// Presentation only, and doubly guarded: the overlay code compiles **only**
/// under the renderer adapter's `debug-hud` cargo feature (absent from a default
/// release build, ADR 0015), and even when compiled it draws **only** while
/// `enabled`. The panel toggles let the Director show or hide each section. It
/// reads a derived snapshot and holds no game state — moving the camera or the
/// reticle can never change a height (ADR 0020 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudParams {
    /// `render.hud.enabled` — draw the overlay at all (only meaningful when the
    /// `debug-hud` feature is compiled in).
    pub enabled: bool,
    /// `render.hud.show_camera` — show the camera-pose section (yaw/pitch/
    /// distance and eye position).
    pub show_camera: bool,
    /// `render.hud.show_reticle` — show the reticle section (the grid `(x, y)`
    /// and height of the vertex under the screen centre).
    pub show_reticle: bool,
}

/// `render.animation.*` — the shaping-change animation (ADR 0022 §5; issue
/// #9/#10 Phase 3). Render-only: when a command changes the height field, the
/// renderer eases the *visual* surface from its old shape to the new one over
/// `duration_ms`, purely in the adapter — no wall-clock, float, or frame-rate
/// value ever reaches the core (I3). Because it carries a float it derives no
/// `Eq` (like the rest of [`RenderParams`]).
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationParams {
    /// `render.animation.duration_ms` — how long, in milliseconds, each vertex
    /// takes to settle from its old visual height to a shaping command's new
    /// one. `0` snaps instantly (the Phase-2 behaviour).
    pub duration_ms: f32,
    /// `render.animation.ripple_ms_per_unit` — how much later, per unit of
    /// distance from the shaped vertex, an outer vertex starts its settle
    /// (issue #9/#10 Phase 4). This staggers the cascade so it *ripples outward*
    /// from the click rather than all vertices settling at once — the Populous
    /// feel (ADR 0022 §5). With unit grid spacing this is milliseconds per
    /// vertex ring. `0` makes the whole change settle together (Phase-3
    /// behaviour).
    pub ripple_ms_per_unit: f32,
}

/// `input.*` — input mapping & sensitivities for the interactive workbench
/// (ADR 0022; issue #9 Phase 2).
///
/// Like [`RenderParams`], a standalone projection that sits **outside** the
/// determinism boundary (docs/40-parameterisation.md §2.2): input bindings are
/// presentation/UX, never a core [`Params`] field. The `config-loader` projects
/// `input.*` into this type and hands it to the renderer adapter, which turns a
/// mouse gesture into a discrete `TerrainCommand`. The command it produces is
/// integer and replayable (I3); only the *binding* — which button, how far is a
/// click — lives here. Because it carries a float sensitivity it derives no
/// `Eq` (like [`RenderParams`]).
#[derive(Debug, Clone, PartialEq)]
pub struct InputParams {
    /// `input.shape.*` — the terrain-shaping controls.
    pub shape: ShapeInputParams,
}

/// `input.shape.*` — how a mouse gesture shapes the land (ADR 0022, the
/// Director's control-scheme ruling): which button raises the picked vertex,
/// which lowers it, and the click-vs-drag motion threshold that tells a shaping
/// *click* from a camera *drag*.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeInputParams {
    /// `input.shape.raise_button` — the pointer button that raises the picked
    /// vertex by one step (its cascade rippling outward, ADR 0017 §3).
    pub raise_button: PointerButton,
    /// `input.shape.lower_button` — the pointer button that lowers it — the
    /// mirror of `raise_button`.
    pub lower_button: PointerButton,
    /// `input.shape.click_drag_threshold_px` — the largest accumulated cursor
    /// motion, in physical pixels, still treated as a shaping *click*. A
    /// press→release that moves further is a camera *drag* (orbit/pan) and
    /// issues no command; `0` makes any motion a drag.
    pub click_drag_threshold_px: f32,
}

/// `input.shape.{raise,lower}_button` — a bindable pointer button. Named modes,
/// not raw platform codes, so a binding is stable across windowing backends and
/// authored as a readable string (`"left"`) in config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    /// The left mouse button.
    Left,
    /// The right mouse button.
    Right,
    /// The middle mouse button (wheel click).
    Middle,
}
