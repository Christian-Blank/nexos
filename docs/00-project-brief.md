---
kind: project_brief
version: 0.1
id: PB-NEXOS-CLI
links:
  north_star: docs/01-north-star.md
components:
  - comp-nexoscli-core
context:
  problem: >
    Solo-to-platform work creates drift and rework. We need a unified CLI and meta-layer
    so humans and AI can build incrementally with low cognitive load.
  constraints:
    - Linux/macOS only (devcontainer on macOS).
    - Static plugin linking for strictness; compose Clap before parsing.
    - JSON is the CLI boundary; protobuf later for services.
out_of_scope:
  - GUI
---

# Project Brief — NEX CLI

Goal: ship a walking skeleton (one binary + issue domain local) with determinism, event log, and tools manifest.
