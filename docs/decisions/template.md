# NNNN — <short decision title>

- **Status:** Proposed | Accepted | Superseded by NNNN
- **Date:** YYYY-MM-DD
- **Deciders:** <agent/session or human director>
- **Related:** <links to other ADRs or docs>

## Context

What problem or force prompted this decision? What constraints (contract invariants, [`../60-constraints.md`](../60-constraints.md)) apply? Keep it factual.

## Decision

The choice, stated plainly and unambiguously. "We will …".

## Player & experience impact

*Required (contract §4.2).* What this decision changes for the **player**, the **experience (UI/UX)**, and **future design flexibility** — in outcome terms a human director can judge without reading code. What can a player now do, feel, or see that they couldn't? What future design options does it keep open or foreclose? For a purely internal decision (tooling, process) with no reachable player/UX effect, **say so explicitly** and give its flexibility-or-process outcome instead — never leave this blank. Lead with the outcome; implementation detail belongs in **Consequences**, not here.

## Consequences

- **Positive:** what this makes easier/safer.
- **Negative / trade-offs:** what this makes harder, what we give up.
- **Enforcement / gate impact:** what tooling, tests, or gate checks this decision requires or changes (per contract §6).
- **Docs to update:** which docs change as part of accepting this ADR (I6).

## Alternatives considered

Briefly, the main options not chosen and why.
