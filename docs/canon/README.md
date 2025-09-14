---
title: "canon — Canon (Normative)"
---

# canon — Canonical Operating Doctrine

Purpose
- This subdomain holds the canonical, normative doctrine for safe speed.
- It defines principles, shapes, gates, invariants, lifecycle, events, roles, and change control across products and platforms.
- Folders are treated as domains/bounded contexts; this folder is the canon domain.

Start here
- Read `00_canon.md` for the full doctrine (with reading order and TL;DR).
- Treat `00_canon.md` as the single source of truth; projections are downstream.

Change control
- Changes follow Canon Change Proposals (CCP) under `ccp/` (when present).
- Status flow: Draft → Review → Accepted → Implemented → Superseded.

Non‑redundancy
- Avoid duplicating normative text elsewhere; link back to sections in `00_canon.md`.
- Projections (vaults/dashboards) are read‑only views and must reconcile via packages.

Structure (common files)
- `00_canon.md` — Canon text (normative).
- `primitives.yaml`, `invariants.yaml`, `flowspec.yaml` — referenced sketches/skeletons (informative).

Contributing
- Propose a CCP and cite the specific sections of `00_canon.md` to amend.
- Keep artifacts atomic, versioned, and front‑matter indexed.
