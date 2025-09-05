# NEXOS CLI Reference (generated)

## Top-level Help

```
System CLI with plugin architecture

Usage: nexoscli [OPTIONS] [COMMAND]

Commands:
  version      Show version
  plugins      List registered plugins
  tools        Tools & schemas
  completions  Generate shell completions
  man          Print man page to stdout
  issue        Issue management
  dev          Developer utilities
  task         Tasks: new/start/pause/resume/stage/status/list/end/bundle
  rules        Rules management (init/lint/export)
  workflow     Local workflow runner with idempotent activities, signals, timeouts, and visibility
  term         Run commands with provenance (non-interactive)
  bundle       TracePack bundles
  demo         Demo plugin
  cline        Cline task history (locate/import) with idempotent ingest
  graph        Graph tools
  replay       Replay/extract TracePacks from logged runs
  help         Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...       Increase verbosity (-v, -vv, -vvv)
      --output <output>  Output format [default: json] [possible values: text, json, yaml]
  -h, --help             Print help

```

## Tool Specs (for agents)

### `rules.lint`

Validate rules front-matter

### `rules.export`

Export rules bundle

### `dev.init`

Initialize .nexoscli/ (Plan/Apply, idempotent)

### `dev.plugin.new`

Scaffold a new plugin and optionally register it

### `dev.plugins.check`

Verify plugin folder structure and registration

### `dev.doctor`

Verify local setup (dotdir, event log write, config, OTEL envs); --ci aggregates checks

### `dev.telemetry.report`

Aggregate events.jsonl → counts, p50/p90/avg, error leaderboard

### `dev.plans.start`

Start a plan: create+start task, update front-matter, move next→current

### `dev.plans.finalize`

Finalize a plan: mark done, end task, bundle, move current→done

### `dev.plans.list`

List plans in docs/project/{next,current,done,archived}

### `dev.plans.status`

Show plan front-matter and derived ids

### `dev.plans.next`

Show the next plan candidate (docs/project/next)

### `replay.bag.verify`

Verify a TracePack/NEXBag integrity; checks events stats/digests and optional rotation index continuity

### `replay.verify`

Verify a TracePack (dir or .tgz) and explain findings

### `replay.normalize`

Normalize events to v0.3 (from bag or events file)

### `replay.schema.events`

JSON Schema for events v0.3

### `replay.schema.list`

List available schemas

### `replay.schema.get`

Get schema JSON

### `replay.validate`

Validate bag artifacts (events/fileops) against schemas

### `workflow.init`

Create a workflow spec template

### `workflow.plan`

Plan (idempotency-aware)

### `workflow.validate`

Validate a workflow spec (schema + invariants)

### `workflow.run`

Run a workflow spec sequentially

### `workflow.status`

Show effects, history, visibility

### `workflow.signal`

Send a signal to a running workflow

### `workflow.query`

Query workflow runtime state

### `workflow.list`

List workflows (visibility index)

### `workflow.upsert-sa`

Upsert search attributes for a workflow

### `graph.check`

Validate docs/exec_graph.yaml (acyclic, unique ids)

### `graph.impact`

List nodes impacted (downstream) by the given id

### `cline.locate`

Find Cline storage locations

### `cline.import`

Import Cline task history with idempotent ingest

### `issue.new`

Create a new local issue

### `issue.list`

List issues (optionally by state)

### `issue.show`

Show a single issue by ID

### `issue.close`

Close an issue by ID

### `demo.hello`

Say hello

### `bundle.build`

Build a TracePack bundle

### `bundle.verify`

Verify a TracePack bundle

