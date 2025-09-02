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
  bundle       TracePack bundles
  graph        Graph tools
  issue        Issue management
  dev          Developer utilities
  cline        Cline task history (locate/import) with idempotent ingest
  demo         Demo plugin
  rules        Rules management (init/lint/export)
  replay       Replay/extract TracePacks from logged runs
  task         Tasks: new/start/pause/resume/stage/status/list/end/bundle
  workflow     Local workflow runner with idempotent activities, signals, timeouts, and visibility
  term         Run commands with provenance (non-interactive)
  help         Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...       Increase verbosity (-v, -vv, -vvv)
      --output <output>  Output format [default: json] [possible values: text, json, yaml]
  -h, --help             Print help

```

## Tool Specs (for agents)

### `issue.new`

Create a new local issue

### `issue.list`

List issues (optionally by state)

### `issue.show`

Show a single issue by ID

### `issue.close`

Close an issue by ID

### `cline.locate`

Find Cline storage locations

### `cline.import`

Import Cline task history with idempotent ingest

### `rules.lint`

Validate rules front-matter

### `rules.export`

Export rules bundle

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

### `graph.check`

Validate docs/exec_graph.yaml (acyclic, unique ids)

### `graph.impact`

List nodes impacted (downstream) by the given id

### `demo.hello`

Say hello

### `bundle.build`

Build a TracePack bundle

### `bundle.verify`

Verify a TracePack bundle

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

