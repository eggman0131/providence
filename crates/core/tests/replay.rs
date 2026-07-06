//! Determinism/replay harness (contract §7.1, invariant I3, ADR 0009).
//!
//! Two runs with identical seed + params must produce a bit-identical state
//! history, and that history's fingerprint must match the committed golden
//! hash. The golden changes only on an intentional, reviewed core change.

use providence_config::{
    EconomyParams, ManaMode, ManaParams, OpponentParams, Params, PlaceholderParams, SimParams,
    WinLossParams,
};
use providence_core::hash::Fnv1a64;
use providence_core::rng::SplitMix64;
use providence_core::state::{State, step};

const SEED: u64 = 0xD1CE;
const STEPS: u64 = 1_000;

/// Committed golden fingerprint of the full state history for
/// (`SEED`, `STEPS`, the fixture params below). Recompute ONLY for an
/// intentional core change, and call the change out in the PR.
const GOLDEN: u64 = 0x804F_981D_B5F9_5BC5;

fn fixture_params() -> Params {
    Params {
        sim: SimParams {
            opponent: OpponentParams { enabled: true },
            economy: EconomyParams {
                mana: ManaParams {
                    mode: ManaMode::Normal,
                },
            },
            winloss: WinLossParams { enabled: true },
            placeholder: PlaceholderParams { tick_increment: 1 },
        },
    }
}

/// Run the placeholder simulation and fingerprint every intermediate state.
fn run_history_fingerprint() -> u64 {
    let params = fixture_params();
    let mut rng = SplitMix64::new(SEED);
    let mut state = State::initial();
    let mut hasher = Fnv1a64::new();
    for _ in 0..STEPS {
        state = step(&state, &params, &mut rng);
        hasher.write_u64(state.tick);
        hasher.write_u64(state.accumulator);
    }
    hasher.finish()
}

#[test]
fn identical_inputs_produce_identical_histories() {
    assert_eq!(
        run_history_fingerprint(),
        run_history_fingerprint(),
        "two runs with the same seed + params diverged (I3 violation)"
    );
}

#[test]
fn history_matches_committed_golden() {
    assert_eq!(
        run_history_fingerprint(),
        GOLDEN,
        "state history diverged from the committed golden hash; if this core \
         change is intentional, update GOLDEN and say so in the PR"
    );
}

#[test]
fn params_change_observably_changes_behaviour() {
    // The no-code-change rule (docs/40-parameterisation.md §6.1): a config
    // value change must change observable behaviour with no source edit.
    let mut params = fixture_params();
    params.sim.placeholder.tick_increment = 5;
    let mut rng = SplitMix64::new(SEED);
    let mut state = State::initial();
    for _ in 0..3 {
        state = step(&state, &params, &mut rng);
    }
    assert_eq!(
        state.tick, 15,
        "tick_increment=5 over 3 steps must yield tick 15"
    );
}
