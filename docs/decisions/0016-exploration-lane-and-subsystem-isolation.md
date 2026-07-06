# 0016 — Exploration lane and subsystem isolation

- **Status:** Proposed
- **Date:** 2026-07-06
- **Deciders:** Director + agent (fresh-start session)
- **Related:** [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (§3 Definition of Done, §6 enforcement, §7 phasing; I1, I3, I6), [`0009`](./0009-enforcement-tooling-and-the-gate.md) (the gate), [`0004`](./0004-deterministic-core-ports-and-adapters.md) (deterministic core, ports & adapters), [`0003`](./0003-parameterisation-first.md) (parameterisation-first), [`0008`](./0008-toml-config-format-types-first-schema.md) (config schema), [`../40-parameterisation.md`](../40-parameterisation.md)

## Context

This repository is a **fresh start**: the governance, the gate ([ADR 0009](./0009-enforcement-tooling-and-the-gate.md)), and the empty crate skeletons were carried forward from the prior build (`providence-legacy` @ `8ca4293`); the game mechanics, renderer, and advisors were deliberately left behind, to be rebuilt differently. This ADR records *why differently*.

The prior build worked — but iterating on a **single** mechanic became expensive out of all proportion, for two compounding reasons:

1. **The gate is calibrated for committed work, and there is only one lane.** Every exploratory tweak paid the full tax: determinism/replay (I3), magic-number scan (I1), coverage (I5), schema drift, and docs-in-the-same-commit (I6). That is correct for code headed to `main`; it is crippling for a throwaway probe of "does this terrain mechanic feel right".

2. **Subsystems were coupled, so relaxing one constraint cascaded.** The motivating example: to feel out terrain build/destroy, the director asked for mana to be effectively unlimited. Mana was load-bearing for the opponent's economy, so "unlimited mana" changed the opponent's behaviour, which changed the recorded simulation, which broke the determinism/replay fixtures — and a session was spent repairing subsystems the experiment never intended to touch. **The opponent did not even need to be running.**

The lesson is not "less rigour". The governance is the point of this project ([`30-ai-agent-contract.md`](../30-ai-agent-contract.md) §6.1: *anything not enforced by a tool is only a recommendation*). The lesson is that **exploration and governed construction are different activities** and the toolchain currently only serves the second. We need a cheap, **blast-radius-limited** way to probe one mechanic, without weakening the gate that guards `main`.

## Decision

We will support **two lanes** and make **subsystem isolation** a structural requirement of the rebuild.

### 1. An exploration lane: `cargo xtask explore`

A fast subset for `explore/*` branches: **format + clippy** — one compile-and-lint pass (clippy type-checks, so no separate `cargo check` is needed) — plus, once there is an app to launch, a boot smoke-test. It **skips** determinism/replay, golden-image, coverage, magic-number, schema-drift, and config-key-integrity checks.

`cargo xtask explore` is **not** a Definition of Done. The full **`cargo gate`** (ADR 0009) is unchanged and remains the *single* definition of "green" for `main` and for promotion (§3). The explore lane only answers "does this compile and lint" so a probe can run.

### 2. Exploration branches are exempt from I1 and I6 until promotion

On an `explore/*` branch, behavioural literals (I1) and same-commit doc updates (I6) are **not** required — an experiment is allowed to be scrappy. An **ADR is required only when promoting** a direction into the governed core, never for a throwaway probe. Exploration branches are second-class by construction: they are never merged to `main`; a surviving idea is *re-implemented* on a governed branch, not fast-forwarded.

### 3. Subsystem isolation is mandatory (isolation by default)

Every major simulation subsystem — opponent/advisor, economy/mana, win-loss, and any future peer — must be **independently disable-able through config**, and **disabling one must not break the build or the remaining subsystems**. Cross-subsystem influence flows through explicit seams (a subsystem reads its *own* budget/state), never through shared mutable coupling that makes one knob leak into another.

Concretely, the rebuilt parameter layer will reserve:

| Key | Meaning |
|---|---|
| `sim.opponent.enabled` | `false` ⇒ no rival deity; the loop runs, nothing casts against the player. |
| `sim.economy.mana.mode` | `normal` \| `fast` \| `unlimited` — first-class god-mode mana, not a hack. |
| `sim.winloss.enabled` | `false` ⇒ no win/loss evaluation during free play. |
| `sim.<subsystem>.enabled` | the general form: every subsystem carries an on/off seam. |

A **`sandbox` config profile** (a layer, per [ADR 0008](./0008-toml-config-format-types-first-schema.md)) composes these into one flag — "let me play with one mechanic": opponent off, mana unlimited, win/loss off. With `sim.opponent.enabled = false`, flipping mana to `unlimited` touches nothing the opponent owns, because the opponent is not running — and even when it is, it reads its own budget through the seam. **The mana cascade cannot recur.**

### 4. Determinism is scoped to the governed configuration (I3 clarified, not weakened)

The replay/determinism golden ([ADR 0009](./0009-enforcement-tooling-and-the-gate.md) §3) asserts reproducibility for the **committed default configuration**. Sandbox/exploration-only configuration is explicitly **outside** the deterministic contract: toggling it *cannot* make the replay test fail, because it is not part of what determinism promises. The shipped game's determinism is unchanged — we are scoping *what* must be deterministic, not relaxing the guarantee.

### 5. Promotion path

When an experiment earns its place it moves onto a governed branch and pays the full price then, once: its config keys + schema entries ([ADR 0008](./0008-toml-config-format-types-first-schema.md)), its determinism coverage (I3), its docs in the same commit (I6), an ADR if it is architectural (§5), and a green `cargo gate`.

**Sequencing.** This fresh baseline has no parameter layer yet (it was left behind). The `sim.*.enabled` keys, the mana `mode`, and the `sandbox` profile therefore land **with the parameter-layer rebuild**, honouring this ADR as their contract. What this ADR ships now is the workflow decision and the `cargo xtask explore` lane.

## Consequences

- **Positive:**
  - Iterating on one mechanic is cheap again; the fast lane is *shared and named*, not re-derived per session.
  - An experiment's blast radius is bounded by construction — the mana/opponent cascade is structurally prevented, not remembered.
  - The governed gate is **untouched**; `main` quality and I3 determinism are exactly as strong as before.
  - Isolation-by-default makes the eventual full build cleaner — seams are designed in, not retrofitted.
- **Negative / trade-offs:**
  - Two lanes to keep straight; a risk that experiments linger un-promoted (mitigated: `explore/*` is never merged, so stale probes cannot rot `main`).
  - Every subsystem pays a small up-front design cost (an `enabled` seam + honest defaults) so that isolation actually holds.
  - One more `xtask` subcommand to maintain.
- **Enforcement / gate impact:** adds `cargo xtask explore` (fast subset: fmt + clippy — clippy is the compile-and-lint pass). The `cargo gate` check list (ADR 0009) is **unchanged**. A future gate check *may* assert that every `sim.*` subsystem exposes an `enabled` switch — deferred until the parameter layer exists. No invariant *values* change: I1 and I6 gain an explicit **exemption scope** (`explore/*` branches, pre-promotion); I3 gains an explicit **scope** (the governed configuration).
- **Docs to update (this change):** `decisions/README.md` (index). Deferred to their rebuild: `40-parameterisation.md` (the `sim.*.enabled` / mana `mode` / `sandbox` conventions), `30-ai-agent-contract.md` (record the exploration lane and the I1/I6 exemption + I3 scoping when that doc is next revised), and `CLAUDE.md` (a pointer to the lane).

## Alternatives considered

- **Keep one lane; just run fewer checks by hand while exploring.** Nothing stops the coupling cascade, and every agent re-invents which checks to skip. Rejected — the fast lane must be named and shared, and isolation must be structural.
- **Feature-flag experiments in code (`#[cfg]`), not config.** Pushes on/off to compile time, fragments the build, and gives no *runtime* sandbox — the mana example needs to flip mid-session, not per-build. Rejected.
- **Loosen determinism globally during exploration.** I3 is non-negotiable for the shipped game ([ADR 0004](./0004-deterministic-core-ports-and-adapters.md)). Scoping *what* must be deterministic preserves the guarantee; a global loosening would not. Rejected.
- **Just delete the heavy tests.** Throws away the governance this project is explicitly built to keep. Rejected.
