Patches for updated Plan, we want to use Postgres

Great catch—let’s switch everything over to Postgres for Temporal. Below is a drop‑in update to the plan you pasted, replacing SQLite with a clean Postgres setup and keeping Python 3.13 + uv. I’ve kept the structure identical so you can copy/paste the files.

⸻

1) docs/project/next/PLAN-56_temporal-v0-git-files-to-md.md (updated for Postgres)

---
id: 56
title: "Temporal v0: list files of last commit on main → write Markdown to repo"
status: next
owner: "@temporal/dri"
created: 2025-09-08
depends_on: [53,54,55]
out_of_scope:
  - Committing the generated file (done in Plan 57)
  - Telemetry refactor (follow-up)
goal: >
  Prove end-to-end Temporal integration by running a CLI activity that uses git + filesystem:
  resolve last commit on main, list files, write a Markdown doc back into the repo.
success_metrics:
  - temporal: outbox → drainer → worker → activity path executes successfully
  - artifact: docs/commits/<sha>.md exists with N≥1 lines
  - rerun: idempotent step in workflow skips when inputs unchanged
  - local-only: same command works via local engine (no Temporal)
  - infra: Temporal server persists state in **Postgres** (not SQLite)
deliverables:
  - plugins/git: `git last-commit-files` + `git last-commit-files-md`
  - workflow spec: `.nexoscli/workflows/WF-temporal-v0/spec.yaml`
  - adapters/temporal-worker: Python 3.13 + uv, typed
  - **docker-compose.temporal.postgres.yml**: Postgres + Temporal + UI + worker + drainer (standalone)
acceptance_criteria:
  - [ ] `cargo build` succeeds including `plugins/git`
  - [ ] `./target/debug/nexoscli git last-commit-files --branch main --mode diff` returns JSON with {commit, files[]}
  - [ ] `./target/debug/nexoscli git last-commit-files-md --branch main --out docs/commits/{sha}.md` writes a Markdown file
  - [ ] `./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-temporal-v0/spec.yaml --engine temporal --output json` emits outbox files
  - [ ] `uv run --project adapters/temporal-worker nexos-temporal-drain` + worker produces the Markdown file
  - [ ] Re-run with same inputs is idempotent (workflow shows “idempotent match” for the step)
  - [ ] Temporal runs against **Postgres** and remains healthy across container restarts
plan:
  - A) Add `plugins/git` (two commands; JSON envelopes; ToolSpecs)
  - B) Add workflow spec `WF-temporal-v0`
  - C) Upgrade adapters/temporal-worker to Python 3.13 + uv; keep types
  - D) Supply **docker-compose.temporal.postgres.yml** for Postgres-backed Temporal + UI + worker + drainer
risks:
  - Non-main default branch → mitigated by `--branch` flag, default=main with fallback to HEAD
  - Git not present in PATH inside containers → worker/drainer install `git`
  - File permissions on bind mounts → instruct running compose on repo root with proper UID/GID
  - Postgres boot order → temporal depends_on postgres; healthcheck ensures readiness


⸻

2) plugins/git (unchanged from your plan)

Keep exactly as in the plan you pasted. No DB involvement here.

⸻

3) Minimal workflow spec (unchanged)

.nexoscli/workflows/WF-temporal-v0/spec.yaml — same as in your paste (no DB coupling).

⸻

4) Temporal bridge tweaks (Python 3.13 + uv, typed)

Keep the pyproject.toml change you already had (Python 3.13, typing-extensions, mypy/ruff optional). No DB code changes are needed in worker/drainer—the Temporal server handles storage via Postgres.

⸻

5) Standalone Temporal stack with Postgres (drop-in compose)

Create docker-compose.temporal.postgres.yml at the repo root:

version: "3.8"

services:
  postgres:
    image: postgres:16
    environment:
      - POSTGRES_USER=temporal
      - POSTGRES_PASSWORD=temporal
      - POSTGRES_DB=temporal
    ports:
      - "5432:5432"
    volumes:
      - temporal_pg_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U temporal"]
      interval: 5s
      timeout: 3s
      retries: 30

  temporal:
    image: temporalio/auto-setup:1.23
    environment:
      # Switch from sqlite → **postgresql**
      - DB=postgresql
      - DB_PORT=5432
      - POSTGRES_SEEDS=postgres
      - POSTGRES_USER=temporal
      - POSTGRES_PWD=temporal
      - LOG_LEVEL=info
      # Optional: enable default dynamic config
      - DYNAMIC_CONFIG_FILE_PATH=config/dynamicconfig/development.yaml
    ports:
      - "7233:7233"   # Temporal frontend
    depends_on:
      postgres:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "tctl", "--address", "127.0.0.1:7233", "cluster", "health"]
      interval: 5s
      timeout: 3s
      retries: 30

  temporal-ui:
    image: temporalio/ui:latest
    environment:
      - TEMPORAL_ADDRESS=temporal:7233
    ports:
      - "8233:8080"   # Temporal UI
    depends_on:
      - temporal

  worker:
    image: python:3.13-slim
    working_dir: /workspace
    volumes:
      - ./:/workspace:cached
    environment:
      - TEMPORAL_ADDRESS=temporal:7233
      - TEMPORAL_NAMESPACE=default
      - TEMPORAL_TASK_QUEUE=nexos
      - NEXOSCLI_BIN=/workspace/target/debug/nexoscli
    command: >
      sh -lc "
        set -e
        apt-get update && apt-get install -y --no-install-recommends curl ca-certificates git && rm -rf /var/lib/apt/lists/* &&
        curl -LsSf https://astral.sh/uv/install.sh | sh -s -- --yes --bin-dir /usr/local/bin &&
        uv run --project adapters/temporal-worker nexos-temporal-worker
      "
    depends_on:
      - temporal

  drainer:
    image: python:3.13-slim
    working_dir: /workspace
    volumes:
      - ./:/workspace:cached
    environment:
      - TEMPORAL_ADDRESS=temporal:7233
      - TEMPORAL_NAMESPACE=default
      - TEMPORAL_TASK_QUEUE=nexos
    command: >
      sh -lc "
        set -e
        apt-get update && apt-get install -y --no-install-recommends curl ca-certificates git && rm -rf /var/lib/apt/lists/* &&
        curl -LsSf https://astral.sh/uv/install.sh | sh -s -- --yes --bin-dir /usr/local/bin &&
        uv run --project adapters/temporal-worker nexos-temporal-drain --outbox .nexoscli/engine/outbox
      "
    depends_on:
      - temporal
      - worker

volumes:
  temporal_pg_data:

What changed vs. the SQLite compose
	•	Added a Postgres service with persistent volume temporal_pg_data.
	•	Set DB=postgresql and Postgres connection envs on the Temporal service.
	•	Kept UI, worker, drainer the same, but ensured git is installed and uv is the default Python runner.

Notes
	•	The temporalio/auto-setup image will create both temporal and temporal_visibility DBs as needed using the temporal superuser.
	•	You must have a local nexoscli binary at target/debug/nexoscli before starting the stack (or adjust NEXOSCLI_BIN).

⸻

6) Runbook (Postgres)

# 0) Build the CLI so the worker can shell out to it
cargo build --workspace

# 1) Bring up Postgres + Temporal + UI + worker + drainer
docker compose -f docker-compose.temporal.postgres.yml up -d postgres temporal temporal-ui worker drainer

# 2) Sanity-check Temporal (UI)
#    http://localhost:8233

# 3) Produce outbox envelopes (from repo root, host shell)
./target/debug/nexoscli workflow run \
  --spec .nexoscli/workflows/WF-temporal-v0/spec.yaml \
  --engine temporal \
  --output json

# 4) The drainer consumes envelopes → worker runs activity → Markdown appears
ls docs/commits/

Restart behavior: because Temporal persists to Postgres, docker compose down && up will retain workflow history and state.

⸻

7) PLAN‑57 (AI Changelog) — unchanged, but compatible

Your stub PLAN‑57 stands as‑is; no DB changes required. It’ll run atop the same Temporal+Postgres stack.

⸻

Why Postgres here?
	•	We align with your stack preference and ensure durable, restart‑safe state for Temporal beyond ephemeral dev cycles.
	•	Keeps telemetry concerns separate (we’ll refactor that later per your note), and it doesn’t affect the Python bridge or Rust plugins.

⸻

Quick diff (if helpful)
	•	Replace your previous docker-compose.temporal.yml with the new docker-compose.temporal.postgres.yml above.
	•	Update the PLAN‑56 doc’s Deliverables/Acceptance to explicitly call out Postgres (already reflected).

If you want, I can also prep a ready‑to‑apply patch (git diff) that adds the compose file and updated PLAN‑56 doc.

======= Original Plan ====

Fantastic—this is exactly the kind of “tight, first win” we want for Temporal. Below you’ll find:
	1.	PLAN‑56 (ready to drop into docs/project/next/)
	2.	A minimal Git/FS plugin (plugins/git) that implements the v0 use case
	3.	A sample workflow spec wired to run via Temporal (outbox → drainer → worker → CLI)
	4.	Temporal bridge tweaks to align with Python 3.13 + uv
	5.	A small Temporal‑only compose you can run independently of VS Code
	6.	A stub PLAN‑57 you can park in next/ for the AI changelog follow‑up

Everything below is copy‑pasteable. If a path already exists, treat it as a patch/replace.

⸻

1) docs/project/next/PLAN-56_temporal-v0-git-files-to-md.md

---
id: 56
title: "Temporal v0: list files of last commit on main → write Markdown to repo"
status: next
owner: "@temporal/dri"
created: 2025-09-08
depends_on: [53,54,55]
out_of_scope:
  - Committing the generated file (done in Plan 57)
  - Telemetry refactor (follow-up)
goal: >
  Prove end-to-end Temporal integration by running a CLI activity that uses git + filesystem:
  resolve last commit on main, list files, write a Markdown doc back into the repo.
success_metrics:
  - temporal: outbox → drainer → worker → activity path executes successfully
  - artifact: docs/commits/<sha>.md exists with N≥1 lines
  - rerun: idempotent step in workflow skips when inputs unchanged
  - local-only: same command works via local engine (no Temporal)
deliverables:
  - plugins/git: `git last-commit-files` + `git last-commit-files-md`
  - workflow spec: `.nexoscli/workflows/WF-temporal-v0/spec.yaml`
  - adapters/temporal-worker: Python 3.13 + uv, typed
  - docker-compose.temporal.yml: temporal + ui + worker + drainer (standalone)
acceptance_criteria:
  - [ ] `cargo build` succeeds including `plugins/git`
  - [ ] `./target/debug/nexoscli git last-commit-files --branch main --mode diff` returns JSON with {commit, files[]}
  - [ ] `./target/debug/nexoscli git last-commit-files-md --branch main --out docs/commits/{sha}.md` writes a Markdown file
  - [ ] `./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-temporal-v0/spec.yaml --engine temporal --output json` emits outbox files
  - [ ] `uv run --project adapters/temporal-worker nexos-temporal-drain` + worker produces the Markdown file
  - [ ] Re-run with same inputs is idempotent (workflow shows “idempotent match” for the step)
plan:
  - A) Add `plugins/git` (two commands; JSON envelopes; ToolSpecs)
  - B) Add workflow spec `WF-temporal-v0`
  - C) Upgrade adapters/temporal-worker to Python 3.13 + uv; keep types
  - D) Supply `docker-compose.temporal.yml` for Temporal + UI + worker + drainer
risks:
  - Non-main default branch → mitigated by `--branch` flag, default=main with fallback to HEAD
  - Git not present in PATH inside containers → document `NEXOSCLI_BIN` and mount repo into worker
  - File permissions on bind mounts → instruct running compose on repo root with proper UID/GID


⸻

2) New plugin: plugins/git (Git + FS activity)

What it does
	•	git last-commit-files → discover the tip commit of --branch (default main, fallback to HEAD) and list files
	•	--mode diff (default): files changed in the last commit
	•	--mode snapshot: all files in that commit’s tree
	•	git last-commit-files-md → same, then writes docs/commits/{sha}.md (templated with {sha}) with a bullet list of paths.

plugins/git/Cargo.toml

[package]
name = "nexoscli-plugin-git"
version = "0.1.0"
edition = "2021"

[dependencies]
nexoscli-core = { path = "../../nexoscli-core" }
anyhow        = { workspace = true }
serde         = { workspace = true, features = ["derive"] }
serde_json    = { workspace = true }
schemars      = { workspace = true }
clap          = { workspace = true }
time          = { workspace = true }
walkdir       = { workspace = true }

plugins/git/README.md

# git plugin

Git + filesystem helpers for workflows.

## Commands

- `git last-commit-files [--branch main] [--mode diff|snapshot]`
- `git last-commit-files-md [--branch main] [--mode diff|snapshot] [--out PATH] [--header TEXT] [--prefix TEXT]`

### Examples

```bash
# JSON list of files changed in last commit on main
nexoscli git last-commit-files --branch main --mode diff --output json

# Write Markdown under docs/commits/<sha>.md (default)
nexoscli git last-commit-files-md --branch main --mode diff --output json

Output (success envelope):

{
  "branch": "main",
  "commit": "abcd1234...",
  "mode": "diff",
  "count": 7,
  "files": ["Cargo.toml","src/lib.rs", "..."],
  "out": "docs/commits/commit-abcd1234.md"
}

**`plugins/git/src/lib.rs`**
```rust
use clap::{Arg, ArgMatches, Command as ClapCommand, ValueHint};
use nexoscli_core::{plugin::{Plugin, ToolSpec, CommandContext}, Result};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fs, path::{Path, PathBuf}, process::Command as Proc};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub struct GitPlugin;
impl GitPlugin { pub fn new() -> Self { Self } }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct FilesOut {
    branch: String,
    commit: String,
    mode: String,
    count: usize,
    files: Vec<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    out: Option<String>,
    generated_at: String,
}

impl Plugin for GitPlugin {
    fn name(&self) -> &'static str { "git" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "Git helpers: list last-commit files and write Markdown" }

    fn augment_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
            ClapCommand::new("git").about(self.describe())
                .subcommand(
                    ClapCommand::new("last-commit-files")
                        .about("List files in the last commit on a branch (diff or full snapshot)")
                        .arg(Arg::new("branch").long("branch").value_hint(ValueHint::Other).default_value("main"))
                        .arg(Arg::new("mode").long("mode").value_parser(["diff","snapshot"]).default_value("diff"))
                )
                .subcommand(
                    ClapCommand::new("last-commit-files-md")
                        .about("Write a Markdown file with the file list of the last commit")
                        .arg(Arg::new("branch").long("branch").default_value("main"))
                        .arg(Arg::new("mode").long("mode").value_parser(["diff","snapshot"]).default_value("diff"))
                        .arg(Arg::new("out").long("out").value_hint(ValueHint::FilePath)
                            .help("Output path; supports {sha}. Default: docs/commits/commit-{sha}.md"))
                        .arg(Arg::new("header").long("header").value_hint(ValueHint::Other)
                            .help("Optional H1 header. Supports {sha} and {branch}."))
                        .arg(Arg::new("prefix").long("prefix").value_hint(ValueHint::Other)
                            .help("Optional bullet prefix before each path, e.g., './'"))
                )
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match m.subcommand() {
            Some(("last-commit-files", sm)) => {
                let branch = sm.get_one::<String>("branch").unwrap().to_string();
                let mode = sm.get_one::<String>("mode").unwrap().to_string();
                let (commit, files) = last_commit_files(&branch, &mode)
                    .map_err(|e| anyhow::anyhow!(format!("git error: {e}")))?;
                let out = FilesOut {
                    branch, commit, mode,
                    count: files.len(), files,
                    out: None, generated_at: now(),
                };
                Ok(ctx.ok("git","last-commit-files",&out))
            }
            Some(("last-commit-files-md", sm)) => {
                let branch = sm.get_one::<String>("branch").unwrap().to_string();
                let mode   = sm.get_one::<String>("mode").unwrap().to_string();
                let header = sm.get_one::<String>("header").cloned();
                let prefix = sm.get_one::<String>("prefix").cloned().unwrap_or_default();
                let (commit, files) = last_commit_files(&branch, &mode)
                    .map_err(|e| anyhow::anyhow!(format!("git error: {e}")))?;
                let out_path = compute_out_path(
                    sm.get_one::<String>("out").map(|s| s.as_str()),
                    &commit
                );
                ensure_parent(out_path.parent().unwrap())?;
                let mut body = String::new();
                let h1 = header.unwrap_or_else(|| format!("Files {} in commit `{}` on `{}`",
                    if mode=="snapshot" {"present"} else {"changed"}, &commit[..8], branch));
                body.push_str(&format!("# {h1}\n\n"));
                body.push_str(&format!("_Generated: {}_\n\n", now()));
                for p in &files {
                    body.push_str(&format!("- {}{}\n", prefix, p));
                }
                fs::write(&out_path, body.as_bytes())?;
                let out = FilesOut {
                    branch, commit, mode,
                    count: files.len(), files,
                    out: Some(path_str(&out_path)), generated_at: now(),
                };
                Ok(ctx.ok("git","last-commit-files-md",&out))
            }
            _ => Ok(ctx.err("git","unknown","git.unknown",
                 "unknown git command", Some("use: last-commit-files | last-commit-files-md"), 2)),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "git.last-commit-files".into(),
                about: "List files in the last commit (diff or snapshot)".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(FilesOut),
            },
            ToolSpec {
                name: "git.last-commit-files-md".into(),
                about: "Write Markdown file with file paths from last commit".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(FilesOut),
            }
        ]
    }
}

/* --- helpers --- */

fn now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn ensure_parent(dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dir)?;
    Ok(())
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

fn compute_out_path(out_cli: Option<&str>, sha: &str) -> PathBuf {
    let sha8 = &sha[..8.min(sha.len())];
    let default = format!("docs/commits/commit-{}.md", sha8);
    let base = out_cli.unwrap_or(&default).to_string()
        .replace("{sha}", sha8);
    PathBuf::from(base)
}

fn git(args: &[&str]) -> anyhow::Result<String> {
    let out = Proc::new("git").args(args).output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        anyhow::bail!(format!("git {:?} failed: {}", args, err))
    }
}

fn resolve_commit(branch: &str) -> anyhow::Result<(String, String)> {
    // Try branch, fallback to HEAD
    let mut used = branch.to_string();
    let sha = match git(&["rev-parse", "--verify", branch]) {
        Ok(s) => s,
        Err(_) => {
            used = "HEAD".into();
            git(&["rev-parse", "--verify", "HEAD"])?
        }
    };
    Ok((used, sha))
}

fn last_commit_files(branch: &str, mode: &str) -> anyhow::Result<(String, Vec<String>)> {
    let (_used, sha) = resolve_commit(branch)?;
    let raw = if mode == "snapshot" {
        git(&["ls-tree", "-r", "--name-only", &sha])?
    } else {
        git(&["diff-tree", "--no-commit-id", "--name-only", "-r", &sha])?
    };
    let files = raw
        .lines()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    Ok((sha, files))
}

Workspace addition: add plugins/git to your root Cargo workspace members if needed.

⸻

3) Minimal workflow spec (Temporal engine)

Create: .nexoscli/workflows/WF-temporal-v0/spec.yaml

version: "0.1"
id: WF-temporal-v0
name: "Temporal v0: last-commit files → Markdown"
run:
  build_id: "build-56"
  change_ids: ["CH_PLAN_56"]
  memo: {}
  search_attributes: {"task_queue":"nexos"}
defaults:
  activity_retry: { use_default: true, initial_interval_ms: 500, backoff_coefficient: 2.0, maximum_interval_ms: 5000, maximum_attempts: 0 }
  timeouts: { start_to_close_ms: 15000, schedule_to_close_ms: 60000, heartbeat_ms: 5000 }
steps:
  - id: list-and-write
    activity: "git.last-commit-files-md"
    args:
      branch: "main"
      mode: "diff"            # or "snapshot"
      out: "docs/commits/commit-{sha}.md"
      header: "Files changed in {sha} on {branch}"
      prefix: ""
    idempotent: true
    retry: { max_attempts: 2, backoff_ms: 300 }
    timeouts: {}
    resources: ["git","fs"]

Runbook (local first, then Temporal)

# Build all
cargo build --workspace

# Smoke locally
./target/debug/nexoscli git last-commit-files --branch main --mode diff --output json
./target/debug/nexoscli git last-commit-files-md --branch main --mode diff --output json
# => docs/commits/commit-<sha>.md

# Emit Temporal envelopes
./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-temporal-v0/spec.yaml --engine temporal --output json
# => .nexoscli/engine/outbox/*.json

# In another terminal: run worker + drain (see sections below)


⸻

4) Temporal bridge: Python 3.13 + uv + typing (small edits)

Update adapters/temporal-worker/pyproject.toml to Python 3.13 and pin tooling:

[project]
name = "nexos-temporal-bridge"
version = "0.1.0"
description = "Nexos Temporal Bridge: worker and outbox drainer"
readme = "README.md"
requires-python = ">=3.13,<3.14"
authors = [{ name = "Nexos" }]
dependencies = [
  "temporalio>=1.7.0",
  "typing-extensions>=4.12.2",
]

[project.optional-dependencies]
dev = ["mypy>=1.10", "ruff>=0.5.0"]

[tool.mypy]
python_version = "3.13"
strict = true
warn_unused_ignores = true
warn_redundant_casts = true
disallow_untyped_defs = true
check_untyped_defs = true

[tool.ruff]
target-version = "py313"

[project.scripts]
nexos-temporal-worker = "worker:main"
nexos-temporal-drain = "drain_outbox:main"

[build-system]
requires = ["setuptools>=61.0"]
build-backend = "setuptools.build_meta"

The code you already have (worker.py, drain_outbox.py) is typed; this mostly bumps versions and enables strict mypy if you run uv run --project adapters/temporal-worker mypy adapters/temporal-worker.

⸻

5) Standalone Temporal stack (no VS Code needed)

Create docker-compose.temporal.yml at repo root:

version: "3.8"

services:
  temporal:
    image: temporalio/auto-setup:1.23
    environment:
      - DB=sqlite
      - LOG_LEVEL=info
    ports:
      - "7233:7233"   # Temporal frontend
    healthcheck:
      test: ["CMD", "tctl", "--address", "127.0.0.1:7233", "cluster", "health"]
      interval: 5s
      timeout: 3s
      retries: 30

  temporal-ui:
    image: temporalio/ui:latest
    environment:
      - TEMPORAL_ADDRESS=temporal:7233
    depends_on:
      - temporal
    ports:
      - "8233:8080"   # UI

  worker:
    image: python:3.13-slim
    working_dir: /workspace
    volumes:
      - ./:/workspace:cached
    environment:
      - TEMPORAL_ADDRESS=temporal:7233
      - TEMPORAL_NAMESPACE=default
      - TEMPORAL_TASK_QUEUE=nexos
      - NEXOSCLI_BIN=/workspace/target/debug/nexoscli
    command: >
      sh -lc "
        pip install --no-cache-dir -e adapters/temporal-worker &&
        nexos-temporal-worker
      "
    depends_on:
      - temporal

  drainer:
    image: python:3.13-slim
    working_dir: /workspace
    volumes:
      - ./:/workspace:cached
    environment:
      - TEMPORAL_ADDRESS=temporal:7233
      - TEMPORAL_NAMESPACE=default
      - TEMPORAL_TASK_QUEUE=nexos
    command: >
      sh -lc "
        pip install --no-cache-dir -e adapters/temporal-worker &&
        nexos-temporal-drain --outbox .nexoscli/engine/outbox
      "
    depends_on:
      - temporal
      - worker

Use it

# Terminal A (repo root)
docker compose -f docker-compose.temporal.yml up -d temporal temporal-ui worker drainer

# Terminal B (repo root)
./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-temporal-v0/spec.yaml --engine temporal --output json

# Watch the file appear
ls docs/commits/
# UI: http://localhost:8233

If you prefer uv inside containers, replace the pip install commands with:
sh -lc "curl -LsSf https://astral.sh/uv/install.sh | sh -s -- --yes --bin-dir /usr/local/bin && uv run --project adapters/temporal-worker nexos-temporal-worker"
(same idea for the drainer).

⸻

6) PLAN‑57 (stub): AI changelog via OpenAI SDK

Drop this stub now so it’s visible on the board.

docs/project/next/PLAN-57_temporal-v1-ai-changelog.md

---
id: 57
title: "Temporal v1: AI changelog (OpenAI SDK) from last commit"
status: next
owner: "@ai/dri"
depends_on: [56]
goal: >
  Read all files participating in the last commit on main, summarize diffs into a concise CHANGELOG entry,
  write `changelog/<YYYY-MM-DD>_<sha8>.md`, and create a git commit with that new file.
acceptance_criteria:
  - [ ] CLI/Activity produces a deterministic changelog file path with {date} and {sha}
  - [ ] Summaries fit ≤ 500 tokens, bulletized by file, with a single higher-level paragraph at top
  - [ ] File is written to `changelog/` and committed with message "chore(changelog): <sha8>"
  - [ ] End-to-end Temporal path green (outbox→drain→worker→CLI)
non_goals:
  - Multi-commit rollups
  - Release notes formatting (follow-up)
design_sketch:
  - New Python 3.13 module `adapters/ai-changelog/` (uv project) with CLI `nexos-ai-changelog`
    - args: branch, mode (diff), out, openai_model, max_tokens
    - uses `git show <sha>:<path>` to read blob contents at commit time for context
    - OpenAI SDK call with temperature ~0.2, system prompt tuned for changelog
  - Command bridged into a workflow step: `activity: "term.run"` or a small Rust plugin shim `ai.changelog`


⸻

Verification checklist (for Plan‑56)
	•	plugins/git compiles and shows in ./target/debug/nexoscli git --help
	•	Local run writes docs/commits/commit-<sha8>.md with a bullet per file
	•	.nexoscli/workflows/WF-temporal-v0/spec.yaml exists and validates via workflow validate
	•	workflow run --engine temporal emits outbox files
	•	Worker + drainer move envelopes and produce the Markdown
	•	Re-running with same args yields idempotent skip in workflow history

⸻

Notes & rationale
	•	Why a dedicated git plugin? It gives us agent-grade, contract-first operations instead of free-form shell, and it composes cleanly as a workflow activity (git.last-commit-files-md).
	•	Two modes (diff/snapshot) cover both interpretations of “files in the last commit.” Defaulting to diff matches typical intent.
	•	Python 3.13 + uv aligns with your stack; we stuck to type hints and strict mypy to keep the bridge maintainable.
	•	Temporal compose runs without devcontainer or telemetry; telemetry refactor stays a follow-up as requested.