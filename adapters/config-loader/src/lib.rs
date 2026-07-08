//! Config-loading adapter (ADR 0008, refined by ADR 0009).
//!
//! Pipeline (docs/40-parameterisation.md §4–5): parse each TOML layer →
//! deep-merge by key (defaults → scenario/content pack → user/local) →
//! check `meta.schema_version` → deserialise the merged whole into the
//! authoring structs (`deny_unknown_fields` rejects stray keys) → `garde`
//! semantic validation → map into the immutable `no_std`
//! [`providence_config::Params`] the core consumes.
//!
//! The authoring structs here are the single source of truth for the
//! machine-readable JSON Schema (`docs/contracts/config.schema.json`),
//! generated via [`schema_json`] and kept drift-free by the gate's
//! regenerate-and-diff check.

#![forbid(unsafe_code)]

mod authoring;
mod error;
mod merge;

use std::fs;
use std::path::Path;

use garde::Validate;
use providence_config::{InputParams, Params, RenderParams};

pub use crate::authoring::ConfigRoot;
pub use crate::error::ConfigError;

/// The schema version this loader supports. A `meta.schema_version` mismatch
/// is a defined migration point, never a silent misread
/// (docs/40-parameterisation.md §4).
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// One configuration layer: a display name (for error messages) and its
/// TOML text. Layers merge in slice order; later overrides earlier by key.
#[derive(Debug, Clone)]
pub struct Layer {
    /// Where this layer came from (file name or description) — used in errors.
    pub name: String,
    /// The layer's TOML source text.
    pub text: String,
}

/// The generated JSON Schema for the full configuration, pretty-printed.
///
/// Committed at `docs/contracts/config.schema.json`; the gate regenerates
/// and diffs it so the schema can never drift from these types (ADR 0008).
#[must_use]
pub fn schema_json() -> String {
    // Subschemas are inlined (no `$ref`) so the artifact is directly
    // walkable by the gate's key-integrity check and by editors.
    let generator = schemars::generate::SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator();
    let schema = generator.into_root_schema_for::<ConfigRoot>();
    let mut text = serde_json::to_string_pretty(&schema)
        .expect("schema serialisation cannot fail for a schemars-derived type");
    text.push('\n');
    text
}

/// Load, merge, and fully validate the layers, returning immutable params.
pub fn params_from_layers(layers: &[Layer]) -> Result<Params, ConfigError> {
    let root = config_root_from_layers(layers)?;
    Ok(root.into_params())
}

/// Load, merge, and fully validate the layers, returning the standalone
/// presentation params (ADR 0020 §4).
///
/// Separate from [`params_from_layers`] so `render.*` never travels with the
/// core's [`Params`]: the same validated config yields two disjoint
/// projections, and only this one reaches the renderer adapter.
pub fn render_params_from_layers(layers: &[Layer]) -> Result<RenderParams, ConfigError> {
    let root = config_root_from_layers(layers)?;
    Ok(root.into_render_params())
}

/// Load, merge, and fully validate the layers, returning the standalone input
/// params (ADR 0022).
///
/// A third disjoint projection alongside [`params_from_layers`] and
/// [`render_params_from_layers`]: the same validated config yields the core
/// [`Params`], the presentation [`RenderParams`], and these [`InputParams`],
/// and only this one carries the workbench's shaping bindings to the renderer.
pub fn input_params_from_layers(layers: &[Layer]) -> Result<InputParams, ConfigError> {
    let root = config_root_from_layers(layers)?;
    Ok(root.into_input_params())
}

/// Load, merge, and fully validate the layers, returning the authoring root
/// (used by the gate's config checks; games use [`params_from_layers`]).
pub fn config_root_from_layers(layers: &[Layer]) -> Result<ConfigRoot, ConfigError> {
    if layers.is_empty() {
        return Err(ConfigError::NoLayers);
    }
    let mut merged: Option<toml::Value> = None;
    for layer in layers {
        let value: toml::Value =
            toml::from_str(&layer.text).map_err(|source| ConfigError::Parse {
                layer: layer.name.clone(),
                source,
            })?;
        match merged.as_mut() {
            Some(base) => merge::deep_merge(base, value),
            None => merged = Some(value),
        }
    }
    let merged = merged.expect("at least one layer checked above");

    let root: ConfigRoot = merged
        .try_into()
        .map_err(|source| ConfigError::Deserialize { source })?;

    if root.meta.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ConfigError::SchemaVersion {
            found: root.meta.schema_version,
            supported: SUPPORTED_SCHEMA_VERSION,
        });
    }

    root.validate()
        .map_err(|report| ConfigError::Validation { report })?;
    Ok(root)
}

/// Load params from a config directory: `default.toml` (required) overlaid
/// by `local.toml` (optional user layer). The governed load path.
pub fn load_dir(dir: &Path) -> Result<Params, ConfigError> {
    load_with_profile(dir, None)
}

/// Load params from a config directory, optionally interposing a named profile
/// layer between the defaults and the user layer.
///
/// Layer order (docs/40-parameterisation.md §4, ADR 0008): `default.toml`
/// (required) → `<profile>.toml` (the scenario/content-pack slot, when a
/// profile is named) → `local.toml` (optional user overrides). Later layers
/// override earlier by key.
///
/// The `sandbox` profile (ADR 0016 §3) is loaded as `Some("sandbox")`. A
/// named-but-missing profile file is an error — a mistyped profile must not
/// silently fall back to the governed defaults.
pub fn load_with_profile(dir: &Path, profile: Option<&str>) -> Result<Params, ConfigError> {
    params_from_layers(&read_layers(dir, profile)?)
}

/// Load the presentation params (`render.*`) from a config directory — the
/// renderer adapter's load path, mirroring [`load_dir`] (ADR 0020 §4). Uses
/// the same governed layer order; presentation config is profile-independent
/// for now, so no profile is interposed.
pub fn load_render(dir: &Path) -> Result<RenderParams, ConfigError> {
    render_params_from_layers(&read_layers(dir, None)?)
}

/// Load the input params (`input.*`) from a config directory — the renderer
/// adapter's input load path, mirroring [`load_render`] (ADR 0022). Uses the
/// same governed layer order; input config is profile-independent for now, so
/// no profile is interposed.
pub fn load_input(dir: &Path) -> Result<InputParams, ConfigError> {
    input_params_from_layers(&read_layers(dir, None)?)
}

/// Read the ordered config layers from `dir`: `default.toml` (required) →
/// `<profile>.toml` (when named) → `local.toml` (optional). The shared file
/// step behind [`load_with_profile`] and [`load_render`]; a named-but-missing
/// profile is a loud error, never a silent fall-back to the defaults.
fn read_layers(dir: &Path, profile: Option<&str>) -> Result<Vec<Layer>, ConfigError> {
    let mut layers = Vec::new();

    let default_path = dir.join("default.toml");
    let default_text = fs::read_to_string(&default_path).map_err(|source| ConfigError::Io {
        path: default_path.clone(),
        source,
    })?;
    layers.push(Layer {
        name: default_path.display().to_string(),
        text: default_text,
    });

    if let Some(profile) = profile {
        let profile_path = dir.join(format!("{profile}.toml"));
        let profile_text = fs::read_to_string(&profile_path).map_err(|source| ConfigError::Io {
            path: profile_path.clone(),
            source,
        })?;
        layers.push(Layer {
            name: profile_path.display().to_string(),
            text: profile_text,
        });
    }

    let local_path = dir.join("local.toml");
    if local_path.exists() {
        let local_text = fs::read_to_string(&local_path).map_err(|source| ConfigError::Io {
            path: local_path.clone(),
            source,
        })?;
        layers.push(Layer {
            name: local_path.display().to_string(),
            text: local_text,
        });
    }

    Ok(layers)
}

#[cfg(test)]
mod tests {
    use providence_config::{ManaMode, PointerButton, Shape};

    use super::{
        Layer, SUPPORTED_SCHEMA_VERSION, input_params_from_layers, params_from_layers,
        render_params_from_layers,
    };

    /// The `[render.*]` block shared by the fixtures — mirrors the shipped
    /// `config/default.toml` so the default layer stays complete now that
    /// `render` is a required section (ADR 0020 §4).
    const RENDER_TOML: &str = "\
        [render.camera]\n\
        fov_degrees = 45.0\nnear = 0.1\nfar = 1000.0\n\
        initial_distance = 24.0\ninitial_yaw_degrees = 45.0\ninitial_pitch_degrees = 30.0\n\
        min_distance = 6.0\nmax_distance = 120.0\n\
        min_pitch_degrees = 5.0\nmax_pitch_degrees = 85.0\n\
        orbit_speed = 0.4\npan_speed = 0.05\nzoom_speed = 0.1\n\n\
        [render.lighting]\n\
        azimuth_degrees = 135.0\nelevation_degrees = 45.0\nambient = 0.25\ndiffuse = 0.85\n\n\
        [render.material]\n\
        water_rgb = [0.16, 0.34, 0.44]\nshore_rgb = [0.80, 0.74, 0.53]\n\
        land_rgb = [0.33, 0.49, 0.24]\nmountain_rgb = [0.45, 0.42, 0.38]\n\
        peak_rgb = [0.92, 0.93, 0.95]\n\n\
        [render.water]\n\
        rgb = [0.11, 0.34, 0.52]\nopacity = 0.72\nsurface_lift = 0.2\n\
        ripple_amplitude = 0.14\nripple_speed = 1.6\nripple_scale = 0.55\n\n\
        [render.background]\n\
        rgb = [0.05, 0.06, 0.09]\n\n\
        [render.mesh]\nvertical_scale = 1.0\n\n\
        [render.window]\nwidth = 1280\nheight = 720\n\n\
        [render.hud]\nenabled = true\nshow_camera = true\nshow_reticle = true\n\n\
        [render.animation]\nduration_ms = 250.0\nripple_ms_per_unit = 18.0\n";

    /// The `[input.*]` block shared by the fixtures — mirrors the shipped
    /// `config/default.toml` so the default layer stays complete now that
    /// `input` is a required section (ADR 0022).
    const INPUT_TOML: &str = "\
        [input.shape]\n\
        raise_button = \"left\"\nlower_button = \"right\"\nclick_drag_threshold_px = 6.0\n";

    /// The `[sim.*]` + `[content.*]` blocks shared by the fixtures — every
    /// subsystem present and on, mana metered (mirrors the shipped
    /// `config/default.toml`). Kept as one const so both the default layer and
    /// the version-mismatch fixture stay complete as the schema grows.
    const SIM_CONTENT_TOML: &str = "\
        [sim.worldgen]\n\
        width = 64\nheight = 64\nseed = 1337\nsea_level = 0\nland_percent = 55\n\
        shape = \"island\"\nrelief = 12\nfeature_size = 16\ndetail = 3\n\n\
        [sim.opponent]\nenabled = true\n\n\
        [sim.economy.mana]\nmode = \"normal\"\n\n\
        [sim.winloss]\nenabled = true\n\n\
        [sim.terrain]\nmax_step = 1\nmax_height = 64\n\n\
        [sim.terrain.raise]\nmana_cost = 1\n\n\
        [sim.placeholder]\ntick_increment = 1\n\n\
        [content.terrain.shore]\nband = 2\n\n\
        [content.terrain.mountain]\nmin_height = 12\n\n\
        [content.terrain.tree]\ndensity_permille = 120\n\n\
        [content.terrain.rock]\ndensity_permille = 200\n";

    /// A complete governed default layer: every subsystem present and on,
    /// mana metered (mirrors the shipped `config/default.toml`).
    fn default_layer() -> Layer {
        Layer {
            name: "default.toml".into(),
            text: format!(
                "[meta]\nschema_version = {SUPPORTED_SCHEMA_VERSION}\n\n\
                 {SIM_CONTENT_TOML}\n{RENDER_TOML}\n{INPUT_TOML}"
            ),
        }
    }

    /// Floats compared within a small tolerance: clippy forbids `==` on floats,
    /// and the TOML→`f32` round-trip makes exact equality brittle.
    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-5
    }

    /// Element-wise [`approx`] for an RGB triple.
    fn approx3(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= 1e-5)
    }

    #[test]
    fn default_layer_alone_loads() {
        let params = params_from_layers(&[default_layer()]).expect("default layer must load");
        assert_eq!(params.sim.placeholder.tick_increment, 1);
        assert!(params.sim.opponent.enabled);
        assert_eq!(params.sim.economy.mana.mode, ManaMode::Normal);
        assert!(params.sim.winloss.enabled);
        // Worldgen + the first content table project through (ADR 0021).
        assert_eq!(
            (params.sim.worldgen.width, params.sim.worldgen.height),
            (64, 64)
        );
        assert_eq!(params.sim.worldgen.shape, Shape::Island);
        assert_eq!(params.sim.worldgen.land_percent, 55);
        assert_eq!(params.content.terrain.shore.band, 2);
        assert_eq!(params.content.terrain.mountain.min_height, 12);
    }

    #[test]
    fn shape_round_trips_each_value() {
        for (authored, expected) in [
            ("island", Shape::Island),
            ("continent", Shape::Continent),
            ("archipelago", Shape::Archipelago),
            ("inland", Shape::Inland),
        ] {
            let overlay = Layer {
                name: "local.toml".into(),
                text: format!("[sim.worldgen]\nshape = \"{authored}\"\n"),
            };
            let params = params_from_layers(&[default_layer(), overlay])
                .expect("each shape must parse and map");
            assert_eq!(params.sim.worldgen.shape, expected);
        }
    }

    #[test]
    fn out_of_range_land_percent_is_rejected() {
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[sim.worldgen]\nland_percent = 100\n".into(),
        };
        params_from_layers(&[default_layer(), overlay])
            .expect_err("land_percent at 100 must fail garde validation (max 99)");
    }

    #[test]
    fn later_layer_overrides_by_key() {
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[sim.placeholder]\ntick_increment = 5\n".into(),
        };
        let params =
            params_from_layers(&[default_layer(), overlay]).expect("overlay merge must load");
        assert_eq!(
            params.sim.placeholder.tick_increment, 5,
            "later layers override earlier"
        );
    }

    #[test]
    fn mana_mode_round_trips_each_value() {
        for (authored, expected) in [
            ("normal", ManaMode::Normal),
            ("fast", ManaMode::Fast),
            ("unlimited", ManaMode::Unlimited),
        ] {
            let overlay = Layer {
                name: "local.toml".into(),
                text: format!("[sim.economy.mana]\nmode = \"{authored}\"\n"),
            };
            let params =
                params_from_layers(&[default_layer(), overlay]).expect("each mana mode must parse");
            assert_eq!(params.sim.economy.mana.mode, expected);
        }
    }

    /// The first-slice guarantee (ADR 0016 §3): flipping the mana mode — even
    /// all the way to `unlimited` — changes nothing the opponent subsystem
    /// owns. The coupling cascade that forced the reset is impossible by
    /// construction, because the subtrees are disjoint. A regression that made
    /// an opponent parameter depend on mana mode fails here.
    #[test]
    fn flipping_mana_mode_leaves_the_opponent_subtree_untouched() {
        let governed = params_from_layers(&[default_layer()]).expect("default must load");
        let unlimited = Layer {
            name: "sandbox.toml".into(),
            text: "[sim.economy.mana]\nmode = \"unlimited\"\n".into(),
        };
        let flipped =
            params_from_layers(&[default_layer(), unlimited]).expect("mana override must load");

        assert_eq!(
            flipped.sim.economy.mana.mode,
            ManaMode::Unlimited,
            "the mana knob did move"
        );
        assert_eq!(
            governed.sim.opponent, flipped.sim.opponent,
            "mana mode must not be an input to anything the opponent owns (ADR 0016 §3)"
        );
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[sim.placeholder]\nnot_a_real_key = 3\n".into(),
        };
        let err = params_from_layers(&[default_layer(), overlay])
            .expect_err("unknown keys must fail validation (40-parameterisation §2.4)");
        assert!(
            err.to_string().contains("not_a_real_key"),
            "error must name the offending key"
        );
    }

    #[test]
    fn unknown_mana_mode_is_rejected() {
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[sim.economy.mana]\nmode = \"infinite\"\n".into(),
        };
        params_from_layers(&[default_layer(), overlay])
            .expect_err("an unrecognised mana mode must fail deserialisation");
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[sim.placeholder]\ntick_increment = 0\n".into(),
        };
        params_from_layers(&[default_layer(), overlay])
            .expect_err("tick_increment below its minimum must fail garde validation");
    }

    #[test]
    fn default_layer_projects_render_params() {
        let render = render_params_from_layers(&[default_layer()])
            .expect("the default layer must yield render params");
        assert!(approx(render.camera.fov_degrees, 45.0));
        assert!(approx(render.camera.initial_distance, 24.0));
        assert!(approx(render.lighting.ambient, 0.25));
        assert!(approx3(render.material.shore_rgb, [0.80, 0.74, 0.53]));
        assert!(approx3(render.material.peak_rgb, [0.92, 0.93, 0.95]));
        // The living water surface projects through (ADR 0023, Phase 2).
        assert!(approx3(render.water.rgb, [0.11, 0.34, 0.52]));
        assert!(approx(render.water.opacity, 0.72));
        assert!(approx(render.water.surface_lift, 0.2));
        assert!(approx(render.water.ripple_amplitude, 0.14));
        assert!(approx(render.water.ripple_speed, 1.6));
        assert!(approx(render.water.ripple_scale, 0.55));
        assert!(approx3(render.background.rgb, [0.05, 0.06, 0.09]));
        assert!(approx(render.mesh.vertical_scale, 1.0));
        assert_eq!((render.window.width, render.window.height), (1280, 720));
        assert_eq!(
            (
                render.hud.enabled,
                render.hud.show_camera,
                render.hud.show_reticle
            ),
            (true, true, true),
            "the debug/HUD toggles project through (ADR 0015; issue #8 Phase 3)"
        );
        assert!(
            approx(render.animation.duration_ms, 250.0),
            "the shaping-animation duration projects through (ADR 0022 §5; Phase 3)"
        );
        assert!(
            approx(render.animation.ripple_ms_per_unit, 18.0),
            "the ripple stagger projects through (ADR 0022 §5; Phase 4)"
        );
        // Exercise Debug of the projected RenderParams tree.
        assert!(format!("{render:?}").contains("RenderParams"));
    }

    #[test]
    fn a_negative_animation_duration_is_rejected() {
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[render.animation]\nduration_ms = -1.0\n".into(),
        };
        render_params_from_layers(&[default_layer(), overlay])
            .expect_err("a negative animation duration must fail garde validation");
    }

    #[test]
    fn an_out_of_range_water_opacity_is_rejected() {
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[render.water]\nopacity = 1.5\n".into(),
        };
        render_params_from_layers(&[default_layer(), overlay])
            .expect_err("a water opacity above 1.0 must fail garde validation (ADR 0023, Phase 2)");
    }

    #[test]
    fn render_params_are_disjoint_from_the_core_params() {
        // The two projections of one validated config are independent: the
        // render load path carries no sim state and vice versa (ADR 0020 §4).
        let params = params_from_layers(&[default_layer()]).expect("params must load");
        let render = render_params_from_layers(&[default_layer()]).expect("render must load");
        assert_eq!(params.sim.terrain.max_height, 64);
        assert!(approx(render.camera.far, 1000.0));
    }

    #[test]
    fn out_of_range_render_value_is_rejected() {
        // Presentation config is validated on the same whole-root pass as sim
        // config: an impossible field of view fails before any params are built.
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[render.camera]\nfov_degrees = 200.0\n".into(),
        };
        render_params_from_layers(&[default_layer(), overlay])
            .expect_err("a field of view above 179° must fail garde validation");
    }

    #[test]
    fn default_layer_projects_input_params() {
        let input = input_params_from_layers(&[default_layer()])
            .expect("the default layer must yield input params");
        assert_eq!(input.shape.raise_button, PointerButton::Left);
        assert_eq!(input.shape.lower_button, PointerButton::Right);
        assert!(approx(input.shape.click_drag_threshold_px, 6.0));
    }

    #[test]
    fn pointer_button_round_trips_each_value() {
        for (authored, expected) in [
            ("left", PointerButton::Left),
            ("right", PointerButton::Right),
            ("middle", PointerButton::Middle),
        ] {
            let overlay = Layer {
                name: "local.toml".into(),
                text: format!("[input.shape]\nraise_button = \"{authored}\"\n"),
            };
            let input = input_params_from_layers(&[default_layer(), overlay])
                .expect("each pointer button must parse and map");
            assert_eq!(input.shape.raise_button, expected);
        }
    }

    #[test]
    fn a_negative_click_drag_threshold_is_rejected() {
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[input.shape]\nclick_drag_threshold_px = -1.0\n".into(),
        };
        input_params_from_layers(&[default_layer(), overlay])
            .expect_err("a negative click/drag threshold must fail garde validation");
    }

    #[test]
    fn unknown_pointer_button_is_rejected() {
        let overlay = Layer {
            name: "local.toml".into(),
            text: "[input.shape]\nraise_button = \"scroll\"\n".into(),
        };
        input_params_from_layers(&[default_layer(), overlay])
            .expect_err("an unrecognised pointer button must fail deserialisation");
    }

    #[test]
    fn schema_version_mismatch_is_a_migration_error() {
        let bad = Layer {
            name: "default.toml".into(),
            text: format!(
                "[meta]\nschema_version = 999\n\n\
                 {SIM_CONTENT_TOML}\n{RENDER_TOML}\n{INPUT_TOML}"
            ),
        };
        let err = params_from_layers(&[bad]).expect_err("version mismatch must be an error");
        assert!(
            err.to_string().contains("999"),
            "error must state found vs supported versions"
        );
    }
}
