# 0007 — wgpu as the 3D rendering framework

- **Status:** Accepted
- **Date:** 2026-07-02
- **Deciders:** Founding session (human director + agent)
- **Related:** [`../20-architecture.md`](../20-architecture.md), [`../60-constraints.md`](../60-constraints.md), [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (I2, I3, I4, I5, I8), [`0004`](./0004-deterministic-core-ports-and-adapters.md), [`0005`](./0005-macbook-only-offline-runtime.md), [`0006`](./0006-rust-language-and-runtime.md)

## Context

The rendering framework was left open by [ADR 0006](./0006-rust-language-and-runtime.md). The visual target fixed this session is a **3D terrain mesh**, which needs a real GPU pipeline. Whatever is chosen is, per [`20-architecture.md`](../20-architecture.md) and [ADR 0004](./0004-deterministic-core-ports-and-adapters.md), a **renderer/input adapter** behind `RendererPort`/`InputPort`: it presents derived frames and holds **no** simulation state and no game rules; it sits *outside* the determinism boundary (I3). It runs on Apple Silicon, offline, via Metal (I7, ADR 0005), in Rust (ADR 0006).

The decisive additional force: the codebase is authored **entirely by agents** (contract §1), so *how well Claude models can write, debug, and maintain the framework* is a first-class selection criterion — not an afterthought. That criterion has two dominant failure modes: **stale-API generation** (models emit code for an outdated framework version), which punishes fast-churning APIs; and **blind visual debugging** (a model cannot see a wrong frame), which punishes niche, thinly-documented abstractions. Two harness capabilities partly offset both: this agent can **read images** (so a rendered frame can be visually self-checked), and **Context7** fetches version-current library docs (countering stale-API generation).

## Decision

We will use **wgpu** as the 3D rendering framework, implemented in a **`renderer` adapter crate** that realises `RendererPort`, targeting the Metal backend on Apple Silicon, with **WGSL** shaders. A windowing/event crate (**`winit` expected**) provides the surface and raw events consumed by the renderer and the `input` adapter. Both are adapter-layer dependencies, chosen at latest stable and pinned (I8).

- **The renderer owns no game state.** It consumes derived **render frames** pushed through `RendererPort`; it contains no rules and no simulation state. `wgpu`/`winit` and their transitive trees live **only** in the `renderer` (and windowing/`input`) adapter crate(s); `core`/`config`/`ports`/`app` stay free of them, keeping the core zero-dependency (I8). `cargo-deny` is scoped to enforce this.
- **Mandated headless render-to-PNG path.** The renderer adapter **must** support a headless mode that renders a frame to an off-screen texture and writes a PNG. This is the primary way an agents-only project self-corrects graphics: an agent (and the `/verify` step, I5) renders a frame, reads the image, and checks it — no display required. It also enables **golden-image visual regression** tests. These use **perceptual/tolerance comparison, never bit-equality**: GPU output is not bit-identical across drivers, and this visual check is **separate from and does not touch** the I3 bit-for-bit *core* determinism, which is a pure-CPU property of the simulation.
- **Version-pinned-docs practice.** Because stale-API generation is the main model-failure mode, agents fetch **version-current `wgpu`/WGSL docs (Context7)** before authoring or editing renderer code; the pinned `wgpu`/`winit` versions in `Cargo.lock` are the source of truth.

**Out of scope for this ADR** (later feature/design work or their own ADRs): the terrain mesh representation, camera model, lighting, level-of-detail, and art style (tunable aspects are config per I1); and any separate **debug/HUD UI** layer (e.g. an immediate-mode `egui` overlay) — now decided in [ADR 0015](./0015-debug-hud-ui-layer.md) (a read-only, feature-gated egui overlay inside this renderer adapter). *(Update: the **camera model** and the **frame contract** — `RendererPort` + the `TerrainFrame` snapshot — are now fixed in [ADR 0020](./0020-workbench-runtime-and-rendererport.md).)*

## Consequences

- **Positive:**
  - **Cleanest architectural fit:** `wgpu` is just a GPU device you draw with — it does not own the main loop and has no ECS `World`, so it slots behind `RendererPort` with no temptation to absorb game state (the standing hazard that ruled out `bevy`). Directly protects I2/I3/I4.
  - **Strong model competence:** deep, stable training representation and *standard* GPU concepts (buffers, pipelines, bind groups, WGSL) that Claude handles well; WGSL is a small, stable shader language.
  - **Native Metal performance** on the M5 Max for the interactive frame budget, with direct control over the terrain pipeline.
  - The **headless PNG path** gives agents a real visual feedback loop and enables golden-image regression — turning "blind debugging" into "look at the frame."
  - Transitive deps confined to the adapter → `core` honours I8.
- **Negative / trade-offs:**
  - Low-level: **more renderer code to author and maintain** than a batteries-included engine — camera, scene management, mesh upload, LOD, and lighting are hand-built.
  - Blind visual debugging is mitigated by the PNG path but **not eliminated**; subtle GPU bugs (z-fighting, winding, coordinate spaces) remain harder for a model than CPU logic.
  - `wgpu` has its own cross-version API breaks — managed by pinning + Context7, but a recurring maintenance touch.
  - No editor/scene inspector (the price of not choosing Godot).
- **Enforcement / gate impact:** the `renderer` adapter ships with its test double (the headless no-op renderer already in the ports table) **plus** the headless-PNG capture; optional golden-image visual-regression checks use perceptual tolerance (explicitly *not* I3 bit-equality); `cargo-deny` is scoped so `wgpu`/`winit` cannot leak into `core` *(correction, [ADR 0020](./0020-workbench-runtime-and-rendererport.md): in this codebase that confinement is enforced by the **boundary checker** — the crate DAG plus the zero-external-deps rule — not `deny.toml`; `cargo-deny` still governs licenses/pinning)*; versions pinned and recorded.
- **Docs to update (this change):** `decisions/README.md` (index + open list), `20-architecture.md` (renderer adapter now concretely `wgpu`, with the headless-PNG capability noted). No invariant changes.

## Alternatives considered

- **bevy.** Fastest path to visible 3D terrain and a huge community, but the **worst API churn** of the candidates (frequent breaking releases → stale-API model failures), and it wants to own the main loop while its ECS `World` tempts the exact game-state-in-the-renderer violation that breaks I3/I2. Context7 mitigates the churn but not the boundary pull. Rejected for a many-session, agents-only, strict-boundary project.
- **Godot via `gdext`.** Godot itself is enormously documented and its editor is the best antidote to blind debugging, but the **Rust binding is the youngest and least-represented** of the options (contradicting the model-competence criterion), Godot wants to own the loop/scene (awkward as a pure adapter), and it adds a heavy external engine. Its best-understood language, GDScript, is out (splits the language, breaks determinism, ADR 0006). Rejected.
- **Niche higher-level Rust renderers (`three-d`, `rend3`).** Less code to write, but **thin training representation** means *more* model blind spots — contradicting the very criterion that motivated this decision — plus smaller/less-active communities. Rejected.
- **macroquad.** Simple and pleasant, but 2D-first and not suited to a 3D terrain mesh. Rejected.
