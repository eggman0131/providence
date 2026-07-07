# 30 — The AI-Agent Contract

> **Status:** Active · **Audience:** Every AI agent (Opus / Sonnet) that reads, writes, or reviews code in this repository · **Authority:** Governing document. This file is loaded into context for *every* task (see the Context Diet in [`README.md`](./README.md)).

This project has **no human code author**. It is built and maintained entirely by AI agents. This document is therefore not advice — it is the *constitution* of the codebase. If you are an agent working here, you are bound by it. When code and this contract disagree, the contract wins and the code is a defect to be fixed.

---

## 1. Purpose & authority

1.1 This contract is the **single source of truth** for *how* work is done here. The other docs describe *what* we are building; this describes the rules under which any change is made.

1.2 **Precedence order.** When guidance conflicts, resolve in this order (highest first):
1. This contract (`30-ai-agent-contract.md`)
2. [`20-architecture.md`](./20-architecture.md) — structural rules
3. [`40-parameterisation.md`](./40-parameterisation.md), [`50-llm-opponent.md`](./50-llm-opponent.md) — subsystem rules
4. [`10-game-design.md`](./10-game-design.md), [`00-vision.md`](./00-vision.md), [`60-constraints.md`](./60-constraints.md) — intent & limits
5. Accepted ADRs in [`decisions/`](./decisions/) — refine and may override the above *for the decision they record*
6. Code, comments, and commit messages

1.3 **Self-enforcement.** There is no human reviewer to catch a violation. You must assume that anything you merge is final. Verify with tooling, not with hope (§6).

1.4 **When blocked.** If a required action would violate this contract, do **not** proceed and route around it silently. Stop, state the conflict plainly, and either (a) propose an ADR that amends the contract (§8), or (b) ask the human director for a decision. Never weaken an invariant to make a task easier. When you take route (b), frame the choice by its **outcomes** — gameplay, player experience, future flexibility — per §4.2, so the director can rule on it without reading the code.

---

## 2. Prime invariants (non-negotiable)

These are enforced by tooling wherever possible (§6). A change that breaks one is a defect regardless of whether it "works".

- **I1 — Parameterisation-first.** All tunable behaviour, balance, and content lives in versioned, schema-validated configuration — never in code. A literal that affects behaviour/balance/content and is hard-coded in a source file is a defect ("magic number"). Litmus test: *could a designer change this without editing code?* If it should be yes, it must be config. See [`40-parameterisation.md`](./40-parameterisation.md). *Scope:* relaxed on throwaway `explore/*` branches until promotion ([ADR 0016](./decisions/0016-exploration-lane-and-subsystem-isolation.md)).

- **I2 — Modularity & boundaries.** The system is a set of small modules with explicit, one-directional dependencies. Dependencies point **inward** toward the deterministic core; the core imports nothing outward. No cyclic dependencies. Boundaries are enforced by tooling, not convention. See [`20-architecture.md`](./20-architecture.md).

- **I3 — Deterministic core.** The simulation core is pure and reproducible: **same seed + same inputs ⇒ same outputs**, bit-for-bit, forever. No wall-clock reads, no ambient randomness, no hidden I/O, no network, no filesystem inside the core. All randomness flows through a seeded RNG port. *Scope:* the guarantee covers the **committed (governed) configuration**; sandbox/exploration-only toggles sit outside the contract and cannot break replay ([ADR 0016](./decisions/0016-exploration-lane-and-subsystem-isolation.md)).

- **I4 — Everything I/O behind ports.** Every side effect — LLM, rendering, input, persistence, clock, randomness, audio, logging — is reached only through an interface ("port") with swappable adapters and a test double. Core code depends on port *interfaces*, never on a concrete adapter.

- **I5 — Tested & verified.** Every change ships with tests. The deterministic core carries the highest coverage bar. A task is **not done** until the full gate is green (§3) *and* the affected behaviour has been exercised end-to-end, not merely unit-tested.

- **I6 — Docs-as-code.** Behaviour changes update the relevant doc in the same change. Architectural changes add an ADR (§5, §8). Docs and code must never drift; a stale doc is a defect. *Scope:* relaxed on throwaway `explore/*` branches until promotion, at which point the surviving change re-enters under full docs-as-code ([ADR 0016](./decisions/0016-exploration-lane-and-subsystem-isolation.md)).

- **I7 — MacBook-only, offline-capable.** The only supported target is a single MacBook (Apple Silicon assumed until an ADR says otherwise). No runtime requires network access; the LLM runs locally. Do not add cloud/runtime-network dependencies. See [`60-constraints.md`](./60-constraints.md).

- **I8 — Dependency freshness & minimalism.** Prefer *not* adding a dependency. When one is genuinely needed, **research the latest stable version and prefer it**, confirm it supports the target platform offline, pin it, and record the choice (ADR or changelog) with the version and why.

- **I9 — Reproducible environment.** One command sets up the environment; one command runs the full gate. The toolchain is pinned. What runs locally is exactly what "CI" would run — there is no separate cloud CI to hide behind.

---

## 3. Definition of Done

A change is **done** only when *all* of the following hold. This is a checklist you must self-verify before considering any task complete:

- [ ] **Types** — static type check passes with zero errors.
- [ ] **Format & lint** — formatter and linter pass with zero warnings.
- [ ] **Boundaries** — the dependency/boundary checker reports no violations (I2, I4).
- [ ] **Tests** — the full test suite passes; new/changed behaviour has new/updated tests.
- [ ] **Coverage** — coverage meets the configured threshold; the deterministic core meets its higher threshold.
- [ ] **No magic numbers** — no new behavioural literal introduced in code (I1); new tunables added to config + schema.
- [ ] **Config validates** — all config files validate against the schema; every referenced key exists.
- [ ] **Determinism** — a replay/seed test confirms the core still produces identical output for identical seed+inputs (I3).
- [ ] **Docs updated** — affected docs updated in the same change (I6).
- [ ] **ADR** — an ADR is added if the change is architectural (§5).
- [ ] **Verified** — the change was actually run and the intended effect observed, not just asserted by tests.

The mechanised form of this list is **the gate**: a single command (§9) that runs every automatable check above. "Done" ⇒ the gate is green.

> **Two lanes ([ADR 0016](./decisions/0016-exploration-lane-and-subsystem-isolation.md)).** The Definition of Done above governs work headed for `main`. A separate fast lane — `cargo xtask explore` (format + clippy only) — exists for throwaway probes on `explore/*` branches; it is **not** a Definition of Done and never closes a task. `explore/*` branches are never merged: a surviving idea is re-implemented on a governed branch and pays this checklist in full there.

---

## 4. Agent workflow

Follow this loop for every task:

1. **Load the right context.** Apply the Context Diet in [`README.md`](./README.md): always load this contract, plus only the docs the task touches. Do not read the whole repo "to be safe" — that is context bloat and it degrades your output.
2. **Understand & plan.** Restate the task, identify the module(s) and port(s) involved, and the parameters and schema keys affected. Locate existing utilities to reuse before writing anything new.
3. **Implement in small modular units.** One responsibility per module. Prefer composition over inheritance and pure functions in the core. Write code that matches the surrounding style. Avoid clever code — the next agent (possibly a smaller model) must be able to understand it from local context alone.
4. **Add config, not constants.** New tunables go to config + schema with sane defaults (I1).
5. **Test.** Unit-test pure logic; use test doubles for ports; add/adjust a determinism replay test if the core changed.
6. **Run the gate** (§9). Fix everything red.
7. **Update docs & ADRs** (I6, §5).
8. **Self-review against §3.** Only then is the task done.

### 4.1 Guidance for the model doing the work
- **Keep modules context-sized.** A module should fit comfortably in a single reading. If it doesn't, split it. This is a hard design goal because the maintainer is a language model with finite context.
- **Name things explicitly.** No abbreviations that require tribal knowledge. Names are the primary documentation.
- **Fail loudly, early, and with context.** Validate inputs at boundaries; never swallow errors in the core.
- **Prefer deletion.** Dead code, unused config keys, and speculative abstractions are defects. Remove them.

### 4.2 Explaining decisions — write for the human's role

The gate judges *implementation* (§6); it does that better than a human reading a diff. The human director is the one judge the gate cannot replace — the judge of **outcomes**: what a decision does to gameplay, to the player's experience, and to the design flexibility left open to future decisions. Write every decision that reaches a human for *that* job ([ADR 0018](./decisions/0018-outcome-framed-decision-rationale.md)):

- **Whenever a decision is escalated for a human ruling (§1.4) or recorded for human review** — an ADR (§5, §8), a non-trivial commit message, or a doc that explains a choice — **lead the rationale with its impact on the player, the experience (UI/UX), and future flexibility**, stated so a human can judge it without reading the code. Implementation detail is recorded too, but *after* and *in service of* the outcome framing, never as the headline. "We chose an integer height field" is not a rationale a director can rule on; "crisper stepped land and a drift-free core" is.
- The **structural home** of this norm is the ADR template's required **Player & experience impact** section. A purely internal decision (tooling, process) still fills it — by stating plainly that there is no player-facing effect and giving its flexibility/process outcome instead.
- This is a **judgement norm, not a gate check.** It cannot be mechanically enforced (§6.1), and it is deliberately **not** an invariant (I1–I9), whose defining property is tool-enforceability (§2). The template section is its surrogate; honouring it is on you.

---

## 5. Change classification

Classify every change; the class dictates required ceremony:

| Class | Definition | Requires |
|---|---|---|
| **Trivial** | Typo, comment, doc wording, non-behavioural refactor with no boundary/API change. | Gate green. |
| **Feature / balance** | New behaviour, new/changed parameters, new content, bug fix. | Gate green + tests + docs updated. |
| **Architectural** | New/changed module boundary, new port, new dependency, new namespace root, change to determinism or the enforcement framework, or any change to an invariant. | Gate green + tests + docs + **an ADR** (§8). |
| **Exploration** | A throwaway probe on an `explore/*` branch — feeling out a mechanic; **never merged to `main`**. | `cargo xtask explore` green (fmt + clippy); exempt from I1/I6 until promotion ([ADR 0016](./decisions/0016-exploration-lane-and-subsystem-isolation.md)). |

When in doubt, treat it as the heavier class. **Exploration is never a default** — you are in it only by deliberately working on an `explore/*` branch ([ADR 0016](./decisions/0016-exploration-lane-and-subsystem-isolation.md)).

---

## 6. Enforcement

6.1 **The governing principle:** *Anything not enforced by a tool is only a recommendation.* The purpose of this contract is to make its invariants **mechanically checkable**. Where an invariant is not yet enforced by tooling, adding that enforcement is itself high-priority work.

6.2 **Required tool capabilities.** The specific tools are chosen in [ADR 0009](./decisions/0009-enforcement-tooling-and-the-gate.md). Whatever toolchain is selected **must** provide, at minimum:
- a **static type checker** (zero-error policy);
- a **formatter** (canonical, non-negotiable style);
- a **linter** (style + correctness lints);
- a **dependency/boundary checker** that fails the build on an illegal import direction or a cycle (enforces I2/I4);
- a **test runner with coverage** measurement and thresholds (enforces I5);
- a **schema validator** for configuration (enforces I1) that also rejects keys outside registered namespaces (see [`40-parameterisation.md`](./40-parameterisation.md));
- a **one-command local gate** that runs all of the above and is the single definition of "green" (I9).

6.3 Each capability, once a tool is chosen for it, is recorded in an ADR (§8) and wired into the gate.

---

## 7. Bootstrapping order (enforcement-first)

> This section resolves the **bootstrapping paradox**: the tooling that keeps this contract honest does not yet exist, and there is no human to keep it honest in the meantime. Therefore the tooling comes first — always.

7.1 **Phase 1 is exclusively the enforcement framework.** Before *any* game-simulation mechanic is written, code generation must produce, and prove green, the complete enforcement scaffold:
- the **one-command local gate** script (§9);
- type-checker, formatter, and linter configurations;
- the **dependency/boundary checker** and its rules (encoding I2/I4);
- the **schema-validation harness** for configuration (encoding I1 + namespacing);
- the **test runner + coverage** setup with thresholds (encoding I5);
- the **determinism/replay test harness** (encoding I3).

7.2 **The gate must be green on an empty project.** The scaffold is validated against a placeholder/empty codebase (e.g. a trivial module and a trivial passing test) so that the gate demonstrably runs end-to-end and passes *before* any domain code exists.

7.3 **Hard gate on domain code.** **No** domain, core-simulation, or gameplay code may be authored until §7.1 is complete and §7.2 is green. An agent asked to "just start on the game" must first verify the enforcement framework exists and passes; if it does not, building it *is* the task.

7.4 **Subsequent phases build only on a passing gate**, in dependency order:
1. Enforcement framework (this section) →
2. Config/parameter layer + schema →
3. Deterministic core (world, economy, powers, turn loop, win conditions) →
4. Ports & adapters (persistence, clock/RNG, input, renderer, logging) →
5. LLM strategic-advisor adapter →
6. Presentation / UX polish.

Each phase leaves the gate green. A red gate halts all forward work until fixed.

> **Foundation-first supersedes this *sequence* for the current build ([ADR 0019](./decisions/0019-foundation-first-terrain-workbench.md)).** The ladder above remains the dependency-ordering *principle* and the fallback. For this build the gameplay sequence is depth-first on the load-bearing element instead: the **terrain core** ([ADR 0017](./decisions/0017-vertex-heightfield-terrain.md)) is built to real depth **alongside a prioritised 3D terrain workbench** (renderer + input, normally phases 4/6) so the land can be *seen and felt* in motion and judged right; everything above terrain — economy/faith, followers, powers, the rival, win/loss — is **parked** until then. Only the *order* changes: §7.1–7.3 are unchanged (enforcement first — Phase 1, done), and I2/I3/I4 hold — the renderer still depends inward on core/ports and the terrain sim stays deterministic (fixed timestep, discrete vertex commands).

---

## 8. Amendment process

8.1 This contract, the invariants, the architecture, and any chosen tool are changed **only through an Architecture Decision Record** (see [`decisions/`](./decisions/)). No silent edits.

8.2 An amendment ADR states the current rule, the proposed change, the rationale, the consequences, and what tooling/gate changes are required to enforce the new state. On acceptance, the relevant doc(s) are updated in the same change (I6).

8.3 Invariants I1–I9 may be amended, but the bar is high: the ADR must show the invariant is genuinely wrong or obsolete, not merely inconvenient for the task at hand.

---

## 9. The gate (reference)

The gate is the executable form of §3 and §6.2. Its command is **`cargo gate`** (an `xtask` binary; fixed by [ADR 0009](./decisions/0009-enforcement-tooling-and-the-gate.md)). Every task ends by running the gate and making it green. A separate fast lane — **`cargo xtask explore`** (format + clippy only) — exists for `explore/*` probes ([ADR 0016](./decisions/0016-exploration-lane-and-subsystem-isolation.md)); it is deliberately *not* the gate and never closes a task. If a check cannot yet be automated, that gap is tracked as a defect against §6.1.
