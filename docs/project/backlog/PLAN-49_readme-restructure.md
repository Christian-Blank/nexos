Below is a self‑contained, top‑down README system v0 you can hand to a human DRI or run with Cline. It’s deliberately simple, deterministic, and enforceable. It defines rules, front‑matter schema, file conventions, templates, and workflows (human + LLM). It also sketches how per‑folder configs inherit from a repo policy while staying easy to reason about.

⸻

README System v0 — Deterministic, Graph‑Linked, DRI‑Operated

Scope: README.md files only.
Goals: fast orientation, bounded context, minimal drift, deterministic updates, graph navigation, enforceable rules.

⸻

1) Mental model
	•	The repository is a tree of documentation nodes. Every “significant” folder (domain) has a README that states its purpose, boundaries, interfaces, and links to children.
	•	Top‑down: The root README.md holds global context; each child README narrows scope; leaf READMEs focus on their folder’s specifics.
	•	Graph: Parent ↔ child links are explicit and derive from front‑matter, not ad‑hoc markdown. A machine can reconstruct the graph from READMEs alone.
	•	Determinism: Each README mixes manual sections (human‑authored) and auto blocks (machine‑generated) delimited by markers so tools can update them without touching manual text.

⸻

2) Non‑negotiable rules (v0)
	1.	Presence: All “major” folders must contain a README.md.
	2.	Front‑matter: Every README must start with YAML front‑matter conforming to readme.v0.
	3.	Graph links: Parent(s) and children must be declared in front‑matter (explicit or via a folder‑level rule).
	4.	Boundaries: Each README must include Purpose, In‑scope, Out‑of‑scope, Interfaces (Inputs/Outputs).
	5.	Canonicality: If any other document conflicts with a README, the source_of_truth declared in the README wins.
	6.	Auto blocks: Machine‑generated content lives only between <!-- BEGIN: AUTO (id=…) --> and <!-- END: AUTO -->. No human edits inside.
	7.	Staleness SLO: last_reviewed must be ≤ 45 days old for non‑leaf domains and ≤ 90 days for leaf folders. (Warn at 30/60 days.)
	8.	No code in docs: READMEs may link to code; they must not contain runnable code except small fenced snippets (policy‑bounded).
	9.	One concern: A README describes its folder. Cross‑domain content belongs to the nearest shared parent.

These are minimal; they keep drift low and tooling simple.

⸻

3) Front‑matter schema (readme.v0)

Each README starts with:

---
schema: readme.v0
title: "<Short human title>"
summary: "<Single sentence purpose>"
kind: root|domain|leaf
owner: "@team/handle"          # DRI or team
status: live|draft|deprecated
version: "0.1.0"               # bump on material changes
last_reviewed: "2025-09-05T00:00:00Z"

# Graph
parents:
  - "../README.md"             # relative paths to parent README(s)
children:
  # either explicit children (stable deterministic list)…
  - "./subdomain-a/README.md"
  - "./subdomain-b/README.md"
  # … or omit children and rely on auto rule (see policy/config)

# Canonical sources (truth)
source_of_truth:
  - "contracts/schemas/"       # directories or files that are authoritative
  - "docs/ADRs/"

# Scope and interfaces (brief, details go in body)
in_scope:
  - "What this folder owns/does"
out_of_scope:
  - "What belongs elsewhere"
inputs:
  - "Upstream inputs (files, events, APIs)"
outputs:
  - "Downstream outputs (files, events, APIs)"

# Tags, optional
tags: [ "docs", "architecture", "cli" ]

# Auto block configuration (optional; can also live in per-folder .readme.yaml)
auto:
  children_glob: ["*/README.md"]   # direct subdirs by default
  dir_tree_depth: 1                # 0=none
  badges: true
---

Why this set? It’s the smallest useful surface to: build the graph, enforce staleness, show ownership, and keep scope clean.

⸻

4) File conventions
	•	Path: …/<folder>/README.md (always this exact casing).
	•	Markers: Auto blocks must use:

<!-- BEGIN: AUTO (id=children) -->
…generated content…
<!-- END: AUTO -->

id is a stable token per block (e.g., children, dir-tree, badges).

	•	Linking: Use relative links to other READMEs (../README.md) for rename resilience.
	•	Length: Keep top sections concise. Put deep detail behind headings or link downwards.

⸻

5) Templates (copy/paste)

5.1 Root README template

---
schema: readme.v0
title: "NexOS"
summary: "Contracts-first CLI with deterministic envelopes, events, and replay."
kind: root
owner: "@core/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-05T00:00:00Z"
parents: []
children:
  - "./docs/README.md"
  - "./plugins/README.md"
  - "./nexoscli-core/README.md"
  - "./nexoscli-bin/README.md"
  - "./rules/README.md"
  - "./examples/README.md"
  - "./contracts/README.md"
  - "./adapters/README.md"
source_of_truth:
  - "docs/ADRs/"
  - "contracts/schemas/"
in_scope:
  - "Repo overview and navigation"
out_of_scope:
  - "Per-folder specifics (see children)"
inputs: []
outputs: []
tags: [ "root" ]
auto:
  dir_tree_depth: 0
---

# NexOS — Overview

Brief overview (what, why, north star). Link to **ADRs**, **CLI reference**, and **playbooks**.

## Quick links

- Contracts → `contracts/README.md`
- Plugins → `plugins/README.md`
- Rules → `rules/README.md`

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated children list from front‑matter or glob._
<!-- END: AUTO -->

5.2 Domain README template (e.g., plugins/README.md)

---
schema: readme.v0
title: "Plugins/"
summary: "Extensible CLI functionality grouped by plugin crates."
kind: domain
owner: "@cli/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-05T00:00:00Z"
parents: ["../README.md"]
children: []                        # leave empty to use auto.children_glob
source_of_truth:
  - "../nexoscli-bin/Cargo.toml"
  - "./*/Cargo.toml"
in_scope:
  - "Plugin crates and their READMEs"
out_of_scope:
  - "Core contract types (see nexoscli-core)"
inputs:
  - "Workspace Cargo configuration"
outputs:
  - "Registered commands in nexoscli-bin"
tags: [ "plugins", "rust" ]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
---

# Plugins/

What lives here, how plugins are registered, the rules to add a new plugin.

## Required structure

- `plugins/<name>/Cargo.toml`
- `plugins/<name>/src/lib.rs`
- `plugins/<name>/README.md`

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated: list of plugin READMEs with short descriptions._
<!-- END: AUTO -->

## Interfaces

- **Inputs:** workspace manifests
- **Outputs:** exported tool specs, commands

5.3 Leaf README template (e.g., plugins/task/README.md)

---
schema: readme.v0
title: "task plugin"
summary: "Task lifecycle: new/start/pause/resume/stage/status/list/end/bundle."
kind: leaf
owner: "@plugins/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-05T00:00:00Z"
parents: ["../README.md", "../../README.md"]
children: []
source_of_truth:
  - "./src/lib.rs"
  - "./README.md"        # this file
in_scope:
  - "User-facing behavior and CLI surface of task plugin"
out_of_scope:
  - "Workflow engine internals (see plugins/workflow)"
inputs:
  - "Events log"
outputs:
  - ".nexoscli/tasks/*, envelopes"
tags: [ "plugin", "tasks" ]
auto: { }
---

# task plugin

What it does, key commands, and links to tests.

## Commands

- `nexoscli task new …`
- `nexoscli task start …`

## Interfaces

- **Inputs:** …
- **Outputs:** …


⸻

6) Per‑folder config & inheritance
	•	Repo policy (root): .nexoscli/readme.policy.yaml
	•	Folder override (optional): .readme.yaml in any folder with a README.

Policy fields (v0):

version: v0.1
default_kind: domain                   # used when not set
default_auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
  badges: true

# Which folders must have READMEs (enforced)
required_folders:
  - "docs"
  - "plugins"
  - "nexoscli-core"
  - "nexoscli-bin"
  - "rules"
  - "examples"
  - "contracts"
  - "adapters"

# Staleness SLOs (days)
slo:
  domain_last_reviewed_days: 45
  leaf_last_reviewed_days: 90

# Lint rules (simple booleans)
lint:
  require_frontmatter: true
  require_scope_sections: true         # Purpose/In/Out/Interfaces
  require_graph_links: true

Folder override .readme.yaml example (plugins/):

# plugins/.readme.yaml
title: "Plugins/"
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
tags: ["plugins", "rust"]

Resolution order: For any README, values come from: front‑matter > .readme.yaml > repo policy defaults.

Radical candor: keep overrides sparse. If every folder is special, nothing is standard. Prefer policy → small overrides → explicit front‑matter only when it truly differs.

⸻

7) Authoring workflows (human & Cline)

7.1 Create a new folder README (deterministic)
	1.	Decide location and confirm it’s a domain or leaf.
	2.	Copy template (domain or leaf). Fill title, summary, owner, kind.
	3.	Set graph:
	•	parents: relative links (always include the immediate parent folder’s README).
	•	children: leave empty for auto discovery, or list explicitly (stable ordering).
	4.	Fill scope sections: Purpose, In‑scope, Out‑of‑scope, Interfaces.
	5.	Add auto blocks as needed:
	•	children list block
	•	Dir tree: <!-- BEGIN: AUTO (id=dir-tree) -->…<!-- END: AUTO -->
	6.	Run checks:
nexoscli dev repo check --fix-readmes --output json (should pass or only warn).
	7.	Commit message: docs(readme): add <path> README (owner @team).

Acceptance criteria:
	•	Front‑matter validates, parent links resolve, dev repo check passes, SLO fields filled.

7.2 Update an existing README
	1.	Read the front‑matter fields; bump version if user‑visible behavior or scope changed.
	2.	Update manual text only outside auto markers.
	3.	If you changed structure (moved folders), update parents/children.
	4.	Update last_reviewed to current time if you materially reviewed all sections.
	5.	Run nexoscli dev repo check and scan the diff for only intentional changes.

Acceptance criteria:
	•	No auto block touched manually; graph still connected; checks pass.

7.3 Move/rename a folder
	1.	Move folder via git.
	2.	Fix relative links in parent’s children and the moved folder’s parents.
	3.	Search for any other README linking to the old path (optional).
	4.	Run checks. Commit with both code and README changes together.

7.4 Weekly DRI hygiene loop
	•	nexoscli dev repo check — confirm structure.
	•	List stale READMEs by SLO (script or soon CLI): inspect and refresh.
	•	Spot check: each domain has Purpose/Scope/Interfaces.
	•	Tie to PR gate (soft warning) and monthly hard gate for SLO.

⸻

8) Cline / LLM execution recipe

Give this as the system prompt or .workflow.md to Cline.

Allowed tools/commands:
	•	cat, ls, git status, git diff
	•	nexoscli dev repo check --output json
	•	Only write within README.md of the assigned folder (and the parent’s README if instructed).

Guardrails:
	•	Scope is the assigned folder (and its parent for linking). Do not edit other files.
	•	Do not change text inside <!-- BEGIN: AUTO … --> blocks.
	•	Always preserve and update front‑matter fields as needed.
	•	Resolve all relative links locally (verify files exist).
	•	Output a final change summary (“what/why/acceptance”).

Procedure:
	1.	Read repo policy .nexoscli/readme.policy.yaml and local .readme.yaml (if present).
	2.	If README missing → create from template; else update.
	3.	Ensure front‑matter adheres to readme.v0.
	4.	If children is empty, insert/refresh an AUTO children block listing all */README.md (one level) in stable sorted order.
	5.	If configured, insert a dir tree block to configured depth.
	6.	Update Purpose/In/Out/Interfaces wording for clarity; keep concise.
	7.	Set/refresh last_reviewed.
	8.	Run nexoscli dev repo check and report any errors you didn’t cause.
	9.	Print a final, minimal diff (or summary) and stop.

Rubric (self‑check before saving):
	•	Front‑matter present, valid, and minimal.
	•	Parent link(s) valid and relative.
	•	Children links present (auto or explicit) and valid.
	•	Scope sections concise and correct.
	•	No edits inside AUTO blocks.
	•	last_reviewed updated if materially reviewed.

⸻

9) How the graph is constructed (deterministically)
	•	Nodes: every README with schema: readme.v0.
	•	Edges: parents and children fields (if both are present, they should agree; the parent’s children is canonical if conflicts arise).
	•	Defaults: If children empty and auto.children_glob applies, the tool resolves children = glob matches, sorted lexicographically.
	•	Uniqueness: Node id: readme::<repo>::<relative_path>. Renames change id; that’s OK (we’re not enforcing immutable ids v0).

⸻

10) Enforcement & checks (tie‑in with your CLI)

Today (already in tree):
	•	nexoscli dev repo check
Ensures required READMEs exist; complements it to also validate:
	•	required front‑matter keys present (schema,title,kind,owner,status,last_reviewed)
	•	graph links resolve (optional: add in a follow‑up)
	•	no code leaked into docs/ (already enforced)

Near‑term small additions (optional):
	•	nexoscli dev readme verify [--path .]
Validates front‑matter schema and AUTO markers only (no writes).
	•	nexoscli dev readme build [--path .]
Refreshes AUTO blocks deterministically (children list, dir tree).
	•	nexoscli dev readme stale --days 45
Lists READMEs exceeding SLO for quick DRIs action.

These can be incremental, but the rules, templates, and workflows above already work with your current dev repo check.

⸻

11) “Radical candor” on design choices
	•	Front‑matter vs. central registry: Putting graph edges in front‑matter keeps ownership local and supports monorepo splits. A central registry tends to drift and create a single choke point. Trade‑off: you must keep parent/child lists aligned—so we declare parent → children is canonical on conflict.
	•	Auto blocks over full generation: Mixing manual with generated blocks avoids brittle “full regeneration” diffs. It lets DRIs craft clear prose while tools keep indices accurate.
	•	Per‑folder overrides: Powerful but easy to abuse. Keep .readme.yaml tiny. If you find yourself adding many overrides, reconsider the policy defaults.
	•	Staleness SLOs: These are essential; stale READMEs cause more damage than missing ones because they look trustworthy. Alerts + simple cadence prevent rot.
	•	No sacred cows: If later we need cross‑cutting catalogs (e.g., “All engines”), build a query view from the README graph—don’t shove aggregate lists into READMEs themselves.

⸻

12) Quick start checklist for the DRI
	1.	At repo root, add .nexoscli/readme.policy.yaml (sample above).
	2.	Create/refresh root README from template.
	3.	For each major folder in policy, create a README from domain template.
	4.	For plugin/leaf folders, create a README from leaf template.
	5.	Run: nexoscli dev repo check --fix-readmes.
	6.	Open a PR titled: docs(readmes): bootstrap v0 graph (policy + templates).

⸻

13) Example: converting an existing area (plugins/)
	1.	Add plugins/.readme.yaml with children_glob: ["*/README.md"].
	2.	Create plugins/README.md from domain template (leave children: []).
	3.	For each plugin folder missing README, create from leaf template.
	4.	Refresh AUTO block (manually for now; tooling can come next).
	5.	Run checks and commit.

⸻

14) What “good” looks like (heuristics)
	•	You can land anywhere in the repo and, by following parents, reach the root in ≤ 3 clicks.
	•	Every README’s Purpose explains “why this folder exists” in ≤ 2 lines.
	•	In/Out scope reads like an interface contract, not a novel.
	•	Children section is accurate without manual curation.
	•	You can see owner, last review, and status at a glance.

⸻

If you’d like, I can generate the initial root + domain + leaf READMEs for your current tree using the templates above (with your exact folder names) and hand you a ready‑to‑commit patch.