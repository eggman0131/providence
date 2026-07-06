//! Application layer — orchestration only, no game rules
//! (docs/20-architecture.md §2.3).
//!
//! Phase-1 scope: own a session (params + seed + current state) and advance
//! it through the core's `step`. The turn scheduler, command application,
//! and port mediation land in later phases (contract §7.4).

#![forbid(unsafe_code)]

use providence_config::Params;
use providence_core::rng::SplitMix64;
use providence_core::state::{State, step};

/// A running game session: current state plus the config and seed it runs
/// under (docs/20-architecture.md §2.3).
#[derive(Debug)]
pub struct Session {
    params: Params,
    rng: SplitMix64,
    state: State,
}

impl Session {
    /// Start a session from validated params and a seed.
    #[must_use]
    pub fn new(params: Params, seed: u64) -> Self {
        Self {
            params,
            rng: SplitMix64::new(seed),
            state: State::initial(),
        }
    }

    /// Advance the simulation by one step.
    pub fn advance(&mut self) {
        self.state = step(&self.state, &self.params, &mut self.rng);
    }

    /// Current state (read-only).
    #[must_use]
    pub fn state(&self) -> &State {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use providence_config::{
        EconomyParams, ManaMode, ManaParams, OpponentParams, Params, PlaceholderParams, SimParams,
        WinLossParams,
    };

    use super::Session;

    fn params() -> Params {
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

    #[test]
    fn sessions_with_identical_inputs_stay_identical() {
        let mut a = Session::new(params(), 42);
        let mut b = Session::new(params(), 42);
        for _ in 0..100 {
            a.advance();
            b.advance();
        }
        assert_eq!(
            a.state(),
            b.state(),
            "same seed + params must stay bit-identical (I3)"
        );
    }

    #[test]
    fn advancing_moves_the_tick_by_the_configured_increment() {
        let mut session = Session::new(params(), 42);
        session.advance();
        session.advance();
        assert_eq!(session.state().tick, 2);
    }
}
