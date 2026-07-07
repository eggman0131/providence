//! The gate (contract §9, ADR 0009).
//!
//! `cargo gate` (alias for `cargo xtask gate`) runs every enforcement check
//! and is the single definition of "green" — locally and in CI (ADR 0010).
//! `cargo xtask setup` is the one-command environment provision (I9).
//! `cargo xtask schema --write` regenerates the committed schema artifact.

mod boundary;
mod coverage;
mod doc_review;
mod keys;
mod magic;
mod run;
mod schema;
mod tools;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    match arg_refs.as_slice() {
        ["setup"] => tools::setup(),
        ["gate"] => gate(),
        ["explore"] => explore(),
        ["schema"] => schema::check().map_or(ExitCode::FAILURE, |()| ExitCode::SUCCESS),
        ["schema", "--write"] => schema::write(),
        ["doc-review", rest @ ..] => doc_review::run(rest),
        _ => {
            eprintln!(
                "usage: cargo xtask <setup | gate | explore | schema [--write] \
                 | doc-review [--since <ref>] [--json]>"
            );
            ExitCode::FAILURE
        }
    }
}

/// The workspace root (xtask's manifest dir is `<root>/xtask`).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask always lives one level below the workspace root")
        .to_path_buf()
}

/// One gate check: display name + the function that runs it.
type Check = (&'static str, fn() -> Result<(), String>);

/// The full gate (ADR 0009): every enforcement check, in order — the single
/// definition of "green" for `main` and for promotion (§3).
///
/// Order: cheap static checks first, the instrumented test+coverage run last.
fn gate() -> ExitCode {
    let checks: &[Check] = &[
        ("format (rustfmt --check)", check_format),
        ("lint (clippy -D warnings)", check_clippy),
        ("boundaries (crate-graph direction, I2/I4)", boundary::check),
        (
            "magic numbers (core behavioural literals, I1)",
            magic::check,
        ),
        (
            "schema drift (regenerate-and-diff, ADR 0008)",
            schema::check,
        ),
        (
            "config validity (layers + merged whole)",
            keys::check_config_validates,
        ),
        (
            "config keys (namespaces + schema integrity)",
            keys::check_keys,
        ),
        (
            "dependencies (cargo-deny: advisories/licenses/bans)",
            check_deny,
        ),
        (
            "tests + coverage (thresholds per crate, I5)",
            coverage::check,
        ),
    ];
    run_lane("gate", "", checks)
}

/// The exploration fast lane (ADR 0016): format + clippy only — one
/// compile-and-lint pass (clippy type-checks, so no separate `cargo check`).
/// It deliberately SKIPS determinism/replay, golden-image, coverage,
/// magic-number, and schema checks. It is **not** a Definition of Done:
/// `cargo gate` stays the only definition of "green" for `main` (§3).
fn explore() -> ExitCode {
    let checks: &[Check] = &[
        ("format (rustfmt --check)", check_format),
        ("lint (clippy -D warnings)", check_clippy),
    ];
    run_lane(
        "explore",
        " — fast lane, NOT the gate; run `cargo gate` before promoting",
        checks,
    )
}

/// Run `checks` in order, printing progress, and return an exit code.
/// `lane` labels the banner and summary; `note` is appended to the banner.
/// Shared by the full [`gate`] and the [`explore`] fast lane (ADR 0016).
fn run_lane(lane: &str, note: &str, checks: &[Check]) -> ExitCode {
    println!("{lane}: {} checks{note}\n", checks.len());
    let mut failures = Vec::new();
    for (name, check) in checks {
        println!("──► {name}");
        match check() {
            Ok(()) => println!("  ✓ {name}\n"),
            Err(message) => {
                println!("  ✗ {name}\n    {message}\n");
                failures.push(*name);
            }
        }
    }

    if failures.is_empty() {
        println!("{lane}: GREEN ({} checks)", checks.len());
        ExitCode::SUCCESS
    } else {
        println!(
            "{lane}: RED — {} of {} checks failed:",
            failures.len(),
            checks.len()
        );
        for name in &failures {
            println!("  ✗ {name}");
        }
        ExitCode::FAILURE
    }
}

/// Canonical style: rustfmt defaults, checked, zero tolerance (§6.2).
fn check_format() -> Result<(), String> {
    run::cargo(&["fmt", "--all", "--", "--check"])
}

/// Type check + lints: clippy over everything, all warnings denied (§6.2).
///
/// `--features debug-hud` brings the renderer's feature-gated `egui` overlay
/// (ADR 0015; issue #8 Phase 3) under the linter — it is off in the default
/// build, so without this the overlay would go unchecked (ADR 0020 enforcement).
fn check_clippy() -> Result<(), String> {
    run::cargo(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--features",
        "debug-hud",
        "--",
        "-D",
        "warnings",
    ])
}

/// External-dependency policy: advisories, licenses, bans, sources (I8).
fn check_deny() -> Result<(), String> {
    run::cargo(&["deny", "check"])
}
