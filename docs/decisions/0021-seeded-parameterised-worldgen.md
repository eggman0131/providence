# 0021 — Seeded, parameterised worldgen over a shape × relief space

- **Status:** Accepted
- **Date:** 2026-07-07
- **Deciders:** Director + agent
- **Related:** [`../10-game-design.md`](../10-game-design.md) (§2 world, §3 land shaping), [`../20-architecture.md`](../20-architecture.md) (§2.2 core), [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (§5 change classification, I1/I3), [`../40-parameterisation.md`](../40-parameterisation.md) (§3 `sim.worldgen.*`/`content.terrain.*`, §4 structural/load-time), [`0017`](./0017-vertex-heightfield-terrain.md) (the height field + step invariant + the immovable seam this fills), [`0019`](./0019-foundation-first-terrain-workbench.md) (foundation-first — this is the real land the workbench shows), [`0016`](./0016-exploration-lane-and-subsystem-isolation.md) (subsystem isolation; worldgen as always-on foundation), [`0008`](./0008-toml-config-format-types-first-schema.md) (keys land with the code that reads them), [`0004`](./0004-deterministic-core-ports-and-adapters.md) (deterministic core, I3), [`0002`](./0002-llm-as-strategic-advisor.md) (record-replay rests on I3), [`0018`](./0018-outcome-framed-decision-rationale.md); implements issue [#7](https://github.com/eggman0131/providence/issues/7), and is the terrain [#11](https://github.com/eggman0131/providence/issues/11) will judge.

## Context

[ADR 0017](./0017-vertex-heightfield-terrain.md) fixed *what terrain is* — an integer height field sampled at vertices, governed by the bounded-step invariant — and *reserved* the immovable-feature seam (§5). It deliberately did **not** decide *how a seed becomes a world*. Issue [#7](https://github.com/eggman0131/providence/issues/7) is the first code that needs that answer: the workbench ([ADR 0020](./0020-workbench-runtime-and-rendererport.md)) today draws a hand-built demo bump ([`crates/providence/src/main.rs`](../../crates/providence/src/main.rs) `build_workbench_field`), not the game's terrain, so [#11](https://github.com/eggman0131/providence/issues/11) ("does the stepped land feel right?") has nothing real to judge. Worldgen is the piece that turns the height field into a *place*.

Two forces make the generation model a decision worth recording, not a mere implementation detail:

1. **It defines what every fresh world *is*.** Whether a seed yields an island, a coastline, an archipelago, or a lake-dotted continent — and whether the land is gentle or dramatic — is the terrain the Director evaluates and the substrate every parked subsystem (settlements, powers, the rival) later reads. The Director's design steer was explicit: these are **not** one baked-in choice but **knobs chosen per world** — a design space to keep open, not a single map to hardcode.

2. **A parameterised generator must still guarantee the invariant, deterministically.** Arbitrary noise quantised to integers will violate the step invariant wherever the slope is steep; and any float or ambient randomness in the core breaks reproducibility ([I3](../30-ai-agent-contract.md)). The generator has to span a wide feel-space *and* always emit a field that already satisfies the invariant, bit-for-bit from its seed.

This ADR fixes the **worldgen model** and its **parameter surface**. It does not re-decide the terrain representation ([ADR 0017](./0017-vertex-heightfield-terrain.md)) or add new namespace roots — `sim.worldgen.*` and `content.terrain.*` are already registered ([`40-parameterisation.md`](../40-parameterisation.md) §3). It composes them.

## Decision

We will generate the world as a **pure, seeded function in `providence-core` that produces an integer `HeightField` already satisfying the step invariant, parameterised over a shape × relief space by `sim.worldgen.*` — never a single baked-in world.**

1. **Worldgen is a pure function of a `u64` seed.** Generation runs in `providence-core` and draws only from the existing in-core [`SplitMix64`](../../crates/core/src/rng.rs) PRNG seeded from `sim.worldgen.seed`. Same seed + same `sim.worldgen.*` ⇒ the same field, forever — no clock, no I/O, no ambient randomness ([I3](../30-ai-agent-contract.md)). It uses **no `RngPort`**: that port models the *recordable runtime* randomness a live sim replays; worldgen is a one-shot pure derivation and needs no external, recorded stream.

2. **Shape and relief are parameters, not constants.** `sim.worldgen.*` carries a **shape** control (how land is arranged — island / continent-with-coast / archipelago / mostly-land-with-lakes), a **relief** control (how gentle-to-dramatic the vertical land is), plus dimensions, sea level / land ratio, and seed. One generator spans the whole space; the **seed** varies the specific instance *within* the flavour those knobs set. A given [`config/default.toml`](../../config/default.toml) names the out-of-box world; changing the knobs yields a different *kind* of world from the same code. This is the Director's steer made concrete, and it is [I1](../30-ai-agent-contract.md) by construction — the world's character is data, not a literal in a source file.

3. **Generation approach: noise → shape mask → integer band → conform.** Seeded multi-octave value noise is shaped by a parameterised **mask** (the shape control — e.g. a radial/edge falloff for island-ness), mapped into an integer height band set by sea level and relief, then passed through a deterministic **conform pass** that enforces the step invariant, reusing the same "restore the invariant" discipline issue #6 already built in [`crates/core/src/terrain/shape.rs`](../../crates/core/src/terrain/shape.rs). The field worldgen hands back **already satisfies the invariant** — nothing downstream has to repair it.

4. **Terrain-type and buildable derivations realise [ADR 0017](./0017-vertex-heightfield-terrain.md) §1 — no new model.** Water / land / shore / mountain and *buildable* faces are **pure functions** of height versus sea level plus a `content.terrain.*` type catalogue (the thresholds that name "shore" and "mountain"). These are derivations, not decisions this ADR reopens.

5. **Immovable features realise the [ADR 0017](./0017-vertex-heightfield-terrain.md) §5 reserved seam.** Worldgen places terrain-owned immovables (rock, trees) described in `content.terrain.*`; a raise/lower cascade that would disturb one **halts** there, or the op is refused — no silent destruction ([ADR 0016](./0016-exploration-lane-and-subsystem-isolation.md)). *Cross-subsystem* immovables (opponent buildings) and **initial settlement placement** stay **parked** — they live above terrain; #7 delivers the terrain substrate, its derivations, and terrain-owned immovables only.

6. **`sim.worldgen.*` is an always-on foundation subsystem — no `enabled` seam.** Like `sim.terrain.*`, worldgen produces the substrate the whole game stands on; it is not a toggleable gameplay module, so it carries no on/off seam (the documented foundation exemption, [ADR 0016](./0016-exploration-lane-and-subsystem-isolation.md)). Issue [#1](https://github.com/eggman0131/providence/issues/1)'s enabled-seam gate check must exempt `sim.worldgen` exactly as it exempts `sim.terrain`.

7. **No new namespace roots; keys land with the code.** `sim.worldgen.*` and `content.terrain.*` are already-registered roots ([`40-parameterisation.md`](../40-parameterisation.md) §3). Their concrete keys — dimensions, `seed`, sea level / `land_ratio`, `shape`, `relief`, and the `content.terrain.*` type/immovable catalogue — are **structural / load-time** (§4) and enter the generated schema **with the phase that reads them** ([ADR 0008](./0008-toml-config-format-types-first-schema.md)), not all at once.

This is an **architectural** change ([contract §5](../30-ai-agent-contract.md)): it introduces the worldgen subsystem model and its determinism contract. It adds **no** dependency and **no** boundary edge — worldgen is pure core.

**Two sub-choices, ruled on by the Director (2026-07-07):**

- **(a) Shape is a named enum, not a scalar.** `sim.worldgen.shape` is an **enum** of named modes (`island` / `continent` / `archipelago` / `inland`), each carrying its own mask — chosen over a single `island_bias`-style blend scalar for legibility (the modes map directly to distinct arrangements a designer can name, rather than an opaque "what does 0.6 look like?" dial).
- **(b) The default world is island + mixed relief.** [`config/default.toml`](../../config/default.toml) ships `shape = "island"` with mixed relief (rolling flats plus distinct high ground) — the classic inspiration, and the flavour that shows the stepped land off most clearly for [#11](https://github.com/eggman0131/providence/issues/11). Trivially overridden in `local.toml`.

## Player & experience impact

- **Player / experience:** this is the decision that makes the world *somewhere* — a coastline, high ground, and flat buildable land, **generated fresh from a seed and reproducible**. It gives the workbench a real map to look at and (soon) sculpt, and it is the terrain [#11](https://github.com/eggman0131/providence/issues/11) finally lets the Director judge by *looking*. The immovable seam means a shaping op can never silently erase rock or trees the world placed.
- **Future flexibility:** because shape and relief are **parameters**, the world space stays wide open — island to continent, gentle to dramatic — without committing to one map, and a future in-game **world-setup screen** is just UX that surfaces these same `sim.worldgen.*` knobs (parked with the game shell; parameterising now is what keeps that door open). The derivations (shore/mountain/buildable) and the immovable records are the hooks the parked subsystems (settlements, powers) will read.
- **What it forecloses:** nothing above terrain — settlements, powers, and cross-subsystem immovables remain parked. It commits the core to *owning* worldgen (a pure, integer, seeded generator), which is exactly where determinism (I3) wants it.

## Consequences

- **Positive:**
  - The workbench shows the **real** terrain, unblocking [#11](https://github.com/eggman0131/providence/issues/11) — the whole point of foundation-first here ([ADR 0019](./0019-foundation-first-terrain-workbench.md)).
  - Pure-seeded generation keeps [I3](../30-ai-agent-contract.md) clean: bit-for-bit reproducibility, covered by a seed/determinism test, no floats or ambient randomness in the core.
  - Conform-by-construction means **every** emitted field is invariant-valid; no downstream code has to defend against a malformed world.
  - One parameterised generator spans all world flavours — no per-flavour code, and the world's character is config ([I1](../30-ai-agent-contract.md)).
  - The [ADR 0017](./0017-vertex-heightfield-terrain.md) §5 immovable seam becomes real, keeping terrain from silently wrecking what other subsystems own.
- **Negative / trade-offs:**
  - The generator must be general enough to span the space **and** always conform — the conform pass is real complexity and must be determinism-tested; noise + mask + conform is more moving parts than a fixed island generator.
  - A degenerate point in the parameter space (e.g. extreme relief on a tiny map) could yield an ugly or near-empty world; bounded by schema ranges and cross-key invariants ([`40-parameterisation.md`](../40-parameterisation.md) §4), not by the generator silently clamping.
  - Fixing the parameter surface now, before settlements/powers read the derivations, risks a later addition; mitigated by scoping #7 to terrain-only outputs and treating settlement placement as parked.
- **Enforcement / gate impact:**
  - New `sim.worldgen.*` + `content.terrain.*` keys join the **generated schema** ([ADR 0008](./0008-toml-config-format-types-first-schema.md)) with the phase that reads them; `content.terrain.*` is the first `content.*` table to land (keyed-collection style, [`40-parameterisation.md`](../40-parameterisation.md) §3).
  - Worldgen is **core**, so — unlike the GPU renderer — it *is* gated: pure, headlessly unit-tested at the highest coverage bar, and the replay/determinism golden ([`crates/core`](../../crates/core) replay suite, I3) **extends** to cover "same seed ⇒ same field" and the immovable-halt path.
  - The **boundary is unchanged** — worldgen stays in `providence-core`, imports nothing outward; no new `deny.toml` entry, no new crate.
  - Issue [#1](https://github.com/eggman0131/providence/issues/1)'s enabled-seam check must **exempt `sim.worldgen`** as a foundation subsystem (as it does `sim.terrain`).
- **Docs to update (on acceptance / with implementation, I6):** this ADR + [`decisions/README.md`](./README.md) index; [`0017`](./0017-vertex-heightfield-terrain.md) (its reserved immovable seam and "worldgen arrives with #7" now realised → point here); [`../10-game-design.md`](../10-game-design.md) §2 (worldgen model + params) and §3 (immovables); [`../40-parameterisation.md`](../40-parameterisation.md) §3 (concrete `sim.worldgen.*` + `content.terrain.*`); [`../70-glossary.md`](../70-glossary.md) (terrain *type*/shore/mountain, *worldgen*, *immovable feature*); [`CLAUDE.md`](../../CLAUDE.md) (core now generates worlds). Concrete keys, schema, and `default.toml` entries land **with the code that reads them**, phase by phase ([ADR 0008](./0008-toml-config-format-types-first-schema.md)).

## Alternatives considered

- **Bake one world flavour (island, mixed relief) as constants.** Simplest generator — but it is exactly the magic-number-class commitment ([I1](../30-ai-agent-contract.md)) the Director rejected: it forecloses the world space and hides the world's character in code. Rejected in favour of `sim.worldgen.*` parameters.
- **Gradient-limited noise (never exceed max slope, so the invariant holds without a conform pass).** Guarantees the invariant cheaply — but caps relief so hard the "dramatic" end of the space is unreachable at real map sizes. Rejected; the conform pass buys the full relief range.
- **Generate continuous float heights, keep them, quantise only for display.** Common in engines — but floats in the core violate [I3](../30-ai-agent-contract.md), and the integer field must be the single source of truth ([ADR 0017](./0017-vertex-heightfield-terrain.md)). Rejected.
- **Route worldgen through the `RngPort`.** Consistent-looking with "randomness behind a port" — but that port exists for *recordable, replayable* runtime randomness; worldgen is a pure one-shot function of a `u64` seed, already deterministic via in-core `SplitMix64`. A port would add a side-effect seam where there is no side effect. Rejected.
- **Include initial settlement placement now** (per [`40-parameterisation.md`](../40-parameterisation.md) §3's `sim.worldgen.*` list). Rejected/deferred — settlements are above terrain and parked; #7 delivers the terrain substrate, its derivations, and terrain-owned immovables only.
- **Defer worldgen; keep the hand-built demo field.** Rejected — [#11](https://github.com/eggman0131/providence/issues/11) cannot judge the real terrain on a synthetic bump; generating the world *is* the foundation-first move here ([ADR 0019](./0019-foundation-first-terrain-workbench.md)).
