//! Authoring structs — the single source of truth for the config schema
//! (ADR 0008): `serde` shapes the keys (`deny_unknown_fields` rejects
//! anything outside them), `garde` carries ranges and cross-key invariants,
//! `schemars` generates the committed JSON Schema.
//!
//! These are std types; they map into the `no_std` `providence-config`
//! param types via [`ConfigRoot::into_params`] (ADR 0009 refinement).

use garde::Validate;
use providence_config::{
    EconomyParams, ManaMode, ManaParams, OpponentParams, Params, PlaceholderParams, SimParams,
    WinLossParams,
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

/// `sim.placeholder.*` — placeholder parameters proving config → core wiring.
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct PlaceholderSection {
    /// `sim.placeholder.tick_increment` — ticks the placeholder state
    /// advances per step. Hot-reloadable (a pure balance value).
    #[garde(range(min = 1))]
    pub tick_increment: u64,
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
                placeholder: PlaceholderParams {
                    tick_increment: self.sim.placeholder.tick_increment,
                },
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
