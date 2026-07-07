---
name: Feature
about: A new capability or mechanic, designed outcome-first (ADR 0018) and mapped to the contract
title: ""
labels: ["type:enhancement"]
assignees: []
---

<!-- Add area:* and phase:N labels. Lead with OUTCOMES, not implementation
     (ADR 0018 / contract §4.2): the Director judges what a change does to the
     game, not how it is coded. If any part below turns architectural — a module
     boundary, port, dependency, namespace root, determinism, or the gate — add
     the adr-needed label and open an ADR; do not settle it in this thread
     (contract §5, §8). -->

## Player & Experience Impact
<!-- REQUIRED (ADR 0018, contract §4.2). Lead with the outcome, stated so the
     Director can rule on it WITHOUT reading code:
       - Player: what can a player now do, feel, or decide that they couldn't?
       - Experience (UI/UX): what changes in what they see or how it plays?
       - Future flexibility: what design options does this open or foreclose?
     A purely internal feature (tooling / process) must SAY SO explicitly and
     give its flexibility-or-process outcome instead — never leave this blank. -->

## Architecture & Contract Notes
<!-- Map the work onto the crate graph (docs/20-architecture.md). Dependencies
     point INWARD toward providence-core; the core imports nothing outward and
     stays pure — no clock, I/O, or ambient randomness (I2/I3/I4). Every side
     effect goes behind a port (I4). Behaviour/balance/content is config with a
     schema entry, never a magic number in code (I1). -->
- **Crates / modules touched:**
- **Ports involved** (each side effect behind an interface, I4):
- **Config keys + schema entries added** (I1 — no magic numbers):
- **Determinism** (I3): <!-- does this touch providence-core? if so, the replay/seed test must be extended -->

## Open Decisions
<!-- ADR candidates: anything architectural that is NOT yet decided. Each one
     blocks the phase that depends on it. Label the issue adr-needed and link
     the ADR once opened. If there are none, write: "none — no architectural
     decision required." -->
- [ ] <decision to make> — Refs ADR-00NN (or `adr-needed`)

## Phases
<!-- Break the work into chunks that each fit comfortably in one model's context
     window (contract §4.1) and each leave the gate green (§7.4). One reviewable,
     independently-mergeable step per phase, in dependency order. -->
- [ ] Phase 1 —
- [ ] Phase 2 —

## Definition of Done
<!-- The gate (ADR 0009) is the sole definition of build-green; these mirror
     contract §3. Every box must hold before this issue closes. -->
- [ ] **Gate green** (`cargo gate`)
- [ ] **No magic numbers** — new tunables live in config + schema, not code (I1)
- [ ] **Boundaries clean** — dependencies point inward, no illegal imports or cycles (I2/I4)
- [ ] **Determinism intact** — replay/seed test passes, extended if the core changed (`cargo test -p providence-core --test replay`, I3)
- [ ] **Docs updated** in the same change; ADR added if architectural (I6, §5)
- [ ] **Verified** — exercised end-to-end and the intended effect observed, not just unit-tested (§3)
