//! Boundary-manifest check (ADR 0009 §3): every crate's declared internal
//! dependency edges must be a subset of the allowed DAG realising
//! docs/20-architecture.md §5. Cargo itself guarantees acyclicity; this
//! check guarantees *direction*.
//!
//! `[dev-dependencies]` are exempt: tests live outside the runtime graph
//! (they are the test doubles I4 mandates). Only `[dependencies]` edges are
//! architecture.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

/// The allowed internal dependency edges, by package name
/// (docs/20-architecture.md §5; ADR 0009 §3). Changing an edge here is an
/// architectural change → ADR (contract §5).
fn allowed_edges() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    let mut allowed = BTreeMap::new();
    // The core imports ONLY config-layer data types (§5 rule 1).
    allowed.insert("providence-core", BTreeSet::from(["providence-config"]));
    // Config and ports are leaves: plain data / trait interfaces.
    allowed.insert("providence-config", BTreeSet::new());
    allowed.insert("providence-ports", BTreeSet::new());
    // Application: core + config + port interfaces, never an adapter (§5 rule 2).
    allowed.insert(
        "providence-app",
        BTreeSet::from(["providence-core", "providence-config", "providence-ports"]),
    );
    // Adapters: ports (to implement) + config data types; never each other (§5 rule 3).
    allowed.insert(
        "providence-config-loader",
        BTreeSet::from(["providence-config", "providence-ports"]),
    );
    // The workbench renderer adapter (ADR 0020): implements RendererPort and
    // reads RenderParams; it never imports the core (only a derived snapshot
    // crosses the port). GPU deps (wgpu/winit) are confined to this crate.
    allowed.insert(
        "providence-renderer",
        BTreeSet::from(["providence-config", "providence-ports"]),
    );
    // Composition root: the only crate allowed to see concrete adapters.
    allowed.insert(
        "providence",
        BTreeSet::from([
            "providence-core",
            "providence-config",
            "providence-ports",
            "providence-app",
            "providence-config-loader",
            "providence-renderer",
        ]),
    );
    // The gate itself: dev tooling outside the runtime graph. May use the
    // config-loader (schema regeneration/validation) but never core/app.
    allowed.insert(
        "xtask",
        BTreeSet::from(["providence-config-loader", "providence-config"]),
    );
    allowed
}

/// Crates that must not declare ANY external (crates.io) dependency.
const ZERO_EXTERNAL_DEPS: &[&str] = &["providence-core", "providence-config", "providence-ports"];

/// Verify every workspace member's `[dependencies]` against the allowed DAG.
pub fn check() -> Result<(), String> {
    let root = crate::workspace_root();
    let manifest_text = fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|error| format!("cannot read workspace Cargo.toml: {error}"))?;
    let manifest: toml::Value = toml::from_str(&manifest_text)
        .map_err(|error| format!("workspace Cargo.toml is not valid TOML: {error}"))?;

    let members: Vec<String> = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(|members| members.as_array())
        .ok_or("workspace Cargo.toml has no [workspace] members list")?
        .iter()
        .filter_map(|member| member.as_str().map(str::to_owned))
        .collect();

    let allowed = allowed_edges();
    let mut violations = Vec::new();

    for member in &members {
        let member_manifest_path = root.join(member).join("Cargo.toml");
        let member_text = fs::read_to_string(&member_manifest_path)
            .map_err(|error| format!("cannot read {}: {error}", member_manifest_path.display()))?;
        let member_manifest: toml::Value = toml::from_str(&member_text)
            .map_err(|error| format!("{member}/Cargo.toml is not valid TOML: {error}"))?;

        let package_name = member_manifest
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(|name| name.as_str())
            .ok_or_else(|| format!("{member}/Cargo.toml has no package.name"))?
            .to_owned();

        let Some(allowed_deps) = allowed.get(package_name.as_str()) else {
            violations.push(format!(
                "crate `{package_name}` is not in the allowed dependency DAG; \
                 new crates are architectural changes → ADR (contract §5)"
            ));
            continue;
        };

        let dependency_names: Vec<String> = member_manifest
            .get("dependencies")
            .and_then(|dependencies| dependencies.as_table())
            .map(|table| table.keys().cloned().collect())
            .unwrap_or_default();

        for dependency in &dependency_names {
            let is_internal = dependency.starts_with("providence") || dependency == "xtask";
            if is_internal && !allowed_deps.contains(dependency.as_str()) {
                violations.push(format!(
                    "illegal edge: `{package_name}` → `{dependency}` \
                     (allowed: {allowed_deps:?}; docs/20-architecture.md §5)"
                ));
            }
            if !is_internal && ZERO_EXTERNAL_DEPS.contains(&package_name.as_str()) {
                violations.push(format!(
                    "`{package_name}` must have zero external dependencies \
                     (ADR 0006/0009) but declares `{dependency}`"
                ));
            }
        }
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations.join("\n    "))
    }
}
