# 0018 — Decisions are explained in terms of outcomes, not implementation

- **Status:** Accepted
- **Date:** 2026-07-06
- **Deciders:** Director + agent
- **Related:** contract [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (§1.4 escalation, §4.2 new, §5 change classification, §8 amendment); [`template.md`](./template.md) (the required section this adds); [`0001`](./0001-adopt-architecture-decision-records.md) (ADRs are the decision mechanism); [`0002`](./0002-llm-as-strategic-advisor.md) (LLM advises, engine actuates — the analogous split, here for the human); `README.md` (index).

## Context

This project has **no human code author** (contract, preamble). Every implementation decision is made and enforced by agents against the gate (§6); the gate — not a human — judges whether code is correct, bounded, deterministic, and in-namespace. That leaves an unstated question the contract never answered: **what is the human director actually for, and how should a decision be written so they can do that job?**

The human's role is *not* to review implementation — the gate already does, more reliably than a human reading a diff. The human is the one judge the gate cannot replace: the judge of **outcomes** — what a decision does to gameplay, to the player's experience, and to the design flexibility still open to future decisions. Yet both the ADR template and the escalation path (§1.4) were written implementation-first. The template's most detailed section is "Enforcement / gate impact"; there is no home for "what does this mean for someone playing the game." A human asked to accept *"we will model terrain as an integer height field sampled at grid vertices"* ([ADR 0017](./0017-vertex-heightfield-terrain.md)) has to reverse-engineer the point — crisper stepped land, a drift-free core — out of the mechanics.

This is the human-facing mirror of [ADR 0002](./0002-llm-as-strategic-advisor.md): there, the LLM advises in declarative *intent* and the deterministic engine actuates. Here, the human decides on *outcomes* and the agents + gate actuate the implementation. Neither party should be handed the other's language.

## Decision

We will require every decision that reaches a human — whether **escalated** for a human ruling (§1.4) or **recorded** for human review (an ADR; a non-trivial commit or doc that explains a choice) — to **lead its rationale with outcomes**: the impact on the **player**, on the **experience (UI/UX)**, and on **future design flexibility**, stated so a human can judge it without reading code. Implementation detail is still recorded, but *after* and *in service of* the outcome framing — never as the headline.

This is actuated, not merely declared:

1. **The ADR template gains a required `## Player & experience impact` section**, placed between *Decision* and *Consequences*. It states, in outcome terms, what changes for the player / the experience / future flexibility. A purely internal decision must *say so explicitly* and give its flexibility-or-process outcome instead — the section is never left blank.
2. **The contract gains §4.2** ("Explaining decisions — write for the human's role"), which states the norm and points both the escalation path (§1.4) and the ADR process (§5, §8) at it.
3. **This is a judgement standard, not a gate check.** It cannot be mechanically enforced (§6.1). The template section is its structural surrogate; the section's *presence and non-emptiness* is the closest thing to enforcement it will have.

## Player & experience impact

*(This ADR fills the section it introduces — the meta case, and proof the section works even when a decision touches no game code.)* There is **no direct in-game effect**: no player will play differently because of this decision. Its outcome is on **future design flexibility and the director's ability to steer the game at all**. From here on, every decision about mechanics, balance, powers, and feel is recorded in language the human director can actually adjudicate — which is precisely what keeps a no-human-code-author project from drifting, decision by decision, away from the game its director wants. The cost falls entirely on agents (one more section, written for a different audience); the player and director only ever see the benefit.

## Consequences

- **Positive:**
  - Names, for the first time, what the human is *for* (outcome judgement) and hands them decisions in a language they can rule on — the missing half of the "no human code author" model.
  - Completes the symmetry with [ADR 0002](./0002-llm-as-strategic-advisor.md): intent-vs-actuation for the LLM; outcome-vs-implementation for the human.
  - The template section is a durable prompt: every future ADR must reckon with player / UX / flexibility impact, surfacing decisions that are technically clean but bad *for the game* before they are accepted.
- **Negative / trade-offs:**
  - It is a **judgement norm and cannot be gated** — a lazily-filled section passes the gate exactly as readily as a considered one. Enforcement is structural (the section must exist) and social (review), never mechanical. This limitation is **accepted, not solved**.
  - Slight friction for genuinely internal ADRs (tooling, process), which must now articulate a flexibility/process outcome rather than skip the framing.
- **Enforcement / gate impact:** **none** — nothing mechanical changes. The template section is the only structural artefact. This is deliberately *not* an invariant (I1–I9): the defining property of that set is tool-enforceability (§2 preamble), and adding an unenforceable "invariant" would dilute it.
- **Docs to update (this change, I6):** [`template.md`](./template.md) (add the required section), [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (§4.2 + a pointer in §1.4), `README.md` (index).

## Alternatives considered

- **A line in the contract only, no ADR.** Cheaper, but §8.1 forbids silent edits to the governance framework, and — worse — an unstructured line is a recommendation, not a norm (§6.1). Without the template section it would not survive contact with a hurried future agent. Rejected.
- **Make it an invariant (I10).** Elevates its status, but every invariant is *defined* by being tool-enforced (§2 preamble), and this one provably is not. Rejected, to keep the invariant set honest.
- **Fold it into §4.1 ("Guidance for the model doing the work").** Reasonable, but §4.1 is about *writing code*; this is about *communicating decisions to a human* and spans escalation **and** ADRs, so it earns its own §4.2 plus the template hook. Rejected in favour of a first-class home.
- **Do nothing / rely on good judgement.** The status quo already produced implementation-first ADRs and an implementation-first template. A norm without a structural home does not hold. Rejected.
