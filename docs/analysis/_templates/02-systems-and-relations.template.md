---
schema: analysis.section.v0
title: "Systems & Relations"
owner: "<@dri>"
status: "template"
version: "0.1.0"
parents: ["./README.md","../README.md"]
children: []
---
# Systems & Relations

## Bounded Contexts
- Context A — purpose, owners, SSoT(s)
- Context B — purpose, owners, SSoT(s)

## Relations (Mermaid sketch)
```mermaid
flowchart TD
  A[Context A] -->|Uses| B[Context B]
  B --> C[Context C]
```
Sources of Truth Matrix

Area	SSoT path(s)	Notes
…	…	…
