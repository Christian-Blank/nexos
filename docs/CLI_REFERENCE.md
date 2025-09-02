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
  term         Run commands with provenance (non-interactive)
  issue        Issue management
  task         Tasks: new/start/pause/resume/stage/status/list/end/bundle
  rules        Rules management (init/lint/export)
  cline        Cline task history (locate/import) with idempotent ingest
  bundle       TracePack bundles
  dev          Developer utilities
  demo         Demo plugin
  replay       Replay/extract TracePacks from logged runs
  workflow     Local workflow runner with idempotent activities, signals, timeouts, and visibility
  graph        Graph tools
  help         Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...       Increase verbosity (-v, -vv, -vvv)
      --output <output>  Output format [default: json] [possible values: text, json, yaml]
  -h, --help             Print help

```

## Tool Specs (for agents)

### `workflow.init`

Create a workflow spec template

### `workflow.plan`

Plan (idempotency-aware)

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

### `bundle.build`

Build a TracePack bundle

### `bundle.verify`

Verify a TracePack bundle

### `issue.new`

Create a new local issue

### `issue.list`

List issues (optionally by state)

### `issue.show`

Show a single issue by ID

### `issue.close`

Close an issue by ID

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

### `rules.lint`

Validate rules front-matter

### `rules.export`

Export rules bundle

### `cline.locate`

Find Cline storage locations

### `cline.import`

Import Cline task history with idempotent ingest

### `graph.check`

Validate docs/exec_graph.yaml (acyclic, unique ids)

### `graph.impact`

List nodes impacted (downstream) by the given id

### `demo.hello`

Say hello

