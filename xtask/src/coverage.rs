//! Tests + coverage (I5, ADR 0009 §5): one instrumented `cargo llvm-cov`
//! run executes the whole test suite and produces per-file line coverage;
//! per-crate thresholds are then enforced (core carries the highest bar).
//!
//! Excluded from measurement (documented in ADR 0009): `xtask` (the gate is
//! dev tooling, not the game) and `crates/providence` (composition root —
//! wiring, no logic; its e2e tests still run and still count toward the
//! coverage of the crates they exercise).

use std::fs;

/// Per-crate line-coverage thresholds (gate knobs, tunable without an ADR).
const THRESHOLDS: &[(&str, f64)] = &[
    ("crates/core/", 90.0), // deterministic core: highest bar (I5)
    ("crates/config/", 70.0),
    ("crates/ports/", 70.0),
    ("crates/app/", 70.0),
    ("adapters/config-loader/", 70.0),
];

/// Files never measured (see module docs).
const IGNORE_REGEX: &str = "(xtask/|crates/providence/)";

const REPORT_PATH: &str = "target/llvm-cov/coverage.json";

/// Run the instrumented suite and enforce per-crate thresholds.
pub fn check() -> Result<(), String> {
    let root = crate::workspace_root();
    fs::create_dir_all(root.join("target/llvm-cov"))
        .map_err(|error| format!("cannot create target/llvm-cov: {error}"))?;

    // Runs every workspace test (a test failure fails this command).
    // `--features debug-hud` compiles and exercises the renderer's feature-gated
    // `egui` overlay (ADR 0015; issue #8 Phase 3) so its pure logic is measured
    // and its tests run, matching the clippy pass (ADR 0020 enforcement).
    crate::run::cargo(&[
        "llvm-cov",
        "--workspace",
        "--features",
        "debug-hud",
        "--json",
        "--output-path",
        REPORT_PATH,
        "--ignore-filename-regex",
        IGNORE_REGEX,
    ])?;

    let report_text = fs::read_to_string(root.join(REPORT_PATH))
        .map_err(|error| format!("cannot read coverage report: {error}"))?;
    let report: serde_json::Value = serde_json::from_str(&report_text)
        .map_err(|error| format!("coverage report is not valid JSON: {error}"))?;

    let files = report
        .get("data")
        .and_then(|data| data.get(0))
        .and_then(|first| first.get("files"))
        .and_then(serde_json::Value::as_array)
        .ok_or("coverage report has no data[0].files")?;

    let root_display = root.display().to_string();
    let mut failures = Vec::new();
    println!("  per-crate line coverage:");
    for (crate_prefix, threshold) in THRESHOLDS {
        let (mut total, mut covered) = (0.0_f64, 0.0_f64);
        for file in files {
            let Some(filename) = file.get("filename").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let relative = filename.strip_prefix(&root_display).unwrap_or(filename);
            if relative.trim_start_matches('/').starts_with(crate_prefix) {
                let lines = file.get("summary").and_then(|summary| summary.get("lines"));
                total += lines
                    .and_then(|lines| lines.get("count"))
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                covered += lines
                    .and_then(|lines| lines.get("covered"))
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
            }
        }
        if total == 0.0 {
            println!("    {crate_prefix:<28} (no measurable lines — skipped)");
            continue;
        }
        let percent = covered / total * 100.0;
        println!("    {crate_prefix:<28} {percent:6.2}%  (threshold {threshold}%)");
        if percent < *threshold {
            failures.push(format!(
                "{crate_prefix} line coverage {percent:.2}% is below its {threshold}% threshold"
            ));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n    "))
    }
}
