---
kind: project_brief
version: 0.1
id: PB-NEXOS-DOCS
parent_link: ../README.md
context:
  problem: >
    Documentation needs clear navigation and context for both humans and AI agents
    to understand project structure, decisions, and processes efficiently.
  constraints:
    - Follow knowledge graph pattern with machine-readable front-matter
    - Link to parent context and maintain document hierarchy
    - Keep focused on essential navigation and context
---

# NEXOS Documentation

Core documentation and decision records for the NEXOS CLI project.

## Navigation

**Foundation Documents:**
- [`00-project-brief.md`](00-project-brief.md) - Project goals and scope  
- [`01-north-star.md`](01-north-star.md) - Principles, budgets, and non-goals
- [`exec_graph.yaml`](exec_graph.yaml) - Authoritative execution DAG

**Process & Development:**
- [`PLAYBOOK.md`](PLAYBOOK.md) - Working agreements and development workflow

**Architecture Decisions:**
- [`ADRs/`](ADRs/) - Architecture Decision Records (contract changes)

## For AI Agents

Parse YAML front-matter in each document as the authoritative record. The markdown body provides human context. Build execution DAG from `exec_graph.yaml` and follow dependency chains.

**Entry sequence:** North Star → Project Brief → Execution Graph → relevant ADRs

## Context

This is part of the NEXOS CLI project. See [parent README](../README.md) for complete system overview.
