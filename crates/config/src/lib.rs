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

/// `sim.placeholder.*` — placeholder parameters proving the config → core
/// wiring end-to-end (contract §7.2). Deleted when the Phase-3 core consumes
/// real subsystem state (prefer deletion, contract §4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceholderParams {
    /// `sim.placeholder.tick_increment` — ticks the placeholder state
    /// advances per step.
    pub tick_increment: u64,
}
