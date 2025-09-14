---
title: "docs — Documentation Domain"
---

# docs — Documentation Domain and Knowledge Graph

Purpose
- This folder is the top-level documentation domain.
- Folders are treated as domains and bounded contexts; subfolders are subdomains.
- Acts as a navigable knowledge graph with a single source of truth and non‑redundant data.

How to use
- Start at any folder’s README to understand its purpose and scope, then move up/down the tree based on needed context.
- Prefer linking to canonical artifacts rather than duplicating content.
- Keep documents atomic, versioned, and machine‑navigable (YAML front matter + Markdown).

Conventions
- All Markdown files SHOULD include minimal YAML front matter (title and optional metadata) for indexing.
- Cross‑link using relative paths; avoid duplication (“projection, not duplication”).
- Projections (e.g., vaults/dashboards) are read‑only views; canonical truth lives here in docs.

Key subdomains
- `canon/` — Normative operating doctrine (shapes, gates, invariants). Start with `canon/00_canon.md`.

Change discipline
- Material normative changes SHOULD be proposed via Canon Change Proposals (see `docs/canon/ccp/` when present) and follow the gates defined in the canon.
