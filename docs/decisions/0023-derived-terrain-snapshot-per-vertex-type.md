# 0023 — The derived terrain snapshot grows a per-vertex terrain-type channel

- **Status:** Accepted
- **Date:** 2026-07-07
- **Deciders:** AI-agent session, on the Director's issue [#22](https://github.com/eggman0131/providence/issues/22) rulings
- **Related:** [ADR 0017](./0017-vertex-heightfield-terrain.md) (terrain types), [ADR 0020](./0020-workbench-runtime-and-rendererport.md) (`RendererPort` + the `TerrainFrame` snapshot), [ADR 0021](./0021-seeded-parameterised-worldgen.md) (worldgen + `content.terrain.*`), [ADR 0022](./0022-interactive-shaping-seam-input-command-simdriver.md) (the `SimDriver` interactive seam); issue #22 (terrain visuals).

## Context

The workbench draws a **derived `TerrainFrame` snapshot** ([ADR 0020](./0020-workbench-runtime-and-rendererport.md) §1) — grid dimensions plus row-major heights — and colours it with a two-stop height ramp (`render.palette.*`). The sea is just "the lowest coloured facets"; the land does not read as anywhere.

Issue #22 (terrain visuals) makes the land read as a *place* with **material bands aligned to the simulation's own terrain types** (shore→sand, land→grass, mountain→rock ramping to snow) — the Director's ruling: terrain *type* is the shared carrier for the **look** (this issue) and for **future gameplay** (snow slows breeding, beaches forbid trees). To colour by terrain type, the renderer must be handed the per-vertex type.

The constraints this must satisfy:

- Per [ADR 0020](./0020-workbench-runtime-and-rendererport.md) §1, only a **derived, read-only snapshot** may cross `RendererPort` — never simulation state and never the core (adapters must not import `providence-core`).
- The renderer should **key on derived state**, not re-implement the model's rules (ADR 0020 §1), so a later amendment to the ADR 0017 model costs little rework here.
- The interactive shaping path pulls its snapshot through the **`SimDriver`** port the renderer holds ([ADR 0022](./0022-interactive-shaping-seam-input-command-simdriver.md)), which today serves `width`/`height`/`heights`/`revision`. It is the *live twin* of the same derived snapshot and must carry whatever the batch push carries, so the bands track live edits.
- Determinism (I3) is untouched: this is presentation only.

This is the `adr-needed` decision issue #22 calls for: growing the `RendererPort` snapshot DTO.

## Decision

We will **grow the derived terrain snapshot to carry a per-vertex terrain type**, the first of three field-by-field growths under this one ADR (type now — Phase 1; the waterline datum — Phase 2; immovable features — Phase 3, each landing with the phase that reads it). Concretely:

1. **Extend `TerrainFrame` in place** — it is already *the* derived terrain snapshot; it gains a `types: &[TerrainType]` channel (row-major, parallel to `heights`) plus `types()` / `type_at(x, y)` readers. A frame built only to read heights (picking) passes an **empty** `types` slice; `type_at` then yields `None`.
2. **Mirror the core `TerrainType` as a plain `providence-ports` enum** (`Water`/`Shore`/`Land`/`Mountain`), exactly as `ports::Height` mirrors the core height — so the interface crate and every adapter stay free of a `providence-core` import.
3. **The type channel crosses on both existing snapshot paths.** `RendererPort::present(TerrainFrame)` (batch/headless) carries it, and the **`SimDriver` pull grows a `types()`** method mirroring `heights()` (interactive). They are the batch and interactive forms of one snapshot; the renderer rebuilds the material-banded surface from the fresh pull after every edit without ever re-deriving classification.
4. **The application classifies.** It derives each vertex's type via the core's existing `classify_vertex` (ADR 0017 §1) and maps `core::TerrainType → ports::TerrainType`. No new core behaviour; a pure composition of core reads. The renderer keys the material band on the result.

The former two-stop `render.palette.*` is **subsumed by `render.material.*`** — a base colour per terrain type plus the snow colour the mountain band ramps toward (linear RGB, all config; I1).

## Player & experience impact

The land stops being a flat gray height-ramp and reads as **somewhere**: pale sand at the waterline, green lowland, bare rock and snow on the heights — and, crucially, the bands are **aligned to what the land actually *is*** (the sim's shore/land/mountain types), so the look **tells the truth** about the terrain rather than free-painting over it. Because the material bands trace the terrain-type boundaries, they trace the integer **step** boundaries too — making the terracing *more* legible, so this **sharpens** the #11 stepped-model judgment rather than obscuring it (surface texture, the one part that could flatter a bad model, stays gated behind #11 in Phase 4).

For **future design flexibility** this is the load-bearing move: terrain type becomes the **shared carrier** for the look *and* for gameplay the Director wants later — followers moving or breeding slower on snow, trees forbidden on the beach. Those rules are not built here (followers/economy are parked), but this makes them a clean *addition* on an existing seam, not a retrofit. And because the renderer keys only on the **derived** `(type, height)` — never the model's internals — the ADR 0017 terrain model stays free to change while #11/#12 judge it. Nothing in gameplay is foreclosed: this is presentation only, determinism is untouched, and the stepped model is unchanged.

## Consequences

- **Positive:**
  - One snapshot type grows **field-by-field** with each phase; the renderer stays a pure reader and the classification *rules* stay in the core.
  - The look keys on `(type, height)`, so a #11 amendment to the ADR 0017 model costs little-to-no rework here (ADR 0020's promise, realised).
  - Determinism is untouched — the snapshot is a pure derived read, the core is not modified, and the replay/seed golden is **unchanged** (I3).
- **Negative / trade-offs:**
  - Two call surfaces grow together: the `TerrainFrame` DTO **and** the `SimDriver` port (`types()`), because the interactive path rebuilds the surface itself. Every `TerrainFrame::new` call site gains a `types` argument; heights-only (picking) frames pass an empty slice — a small dual-mode on the DTO, documented on `type_at`.
  - The per-vertex type array is **recomputed on every shaping edit** that moves the field — O(width × height), trivially cheap at workbench sizes, but not free.
- **Enforcement / gate impact:**
  - Extended tests: the `providence-ports` `TerrainFrame`/`SimDriver` contract + mock, the renderer `color`/`mesh` material tests, and the app type-derivation tests. New config keys `render.material.*` gain schema entries (the gate's schema-drift + key-integrity checks); `render.palette.*` is removed.
  - The **boundary checker is unaffected**: no adapter imports the core, and the core still consumes only the `ports` `TerrainCommand` DTO (the `core → ports` edge is unchanged).
  - The **replay/seed determinism test is unchanged** — the core is not touched.
- **Docs to update (this change):** this ADR + the [index](./README.md); [`20-architecture.md`](../20-architecture.md) §2.4 (the `RendererPort`/`SimDriver` snapshot payload); [`40-parameterisation.md`](../40-parameterisation.md) (`render.material.*`, `render.palette.*` removed); [`CLAUDE.md`](../../CLAUDE.md) (the workbench now renders sim-aligned material bands).

## Phase 2 — the waterline datum and the living water surface

The second field-by-field growth this ADR anticipated (issue [#22](https://github.com/eggman0131/providence/issues/22) Phase 2). It adds the *waterline* to the derived snapshot and the renderer floats a **living water surface** at it — a translucent, gently shimmering plane, built to react to a changing shoreline (the Director's ruling).

1. **`TerrainFrame` grows a `waterline: Height`** — the `sim.worldgen.sea_level` datum, carried alongside `heights`/`types` (a picking frame passes an unread `0`). It is *derived* read-only data, so the boundary and I3 hold exactly as for the type channel.
2. **`SimDriver` is unchanged.** Unlike `types` (which re-derives on every edit and so needed a pull method), the waterline is the **session-constant** datum: the renderer caches it from the initial `present` and reconstructs the interactive frame with it. The shoreline still tracks live edits **for free** — it moves because the *terrain* crosses the fixed datum, not because the datum moves — so no new pull seam is added (the issue's "no new seam").
3. **The renderer draws a translucent water pass** (`render.water.*`): a flat plane spanning the grid at the waterline, **alpha-blended over the terrain** and depth-tested (`Less`, no depth-write) so land above the waterline occludes it — the coastline for free, dynamic-shoreline-ready. Worldgen pins the sea floor *flat at the waterline* (ADR 0021), so the plane would be coplanar with the seabed; a small `render.water.surface_lift` floats it just above (no z-fighting), kept below one height step so it never rises over the first dry shore.
4. **The sea is alive.** A time-driven shimmer (`ripple_amplitude`/`ripple_speed`/`ripple_scale`) modulates the surface brightness in the water shader, timed on an **adapter-local wall-clock at the edge** — exactly like the camera (ADR 0020 §3) and the shaping animation (ADR 0022 §5). No clock, float, or frame-rate value reaches the core; the replay golden is **unchanged**.

The `render.water.*` keys (`rgb`/`opacity`/`surface_lift`/`ripple_amplitude`/`ripple_speed`/`ripple_scale`) gain schema entries; the pure water-plane geometry is gate-tested; the GPU water pass is exercised only through the headless capture (I9), whose filmstrip advances the shimmer clock so the sea's *motion* is observable without a display. This ships the calm version; flow, foam, and turbulence are a later escalation the shape leaves room for. Immovable features (Phase 3) are the third and final growth under this ADR.

## Alternatives considered

- **Renderer re-derives the type from heights + thresholds.** Rejected: it pulls the model's classification *rules* into the renderer — against ADR 0020's "key on derived state, not the model's internals" — and duplicates the core's `classify_vertex`. Carrying the already-derived type keeps the rules in one place.
- **A distinct, richer snapshot type alongside `TerrainFrame`.** Rejected: there is exactly one derived terrain snapshot. A second type would fork the `present`/pull paths and the mesh builder for no benefit; extending the existing snapshot in place is simpler and keeps the field-by-field growth coherent.
- **Carry the raw thresholds (`sea_level`, `shore_band`, `mountain_min`) in the snapshot instead of the classified type.** Rejected: thresholds are model/config internals, not derived presentation data; handing across the *classified* type is the ADR-0020-faithful choice and needs no classification logic in the renderer.
- **Grow `SimDriver` to return a whole `TerrainFrame` rather than a piecemeal `types()`.** Rejected for now: a larger churn to the established port shape (`width`/`height`/`heights`/`revision`); a `types()` pull mirrors the existing `heights()` cleanly and is the minimal growth for Phase 1.
