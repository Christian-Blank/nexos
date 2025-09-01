---
kind: project_brief
version: 0.1
id: PB-NEXOS-ADRS
parent_link: ../README.md
context:
  problem: >
    Architecture decisions need to be recorded, tracked, and easily discoverable
    for both humans and AI agents to understand system contracts and constraints.
  constraints:
    - ADRs are only created when contracts truly need to change (evidence-triggered)
    - Follow standard ADR format: Status, Date, Decision, Consequences
    - Link to parent documentation context
    - Machine-readable front-matter for AI navigation
---

# Architecture Decision Records (ADRs)

Records of architecture decisions made during NEXOS CLI development.

## What are ADRs?

Architecture Decision Records capture important decisions about system contracts, constraints, and technical approaches. In NEXOS, ADRs are only created when:

1. **Blockage** - Current approach prevents progress
2. **Pattern repetition** - Issue appears across ≥2 features  
3. **Budget violation** - Current approach violates North Star budgets

## Active ADRs

- [`0001-cli-contracts.md`](0001-cli-contracts.md) - CLI composition, JSON contracts, determinism, event logging

## ADR Process

Per [PLAYBOOK.md](../PLAYBOOK.md):

1. **L2 Meta Review triggers** Change Proposal → ADR if evidence supports it
2. **Evidence-driven only** - No ADRs without concrete evidence of need
3. **Tighten-only principle** - Widening constraints requires ADR approval

## For AI Agents

ADRs represent the authoritative technical contracts for the system. When making decisions:
1. Check relevant ADRs first
2. Prefer ADR guidance over conventions  
3. Flag potential ADR violations for human review

## Context

Part of [NEXOS Documentation](../README.md). See [North Star](../01-north-star.md) for principles and [PLAYBOOK](../PLAYBOOK.md) for process details.
