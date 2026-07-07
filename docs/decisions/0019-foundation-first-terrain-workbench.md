# 0019 — Foundation-first delivery: terrain module and a prioritised 3D terrain workbench

- **Status:** Accepted
- **Date:** 2026-07-06
- **Deciders:** Director + agent
- **Related:** [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (§7 bootstrapping order — this ADR supersedes the §7.4 phase *sequencing*; I2/I3/I4 unchanged), [`0016`](./0016-exploration-lane-and-subsystem-isolation.md) (the legacy post-mortem this builds on), [`0017`](./0017-vertex-heightfield-terrain.md) (the terrain model being built and evaluated), [`0004`](./0004-deterministic-core-ports-and-adapters.md) (deterministic core), [`0002`](./0002-llm-as-strategic-advisor.md) (LLM record-replay depends on determinism), [`0007`](./0007-wgpu-rendering-framework.md) (the renderer the workbench uses), [`0009`](./0009-enforcement-tooling-and-the-gate.md) (the gate — unchanged)

## Context

The contract's bootstrapping order ([§7.4](../30-ai-agent-contract.md)) sequences the *gameplay* build as a horizontal ladder: config → **whole deterministic core** (all six subsystems) → ports & adapters → LLM → presentation. Presentation — the renderer and input that let a human *see and touch* the world — comes last.

Two facts make that ordering the wrong bet for this build:

1. **The legacy build failed by going thin-everywhere, not deep-anywhere.** The prior incarnation built a shallow slice of every subsystem before any one of them was solid. The Director's account (this session): it "totally killed any kind of agility in investigating options, or getting the underpinnings right before embarking on the logic that made it real … so far from right it was hard to evaluate or improve." [ADR 0016](./0016-exploration-lane-and-subsystem-isolation.md) recorded the coupling symptom (the mana→opponent→replay cascade); this ADR records the deeper cause — **breadth before depth left the foundation unsound and unevaluable.**

2. **Terrain is the foundation, and terrain cannot be evaluated unseen.** The whole game pivots on shaping land and reading back what the manipulation did — a cascade rippling outward, a plateau forming, the stepped surface catching light. That is a *perceptual* judgement; a headless dump or a flat top-down grid cannot make it. The land must be seen **moving, in three dimensions, under a camera you can move**, or "is the terrain model right?" is unanswerable — which is exactly the state legacy got stuck in.

The lesson is depth-first on the load-bearing element, with the means to judge it built alongside it. Nothing here weakens the gate, the invariants, or the enforcement-first *principle* (tooling before gameplay — Phase 1 is complete and green). What changes is the **sequence** in which gameplay is built.

## Decision

We will build **foundation-first**, not breadth-first, for the current stage:

1. **Terrain is the first and only deep subsystem for now.** The deterministic terrain model of [ADR 0017](./0017-vertex-heightfield-terrain.md) — the integer vertex height field, the bounded-step cascade, `max_step`/`max_height`, immovable features, water/shore/mountain derivation, buildable-face derivation, and seeded worldgen — is built to real depth and tuned until the Director judges it right.

2. **A 3D terrain workbench is a first-class, prioritised deliverable — not deferred presentation.** A movable-camera (orbit/pan/zoom) `wgpu` view ([ADR 0007](./0007-wgpu-rendering-framework.md)) renders the height field as a lit 3D surface and **animates the manipulation** so a raise/lower and its cascade are visible as they happen; picking a vertex and raising/lowering it is the interaction. The workbench is the **instrument the Director evaluates terrain with**, so the renderer and input needed for it are built **alongside** the terrain core, not in a later phase. Its purpose is judgement and iteration, not shipping polish.

3. **What sits above terrain is deliberately deferred.** Economy/faith, followers/settlements, powers, the rival deity, and win/loss are **parked** until terrain is judged right. Their *shape* will be decided informed by how the world actually plays — not committed to before the foundation is sound.

4. **One load-bearing constraint is retained (nearly free here): the terrain sim stays deterministic.** The height field is integer and pure; the camera and mouse do their float/ray work at the edge and resolve to **discrete vertex commands**; the sim advances on a **fixed timestep** with commands recorded. Real-time 3D presentation therefore does **not** leak wall-clock, float coordinates, or frame-rate coupling into the field. This preserves the record-replay reproducibility the LLM-opponent design rests on ([ADR 0002](./0002-llm-as-strategic-advisor.md), [ADR 0004](./0004-deterministic-core-ports-and-adapters.md), I3) so that whatever is later built above terrain still works.

This **supersedes the §7.4 phase *sequencing*** (build order of phases 2–6) for this build. It does **not** touch §7.1–7.3 (the enforcement framework comes first — done), the dependency direction (I2/I4 — the renderer still depends inward on core/ports; only its *timeline* moves), determinism (I3 — retained, see item 4), or the gate ([ADR 0009](./0009-enforcement-tooling-and-the-gate.md) — unchanged, still the sole definition of green for `main`).

## Player & experience impact

- **Player / experience:** we build the thing the player actually *feels* — land you can see rise, fall, and cascade in a real 3D view you can move around — **first**, and get it right before anything else. The single most defining sensation of the game (shaping the world and watching it respond) is the first thing that exists and the first thing the Director can judge, instead of the last.
- **Future flexibility:** deciding what sits *above* terrain (economy, followers, powers, the rival, win/loss) only **after** terrain feels right keeps all of those options open and lets the *felt* foundation inform them. We avoid committing to the shape of five subsystems before we know how the world plays — the mistake that left the legacy build unevaluable.
- **What it forecloses:** it delays the moment a *whole game loop* exists end-to-end; for a while there is a deeply playable *terrain*, not a winnable *game*. That is the intended trade — a sound, evaluable foundation over an early-but-shallow whole.

## Consequences

- **Positive:**
  - The load-bearing element gets real depth and is judged *in motion*, early — the exact capability legacy lacked.
  - Design decisions about everything above terrain are made with evidence (how the world plays), not speculation, and can't over-fit to a foundation that later shifts.
  - The renderer and input paths are exercised early against the simplest possible sim, surfacing integration reality (framerate, picking, camera feel on target hardware) while it is cheap to change.
  - Determinism — hence the LLM-opponent bet — survives the early renderer, by construction (item 4).
- **Negative / trade-offs:**
  - No whole-game loop for a while; a playable terrain is not a winnable game.
  - The renderer and an input path are built earlier than the classic layering would, before some core subsystems exist — accepted deliberately, with the determinism seam (item 4) as the guard that keeps this from coupling the way legacy did.
  - Risk that the workbench grows beyond "enough to judge terrain" into premature polish — mitigated by scoping it explicitly to evaluation, not shipping.
- **Enforcement / gate impact:** none to the gate itself ([ADR 0009](./0009-enforcement-tooling-and-the-gate.md) unchanged). The terrain core carries the determinism/replay coverage (I3) as normal; the workbench (renderer + input adapters) is governed like any adapter (I2/I4 boundary checks apply). No invariant values change; only the §7.4 build *sequence* is superseded.
- **Docs to update:** this ADR + the [`decisions/README.md`](./README.md) index (now). Follow-up (tracked as a governance issue, per the [ADR 0016](./0016-exploration-lane-and-subsystem-isolation.md) precedent of deferring contract-text edits): note the foundation-first sequencing in [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) §7 and in `CLAUDE.md`'s bootstrapping section, pointing here.

## Alternatives considered

- **Keep §7.4 as-is (whole headless core first, presentation last).** The correct layering for a determinism-first engine in the abstract, but it defers the game's single most important unknown — does shaping *feel* right — to the very end, and makes terrain evaluable only through proxies. That is the failure mode we are escaping. Rejected for this build.
- **Thin vertical/​horizontal slice of everything first (walking skeleton).** This is what legacy did; it left nothing solid enough to evaluate or improve and killed iteration agility. Rejected — it is the specific mistake this ADR exists to not repeat.
- **A throwaway `explore/*` terrain probe for feel, terrain core stays headless-governed ([ADR 0016](./0016-exploration-lane-and-subsystem-isolation.md) lane).** Cheaper, and still available for scrappy experiments — but it makes the *evaluable* view second-class and thrown away, when the view is core to the game and worth building for real. Rejected as the primary path; the explore lane remains available for probes.
- **Drop determinism to make the real-time slice simpler.** Would remove the one retained constraint, but it dismantles the LLM-opponent record-replay design ([ADR 0002](./0002-llm-as-strategic-advisor.md)/[0004](./0004-deterministic-core-ports-and-adapters.md), I3) — a far larger decision, and unnecessary since the fixed-timestep + discrete-command seam (item 4) costs almost nothing for terrain. Rejected.
