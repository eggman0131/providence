//! End-to-end: the SHIPPED default config loads, validates, and drives the
//! deterministic core — exercised, not just asserted (contract §3
//! "Verified"; docs/40-parameterisation.md §6.1).

use std::path::Path;

use providence_config::ManaMode;
use providence_config_loader::{Layer, load_dir, load_with_profile, params_from_layers};

fn config_dir() -> &'static Path {
    // The workspace root, relative to this crate's manifest.
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../config"))
}

#[test]
fn shipped_default_config_loads_and_drives_the_core() {
    let params = load_dir(config_dir()).expect("shipped default.toml must load and validate");
    let increment = params.sim.placeholder.tick_increment;

    let mut a = providence_app::Session::new(params.clone(), 7);
    let mut b = providence_app::Session::new(params, 7);
    for _ in 0..50 {
        a.advance();
        b.advance();
    }
    assert_eq!(
        a.state(),
        b.state(),
        "same config + seed must stay bit-identical (I3)"
    );
    assert_eq!(
        a.state().tick,
        50 * increment,
        "tick must advance by the configured increment"
    );
}

#[test]
fn config_only_change_changes_behaviour_with_no_source_edit() {
    // The no-code-change rule (docs/40-parameterisation.md §6.1), through the
    // real loader: overlay a user layer on the SHIPPED default file and
    // observe different behaviour — zero source edits.
    let default_text = std::fs::read_to_string(config_dir().join("default.toml"))
        .expect("shipped default.toml must be readable");
    let default_layer = Layer {
        name: "default.toml".into(),
        text: default_text,
    };
    let overlay = Layer {
        name: "test-overlay".into(),
        text: "[sim.placeholder]\ntick_increment = 7\n".into(),
    };

    let baseline =
        params_from_layers(std::slice::from_ref(&default_layer)).expect("default layer must load");
    let tuned = params_from_layers(&[default_layer, overlay]).expect("overlay must load");

    let mut baseline_session = providence_app::Session::new(baseline, 7);
    let mut tuned_session = providence_app::Session::new(tuned, 7);
    for _ in 0..3 {
        baseline_session.advance();
        tuned_session.advance();
    }
    assert_eq!(
        tuned_session.state().tick,
        21,
        "tuned increment must be observable"
    );
    assert_ne!(
        baseline_session.state().tick,
        tuned_session.state().tick,
        "a config-only change must change observable behaviour"
    );
}

#[test]
fn sandbox_profile_composes_god_mode_through_the_real_loader() {
    // ADR 0016 §3: the shipped `sandbox` profile, loaded through the real
    // ConfigPort path, composes "opponent off, mana unlimited, win/loss off"
    // into one selectable layer.
    let params = load_with_profile(config_dir(), Some("sandbox"))
        .expect("the sandbox profile must load and validate");

    assert!(
        !params.sim.opponent.enabled,
        "sandbox disables the opponent"
    );
    assert_eq!(
        params.sim.economy.mana.mode,
        ManaMode::Unlimited,
        "sandbox sets mana to unlimited"
    );
    assert!(
        !params.sim.winloss.enabled,
        "sandbox disables win/loss evaluation"
    );
}

#[test]
fn unlimited_mana_leaves_the_opponent_untouched_against_the_shipped_default() {
    // The first-slice guarantee against the SHIPPED default.toml (not a
    // synthetic fixture): setting mana to `unlimited` changes nothing the
    // opponent subsystem owns. The reset's coupling cascade is impossible by
    // construction — the subtrees are disjoint (ADR 0016 §3).
    let default_text = std::fs::read_to_string(config_dir().join("default.toml"))
        .expect("shipped default.toml must be readable");
    let default_layer = Layer {
        name: "default.toml".into(),
        text: default_text,
    };
    let unlimited_mana = Layer {
        name: "mana-only".into(),
        text: "[sim.economy.mana]\nmode = \"unlimited\"\n".into(),
    };

    let governed =
        params_from_layers(std::slice::from_ref(&default_layer)).expect("default must load");
    let flipped =
        params_from_layers(&[default_layer, unlimited_mana]).expect("mana override must load");

    assert_eq!(
        flipped.sim.economy.mana.mode,
        ManaMode::Unlimited,
        "the mana knob did move"
    );
    assert_eq!(
        governed.sim.opponent, flipped.sim.opponent,
        "unlimited mana must not touch anything the opponent owns (ADR 0016 §3)"
    );
}

#[test]
fn a_mistyped_profile_is_an_error_not_a_silent_default() {
    // A named-but-missing profile must fail loudly, never fall back to the
    // governed defaults (a silent fallback would hide a typo'd experiment).
    load_with_profile(config_dir(), Some("no_such_profile"))
        .expect_err("a missing named profile must be an error");
}
