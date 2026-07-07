# 20 — Architecture

> **Status:** Active · **Governed by:** [`30-ai-agent-contract.md`](./30-ai-agent-contract.md) (invariants I2, I3, I4) · **Load this doc for:** any core-simulation, port/adapter, or tooling task.

This document defines the structural rules of the codebase. It is **tool- and language-agnostic**: it describes layers, boundaries, and dependency direction, not a specific framework. The concrete language/framework is chosen in a later ADR; nothing here presumes one.

---

## 1. Shape: ports & adapters around a deterministic core

The system is a **hexagonal (ports-and-adapters)** architecture with a layered interior. The reason is invariant **I3 (deterministic core)**: the simulation must be pure and reproducible, so everything non-deterministic or side-effecting is pushed to the edge, behind interfaces.

```
                    ADAPTERS (edge, side effects, non-deterministic)
        ┌───────────────────────────────────────────────────────────┐
        │  LLM adapter   Renderer   Input   Persistence   Audio       │
        │  Clock/RNG src   Logging sink                                │
        └───────────────▲───────────────────────────▲─────────────────┘
                        │ implements                 │ implements
                    ┌───┴───────────────────────────┴───┐
                    │              PORTS                 │   (interfaces only)
                    │  LLMOpponentPort  RendererPort     │
                    │  InputPort  PersistencePort         │
                    │  ClockRandomPort  ConfigPort        │
                    │  AudioPort  LoggingPort             │
                    └───────────────▲────────────────────┘
                                    │ depends on (interfaces)
                        ┌───────────┴────────────┐
                        │    APPLICATION LAYER    │   (orchestration, no game rules)
                        │  session, command apply │
                        │  turn scheduler, wiring  │
                        └───────────▲─────────────┘
                                    │ depends on
                        ┌───────────┴─────────────┐
                        │   DETERMINISTIC CORE     │   (pure, no I/O, seeded)
                        │  world · terrain · pop   │
                        │  economy · powers        │
                        │  rules · win conditions  │
                        └───────────▲──────────────┘
                                    │ reads (data only)
                        ┌───────────┴─────────────┐
                        │   CONFIG / PARAMETERS    │   (validated data, no logic)
                        └──────────────────────────┘
```

**The dependency rule (I2):** arrows point **inward/downward only**. The core depends on nothing but validated config data. The application layer depends on the core and on port *interfaces*. Adapters depend on port interfaces (which they implement) and may use outside libraries. **No arrow ever points outward from the core, and there are no cycles.**

---

## 2. Layers

### 2.1 Config / parameter layer
Validated, schema-checked data — **no logic**. Loaded and validated at startup, then injected inward as immutable values. See [`40-parameterisation.md`](./40-parameterisation.md). The core receives parameters as data; it never reads files itself (I3).

### 2.2 Deterministic core
The heart of the game. Pure functions and immutable-by-default state. Contains:
- **World & terrain** — an integer height field sampled at grid **vertices** (corners), water, and land-shaping operations that cascade to preserve the step invariant ([ADR 0017](./decisions/0017-vertex-heightfield-terrain.md)).
- **Population / followers** — followers, settlements, growth, allegiance.
- **Economy** — the faith/mana resource: generation, storage, spend.
- **Powers** — divine interventions (data-defined) and their effects on world/population.
- **Rules & turn loop** — how a tick/turn advances state; ordering; legality of actions.
- **Win/loss conditions** — evaluated from state.

Constraints (from I3):
- No wall-clock, no ambient randomness, no I/O, no network, no filesystem, no logging side effects.
- All randomness comes from a **seeded RNG** passed in via `ClockRandomPort`'s deterministic form (a seeded stream), never a global.
- State transitions are explicit: `nextState = step(state, command, params, rng)`. Same inputs ⇒ identical output (bit-for-bit), which the replay harness verifies.

### 2.3 Application layer
Orchestration only — **no game rules live here**. Responsibilities:
- Own a game **session**: hold current state, seed, and config.
- Accept **commands** (from human input or the resolved AI strategy) and apply them via the core's `step`.
- Drive the **turn scheduler** (advance ticks, decide when to solicit an AI decision — see cadence params).
- Mediate between the core and the ports: pull observations for the LLM, push render frames to the renderer, persist/load snapshots.
- Wire adapters to ports at startup (composition root).

### 2.4 Ports (interfaces)
Interfaces the application/core depend on, each with real adapter(s) **and** a test double:

| Port | Purpose | Notable adapters |
|---|---|---|
| `LLMOpponentPort` | Get the rival deity's strategy from an observation. | local-LLM adapter (`llm-ollama`: Ollama over loopback, [ADR 0014](./decisions/0014-ollama-local-llm-runtime.md)); scripted/mock adapter (tests). See [`50-llm-opponent.md`](./50-llm-opponent.md). |
| `RendererPort` | Present a derived **`TerrainFrame`** snapshot (grid dims + row-major heights; no simulation or camera/view state — [ADR 0020](./decisions/0020-workbench-runtime-and-rendererport.md)). The camera is adapter-local view state and never crosses the boundary. | on-screen renderer (`wgpu`/Metal, [ADR 0007](./decisions/0007-wgpu-rendering-framework.md)); headless render-to-PNG (agent visual self-check); no-op test double. |
| `SimDriver` | The interactive seam ([ADR 0022](./decisions/0022-interactive-shaping-seam-input-command-simdriver.md)): the renderer *holds* one to `submit` a discrete **`TerrainCommand`** (`Raise`/`Lower` at integer grid coords) and pull the current snapshot (`width`/`height`/`heights`/`revision`) to draw. Sits alongside `RendererPort`, which is unchanged. | app `WorkbenchSession` (owns the core `World` + recorded command log); the static renderer adapters do not implement it. |
| `InputPort` | Receive human player intent. Concretely realised as `SimDriver::submit(TerrainCommand)` — input reaches the sim *only* as a discrete, recorded command ([ADR 0022](./decisions/0022-interactive-shaping-seam-input-command-simdriver.md)), so a live session still replays bit-for-bit. | device/UI adapter → `TerrainCommand`; scripted input (tests). |
| `PersistencePort` | Save/load sessions & snapshots. | local-storage adapter; in-memory (tests). |
| `ClockRandomPort` | Time and randomness *at the edge*; supplies **seeded** RNG streams to the core. | real clock + seeded RNG; fixed-seed/fixed-time (tests). |
| `ConfigPort` | Load & validate configuration into immutable params. | layered-TOML loader + types-first validator (`serde`/`garde`, [ADR 0008](./decisions/0008-toml-config-format-types-first-schema.md)); in-memory fixture (tests). |
| `AudioPort` | Sound/music cues. | audio adapter; no-op (tests). |
| `LoggingPort` | Structured diagnostic output. | real sink; capturing (tests). |

### 2.5 Adapters
Concrete implementations of ports. This is the **only** layer permitted to touch external libraries, the OS, the screen, the disk, the model runtime, or the network stack (which, per I7, must not be required at runtime). Adapters contain no game rules.

**Debug/HUD overlay ([ADR 0015](./decisions/0015-debug-hud-ui-layer.md)).** A developer-facing diagnostics overlay (`egui`, feature-gated behind `debug-hud`, dev/verify-on and release-off) lives **inside the renderer adapter** — not a new port and not a sibling crate, since it draws on the renderer's own surface and adapters may not import each other (§5.3). It is **strictly read-only**: it presents a derived `DiagnosticsSnapshot` (tick, seed/RNG position, sim-state summary, the last recorded advisor I/O, edge-measured timings) that the application assembles as a pure read and pushes alongside the render frame. It holds no simulation state and never mutates the core. Any sim-affecting debug *action* (pause, single-step, spawn) is emitted as a normal command through `InputPort` and recorded like any input — so a debug-driven session still replays bit-for-bit (I3). Its config is `render.hud.*`.

---

## 3. Determinism boundary

Everything **inside** the core + config is deterministic. Everything **outside** (adapters) may be non-deterministic.

- The **LLM is outside** the boundary (it is non-deterministic). Its output crosses into the deterministic side only after being validated and converted to concrete legal commands by the application/core. This is what makes an LLM opponent compatible with a reproducible engine — see [`50-llm-opponent.md`](./50-llm-opponent.md).
- **Reproducibility contract:** given the same config, the same seed, and the same ordered sequence of commands, the core reproduces an identical state history. To make an LLM-driven session reproducible, its resolved commands (or the raw decisions) are recorded and can be replayed without invoking the model (record-replay).

---

## 4. Data flow of a single turn

```
1. Scheduler decides this tick needs an AI decision (cadence param).
2. App builds an OBSERVATION (compact, structured) from core state.
3. App calls LLMOpponentPort.decide(observation) ──► [outside determinism boundary]
4. Adapter returns a STRATEGY decision (schema-validated; fallback strategy on any failure).
5. App/core TRANSLATES strategy → concrete candidate COMMANDS.
6. Core VALIDATES each command for legality against current state + params.
7. Core APPLIES legal commands: state' = step(state, command, params, rng).   [deterministic]
8. Human INPUT commands for this tick are applied the same way (step).
9. Core advances the tick; evaluates win/loss.
10. App pushes a frame to RendererPort (and, when the `debug-hud` overlay is active, a read-only `DiagnosticsSnapshot` alongside it — ADR 0015); persists a snapshot if due.
```

Steps 6–9 are fully deterministic and independently testable with test doubles for every port (invariant I5).

**Real-time workbench refinement ([ADR 0020](./decisions/0020-workbench-runtime-and-rendererport.md)).** The push in step 10 is the *headless/batch* shape. In the interactive workbench the renderer adapter owns the `winit` event loop — the window drives redraws — so `RendererPort` is called *by* the loop with the current `TerrainFrame`, not from an app-owned `for` loop. The camera moves entirely inside the adapter and never crosses the determinism boundary, so this real-time view does not weaken I3.

**Interactive submit/pull flow ([ADR 0022](./decisions/0022-interactive-shaping-seam-input-command-simdriver.md)).** Alongside that batch push, the live shaping path runs through the `SimDriver` port the renderer *holds* — now **wired end-to-end** (issue #9/#10 Phase 2): the composition root builds a `WorkbenchSession` and hands `&mut session as &mut dyn SimDriver` into `WindowRenderer::run`, and the window event loop drives the loop below on every shaping click:

```
a. Input (a click/drag) is mapped, at the renderer edge, to a discrete TerrainCommand
   (integer grid coords — the float/ray work stays adapter-local).
b. Renderer calls SimDriver.submit(command) ──► app WorkbenchSession.
c. Session APPLIES it: World.apply(params, command) → the bounded raise/lower cascade   [deterministic]
   (immovable-refusal and all), RECORDS (tick, command) in the log, and advances one tick.
d. Renderer PULLS the fresh snapshot (width/height/heights) to draw; `revision` bumps
   only when the heights actually changed, telling the renderer to animate.
e. Replay re-applies the recorded (tick, command) log to a fresh World from the same
   seed + params, stepping tick-by-tick, and reproduces the field bit-for-bit (I3).
```

Steps b–c are the concrete `InputPort` (§2.4): input reaches the sim *only* as a recorded command, and no wall-clock, float, or frame-rate value ever enters the core — so a live, mutating sim stays replayable. Step **a** is realised in the renderer adapter (issue #9 Phase 2): a press→release below the `input.shape.click_drag_threshold_px` motion threshold is a shaping *click* — the cursor picks a vertex (the reticle pick generalised to the live cursor) and the bound button (`input.shape.{raise,lower}_button`) selects raise/lower; more motion is a camera *drag* and shapes nothing. Step **d** eases the *drawn* surface from its old shape to the new one over `render.animation.duration_ms` (issue #9/#10 Phase 3): a render-only mesh tween — the before/after surfaces share topology (the grid is fixed), so the renderer lerps them per vertex, driven by an adapter-local wall-clock. It is presentation only (like the camera, ADR 0020 §3); nothing it computes reaches the core (`duration_ms = 0` snaps). When a future subsystem needs per-tick background evolution, a wall-clock-*paced* accumulator can decide *when* to step in the renderer loop, never *what* a step computes (ADR 0022 §5).

---

## 5. Module boundary rules (enforced)

Per I2/I4 these are checked by tooling (the dependency/boundary checker in the gate), not left to discipline:

1. **Core imports config-layer data types and the ports crate's plain-data DTOs.** It reads validated config and — since [ADR 0022](./decisions/0022-interactive-shaping-seam-input-command-simdriver.md) — consumes the `TerrainCommand` DTO from `ports` (a plain value it dispatches, never a trait it calls outward). It must not import the application or any adapter. Because `ports` is a zero-dependency leaf, this inward-only `core → ports` edge cannot form a cycle. Violations fail the gate.
2. **Application imports core + port interfaces**, never a concrete adapter (adapters are injected at the composition root). It *implements* port interfaces too — e.g. the `SimDriver` interactive seam ([ADR 0022](./decisions/0022-interactive-shaping-seam-input-command-simdriver.md)).
3. **Adapters import port interfaces** (to implement) and external libs; adapters must not import each other.
4. **No cycles** anywhere in the module graph.
5. **New ports and new namespace roots are architectural changes** → require an ADR (contract §5).

The concrete module layout realises the graph above as a **Rust Cargo workspace** ([ADR 0006](./decisions/0006-rust-language-and-runtime.md)): `core` (zero *external* dependency — config data + the ports `TerrainCommand` DTO only, `no_std` + alloc, pure — [ADR 0009](./decisions/0009-enforcement-tooling-and-the-gate.md), [ADR 0022](./decisions/0022-interactive-shaping-seam-input-command-simdriver.md)) · `config` · `ports` (trait interfaces + the DTOs they hand across) · `app` (orchestration) · `adapters/*` (one crate per adapter) · a thin composition-root binary. Because `core` names only inward crates in its `Cargo.toml`, an illegal outward import fails to compile — the rules above are enforced by the crate graph plus the boundary checker, not by convention.
