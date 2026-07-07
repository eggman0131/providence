# 0017 — Vertex height field terrain with a bounded-step invariant

- **Status:** Accepted
- **Date:** 2026-07-06
- **Deciders:** Director + agent
- **Related:** [`../10-game-design.md`](../10-game-design.md) (§2 world, §3 land shaping, §6 powers, §10 fixed/tunable), [`../20-architecture.md`](../20-architecture.md) (§2.2 core), [`../40-parameterisation.md`](../40-parameterisation.md) (§3 `sim.terrain.*`), [`../70-glossary.md`](../70-glossary.md); [`0004`](./0004-deterministic-core-ports-and-adapters.md) (deterministic core, I3), [`0007`](./0007-wgpu-rendering-framework.md) (deferred the terrain **mesh** representation to "their own ADRs"), [`0008`](./0008-toml-config-format-types-first-schema.md) (config keys), [`0016`](./0016-exploration-lane-and-subsystem-isolation.md) (subsystem isolation), [`0021`](./0021-seeded-parameterised-worldgen.md) (seeded worldgen fills this model and realises the immovable seam below); contract I1, I3.

## Context

Terrain shaping is the game's primary verb ([`../00-vision.md`](../00-vision.md): "the land is the game"), but the world model was described only loosely — "a grid of **terrain cells**, each with a height" ([`../10-game-design.md`](../10-game-design.md) §2, [`../20-architecture.md`](../20-architecture.md) §2.2, [`../70-glossary.md`](../70-glossary.md)). That framing conflates two distinct things and leaves the core mechanic underspecified ahead of its Phase-3 implementation. Two questions were open:

1. **Where does height live — on the square or on the grid points?** *Populous II*, the stated inspiration, samples height at grid **corners**; the visible land is the surface spanning them, and you raise/lower a *corner*. "A cell with a height" implies a flat-topped tile, which is not the model.
2. **What keeps terrain coherent under shaping?** With no rule, a raise could open an arbitrary vertical cliff, and real-valued heights would put floating-point non-determinism inside the core (I3).

[ADR 0007](./0007-wgpu-rendering-framework.md) already deferred "the terrain **mesh** representation … to their own ADRs." This ADR fixes the **simulation-side** model the renderer mesh derives from.

## Decision

We will model terrain as an **integer height field sampled at grid vertices**, governed by a **bounded-step invariant**, mutated only by operations that preserve it.

1. **Vertices carry height; faces are derived.** Height is an **integer** stored at each grid **vertex** (corner) — the unit of terrain state. A **face** — the square bounded by four adjacent vertices — is the derived surface followers build on. **Buildable** is a property of a face: its four corners are equal height (flat) and above sea level (dry), and it is contiguous with other buildable faces. This replaces "terrain cell = grid square with a height".

2. **Bounded-step invariant.** Two **orthogonally** adjacent vertices differ in height by at most `sim.terrain.max_step` (default **1**). Only orthogonal pairs are bounded, so diagonally adjacent vertices may differ by up to `2 × max_step` — the intended stepped look, matching the inspiration. Because heights move in whole steps, the field is **integer-valued**: a determinism benefit (I3) — no floating-point terrain state, and the cascade below is exact.

3. **Shaping cascades to preserve the invariant.** Raising (or lowering) a vertex drags its neighbours up (or down) as far as needed to restore the invariant, forming stepped plateaus. The op's faith cost scales with the number of vertices **actually moved**. The cascade is **naturally bounded** by `sim.terrain.max_height`: a cone cannot rise past the world maximum, so its affected radius is bounded by `max_height / max_step`. No separate radius limit is needed for termination.

4. **Every terrain-mutating operation preserves the invariant.** Not only the player's manual raise/lower, but terrain-scale powers — flood, earthquake, raise-mountain ([`../10-game-design.md`](../10-game-design.md) §6) — leave the field satisfying (2). The invariant is a property of the terrain subsystem's state, maintained by everything that writes it.

5. **Immovable features constrain the cascade (reserved seam).** Some world contents — rock, trees, and cross-subsystem entities such as **opponent buildings** — may be flagged as *not destroyable/movable* by a given terrain action. Where a cascade would disturb such a feature it **halts** there (or the op is refused). This is the terrain-side of subsystem isolation ([ADR 0016](./0016-exploration-lane-and-subsystem-isolation.md)): a player's shaping must not silently destroy what another subsystem owns unless the rules allow it. *What* is immovable to *which* action is **content** (`content.terrain.*` / the feature's own record). This ADR reserves the seam; [ADR 0021](./0021-seeded-parameterised-worldgen.md) (issue #7 Phase 3) implements it for terrain-owned immovables (rock, trees), with cross-subsystem immovables (opponent buildings) still parked.

**Sequencing.** The concrete keys named here — `sim.terrain.max_step` (load-time/structural, default 1), `sim.terrain.max_height`, shaping costs, immovability flags — are **design intent**. They are created **with the Phase-3 terrain core** that reads them (and enter the schema then, per [ADR 0008](./0008-toml-config-format-types-first-schema.md)); they are not added to config now, with nothing to consume them. This ADR records the model; it authors no code.

## Consequences

- **Positive:**
  - The primary verb is now precisely specified before it is built.
  - The integer height field makes I3 cleaner — exact state, no float drift — and makes the cascade deterministic by construction.
  - Cascade gives the characteristic stepped feel, and the cost model ("vertices moved") falls out of it rather than being invented.
  - The immovable-feature seam stops terrain from becoming a cross-subsystem wrecking ball, reinforcing [ADR 0016](./0016-exploration-lane-and-subsystem-isolation.md).
  - The renderer mesh ([ADR 0007](./0007-wgpu-rendering-framework.md)) now has a well-defined source of truth to derive from, holding no simulation state (I3).
- **Negative / trade-offs:**
  - A raise is a **multi-vertex** operation, not a single write: legality, cost accounting, and the immovable-halt logic must be implemented and determinism-tested in Phase 3.
  - `max_step` is exposed as config, but the model, mesh, and cascade are written assuming **1**; a value ≠ 1 is not a supported/tested configuration until proven otherwise (documented, revisited via ADR if genuinely wanted).
- **Enforcement / gate impact:** **none now** — this is a decision record ahead of Phase 3; no code, schema, or gate change. When the terrain core lands: its keys join the generated schema (ADR 0008), and the replay/determinism golden (I3) must cover shaping **and** cascade, including immovable-feature halts.
- **Docs to update (this change, I6):** [`../70-glossary.md`](../70-glossary.md) (redefine *terrain cell* → *vertex*; add *face* and *step invariant*), [`../20-architecture.md`](../20-architecture.md) §2.2 (vertex height field), [`../10-game-design.md`](../10-game-design.md) §2/§3/§6/§10 (the model, cascade, powers preserve it, fixed-vs-tunable), [`../40-parameterisation.md`](../40-parameterisation.md) §3 (`sim.terrain.*` names `max_step`/`max_height`), `decisions/README.md` (index).

## Alternatives considered

- **Tile height field (height per square).** Flat-topped tiles with a per-tile height — simple storage but blocky, non-interpolated terrain that neither matches the inspiration nor the raise/lower-a-corner mechanic. Rejected.
- **Continuous float heights + max gradient.** Real-valued heights bounded by a slope limit — more "realistic", but invites floating-point non-determinism inside the core (I3) and a fuzzier, less legible shaping model. Rejected in favour of the exact integer field.
- **Reject illegal ops instead of cascading.** Refuse any raise that would break the invariant, forcing manual ring-by-ring building — simplest rules, but least tactile, unlike the inspiration, and it loses the cost-scales-with-work model. Rejected.
- **`max_step` as a hard constant (no key).** Bake 1 into the model. Rejected in favour of an explicit `sim.terrain.max_step` key (I1, legibility), accepting that ≠ 1 is presently untested.
