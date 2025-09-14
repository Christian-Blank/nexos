---
schema: readme.v0
title: "NexOS V0"
summary: "Contracts-first CLI with deterministic envelopes, events, and replay."
kind: root
owner: "@core/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
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
  - "./scripts/README.md"
source_of_truth:
  - "docs/ADRs/"
  - "contracts/schemas/"
in_scope:
  - "Repo overview and navigation"
out_of_scope:
  - "Per-folder specifics (see children)"
inputs: []
outputs: []
tags: ["root"]
auto:
  dir_tree_depth: 0
---
# NEXOS: The System That Builds The System

IMPORTANT: This is the v0 version of Nexos, and is replaced through the main version. This is not under active development.

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated children list from front‑matter or glob._
<!-- END: AUTO -->

*An open research experiment in human-assisted AI development*

---

## What if we inverted the paradigm?

Instead of AI assisting humans in writing code, what if humans assisted AI in building systems? Not as a copilot, but through **servant leadership** — where humans lead by serving, guiding, and empowering AI to take the general lead in development.

This is NEXOS: a living experiment in building software where every decision, every event, and every pattern becomes data that drives continuous improvement through **signal intelligence** and **emergence**.

## The Experiment

NEXOS is both a CLI framework and a research platform exploring:

- **Human-Assisted AI Development**: The human oversees, assists, and interacts, but the AI takes the operational lead
- **Data-Driven Developer Experience**: Every command, every event, every trace becomes signal for improvement
- **Emergent Intelligence**: Patterns and strategies emerge from comprehensive observability and deterministic replay
- **Recursive Self-Improvement**: The system that builds systems, understanding and evolving itself

## Core Philosophy: Servant Leadership in Code

Traditional development: Human writes → AI suggests → Human decides

NEXOS paradigm: AI proposes → Human guides → System learns → Patterns emerge

This isn't about replacing human judgment — it's about amplifying it through:

1. **Deterministic Execution**: Every operation can be replayed exactly (`Clock`, `Entropy`, `FilesystemAdapter` providers)
2. **Event Sourcing Everything**: Append-only logs capture complete system history (`.nexoscli/log/events.jsonl`)
3. **Machine-First Interfaces**: JSON contracts are primary, human interfaces are overlays (`--output json` everywhere)
4. **Observable by Default**: Structured spans, correlation IDs, and TracePacks for complete visibility

## Signal Intelligence & Emergence

Every interaction generates signals. Signals become patterns. Patterns drive evolution.

```
Command → Event → Trace → Bundle → Analysis → Pattern → Improvement
         ↑                                                      ↓
         └──────────────── Continuous Loop ────────────────────┘
```

### What We're Learning

- **Task Context Injection**: Active tasks automatically tag all operations, creating rich correlation data
- **Session Correlation**: Granular work tracking within task lifecycles (SES-* IDs)
- **Health Monitoring**: System validates its own invariants continuously (`nexoscli dev doctor`)
- **TracePacks & NEXBags**: Portable bundles of execution artifacts for analysis and replay

### How Patterns Emerge

1. **Event Logs**: Every command appends to an immutable ledger
2. **Content Hashing**: SHA256 ensures integrity and enables deduplication
3. **Correlation Analysis**: Task and session IDs reveal workflow patterns
4. **Replay & Simulation**: Deterministic providers enable perfect reproduction
5. **Graph Traversal**: Knowledge graphs connect issues, plans, and implementations

## Architecture as Philosophy

The technical choices reflect the experiment:

```
Static Plugins → Determinism → Event Sourcing → Observability → Learning
      ↓              ↓              ↓                ↓              ↓
  One Binary    Reproducible    Complete      Structured      Continuous
               Anywhere         History        Insights      Improvement
```

### Key Architectural Patterns

- **Plugin-Based**: Domain-specific functionality in `/plugins/`, composed into single binary
- **Contracts Over Conventions**: Stable JSON schemas, versioned events, explicit interfaces
- **Tighten-Only Contexts**: Child contexts can narrow but not widen constraints
- **Content-Addressed Storage**: Files referenced by SHA256 hash in events
- **Recursive Knowledge Graphs**: Every folder can be a mini-repo with its own context

## Current State: v0 Approaching

The experiment is yielding results:

- ✅ **Phase 15 Complete**: Full observability with session tracking and health validation
- ✅ **49 Tests Passing**: Including session correlation and doctor check validation
- ✅ **Event Integrity**: Hash chains, content verification, rotation support
- ✅ **Task Management**: Complete lifecycle with automatic context injection
- ✅ **Plan Lifecycle**: CLI-native plan management with deterministic transitions
- ✅ **Rules Engine**: Pattern matching and validation across the codebase

Active experiments:
- Cline integration for AI pair programming correlation
- Temporal workflow engine bridge for distributed execution
- Annotation system for enriching historical events

## Let's Move Fast Together: First Principles + Data + System Thinking = Enable -> Accelerate -> Multiply

This is open research. Every commit, every event, every pattern is part of the experiment. Every piece of data is viable to learn the patterns and anti patterns of Syntropy.

Great systems are like bones and tissues: They grow and adapt to the continous needs thriving in the face of an ever changing environment. When they break, they communicate, self organize, and self heal.

### The Experimental Chain

Signal Intelligence & Simulation -> Defy Entropy at Scale -> Make Speed Safe -> Autonomously Heal, Adapt, Improve -> Deterministic Evolution and Emergence

### For Humans

1. Read the [North Star](docs/01-north-star.md) — principles and constraints
2. Explore the [Playbook](docs/PLAYBOOK.md) — how we work and learn
3. Check the [Execution Graph](docs/exec_graph.yaml) — what's active now

### For AI Agents

1. Parse YAML front-matter in markdown files for machine-readable contracts
2. Build DAGs from `exec_graph.yaml` for work item traversal
3. Use `--output json` for all commands to get structured `{ data, errors, meta }` envelopes
4. Analyze `.nexoscli/log/events.jsonl` for patterns and correlations

### Quick Start

```bash
# Build the system
cargo build --workspace

# Initialize a project
./target/debug/nexoscli dev init --yes

# Start a task (automatic context injection)
./target/debug/nexoscli task new --title "Explore emergence"
./target/debug/nexoscli task start --task TASK-*

# Every command now carries context
./target/debug/nexoscli --output json issue new --title "Pattern discovered"

# Check system health
./target/debug/nexoscli dev doctor --ci --profile v0

# Build a TracePack for analysis
./target/debug/nexoscli task bundle --task TASK-* --out ./bundle.tgz
```

### Plan Lifecycle Hygiene (Plan 50)

```bash
./target/debug/nexoscli dev plans verify --output json
./target/debug/nexoscli dev plans index build --output json
./target/debug/nexoscli dev doctor --ci --checks plans --output json
```

## The Vision: Emergence Through Observation

We're not building another CLI tool. We're exploring what happens when:

- Every action becomes data
- Every pattern becomes knowledge
- Every system understands itself
- Every evolution is deterministic

This is software development as a data science problem. This is human-AI collaboration as servant leadership. This is building the system that builds the system.

---

### Technical Documentation

For implementation details and contribution guidelines:

- [CLAUDE.md](CLAUDE.md) — AI agent instructions and patterns
- [CLI Contracts](docs/ADRs/0001-cli-contracts.md) — Core technical contracts
- [Observability Guide](docs/OBSERVABILITY.md) — Tracing and monitoring
- [Plugin Development](plugins/README.md) — Extending the system
- [Event Schemas](docs/EVENTS_SCHEMA.md) — Event structure and versioning

### Repository Structure

```
nexoscli-core/        # Core library with providers and plugin system
nexoscli-bin/         # Main binary composing plugins
plugins/              # Domain-specific functionality
docs/                 # Documentation and specifications
.nexoscli/            # Runtime data (events, tasks, workflows)
```

---

*NEXOS is an experiment in progress. We're learning what emerges when humans and AI build together with complete observability and deterministic execution. Join us in exploring the future of software development.*
