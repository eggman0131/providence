# 10 — Game Design (v1)

> **Status:** Living design · **Governed by:** [`30-ai-agent-contract.md`](./30-ai-agent-contract.md) · **Load this doc for:** any gameplay/mechanics or balance task (with [`40-parameterisation.md`](./40-parameterisation.md)).

This is a **concrete, coherent v1 design** for our *inspired-by* god-game — our own mechanics in the Populous II spirit, not a clone. It is a **living document**: mechanics are refined via ADRs, and **every quantity below names a configuration key** (per I1) rather than a fixed number. The keys use the namespaces defined in [`40-parameterisation.md`](./40-parameterisation.md); values live in config, not here.

> Convention: `key.path` in `code font` denotes a config key. This doc never states the value — only that the value is configurable and what it governs.

---

## 1. Fantasy & goal

You are a young deity contesting a world with a rival deity (the LLM opponent). You cannot command mortals directly; you influence them by **shaping the land** and **spending faith** as divine power. You win by making your following dominant and the rival's untenable.

---

## 2. The world

- A grid of **terrain cells**, each with a height and type (land, water, and derived types like shore/mountain). Map dimensions and generation come from `sim.worldgen.*`; terrain-type catalogue from `content.terrain.*`.
- Sea level, initial land ratio, and starting settlement placement: `sim.worldgen.*`.
- The world is generated from a **seed** (I3); the same seed reproduces the same world.

## 3. Core verb — land shaping

- The player **raises and lowers** terrain. Flat, dry, contiguous land is buildable; steep or flooded land is not.
- Each shaping op consumes faith and is bounded by cost/slope/height rules in `sim.terrain.*` (e.g. `sim.terrain.raise.mana_cost`, `sim.terrain.max_height`, slope limits).
- Shaping is the root of the loop: **flatten land → followers build & multiply → more faith → more power to shape and intervene.**

## 4. Followers, settlements & population

- **Followers** live in **settlements** built on buildable land. Housing capacity, growth, and migration are governed by `sim.population.*` (e.g. `sim.population.follower.growth_per_tick`, housing capacity keys) and follower-type definitions in `content.followers.*`.
- Population is the engine of the economy: more (and happier, safer) followers → more faith.
- Followers can shift allegiance under pressure/incentive; conversion rules in `sim.population.*`.

## 5. Economy — faith / mana

- Followers generate **faith** (the mana resource) each tick per `sim.economy.*` (e.g. `sim.economy.mana.regen_rate`, worship yield per follower, storage cap `sim.economy.mana.storage_cap`).
- Faith is the single currency for **all** divine action: land shaping (§3) and powers (§6).
- The economy is intentionally a tight loop so both sides face the same expand-vs-spend tension.
- Mana generation has a `sim.economy.mana.mode` (`normal` | `fast` | `unlimited`) — a first-class god-mode for isolated exploration, not a hack ([ADR 0016](./decisions/0016-exploration-lane-and-subsystem-isolation.md); see [`40-parameterisation.md`](./40-parameterisation.md) §7). Each deity reads its own budget, so raising it never leaks into the rival's economy.

## 6. Divine powers

- Powers are **data-defined content** in `content.powers.*` — a keyed catalogue. Each power record carries fields such as `mana_cost`, magnitude, radius, cooldown, and prerequisites.
- Expected v1 archetypes (final set is content, tunable/extensible without code): terrain-scale effects (e.g. flood, earthquake, raise-mountain), population effects (e.g. inspire growth, blight), and dramatic late-game effects. New powers are added by adding content records — **no code change** (I1).
- Example key: `content.powers.flood.mana_cost` governs the flood power's price; balancing powers is a config task.

## 7. The rival deity

- A second god, driven by the **LLM strategic advisor** ([`50-llm-opponent.md`](./50-llm-opponent.md)), plays by the same rules and economy against you.
- Its intelligence, aggression, cadence, and handicaps are configured under `ai.*` (difficulty, strategy vocabulary, decision cadence). It proposes *intent*; the deterministic engine executes legal actions.
- The whole subsystem is toggleable via `sim.opponent.enabled`: `false` ⇒ no rival casts against the player, and the rest of the game is unaffected (the isolation seam, [`40-parameterisation.md`](./40-parameterisation.md) §7).

## 8. Turn / tick loop

- Time advances in **ticks**; tick length and per-tick action limits are in `sim.rules.*`.
- Each tick (see [`20-architecture.md`](./20-architecture.md) §4): apply human commands and resolved rival commands, run economy/population/terrain updates, evaluate win/loss.

## 9. Win & loss

- Victory/defeat conditions and their thresholds are in `sim.winconditions.*` — e.g. eliminating the rival's followers, or crossing a dominance threshold (share of world population/territory) sustained for a configured duration.
- Multiple win conditions can be toggled per scenario (`content.scenarios.*`), letting different maps play differently as pure content.
- The evaluation subsystem as a whole is toggleable via `sim.winloss.enabled`: `false` ⇒ no win/loss checks during free play (the isolation seam, [`40-parameterisation.md`](./40-parameterisation.md) §7).

---

## 10. What is fixed vs. tunable

- **Fixed (code):** the *existence* of terrain-shaping, an economy, powers-as-catalogue, a tick loop, and a rival deity; the algorithms that run them.
- **Tunable (config):** every rate, cost, cap, threshold, radius, cadence, the entire power catalogue, terrain and follower types, scenarios, and opponent behaviour.

If a design change alters an *algorithm or structure* (not just numbers), it is architectural → ADR (contract §5). If it only changes numbers/content, it is a balance task → config + schema + test, no core edits.

---

## 11. Open questions (to resolve via ADR as v1 matures)

- Exact set of v1 powers and their interactions.
- Allegiance/conversion model detail.
- Precise dominance win metric and duration.
- Camera/interaction model for shaping (belongs partly to `render.*`/`input.*` and the environment discussion).

These are intentionally open; this doc will be refined, not frozen.
