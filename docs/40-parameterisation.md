# 40 — Parameterisation

> **Status:** Active · **Governed by:** [`30-ai-agent-contract.md`](./30-ai-agent-contract.md) (invariant I1) · **Load this doc for:** any balance/tuning task, any change that introduces a tunable, and all schema work.

The project is **parameterised from day one**: behaviour, balance, and content are data, tuned via configuration, never by editing code. This document defines *what* is parameterised, the *namespace* rules, and *how* configuration is validated. The concrete file format is **TOML with a types-first schema** ([ADR 0008](./decisions/0008-toml-config-format-types-first-schema.md)): Rust config structs are the source of truth (`serde` + `deny_unknown_fields` + `garde`), and the machine-readable JSON Schema is generated from them (`schemars`). This doc states the requirements that format must meet; the ADR records how they are met.

---

## 1. Principle & litmus test

**Principle (I1):** if a value affects how the game behaves, how it is balanced, or what content exists, it lives in configuration.

**Litmus test:** *Could a designer change this without editing code?* If the answer should be "yes," the value **must** be a configuration key. A behavioural literal hard-coded in a source file is a defect ("magic number") and fails the gate.

**Not everything is config.** Structural facts — the existence of a module, the shape of a port interface, the *algorithm* — are code. Config expresses *quantities, rates, thresholds, toggles, tables, and content definitions*, not control flow.

---

## 2. Mandatory key namespacing

> This section prevents agent-generated naming collisions and makes every key self-locating. It is enforced by the schema validator (contract §6.2).

2.1 **Every configuration key MUST be a hierarchical, dot-notation path** under a **registered top-level namespace root**. Examples:

```
sim.economy.mana.regen_rate
sim.worldgen.terrain.max_height
sim.population.follower.growth_per_tick
ai.llm.decision.cadence_ticks
ai.difficulty.strategy_trust
render.camera.edge_scroll_speed
content.powers.flood.mana_cost
meta.schema_version
```

2.2 **Registered namespace roots.** A key outside these roots is rejected by the validator. Adding a new root is an **architectural change** (contract §5 → ADR).

| Root | Owns | Deterministic? |
|---|---|---|
| `meta.*` | Config/schema versioning & provenance. | n/a (data about config) |
| `sim.*` | Deterministic simulation: `worldgen`, `terrain`, `economy`, `population`, `powers` (tuning), `rules`, `winconditions`. | Yes — inside the determinism boundary. |
| `content.*` | Content **definitions**: the catalogue of powers, terrain types, follower types, scenarios/maps. | Yes (consumed by core as data). |
| `ai.*` | Rival-deity opponent: `llm` (runtime, cadence, prompts), `difficulty`, `strategy` (see [`50-llm-opponent.md`](./50-llm-opponent.md)). | No — the LLM is outside the boundary. |
| `render.*` | Presentation: camera, palette, HUD, timings. | No. |
| `input.*` | Input mapping & sensitivities. | No. |
| `runtime.*` | Adapter/runtime settings: logging level, persistence location, audio volume. | No. |

2.3 **Naming convention.**
- Path segments are `lower_snake_case`; the hierarchy is expressed by dots.
- Segments read general → specific (`sim.economy.mana.regen_rate`, not `mana_regen_rate_sim`).
- Prefer a shallow, meaningful hierarchy (typically 3–5 segments). Do not encode data in key names that belongs in a table (e.g. don't create `content.powers.flood.mana_cost` *and* `content.powers.fire.mana_cost` as ad-hoc keys if a keyed table of powers is clearer — see §3 content tables).
- No collisions: two keys may not differ only by case or separator style.

2.4 **The validator enforces 2.1–2.3.** Any key not matching a registered root, or violating the convention, fails validation → fails the gate.

---

## 3. Parameter taxonomy

What lives where (illustrative, not exhaustive; every gameplay number ends up under one of these):

- **`sim.worldgen.*`** — map size, seed handling, terrain generation rates, sea level, initial settlement placement.
- **`sim.terrain.*`** — land-shaping costs/limits, max/min height, slope rules, water spread.
- **`sim.economy.*`** — faith/mana regeneration, storage caps, spend rules, worship yield per follower.
- **`sim.population.*`** — follower growth, housing capacity, migration, allegiance/conversion rates.
- **`sim.rules.*`** — tick length, action ordering, per-turn limits.
- **`sim.winconditions.*`** — thresholds and toggles for victory/defeat.
- **`content.powers.*`** — the **catalogue** of divine powers: id, display, `mana_cost`, magnitude, radius, cooldown, prerequisites. (A keyed table, see below.)
- **`content.terrain.*` / `content.followers.*` / `content.scenarios.*`** — definitions of terrain types, follower types, and playable scenarios/maps.
- **`ai.llm.*`** — runtime + model selection (`runtime = "ollama"`, `model = "gemma4:26b-mlx"` — [ADR 0014](./decisions/0014-ollama-local-llm-runtime.md)), decision `cadence_ticks`, timeouts, temperature/seed, prompt-template ids.
- **`ai.difficulty.*`** — `strategy_trust` (how much of the LLM's strategy the engine acts on), resource handicaps, decision frequency.
- **`ai.strategy.*`** — the strategy vocabulary/library the LLM may choose from.
- **`render.* / input.* / runtime.*`** — presentation, controls, adapter settings.

**Content tables.** Catalogue-style content (powers, terrain types, scenarios) is expressed as **keyed collections** under its `content.*` root — each entry a record with named fields — rather than as a sprawl of individual scalar keys. The record fields still follow the naming convention.

---

## 4. Format requirements

The chosen format (ADR) **must** support:

- **Human-editable & commentable** — a designer/agent can read and annotate it directly.
- **Schema-validated** — a machine-readable schema is the source of truth for allowed keys, types, ranges, and required fields. Validation runs at startup and in the gate.
- **Layered overrides** — configuration composes in a defined order: **built-in defaults → scenario/content pack → user/local overrides**. Later layers override earlier by key; the effective config is the merge, and the merge is validated as a whole.
- **Versioned** — `meta.schema_version` records the schema the file targets; a mismatch triggers a defined **migration** path rather than a silent misread.
- **Ranges & invariants** — the schema encodes valid ranges and cross-key invariants (e.g. `min ≤ max`) so bad values are caught before they reach the core.
- **Hot-reload where safe** — presentation/balance keys (`render.*`, most `sim.economy.*`, `content.powers.*` tuning) may be reloadable mid-session; structural keys (`sim.worldgen.*`, map size, schema version) are load-time only. The schema marks which keys are hot-reloadable.

Startup validation must produce **clear, actionable errors** (which file, which key, expected vs. actual). Invalid config never reaches the deterministic core.

---

## 5. Loading & injection

Per the architecture (I3/I4), the **core never reads files**. `ConfigPort` loads and validates configuration into an **immutable parameter object**, which the application injects inward. The core consumes parameters as plain data. This keeps the core deterministic and lets tests supply in-memory fixtures.

---

## 6. The no-code-change rule (and how it is tested)

The promise "tune without touching code" is only real if it is verified:

1. **Content-only change test.** A test changes a config value (e.g. `content.powers.flood.mana_cost`) and asserts the observable behaviour changes — **with no source edit**. This proves the value is genuinely wired through config, not shadowed by a constant.
2. **Magic-number conformance check.** A gate check scans core/simulation source for behavioural numeric/string literals outside the small allow-list (e.g. `0`, `1`, array indices, structural identifiers). Findings fail the gate (enforces I1).
3. **Key-reference integrity.** Every config key read by code must exist in the schema, and every schema key should be reachable/used; orphans on either side are flagged.
4. **Namespace conformance.** The validator rejects any key outside the registered roots and naming convention (§2).

A balance/tuning task is therefore a **config + schema + test** change with **zero** core-source edits — that is the intended, and enforced, workflow.

---

## 7. Subsystem isolation & exploration profiles ([ADR 0016](./decisions/0016-exploration-lane-and-subsystem-isolation.md))

The parameter layer is organised so that **one knob cannot leak into another subsystem** — the structural fix for the coupling cascade that forced the fresh start (unlimited mana silently changed the opponent's economy and broke replay). This is a requirement on the *shape* of `sim.*`, honoured from the layer's first line, not a later add-on.

### 7.1 The `sim.<subsystem>.enabled` seam

7.1.1 Every major simulation subsystem is a **disjoint subtree** under `sim.*` (`sim.opponent.*`, `sim.economy.*`, `sim.winloss.*`, and every future peer). No subsystem's parameters are derived from another's; cross-subsystem influence flows only through explicit seams where a subsystem reads its **own** state/budget, never through shared coupling.

7.1.2 Every toggleable subsystem carries an on/off seam named **`sim.<subsystem>.enabled`** (a `bool`). Disabling one subsystem must **not** break the build or the remaining subsystems — the loop still runs; the disabled subsystem simply does nothing. Reserved seams today:

| Key | Type | Meaning |
|---|---|---|
| `sim.opponent.enabled` | `bool` | `false` ⇒ no rival deity; the loop runs, nothing casts against the player. |
| `sim.economy.mana.mode` | `normal` \| `fast` \| `unlimited` | First-class mana generation mode; `unlimited` is god-mode, not a hack. The economy's control knob (its "off" is not a sandbox use-case, so it exposes `mode` rather than `enabled`). |
| `sim.winloss.enabled` | `bool` | `false` ⇒ no win/loss evaluation during free play. |

New subsystems follow the same convention. *(A future gate check may assert that every `sim.*` subsystem exposes an `enabled` switch — deferred until more subsystems exist, per ADR 0016.)*

### 7.2 The `sandbox` exploration profile

`config/sandbox.toml` is a **named profile layer** occupying the scenario/content-pack slot of the §4 layering order (`default.toml` → `<profile>.toml` → `local.toml`). It composes the seams above into one selectable "let me play with one mechanic" flag — **opponent off, mana unlimited, win/loss off** — and is selected via the loader's `load_with_profile(dir, Some("sandbox"))`. A named-but-missing profile is a **loud error**, never a silent fall-back to the governed defaults.

### 7.3 Determinism is scoped to the governed configuration

The replay/determinism golden (§6, I3) asserts reproducibility for the **committed default configuration** (`default.toml`: every subsystem on, mana `normal`). Sandbox/exploration-only configuration is explicitly **outside** the deterministic contract — toggling it *cannot* make the replay test fail, because it is not part of what determinism promises ([ADR 0016](./decisions/0016-exploration-lane-and-subsystem-isolation.md) §4). The shipped game's determinism is unchanged; this scopes *what* must be deterministic, it does not weaken the guarantee.
