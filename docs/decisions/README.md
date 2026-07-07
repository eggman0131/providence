# Architecture Decision Records (ADRs)

An ADR captures a single significant decision: its context, the choice made, and its consequences. In this project ADRs are **the only mechanism** for making architectural changes or amending the contract (see [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) §5, §8).

## When an ADR is required

- Adding/changing a **module boundary** or a **port**.
- Adding a **dependency** or choosing a **tool/runtime/language/format**.
- Adding a new **config namespace root**.
- Changing **determinism**, the **enforcement framework/gate**, or any **invariant (I1–I9)**.
- Any change classified **architectural** by the contract.

Balance/content/number changes do **not** need an ADR — they are config tasks.

## Process

1. Copy [`template.md`](./template.md) to `NNNN-short-kebab-title.md` using the next zero-padded number.
2. Fill in every section. Be concrete about **consequences** and any **gate/tooling** work the decision creates.
3. Set **Status: Proposed**, then **Accepted** once agreed. Update affected docs in the *same* change (I6).
4. Never rewrite history: to reverse a decision, add a new ADR with **Status: Superseded by NNNN** on the old one and a link both ways.

## Numbering

- Zero-padded, monotonic, never reused: `0001`, `0002`, …
- One decision per ADR. If you are deciding two things, write two ADRs.

## Index

| # | Title | Status |
|---|---|---|
| [0001](./0001-adopt-architecture-decision-records.md) | Adopt Architecture Decision Records | Accepted |
| [0002](./0002-llm-as-strategic-advisor.md) | LLM opponent as a strategic advisor, not an actuator | Accepted |
| [0003](./0003-parameterisation-first.md) | Parameterisation-first (no behavioural constants in code) | Accepted |
| [0004](./0004-deterministic-core-ports-and-adapters.md) | Deterministic core with ports & adapters | Accepted |
| [0005](./0005-macbook-only-offline-runtime.md) | MacBook-only, offline runtime | Accepted |
| [0006](./0006-rust-language-and-runtime.md) | Rust as the implementation language & runtime | Accepted |
| [0007](./0007-wgpu-rendering-framework.md) | wgpu as the 3D rendering framework | Accepted |
| [0008](./0008-toml-config-format-types-first-schema.md) | TOML config format with a types-first schema | Accepted |
| [0009](./0009-enforcement-tooling-and-the-gate.md) | Enforcement tooling and the one-command gate | Accepted |
| [0010](./0010-branch-workflow-and-ci.md) | Branch-based workflow with PRs and CI that runs the gate | Accepted |
| [0011](./0011-advisory-code-scanning.md) | Advisory (non-gating) code scanning with CodeQL | Proposed |
| [0012](./0012-project-management-on-github.md) | Project management and issue tracking on GitHub | Accepted |
| [0013](./0013-advisory-doc-drift-review-on-push.md) | Advisory doc-drift review on every push | Accepted |
| [0014](./0014-ollama-local-llm-runtime.md) | Ollama as the local LLM runtime for the strategic-advisor opponent | Accepted |
| [0015](./0015-debug-hud-ui-layer.md) | A read-only debug/HUD developer overlay (egui, feature-gated) | Accepted |
| [0016](./0016-exploration-lane-and-subsystem-isolation.md) | Exploration lane and subsystem isolation | Accepted |
| [0017](./0017-vertex-heightfield-terrain.md) | Vertex height field terrain with a bounded-step invariant | Accepted |
| [0018](./0018-outcome-framed-decision-rationale.md) | Decisions are explained in terms of outcomes, not implementation | Accepted |
| [0019](./0019-foundation-first-terrain-workbench.md) | Foundation-first delivery: terrain module + prioritised 3D terrain workbench (supersedes §7.4 sequencing) | Accepted |
| [0020](./0020-workbench-runtime-and-rendererport.md) | The workbench runtime shell and the `RendererPort` contract | Accepted |

> **Planning lives on GitHub** ([ADR 0012](./0012-project-management-on-github.md)): tasks and bugs are [Issues](https://github.com/eggman0131/providence/issues); the roadmap / "what's next" is the [Project board](https://github.com/eggman0131/providence/projects). ADRs stay the record of *why*; issues track *what/when*. Both previously-open items are now settled: the **LLM runtime & model** ([#8](https://github.com/eggman0131/providence-legacy/issues/8)) by [ADR 0014](./0014-ollama-local-llm-runtime.md), and the **debug/HUD UI layer** ([#9](https://github.com/eggman0131/providence-legacy/issues/9)) by [ADR 0015](./0015-debug-hud-ui-layer.md).
