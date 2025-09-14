---
title: "Rule: Minimum README per Folder"
id: "CLINE-R-READMEs-01"
version: "1.0"
status: "Living (normative)"
owner_dri: "Docs Steward (rotating)"
applies_to: "All folders in this repository (domains and subdomains)"
---

# Rule: Minimum README per Folder

## Intent

Establish a consistent, low-friction standard so every folder can serve as a reliable entry point into its bounded context. This supports the repository-as-knowledge-graph principle with a single source of truth and non-redundant data.

## Scope

- Applies to every folder (domain or subdomain) that is expected to be read or traversed by humans or agents.
- Folders containing generated/vendor artifacts or build outputs MAY be exempt if clearly documented (see Exceptions).

## Normative Requirements (MUST/SHOULD)

Each applicable folder MUST include a `README.md` that:

1) Identifies the folder
- MUST have a top-level heading that matches or clearly names the folder’s domain/context (e.g., `# jobs — Jobs Domain`).

2) States the purpose
- MUST include a “Purpose” section that succinctly explains what lives here and why (2–8 sentences).

3) Explains how to use or navigate
- MUST include a “How to use” (or “Navigation”) section that tells a reader how to consume or extend artifacts within this folder (e.g., where to start, key files, typical workflows).

4) Conventions and non-redundancy
- SHOULD include conventions relevant to this context (naming, front matter, linking).
- SHOULD link to canonical artifacts instead of duplicating content (“projection, not duplication”).

5) Cross-links and orientation
- SHOULD link up to its parent context and down to key child contexts to support top-down and bottom-up navigation.
- SHOULD prefer relative links.

6) Front matter
- SHOULD include minimal YAML front matter with `title` (and optional metadata including file path and name and parent, sibling, child relationships where applicable) to support indexing.

7) Change discipline (when applicable)
- SHOULD reference change control (e.g., CCPs) if the folder governs normative text or specs.

## Exceptions

- Generated, third-party, cache, and build output folders MAY omit a README if:
  - The parent README lists them as exempt, and
  - A `.keep` (or similar) file plus a comment in the parent README clarifies the exemption.
- In such cases, a minimal one-line `README.md` MAY be used stating: “Exempt: generated/vendor/build output; see parent README.”

## Acceptance Criteria (Machine-Checkable)

A folder conforms if:

- Contains `README.md`.
- `README.md` includes:
  - A top-level `#` heading within the first 10 lines.
  - A “Purpose” section heading (case-insensitive match: `^##?\s*Purpose`).
  - A “How to use” or “Navigation” section heading (case-insensitive match on either).
- If front matter is present, the YAML parses and contains at least `title`.

Suggested regex heuristics (example):
- Heading: `^#\s+.+`
- Purpose: `^##+\s*Purpose\b`
- How to use: `^##+\s*(How to use|Navigation)\b`

Note: Heuristics are guidance; reviewers MAY accept semantically equivalent headings if the intent is clear.

## Minimal Template (Copy/Paste)

```markdown
---
title: "<folder> — <concise domain/context name>"
---

# <folder> — <Concise Domain/Context Name>

Purpose
- Briefly explain what lives here and why (2–8 sentences).
- State how this folder fits the broader domain/bounded context model.

How to use
- Where to start (key files or entrypoints).
- Typical workflows or commands (if any).
- How to extend or contribute.

Conventions
- Front matter usage and required metadata.
- Naming, shapes, linking policy (link canonical artifacts; avoid duplication).

Links
- Up: ../README.md
- Down: ./<child>/README.md
- Canon: link to canonical specs or doctrine if applicable.
```

## Examples in this repository

- `docs/README.md` — top-level documentation domain purpose and navigation.
- `docs/canon/README.md` — canon subdomain purpose and start-here guidance.

## Change Log

- 1.0 — Initial rule publication (minimum README per folder).
