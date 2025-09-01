NEXOS — System CLI Foundation

One CLI to build systems the same way, every time.
Rust core, pluginized domains, deterministic by default, observable end-to-end.
Designed for humans & AI to co-build with low cognitive overhead, tight contracts, and fast, incremental learning.

⸻

Why this exists
	•	You juggle many projects. Context switching and drift kill momentum.
	•	We want one opinionated spine that encodes best practices once, then reuses them everywhere.
	•	The system should be strict (to prevent accidental complexity) and data-driven (to learn quickly by evidence).

North Star (principles & budgets): see docs/01-north-star.md

⸻

What this is (in one page)
	•	A Rust CLI (nexoscli) with a static plugin architecture (one binary; domains = plugins).
	•	Contracts > conventions: stable JSON I/O (--output json) and event logs for replay/analytics.
	•	Determinism & Observability: providers for time/entropy/IO; structured spans (OTel optional).
	•	Recursive graph: root project and any subproject are nodes in the same knowledge graph.
	•	Built to be extended (later adapters/services: Python 3.13/Temporal, Next.js frontends) without changing the core mental model.

CLI contracts (non-negotiables): see ADR-0001 → docs/ADRs/0001-cli-contracts.md

⸻

Architecture at a glance

flowchart LR
  A[nexoscli-bin] --> B[Command tree (clap)]
  A --> C[Event log writer]
  A --> D[Tracing spans]

  subgraph Core
    E[nexoscli-core]
    E --> F[Providers: Clock / Entropy / Io]
    E --> G[Plugin Registry]
  end

  subgraph Plugins (Domains)
    P1[issue (local)]
    P2[dev (generators)]
    P3[agents (future)]
    P4[connectors (future)]
  end

  G --> P1
  G -. feature flags .-> P2
  G -. adapters .-> P3

	•	Static plugins (feature-flagged) → strictness & reproducibility.
	•	Single parse of the composed command tree → correct help/completions/docs.
	•	Event log → .nexoscli/log/events.jsonl (append-only, one JSON per line).

⸻

Process model (how we work)
	•	Atomic features: 1–3 days, ≤ 8 binary acceptance criteria.
	•	Double learning loop after each milestone:
	•	L1 (Feature loop): What value shipped? Evidence? Top 3 frictions → issues.
	•	L2 (Meta loop, evidence-triggered only): Change Proposal → ADR if a contract truly needs to change.
	•	Rule of 3: automate on the 3rd repeat (generators/scaffolds).
	•	Tighten-only: child contexts can narrow budgets; widening requires a Change Proposal → ADR.

Playbook: docs/PLAYBOOK.md

⸻

The knowledge graph (entry points)

Everything is a node with machine-readable front-matter (YAML) + optional prose.
Agents and tools should start here:
	1.	Root North Star → docs/01-north-star.md
	2.	Root Project Brief → docs/00-project-brief.md
	3.	Execution Graph (DAG) → docs/exec_graph.yaml
	4.	Process Artifact Model (human) → docs/meta-model.md
	5.	Process Artifact Model (machine) → meta/MODEL/meta-model.yaml

Domain entry example
	•	Component brief (core CLI): docs/components/nexoscli/brief.md
	•	Atomic issues: issues/atomic/*.md (front-matter includes kind, id, component, acceptance)

Traversal hints (for AI):
	•	Parse files with --- front-matter; treat front-matter as the record and the markdown body as human context.
	•	Build the DAG from docs/exec_graph.yaml. Nodes present there are the authoritative work items.
	•	Follow parent_id and derives_from to walk up; follow depends_on and DAG edges to walk down.
	•	Prefer contracts & acceptance over prose when making decisions.

⸻

Contracts & I/O (for humans and AI)
	•	CLI output: --output json returns one object

{ "data": { /* result */ }, "errors": [], "meta": { "run_id": "…" } }

Text/YAML are renderings only.

	•	Event log: .nexoscli/log/events.jsonl (one JSON per line)
Minimal v0 shape:

{
  "ts": "2025-09-01T12:34:56Z",
  "run_id": "uuid",
  "plugin": "issue",
  "command": "new",
  "args_hash": "sha256:…",
  "content_hash": "sha256:…",
  "exit_code": 0,
  "attrs": { "id": "ISS-0001" }
}


	•	Determinism providers: Clock/Entropy/Io injected; no direct SystemTime::now() or thread_rng() in production paths.

See ADR-0001 for full contract details: docs/ADRs/0001-cli-contracts.md

⸻

Recursive & composable by design

Every bounded context (root project or subfolder app/lib) can be treated like a mini-repo:
	•	Put a one-page README.md with project_brief front-matter in the folder.
	•	Optionally add a child North Star (tighten-only budgets).
	•	If the folder has multiple work items, include a local exec_graph.yaml.
	•	IDs include path prefixes (e.g., apps-nex-cli-…) to avoid collisions.

The global graph is composed from these locals; changes bubble up via evidence and propagate down via impact analysis.

⸻

Current status
	•	Origin Pack v0 in place:
	•	North Star, Project Brief, Execution Graph, ADR-0001, Playbook
	•	First slice (implemented):
	•	Walking skeleton CLI + issue plugin (local) with JSON outputs, event log, and spans
	•	OpenTelemetry integration with Jaeger UI for distributed tracing
	•	JSON Schema export for AI tool integration (nexoscli tools export)
	•	Dev container with full observability stack
	•	Shell completions and man page generation

Check the Execution Graph for the active feature/issue: docs/exec_graph.yaml

⸻

Roadmap (short)
	1.	Milestone A — Foundation (walking skeleton)
Core providers, event log writer, composed Clap, issue new|list|show|close, E2E tests.
	2.	Milestone B — Dev ergonomics
dev init, scaffold plugin <name> (idempotent generators; Plan/Apply).
	3.	Milestone C — First adapter (read-only)
issue import --from github … to validate schemas and mapping.

(Each milestone ends with the double learning loop; meta changes happen only with evidence.)

⸻

Repo layout (high-level)

docs/
  00-project-brief.md        # root brief (front-matter + short prose)
  01-north-star.md           # principles & budgets (singleton)
  exec_graph.yaml            # authoritative DAG of work
  ADRs/0001-cli-contracts.md # CLI I/O, determinism, event log
  PLAYBOOK.md                # working agreements + double loop
  meta-model.md              # human view of process model
  components/nexoscli/brief.md # component contract for core CLI

meta/
  MODEL/meta-model.yaml      # machine view (schemas & invariants)

issues/
  atomic/AI-*.md             # atomic issues (front-matter only is machine-readable)

.nexoscli/
  log/events.jsonl           # append-only event log (created at runtime)


⸻

Quick starts

For humans
	1.	Read the North Star → docs/01-north-star.md
	2.	Skim the Project Brief → docs/00-project-brief.md
	3.	Look at the Execution Graph for what’s active now → docs/exec_graph.yaml

For AI agents
	1.	Parse front-matter of the same three files (above).
	2.	Build an in-memory DAG from exec_graph.yaml.
	3.	Use ADR-0001 to constrain I/O; prefer JSON outputs and event logs as ground truth.

⸻

Glossary (short)
	•	Plugin: a domain module statically linked into the CLI.
	•	Contract: the explicit inputs/outputs/acceptance for a component or command.
	•	Execution Graph: the DAG of features/issues (authoritative plan of record).
	•	Event Log: append-only, machine-readable history for replay/analytics.
	•	Tighten-only: child contexts may narrow budgets/constraints; widening needs a decision (ADR).

⸻

This README is the narrative index. For precise rules and invariants, follow the links to the origin documents and the machine-readable model.
