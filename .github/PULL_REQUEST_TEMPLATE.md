<!-- This template is contract §3 (Definition of Done) made executable. A change
     is DONE only when every box below holds, and "done" ⇒ the gate is green.
     The gate (ADR 0009) is the single, non-negotiable definition of "green" —
     never merge on red (§9). There is no human code reviewer here; you are the
     last check (contract §1.3). -->

## Summary
<!-- What changed and why, in one short paragraph. -->

## Player & experience impact
<!-- Lead with the OUTCOME (ADR 0018, contract §4.2): what this does to the
     player, the experience (UI/UX), and future design flexibility — stated so
     the Director can judge it without reading the diff. A purely internal /
     tooling change with no player-facing effect must say so and give its
     flexibility-or-process outcome instead. -->

## Governance
- **Closes:** <!-- Fixes #N -->
- **Decisions:** <!-- Refs ADR-00NN, or "none — not architectural" -->
- **Change class** (contract §5): <!-- Trivial | Feature/balance | Architectural -->

## Definition of Done (contract §3)
<!-- Self-verify EVERY box. This is the checklist you must complete before merge. -->
- [ ] **Types** — static type check passes with zero errors
- [ ] **Format & lint** — formatter and linter pass with zero warnings
- [ ] **Boundaries** — the dependency/boundary checker reports no violations (I2, I4)
- [ ] **Tests** — the full suite passes; new/changed behaviour has new/updated tests
- [ ] **Coverage** — meets the configured threshold; the deterministic core meets its higher threshold
- [ ] **No magic numbers** — no new behavioural literal in code; new tunables added to config + schema (I1)
- [ ] **Config validates** — all config validates against the schema; every referenced key exists
- [ ] **Determinism** — a replay/seed test confirms identical output for identical seed+inputs (I3)
- [ ] **Docs updated** — affected docs updated in the same change (I6)
- [ ] **ADR** — added if the change is architectural (§5); otherwise N/A
- [ ] **Verified** — the change was actually run and the intended effect observed, not just asserted by tests

## Gate
<!-- Paste the tail of a green `cargo gate` run. A red gate is not mergeable (§9). -->
```
$ cargo gate

```
