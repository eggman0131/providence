//! Authoring structs — the single source of truth for the config schema
//! (ADR 0008): `serde` shapes the keys (`deny_unknown_fields` rejects
//! anything outside them), `garde` carries ranges and cross-key invariants,
//! `schemars` generates the committed JSON Schema.
//!
//! These are std types; they map into the `no_std` `providence-config`
//! param types via [`ConfigRoot::into_params`] (ADR 0009 refinement).

use garde::Validate;
use providence_config::{
    BackgroundParams, CameraParams, EconomyParams, HudParams, LightingParams, ManaMode, ManaParams,
    MeshParams, OpponentParams, PaletteParams, Params, PlaceholderParams, RaiseParams,
    RenderParams, SimParams, TerrainParams, WinLossParams, WindowParams,
};
use schemars::JsonSchema;
use serde::Deserialize;

/// Root of the authored configuration (all layers merged).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct ConfigRoot {
    /// `meta.*` — config/schema versioning and provenance.
    #[garde(dive)]
    pub meta: MetaSection,
    /// `sim.*` — deterministic-simulation parameters.
    #[garde(dive)]
    pub sim: SimSection,
    /// `render.*` — presentation parameters for the workbench renderer
    /// (ADR 0020 §4). Projected into a standalone [`RenderParams`] — **not**
    /// into the core-injected [`Params`], because presentation is outside the
    /// determinism boundary (docs/40-parameterisation.md §2.2).
    #[garde(dive)]
    pub render: RenderSection,
}

/// `meta.*` (docs/40-parameterisation.md §2.2).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct MetaSection {
    /// `meta.schema_version` — the schema this config targets; a mismatch
    /// triggers the migration path, never a silent misread.
    #[garde(range(min = 1))]
    pub schema_version: u32,
}

/// `sim.*` (docs/40-parameterisation.md §2.2).
///
/// One field per simulation subsystem, each a disjoint subtree with its own
/// isolation seam — no subsystem's config is derived from another's
/// ([ADR 0016](../../docs/decisions/0016-exploration-lane-and-subsystem-isolation.md) §3).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct SimSection {
    /// `sim.opponent.*` — the rival-deity subsystem.
    #[garde(dive)]
    pub opponent: OpponentSection,
    /// `sim.economy.*` — the faith/mana economy subsystem.
    #[garde(dive)]
    pub economy: EconomySection,
    /// `sim.winloss.*` — the win/loss evaluation subsystem.
    #[garde(dive)]
    pub winloss: WinLossSection,
    /// `sim.terrain.*` — the vertex height-field subsystem (ADR 0017).
    #[garde(dive)]
    pub terrain: TerrainSection,
    /// `sim.placeholder.*` — Phase-1 gate scaffolding (contract §7.2);
    /// deleted when the Phase-3 core consumes real subsystem state.
    #[garde(dive)]
    pub placeholder: PlaceholderSection,
}

/// `sim.opponent.*` — the rival-deity subsystem (ADR 0016 §3).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct OpponentSection {
    /// `sim.opponent.enabled` — `false` ⇒ no rival deity casts against the
    /// player. The general `sim.<subsystem>.enabled` isolation seam.
    #[garde(skip)]
    pub enabled: bool,
}

/// `sim.economy.*` — the faith/mana economy subsystem.
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct EconomySection {
    /// `sim.economy.mana.*` — the mana resource.
    #[garde(dive)]
    pub mana: ManaSection,
}

/// `sim.economy.mana.*` — the mana resource.
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct ManaSection {
    /// `sim.economy.mana.mode` — mana generation mode (ADR 0016 §3).
    /// Hot-reloadable (a pure balance/exploration value).
    #[garde(skip)]
    pub mode: ManaModeAuthored,
}

/// `sim.economy.mana.mode` values, authored as `snake_case` strings in TOML
/// (`mode = "unlimited"`). Maps to the core [`ManaMode`].
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManaModeAuthored {
    /// Ordinary metered mana — the governed default.
    Normal,
    /// Accelerated regeneration for quicker iteration.
    Fast,
    /// Effectively infinite mana; the sandbox god-mode value.
    Unlimited,
}

/// `sim.winloss.*` — the win/loss evaluation subsystem.
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct WinLossSection {
    /// `sim.winloss.enabled` — `false` ⇒ no win/loss evaluation during free
    /// play. The general `sim.<subsystem>.enabled` isolation seam.
    #[garde(skip)]
    pub enabled: bool,
}

/// `sim.terrain.*` — the vertex height-field subsystem (ADR 0017).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct TerrainSection {
    /// `sim.terrain.max_step` — maximum orthogonal height step; the step
    /// invariant (ADR 0017 §2). Structural: the model assumes 1.
    #[garde(range(min = 1))]
    pub max_step: u32,
    /// `sim.terrain.max_height` — the world height ceiling a raise cannot
    /// exceed; bounds the cascade radius (ADR 0017 §3).
    #[garde(range(min = 1))]
    pub max_height: i32,
    /// `sim.terrain.raise.*` — the raise/lower shaping operation.
    #[garde(dive)]
    pub raise: RaiseSection,
}

/// `sim.terrain.raise.*` — the raise/lower shaping operation (ADR 0017 §3).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct RaiseSection {
    /// `sim.terrain.raise.mana_cost` — mana per vertex actually moved
    /// (ADR 0017 §3). Any value is valid (0 = free shaping in exploration).
    #[garde(skip)]
    pub mana_cost: u32,
}

/// `sim.placeholder.*` — placeholder parameters proving config → core wiring.
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct PlaceholderSection {
    /// `sim.placeholder.tick_increment` — ticks the placeholder state
    /// advances per step. Hot-reloadable (a pure balance value).
    #[garde(range(min = 1))]
    pub tick_increment: u64,
}

/// `render.*` (docs/40-parameterisation.md §2.2) — presentation config for the
/// workbench renderer. Outside the determinism boundary; projected into
/// [`RenderParams`] via [`ConfigRoot::into_render_params`], never into the
/// core-injected [`Params`] (ADR 0020 §4).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct RenderSection {
    /// `render.camera.*` — the view camera.
    #[garde(dive)]
    pub camera: CameraSection,
    /// `render.lighting.*` — the directional light shading the surface.
    #[garde(dive)]
    pub lighting: LightingSection,
    /// `render.palette.*` — how vertex height maps to colour.
    #[garde(dive)]
    pub palette: PaletteSection,
    /// `render.background.*` — the surface the world is drawn against.
    #[garde(dive)]
    pub background: BackgroundSection,
    /// `render.mesh.*` — how the height field becomes a drawable surface.
    #[garde(dive)]
    pub mesh: MeshSection,
    /// `render.window.*` — the on-screen surface (and headless-capture size).
    #[garde(dive)]
    pub window: WindowSection,
    /// `render.hud.*` — the read-only debug/HUD overlay (ADR 0015; issue #8
    /// Phase 3).
    #[garde(dive)]
    pub hud: HudSection,
}

/// `render.camera.*` — the workbench view camera (ADR 0020 §3). Its initial
/// pose, projection lens, and the orbit/pan/zoom controller bounds and
/// sensitivities (issue #8 Phase 2).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct CameraSection {
    /// `render.camera.fov_degrees` — vertical field of view, in degrees.
    #[garde(range(min = 1.0, max = 179.0))]
    pub fov_degrees: f32,
    /// `render.camera.near` — near clip-plane distance.
    #[garde(skip)]
    pub near: f32,
    /// `render.camera.far` — far clip-plane distance.
    #[garde(skip)]
    pub far: f32,
    /// `render.camera.initial_distance` — starting orbit distance from target.
    #[garde(skip)]
    pub initial_distance: f32,
    /// `render.camera.initial_yaw_degrees` — starting orbit yaw, in degrees.
    #[garde(skip)]
    pub initial_yaw_degrees: f32,
    /// `render.camera.initial_pitch_degrees` — starting orbit pitch, in degrees.
    #[garde(skip)]
    pub initial_pitch_degrees: f32,
    /// `render.camera.min_distance` — closest the zoom may dolly in. Positive,
    /// so the eye can never sit on the target.
    #[garde(range(min = 0.001))]
    pub min_distance: f32,
    /// `render.camera.max_distance` — farthest the zoom may pull back. Positive.
    #[garde(range(min = 0.001))]
    pub max_distance: f32,
    /// `render.camera.min_pitch_degrees` — lowest tilt. Held within `(-90, 90)`
    /// so the look-at framing never degenerates at the pole.
    #[garde(range(min = -89.0, max = 89.0))]
    pub min_pitch_degrees: f32,
    /// `render.camera.max_pitch_degrees` — highest tilt, also within `(-90, 90)`.
    #[garde(range(min = -89.0, max = 89.0))]
    pub max_pitch_degrees: f32,
    /// `render.camera.orbit_speed` — degrees of rotation per pixel of drag.
    /// Non-negative (0 pins the orbit).
    #[garde(range(min = 0.0))]
    pub orbit_speed: f32,
    /// `render.camera.pan_speed` — world units of look-at translation per pixel
    /// of drag. Non-negative.
    #[garde(range(min = 0.0))]
    pub pan_speed: f32,
    /// `render.camera.zoom_speed` — fraction of the current distance changed per
    /// unit of scroll. Non-negative.
    #[garde(range(min = 0.0))]
    pub zoom_speed: f32,
}

/// `render.lighting.*` — one directional light plus ambient fill (ADR 0020).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct LightingSection {
    /// `render.lighting.azimuth_degrees` — light compass direction, in degrees.
    #[garde(skip)]
    pub azimuth_degrees: f32,
    /// `render.lighting.elevation_degrees` — light angle above the horizon.
    #[garde(skip)]
    pub elevation_degrees: f32,
    /// `render.lighting.ambient` — ambient light fraction in `[0, 1]`.
    #[garde(range(min = 0.0, max = 1.0))]
    pub ambient: f32,
    /// `render.lighting.diffuse` — diffuse light fraction in `[0, 1]`.
    #[garde(range(min = 0.0, max = 1.0))]
    pub diffuse: f32,
}

/// `render.palette.*` — vertex height → colour, lerped low→high (ADR 0020).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct PaletteSection {
    /// `render.palette.low_rgb` — colour at the lowest drawn height, linear RGB.
    #[garde(skip)]
    pub low_rgb: [f32; 3],
    /// `render.palette.high_rgb` — colour at the highest drawn height, linear RGB.
    #[garde(skip)]
    pub high_rgb: [f32; 3],
}

/// `render.background.*` — the clear colour the world is drawn against.
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct BackgroundSection {
    /// `render.background.rgb` — clear colour, linear RGB.
    #[garde(skip)]
    pub rgb: [f32; 3],
}

/// `render.mesh.*` — height-field → drawable surface (ADR 0020; issue #8).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct MeshSection {
    /// `render.mesh.vertical_scale` — world height of one integer step. A
    /// non-negative presentation scale; 0 flattens the relief (degenerate but
    /// valid), and negatives (which would invert it) are rejected.
    #[garde(range(min = 0.0))]
    pub vertical_scale: f32,
}

/// `render.window.*` — the on-screen surface, and the headless-capture size.
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct WindowSection {
    /// `render.window.width` — initial surface width, in physical pixels.
    #[garde(range(min = 1))]
    pub width: u32,
    /// `render.window.height` — initial surface height, in physical pixels.
    #[garde(range(min = 1))]
    pub height: u32,
}

/// `render.hud.*` — the read-only debug/HUD overlay (ADR 0015; issue #8
/// Phase 3). Every field is a plain boolean toggle; the overlay is presentation
/// only and never touches the core.
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct HudSection {
    /// `render.hud.enabled` — draw the overlay (only when the `debug-hud`
    /// feature is compiled in).
    #[garde(skip)]
    pub enabled: bool,
    /// `render.hud.show_camera` — show the camera-pose section.
    #[garde(skip)]
    pub show_camera: bool,
    /// `render.hud.show_reticle` — show the reticle vertex/height section.
    #[garde(skip)]
    pub show_reticle: bool,
}

impl ConfigRoot {
    /// Map the validated authoring config into the immutable `no_std`
    /// params the core consumes. Purely mechanical; covered by tests.
    #[must_use]
    pub fn into_params(self) -> Params {
        Params {
            sim: SimParams {
                opponent: OpponentParams {
                    enabled: self.sim.opponent.enabled,
                },
                economy: EconomyParams {
                    mana: ManaParams {
                        mode: self.sim.economy.mana.mode.into_param(),
                    },
                },
                winloss: WinLossParams {
                    enabled: self.sim.winloss.enabled,
                },
                terrain: TerrainParams {
                    max_step: self.sim.terrain.max_step,
                    max_height: self.sim.terrain.max_height,
                    raise: RaiseParams {
                        mana_cost: self.sim.terrain.raise.mana_cost,
                    },
                },
                placeholder: PlaceholderParams {
                    tick_increment: self.sim.placeholder.tick_increment,
                },
            },
        }
    }

    /// Project the validated `render.*` config into the standalone
    /// [`RenderParams`] the renderer adapter consumes (ADR 0020 §4). Separate
    /// from [`into_params`](Self::into_params) so presentation config never
    /// travels with the core's [`Params`]. Purely mechanical; covered by tests.
    #[must_use]
    pub fn into_render_params(self) -> RenderParams {
        RenderParams {
            camera: CameraParams {
                fov_degrees: self.render.camera.fov_degrees,
                near: self.render.camera.near,
                far: self.render.camera.far,
                initial_distance: self.render.camera.initial_distance,
                initial_yaw_degrees: self.render.camera.initial_yaw_degrees,
                initial_pitch_degrees: self.render.camera.initial_pitch_degrees,
                min_distance: self.render.camera.min_distance,
                max_distance: self.render.camera.max_distance,
                min_pitch_degrees: self.render.camera.min_pitch_degrees,
                max_pitch_degrees: self.render.camera.max_pitch_degrees,
                orbit_speed: self.render.camera.orbit_speed,
                pan_speed: self.render.camera.pan_speed,
                zoom_speed: self.render.camera.zoom_speed,
            },
            lighting: LightingParams {
                azimuth_degrees: self.render.lighting.azimuth_degrees,
                elevation_degrees: self.render.lighting.elevation_degrees,
                ambient: self.render.lighting.ambient,
                diffuse: self.render.lighting.diffuse,
            },
            palette: PaletteParams {
                low_rgb: self.render.palette.low_rgb,
                high_rgb: self.render.palette.high_rgb,
            },
            background: BackgroundParams {
                rgb: self.render.background.rgb,
            },
            mesh: MeshParams {
                vertical_scale: self.render.mesh.vertical_scale,
            },
            window: WindowParams {
                width: self.render.window.width,
                height: self.render.window.height,
            },
            hud: HudParams {
                enabled: self.render.hud.enabled,
                show_camera: self.render.hud.show_camera,
                show_reticle: self.render.hud.show_reticle,
            },
        }
    }
}

impl ManaModeAuthored {
    /// Map the authored TOML value to the core [`ManaMode`]. Purely
    /// mechanical; covered by the loader tests.
    fn into_param(self) -> ManaMode {
        match self {
            ManaModeAuthored::Normal => ManaMode::Normal,
            ManaModeAuthored::Fast => ManaMode::Fast,
            ManaModeAuthored::Unlimited => ManaMode::Unlimited,
        }
    }
}
