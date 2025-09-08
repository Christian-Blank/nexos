Below is Plan 57 — “AI Changelog v0 (Temporal‑ready)”.
It builds on Plan 56 and delivers our first AI‑enabled workflow that:
	1.	gets the last commit on main,
	2.	reads the changed files (diff‑aware),
	3.	generates a Markdown changelog with OpenAI’s Python SDK,
	4.	writes it to changelog/…md, and
	5.	commits the file back to the repo.

This plan is designed to run locally or through our Temporal bridge without changing core engine contracts.

⸻

1) Scope & success criteria

In scope
	•	A small Python 3.13, typed CLI (adapters/ai-changelog) that talks to OpenAI and emits a changelog.
	•	A thin Rust plugin (plugins/changelog) exposing an activity changelog.ai that shells out to the Python CLI (stable interface for our workflow runner).
	•	A minimal workflow spec WF-ai-changelog that runs one idempotent step to produce & commit the changelog.
	•	Runbooks for local engine and Temporal engine, including idempotency via --build-id git:<sha>.

Success criteria
	•	Single command (local engine): produces changelog/<date>-<sha8>.md and a new commit.
	•	Single command (Temporal engine): envelopes drain, worker runs step, file committed.
	•	Re‑running against the same commit skips (idempotency) once we salt the run with --build-id git:<sha>.
	•	Handles text files; skips obvious binaries; guards large files.

⸻

2) Architecture (v0)

nexoscli workflow run --engine local|temporal
    └─ activity: "changelog.ai"
           └─ plugins/changelog (Rust)
                 └─ spawns adapters/ai-changelog (Python 3.13, OpenAI SDK)
                        ├─ git (discover last commit, list changes, collect diffs)
                        ├─ prompt + model call (Responses API)
                        ├─ write Markdown to changelog/<date>-<sha8>.md
                        └─ git add + commit (message: chore(changelog): <sha8>)

	•	Idempotency model: The workflow plugin salts input_sha with build_id. We standardize on passing --build-id git:<sha> (from git rev-parse) when running the workflow. That makes each commit a distinct idempotency window.
	•	Temporal: unchanged bridge. Outbox → drainer → worker → activity (which runs the Rust plugin → Python CLI).

⸻

3) Repo changes (new files)

Exact file contents are included so you can paste/commit.

3.1 plugins/changelog/Cargo.toml

[package]
name = "nexoscli-plugin-changelog"
version = "0.1.0"
edition = "2021"

[dependencies]
nexoscli-core = { path = "../../nexoscli-core" }
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
clap = "4"
time = { version = "0.3", features = ["formatting"] }

3.2 plugins/changelog/src/lib.rs

use clap::{Arg, ArgMatches, Command as ClapCommand, ValueHint};
use nexoscli_core::{plugin::{Plugin, CommandContext}, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{process::Command, io};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub struct ChangelogPlugin;
impl ChangelogPlugin { pub fn new() -> Self { Self } }

#[derive(Debug, Serialize, Deserialize)]
struct AiOut {
    commit: String,
    files_count: usize,
    out_path: String,
    model: String,
    tokens_input: Option<u32>,
    tokens_output: Option<u32>,
    committed: bool,
}

impl Plugin for ChangelogPlugin {
    fn name(&self) -> &'static str { "changelog" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "AI changelog generation (OpenAI, Python bridge)" }

    fn augment_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
          ClapCommand::new("changelog").about(self.describe())
            .subcommand(
              ClapCommand::new("ai").about("Generate a changelog for the last commit")
                .arg(Arg::new("branch").long("branch").value_hint(ValueHint::Other).default_value("main"))
                .arg(Arg::new("commit").long("commit").value_hint(ValueHint::Other))
                .arg(Arg::new("out-template").long("out-template")
                      .help("Path template; supports {date},{sha8}. Default: changelog/{date}-{sha8}.md")
                      .default_value("changelog/{date}-{sha8}.md"))
                .arg(Arg::new("model").long("model").default_value("gpt-4o-mini"))
                .arg(Arg::new("dry-run").long("dry-run"))
                .arg(Arg::new("max-files").long("max-files").value_parser(clap::value_parser!(u32)).default_value("200"))
                .arg(Arg::new("max-bytes").long("max-bytes").value_parser(clap::value_parser!(u64)).default_value("800000")) // 0.8MB safety
                .arg(Arg::new("openai-base-url").long("openai-base-url").value_hint(ValueHint::Url))
                .arg(Arg::new("cwd").long("cwd").value_hint(ValueHint::DirPath))
            )
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match m.subcommand() {
            Some(("ai", sm)) => {
                let branch = sm.get_one::<String>("branch").unwrap().to_string();
                let commit = sm.get_one::<String>("commit").cloned().unwrap_or_default();
                let out_template = sm.get_one::<String>("out-template").unwrap().to_string();
                let model = sm.get_one::<String>("model").unwrap().to_string();
                let dry = sm.get_flag("dry-run");
                let max_files = *sm.get_one::<u32>("max-files").unwrap();
                let max_bytes = *sm.get_one::<u64>("max-bytes").unwrap();
                let cwd = sm.get_one::<String>("cwd").cloned();

                // Allow overriding the Python entrypoint; default is the installed console script
                let bin = std::env::var("NEXOS_AI_CHANGELOG_BIN").unwrap_or_else(|_| "nexos-ai-changelog".into());

                let mut cmd = Command::new(bin);
                if let Some(d) = &cwd { cmd.current_dir(d); }

                let status = cmd
                    .arg("--branch").arg(&branch)
                    .arg("--commit").arg(&commit)
                    .arg("--out-template").arg(&out_template)
                    .arg("--model").arg(&model)
                    .arg("--max-files").arg(max_files.to_string())
                    .arg("--max-bytes").arg(max_bytes.to_string())
                    .arg(if dry { "--dry-run" } else { "--no-dry-run" })
                    .arg("--json") // force JSON stdout
                    .envs(std::env::vars()) // forward OPENAI_* envs etc.
                    .output();

                match status {
                    Ok(out) => {
                        if !out.status.success() {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            return Ok(ctx.err("changelog","ai","changelog.ai.failed",
                                              &format!("python bridge failed (exit {}): {}\n{}", out.status.code().unwrap_or(-1), stdout, stderr),
                                              Some("check OPENAI_API_KEY and git repo state"),
                                              out.status.code().unwrap_or(1)));
                        }
                        let txt = String::from_utf8_lossy(&out.stdout);
                        let parsed: AiOut = serde_json::from_str(&txt)
                            .map_err(|e| anyhow::anyhow!("bridge JSON parse error: {e}\nstdout:\n{txt}"))?;
                        let now = OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_default();
                        let resp = json!({
                            "at": now,
                            "commit": parsed.commit,
                            "out": parsed.out_path,
                            "files_count": parsed.files_count,
                            "model": parsed.model,
                            "tokens": { "in": parsed.tokens_input, "out": parsed.tokens_output },
                            "committed": parsed.committed,
                            "dry_run": dry
                        });
                        return Ok(ctx.ok("changelog","ai",&resp));
                    }
                    Err(e) => {
                        return Ok(ctx.err("changelog","ai.spawn","changelog.ai.spawn",&format!("spawn error: {}", e),
                                          Some("ensure adapters/ai-changelog is installed (uv/pip)"),
                                          1));
                    }
                }
            }
            _ => Ok(ctx.err("changelog","unknown","changelog.unknown","unknown changelog command", None, 2))
        }
    }
}

Registration: ensure the CLI links this plugin like the others (workspace Cargo.toml and plugin loader).

3.3 adapters/ai-changelog/pyproject.toml

[project]
name = "nexos-ai-changelog"
version = "0.1.0"
description = "Generate commit changelogs from git diffs using OpenAI"
readme = "README.md"
requires-python = ">=3.13"
authors = [{ name = "Nexos" }]
dependencies = [
  "openai>=1.40.0",  # Python SDK (Responses API)
  "typing-extensions>=4.12.0",
]

[project.scripts]
nexos-ai-changelog = "nexos_ai_changelog.cli:main"

[build-system]
requires = ["setuptools>=61.0"]
build-backend = "setuptools.build_meta"

3.4 adapters/ai-changelog/README.md (short)

# nexos-ai-changelog

Small Python 3.13 CLI that:
- finds the last commit on a branch (default `main`)
- collects per-file diffs and statuses
- calls OpenAI (Responses API) to produce a Markdown changelog
- writes to `changelog/<date>-<sha8>.md`
- `git add && git commit -m "chore(changelog): <sha8>"`

Env:
- `OPENAI_API_KEY` (required)
- `OPENAI_BASE_URL` (optional; for proxy/Azure-like endpoints)

Install (dev): `uv pip install -e adapters/ai-changelog`

Run: `nexos-ai-changelog --help`

3.5 adapters/ai-changelog/nexos_ai_changelog/cli.py

from __future__ import annotations
import argparse, os, subprocess, sys, json, textwrap, datetime
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, List, Tuple, Optional

# OpenAI Python SDK (Responses API)
from openai import OpenAI  # Requires openai>=1.40.0

@dataclass
class FileChange:
    status: str  # A,M,D,R,C,..
    path: str
    patch: Optional[str]  # None for D or binary/too-large

def run(cmd: List[str], cwd: Optional[str] = None) -> str:
    p = subprocess.run(cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if p.returncode != 0:
        raise RuntimeError(f"cmd failed ({p.returncode}): {' '.join(cmd)}\n{p.stderr}")
    return p.stdout

def last_commit(branch: str, cwd: Optional[str]) -> str:
    # Use the branch tip; fallback to HEAD if branch missing locally
    try:
        return run(["git", "rev-parse", branch], cwd).strip()
    except Exception:
        return run(["git", "rev-parse", "HEAD"], cwd).strip()

def list_changed_files(commit: str, cwd: Optional[str]) -> List[Tuple[str,str]]:
    # status + path list; includes renames (Rxxx)
    out = run(["git", "diff-tree", "--no-commit-id", "--name-status", "-r", commit], cwd)
    rows = []
    for line in out.splitlines():
        if not line.strip(): continue
        parts = line.split("\t")
        status = parts[0]
        path = parts[-1]  # for Rxxx, last token is new path
        rows.append((status, path))
    return rows

def is_probably_binary(patch: str) -> bool:
    # crude but effective; git shows "Binary files differ" or NULs
    return ("\0" in patch) or ("Binary files" in patch)

def read_patch(commit: str, path: str, cwd: Optional[str], max_bytes: int) -> Optional[str]:
    try:
        patch = run(["git", "show", "--no-color", f"{commit}:{path}"], cwd)
        # That's the full blob; for diff context, ask for diff between parent..commit -- path
        # Use minimal unified context to keep tokens low:
    except Exception:
        patch = ""
    try:
        diff = run(["git", "diff", "--unified=0", "--no-color", f"{commit}~1..{commit}", "--", path], cwd)
    except Exception:
        diff = ""
    body = diff if diff.strip() else patch
    if not body: return None
    if len(body.encode("utf-8", "ignore")) > max_bytes: return None
    if is_probably_binary(body): return None
    return body

def format_prompt(commit: str, files: List[FileChange]) -> str:
    header = textwrap.dedent(f"""
    You are an expert release-notes and changelog assistant.

    Goal: Create a concise, **human-readable Markdown** changelog for commit {commit}.
    Keep it actionable and scoped to this commit.

    Requirements:
    - Start with a one-paragraph **Summary**.
    - Provide **Highlights** (3–8 bullets).
    - Group **Changes by file** (bullets; infer intent, note added/removed/renamed).
    - Call out **Breaking changes** explicitly (if any).
    - Prefer plain language over code dumps; include small inline code where helpful.
    - Keep to ~200–400 words unless the diffs are unusually large.

    Now the diffs (unified=0 when available). Use them, not just filenames.
    """).strip()
    parts = [header, "\n\n<<<BEGIN_DIFFS>>>"]
    for f in files:
        parts.append(f"\n\n# {f.status} {f.path}\n")
        if f.patch:
            parts.append("```diff\n")
            parts.append(f.patch)
            parts.append("\n```\n")
        else:
            parts.append("_(binary/large/no diff content)_\n")
    parts.append("<<<END_DIFFS>>>")
    return "\n".join(parts)

def write_markdown(out_template: str, commit: str, cwd: Optional[str]) -> Path:
    sha8 = commit[:8]
    date = datetime.date.today().isoformat()
    path = out_template.replace("{sha8}", sha8).replace("{date}", date)
    p = Path(cwd or ".").joinpath(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    return p

def main() -> None:
    ap = argparse.ArgumentParser(description="Generate a changelog for the last commit using OpenAI")
    ap.add_argument("--branch", default="main")
    ap.add_argument("--commit", default="")
    ap.add_argument("--out-template", default="changelog/{date}-{sha8}.md")
    ap.add_argument("--model", default="gpt-4o-mini")
    ap.add_argument("--max-files", type=int, default=200)
    ap.add_argument("--max-bytes", type=int, default=800_000)
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--no-dry-run", dest="dry_run", action="store_false")
    ap.add_argument("--cwd", default=None)
    ap.add_argument("--json", action="store_true", help="Emit JSON to stdout (for plugin integration)")
    args = ap.parse_args()

    cwd = args.cwd
    commit = args.commit.strip() or last_commit(args.branch, cwd)
    rows = list_changed_files(commit, cwd)[: args.max_files]

    files: List[FileChange] = []
    for st, path in rows:
        patch = None if st == "D" else read_patch(commit, path, cwd, args.max_bytes)
        files.append(FileChange(status=st, path=path, patch=patch))

    prompt = format_prompt(commit, files)

    # OpenAI client picks up OPENAI_API_KEY; OPENAI_BASE_URL optional (proxy/Azure-like)
    client = OpenAI()  # uses env vars per official SDK

    resp = client.responses.create(model=args.model, input=prompt)  # See OpenAI docs
    md = resp.output_text or ""

    out_path = write_markdown(args.out_template, commit, cwd)
    if not args.dry_run:
        out_path.write_text(md, encoding="utf-8")

        # Commit the file
        try:
            run(["git", "add", str(out_path)], cwd)
            msg = f"chore(changelog): {commit[:8]}"
            run(["git", "commit", "-m", msg], cwd)
            committed = True
        except Exception:
            committed = False
    else:
        committed = False

    result = {
        "commit": commit,
        "files_count": len(files),
        "out_path": str(out_path),
        "model": args.model,
        "tokens_input": getattr(getattr(resp, "usage", None), "input_tokens", None),
        "tokens_output": getattr(getattr(resp, "usage", None), "output_tokens", None),
        "committed": committed,
    }

    if args.json:
        print(json.dumps(result))
    else:
        print(f"Wrote {out_path} (commit {commit[:8]})")
        if committed:
            print("Committed changelog")
        else:
            print("(dry run or commit failed)")

SDK notes: the Python client is used via from openai import OpenAI; client = OpenAI(); client.responses.create(...); print(response.output_text) as shown in OpenAI’s cookbook; the client reads OPENAI_API_KEY from the environment.

3.6 Example workflow spec

.nexoscli/workflows/WF-ai-changelog/spec.yaml

version: "0.1"
id: WF-ai-changelog
name: "AI Changelog v0"
defaults:
  activity_retry: { use_default: true, initial_interval_ms: 500, backoff_coefficient: 2.0, maximum_interval_ms: 30000, maximum_attempts: 0 }
  timeouts: { start_to_close_ms: 60000, schedule_to_close_ms: 300000, heartbeat_ms: 10000 }
run:
  memo: {}
  search_attributes: { task_queue: "nexos" }
  build_id: null        # we will pass --build-id at runtime (git:<sha>)
  change_ids: ["CH_AI_CHANGELOG_V0"]
steps:
  - id: ch1
    activity: "changelog.ai"
    args:
      branch: "main"
      out-template: "changelog/{date}-{sha8}.md"
      model: "gpt-4o-mini"
      max-files: 300
      max-bytes: 1000000
    idempotent: true
    retry: { max_attempts: 3, backoff_ms: 500 }
    timeouts: { start_to_close_ms: 120000 }
    resources: ["git"]


⸻

4) Runbooks

4.1 One‑time setup
	•	Rust: add the new plugins/changelog crate to the workspace and plugin loader (same as other plugins).
	•	Python (dev): uv pip install -e adapters/ai-changelog
	•	OpenAI: set an API key in your shell:

export OPENAI_API_KEY=sk-...   # SDK reads this env var

(The official SDK uses OPENAI_API_KEY from the environment.)

4.2 Local engine (no Temporal)

# Build CLI
cargo build --workspace

# Salt idempotency with the target commit
SHA=$(git rev-parse --short=12 main)

# Run the workflow locally (single step)
./target/debug/nexoscli workflow run \
  --spec .nexoscli/workflows/WF-ai-changelog/spec.yaml \
  --build-id git:$SHA \
  --output json

Expected: file changelog/<today>-<sha8>.md added and committed; subsequent run with the same --build-id skips due to idempotency.

4.3 Temporal engine

Assumes the Temporal drainer/worker from Plan 56 are running.

# Worker (separate terminal)
uv run --project adapters/temporal-worker nexos-temporal-worker

# Drainer (separate terminal)
uv run --project adapters/temporal-worker nexos-temporal-drain

# Ensure the ai-changelog CLI is installed on the same host where the worker runs:
uv pip install -e adapters/ai-changelog

# Salt idempotency with commit
SHA=$(git rev-parse --short=12 main)
export OPENAI_API_KEY=sk-...

# Emit envelopes
./target/debug/nexoscli workflow run \
  --spec .nexoscli/workflows/WF-ai-changelog/spec.yaml \
  --engine temporal \
  --build-id git:$SHA \
  --output json

Expected: envelopes processed → worker runs changelog.ai → Python CLI writes and commits the changelog.

⸻

5) Design details & guardrails
	•	What we summarize: per‑file --unified=0 diffs when available; fall back to blob content for added files; delete/binary/oversized are listed without content to keep tokens safe.
	•	Binary / large file guard: skip anything that looks binary or exceeds --max-bytes (default ~1 MB overall per file content fed to the model).
	•	Model defaults: gpt-4o-mini for cost/latency, configurable via model.
	•	Prompt: steers structure to Summary / Highlights / Changes by file / Breaking changes.
	•	Output path: templated via {date} and {sha8} to keep history tidy and idempotent.
	•	Commit message: chore(changelog): <sha8>.
	•	Idempotency: pass --build-id git:<12-hex> to the workflow run so the step’s input_sha salt changes when the commit changes.
	•	Secrets: only requires OPENAI_API_KEY; optionally OPENAI_BASE_URL if you proxy/route through your own gateway.

⸻

6) Testing & acceptance

6.1 Unit-ish CLI checks

# Dry-run, JSON only (no file written/commit)
nexos-ai-changelog --branch main --dry-run --json | jq .

# Force a specific commit
nexos-ai-changelog --commit $(git rev-parse HEAD) --dry-run --json

Expect: JSON like:

{
  "commit": "abc123...",
  "files_count": 5,
  "out_path": "changelog/2025-09-08-abc12345.md",
  "model": "gpt-4o-mini",
  "tokens_input": 1234,
  "tokens_output": 220,
  "committed": false
}

6.2 Local engine acceptance
	1.	Make a small code change and commit it.
	2.	Run the workflow with --build-id git:<sha>.
	3.	Verify the file exists and a new commit was created.
	4.	Re‑run with the same --build-id → 0 steps run (plan shows idempotent match).
	5.	Make another commit; re‑run with new --build-id → new changelog file and commit.

6.3 Temporal acceptance
	•	With worker/drainer running, repeat steps above with --engine temporal.
	•	Watch Temporal UI: the workflow for WF-ai-changelog should show one activity processed.

⸻

7) Observability & ops
	•	The Python CLI prints nothing (in JSON mode) except the final object; errors propagate to the Rust plugin which returns a structured ctx.err.
	•	Under Temporal, failures surface as activity failures. Replays are safe (idempotent + build_id salt).
	•	If you want traces: the existing OTEL in the dev container can be expanded later; for v0 we keep it simple.

⸻

8) Risks & mitigations
	•	Token blow‑ups: guarded by --max-files and --max-bytes; diffs use unified=0.
	•	Binary files: safely skipped with a placeholder line.
	•	Auth/config drift: CLI purely uses OPENAI_API_KEY/OPENAI_BASE_URL.
	•	Idempotency: requires passing the correct --build-id per commit (documented and automated in runbook).
	•	Temporal schema for Search Attributes: unchanged; our outbox still sets task_queue=nexos.

⸻

9) Follow‑ups (not in v0)
	•	Add --since multi‑commit mode to roll up a range into one changelog.
	•	Store changelogs under changelog/<year>/<month>/… (template supports it).
	•	Optional pre‑commit hook to generate changelog automatically for merge commits.
	•	Add a Rust‑native OpenAI client (or a service) if we later want to avoid Python inside workers.
	•	Wire telemetry (OTEL spans around the Python call and token usage) as part of the telemetry refactor plan.

⸻

10) Why Responses API?

We use the Python SDK’s Responses API because it’s the current, unified interface; it supports simple .responses.create(...) calls and exposes convenience properties like .output_text, all documented in OpenAI’s official cookbook. The SDK picks up the OPENAI_API_KEY from the environment by default.

⸻

TL;DR (what to run)

# Install the Python CLI once
uv pip install -e adapters/ai-changelog

# Build Rust
cargo build --workspace

# Generate changelog for last commit on main (local engine)
SHA=$(git rev-parse --short=12 main)
export OPENAI_API_KEY=sk-...
./target/debug/nexoscli workflow run \
  --spec .nexoscli/workflows/WF-ai-changelog/spec.yaml \
  --build-id git:$SHA \
  --output json
