Fantastic. Here’s a ready‑to‑commit user guide that explains the observability + replay system end‑to‑end, shows how to find and build bags, and gives practical Python recipes (pandas‑friendly) to analyze them—no SDK required.

Suggested path: docs/USER_GUIDE_OBSERVABILITY.md

⸻

Observability & Replay — User Guide

This guide shows how NEXOS captures your work, how to package it into a portable TracePack/NEXBag, and how to analyze those bags with simple Python (pandas). No custom SDK required.

What you get
	•	Tamper‑evident event history (hash‑chained JSONL).
	•	Terminal command capture (stdout/stderr, duration, git pre/post status, patches).
	•	Cline tasks (normalized, redacted).
	•	(Optional) File‑watch events (live file edits while you work).
	•	A single bag you can share or archive—like a ROS bag for dev workflows.

⸻

Quickstart (TL;DR)
	1.	Record your work

# Start a task (adds context to events)
nexoscli task start --task TASK-2025xxxx-slice

# Run commands through the CLI so they’re captured
nexoscli term run -- 'cargo test --workspace'
nexoscli term run -- 'git status'

	2.	Find a run_id

# Each CLI invocation has a run manifest
ls .nexoscli/log/runs/*.manifest.json
# or: jq -r '.run_id' .nexoscli/log/runs/*.manifest.json | tail -n1

	3.	Build a bag (directory or .tgz)

RUN=RUN-...                   # pick one from step 2
nexoscli replay bag build \
  --run-id "$RUN" \
  --include events,term,patches,status,cline \
  --out tracepack.tgz

	4.	Verify & inspect

nexoscli replay bag verify --in tracepack.tgz
nexoscli replay bag info   --in tracepack.tgz --output json

	5.	Analyze in Python (no SDK)

import tarfile, io, json, pandas as pd

# Load events.ndjson from a .tgz bag into a DataFrame
def df_from_tar_ndjson(tgz_path, member_path):
    rows = []
    with tarfile.open(tgz_path, 'r:gz') as tar:
        f = tar.extractfile(member_path)
        for line in io.TextIOWrapper(f, encoding='utf-8'):
            rows.append(json.loads(line))
    return pd.DataFrame(rows)

events = df_from_tar_ndjson('tracepack.tgz', 'tracepack/topics/events.ndjson')
events.head()


⸻

Concepts & terms (2 minutes)
	•	Event: one CLI command execution (plugin + command). Events are written to .nexoscli/log/events.jsonl and to per‑run files under .nexoscli/log/runs/. They’re hash‑chained (tamper‑evident).
	•	Terminal record: output of nexoscli term run -- '<cmd>' with stdout/stderr, timing, git status and patch. Indexed at topics/term/commands.ndjson inside a bag.
	•	TracePack/NEXBag: a portable snapshot of selected “topics” (events, term, patches, status, cline, …) + a manifest.
	•	Run ID: unique execution key for one CLI command. Used to build a bag for that run.

⸻

Where things live (during development)
	•	Event log (global): .nexoscli/log/events.jsonl
	•	Runs: .nexoscli/log/runs/<RUN_ID>.jsonl and <RUN_ID>.manifest.json
	•	Terminal outputs: .nexoscli/term/seq-XXXX-stdout.txt & seq-XXXX-stderr.txt
	•	Git status snapshots: .nexoscli/status/seq-XXXX-pre.status & ...post.status
	•	Patches: .nexoscli/patches/seq-XXXX-<slug>.patch
	•	Cline tasks (normalized): .nexoscli/cline/tasks/*.json

When you build a bag, a clean copy of this data is assembled under a single root (dir or tarball).

⸻

Build, verify, and inspect a bag

Build

# Minimal (events only)
nexoscli replay bag build --run-id RUN-... --include events --out tracepack

# Full (recommended)
nexoscli replay bag build --run-id RUN-... \
  --include events,term,patches,status,cline \
  --out tracepack.tgz

--out can be a directory or a .tgz. If you include cline, tasks within the run time window (±5 minutes) are included.

Verify

# Validates the hash chain and manifest
nexoscli replay bag verify --in tracepack.tgz

Info & cat

# JSON summary: counts, topics, time range
nexoscli replay bag info --in tracepack.tgz --output json

# Print a topic to stdout (for quick grepping/jq)
nexoscli replay bag cat --in tracepack.tgz --topic topics/events.ndjson


⸻

Bag layout (directory form)

tracepack/
  manifest.json                 # bag (nexbag v0.2)
  run_manifest.json             # per-run summary/anchor
  topics/
    events.ndjson               # event.v0.3 lines (hash-chained)
    term/commands.ndjson        # term_record.v0.1 lines (one per shell cmd)
    files/watch.ndjson          # (optional) file_watch_event.v0.1 lines
  patches/
    seq-0001-<slug>.patch
    ...
  status/
    seq-0001-pre.status
    seq-0001-post.status
  cline/tasks/
    2025-09-01T12-00-00Z-task_123.json
    ...
  README.md

Schemas

For exact fields, see:
	•	docs/SPECS/event_v0.3.schema.json
	•	docs/SPECS/term_record_v0.1.schema.json
	•	docs/SPECS/file_watch_event_v0.1.schema.json
	•	docs/SPECS/bag_manifest_v0.2.schema.json

Each JSON line in topics includes a "schema": "..." field. If the schema is bumped in the future, branch your logic on that field.

⸻

Working with topics (shell & jq)
	•	List all plugins used in a run:

jq -r '.plugin' tracepack/topics/events.ndjson | sort | uniq -c | sort -nr


	•	Find failures (exit_code != 0):

jq -r 'select(.exit_code != 0) | [.ts,.plugin,.command,.exit_code] | @tsv' \
  tracepack/topics/events.ndjson


	•	Top slow commands (>5s):

jq -r 'select(.dur_ms > 5000) | [.ts,.plugin,.command,.dur_ms] | @tsv' \
  tracepack/topics/events.ndjson | sort -k4 -nr



⸻

Python analysis (pandas recipes)

These snippets work with both directory bags and .tgz bags. Choose the loader you need.

Utilities

import os, io, json, tarfile, pandas as pd
from pathlib import Path

def df_from_ndjson(path: str) -> pd.DataFrame:
    rows = []
    with open(path, 'r', encoding='utf-8') as f:
        for line in f:
            rows.append(json.loads(line))
    return pd.DataFrame(rows)

def df_from_tar_ndjson(tgz_path: str, member_path: str) -> pd.DataFrame:
    rows = []
    with tarfile.open(tgz_path, 'r:gz') as tar:
        f = tar.extractfile(member_path)  # e.g. 'tracepack/topics/events.ndjson'
        for line in io.TextIOWrapper(f, encoding='utf-8'):
            rows.append(json.loads(line))
    return pd.DataFrame(rows)

Load events & terminal records

# If you extracted to a directory:
events = df_from_ndjson("tracepack/topics/events.ndjson")
term   = df_from_ndjson("tracepack/topics/term/commands.ndjson")

# OR read from a tarball
events = df_from_tar_ndjson("tracepack.tgz", "tracepack/topics/events.ndjson")
term   = df_from_tar_ndjson("tracepack.tgz", "tracepack/topics/term/commands.ndjson")

# Parse timestamps (optional)
events['ts'] = pd.to_datetime(events['ts'], utc=True, errors='coerce')
term['ts']   = pd.to_datetime(term['ts'],   utc=True, errors='coerce')

events.head(), term.head()

Common questions

1) What failed? How often by plugin/command?

fail = events.query("exit_code != 0")
fail_count = (fail
  .groupby(['plugin','command'])['run_id']
  .count()
  .sort_values(ascending=False)
  .rename('failures'))
fail_count.head(10)

2) What’s slow (>5s), by command?

slow = events[events['dur_ms'] > 5000]
slow_summary = (slow
  .groupby(['plugin','command'])['dur_ms']
  .agg(['count','mean','max'])
  .sort_values('mean', ascending=False))
slow_summary.head(10)

3) Correlate terminal outputs with events

events and term both carry run_id and (typically) a seq. If seq differs, join by nearest timestamp.

# If seq aligns:
merged = pd.merge(term, events, on=['run_id','seq'], suffixes=('_term','_ev'), how='left')

# Or nearest timestamp join (1s tolerance window example)
term_sorted = term.sort_values('ts')
events_sorted = events.sort_values('ts')
merged = pd.merge_asof(term_sorted, events_sorted,
                       on='ts', by='run_id',
                       direction='nearest',
                       tolerance=pd.Timedelta('1s'))
merged[['ts','cmd','plugin','command','exit_code_ev','dur_ms_ev']].head()

4) Which commands changed files the most?

Terminal records reference a patch file per command. Compute approximate patch “size” by file size on disk.

from pathlib import Path
import tarfile

def patch_bytes_dir(row, bag_root="tracepack"):
    p = Path(bag_root) / row['files']['patch']
    return p.stat().st_size if p.exists() else 0

def patch_bytes_tgz(row, tgz="tracepack.tgz"):
    path = row['files']['patch']
    with tarfile.open(tgz, 'r:gz') as tar:
        try:
            return tar.getmember(f"tracepack/{path}").size
        except KeyError:
            return 0

# Directory
term['patch_bytes'] = term.apply(patch_bytes_dir, axis=1)

# Tarball
# term['patch_bytes'] = term.apply(lambda r: patch_bytes_tgz(r, "tracepack.tgz"), axis=1)

term.sort_values('patch_bytes', ascending=False)[['ts','cmd','patch_bytes']].head(10)

5) Basic chain‑of‑custody check (validate hash chain in Python)

import hashlib

def sha256_hex(b: bytes) -> str:
    return "sha256:" + hashlib.sha256(b).hexdigest()

def verify_chain(df):
    ok = True
    prev = ""
    for _, row in df.sort_values('seq').iterrows():
        # compute content_hash from a canonical subset (plugin, command, exit_code, dur_ms)
        # note: the CLI already verified this when building the bag; this is illustrative
        minimal = {'plugin': row['plugin'], 'command': row['command'],
                   'exit_code': row['exit_code'], 'dur_ms': row['dur_ms']}
        ch = sha256_hex(json.dumps(minimal, sort_keys=True).encode('utf-8'))
        chained = sha256_hex((prev + ch).encode('utf-8'))
        if ch != row['content_hash'] or chained != row['chain_hash']:
            ok = False; break
        prev = chained
    return ok

verify_chain(events)

6) Cline correlation: tasks per run window

Each bag includes normalized Cline tasks within the run’s time window (±5m). Count and timeline:

# Read all cline tasks from tar
def read_cline_tasks_tgz(tgz="tracepack.tgz"):
    rows = []
    with tarfile.open(tgz, 'r:gz') as tar:
        for m in tar.getmembers():
            if m.name.startswith("tracepack/cline/tasks/") and m.name.endswith(".json"):
                with tar.extractfile(m) as f:
                    rows.append(json.loads(f.read()))
    return pd.DataFrame(rows)

cline = read_cline_tasks_tgz("tracepack.tgz")
cline['created_at'] = pd.to_datetime(cline['created_at'], utc=True, errors='coerce')
cline[['id','created_at','messages']].sort_values('created_at').head()


⸻

Reproducing & forensics
	•	Apply patches on a throwaway branch to understand changes:

git checkout -b replay-test
for p in tracepack/patches/seq-*.patch; do
  git apply --check "$p" && git apply "$p"
done
# Review
git log --oneline replay-test ^main
git diff main..replay-test

	•	Inspect pre/post git status for each command:
	•	tracepack/status/seq-0001-pre.status
	•	tracepack/status/seq-0001-post.status
	•	Re‑run failed commands safely via term run to capture fresh evidence.

⸻

Best practices
	•	Prefer nexoscli term run -- '<cmd>' for shell actions (durations, output digests, patches).
	•	Start a task before working: nexoscli task start --task ... to get context in events/bags.
	•	Import recent Cline context periodically: nexoscli cline import --since 24h.
	•	Build a bag at significant milestones or on failures.
	•	Keep bags next to PRs/issues for traceable reviews.

⸻

Troubleshooting
	•	Bag verify fails
Ensure the bag wasn’t modified. Rebuild:

nexoscli replay bag build --run-id RUN-... --out tracepack.tgz


	•	No patches generated
The command may not have changed tracked files or ran outside a git repo. Verify git status output.
	•	Empty Cline folder
Import tasks first:

nexoscli cline locate
nexoscli cline import --since 7d


	•	Large bags
Use selective --include flags (e.g., omit files/watch) or build a directory bag and compress externally.

⸻

Privacy & redaction
	•	Cline imports auto‑redact tokens/secrets.
	•	Terminal logs can include command output—avoid printing secrets.
	•	Review bags before sharing; they are designed for local analysis unless explicitly published.

⸻

Versioning
	•	Each line in topics includes "schema": "..." (e.g., event.v0.3).
	•	Bag manifest includes "version": "0.2", "format": "nexbag".
	•	If schemas evolve, branch analysis on row['schema'].

⸻

FAQ

Q: Do I need a Python SDK?
No. Bags are JSON/NDJSON. Use the Python snippets here (pandas + stdlib).

Q: Can I analyze directly from a .tgz?
Yes—see df_from_tar_ndjson above.

Q: Can I include only events?
Yes: --include events. Add more topics as needed.

⸻

Appendix — CLI reference (observability slice)

# Show recent runs / telemetry (optional)
nexoscli dev telemetry report --since 24h --output json

# Build a bag
nexoscli replay bag build --run-id RUN-... \
  --include events,term,patches,status,cline \
  --out tracepack.tgz

# Verify, info, cat
nexoscli replay bag verify --in tracepack.tgz
nexoscli replay bag info   --in tracepack.tgz --output json
nexoscli replay bag cat    --in tracepack.tgz --topic topics/events.ndjson

# Capture terminal commands with evidence
nexoscli term run -- 'cargo test --workspace'

# Import Cline context
nexoscli cline locate
nexoscli cline import --since 24h


⸻

That’s it. With this guide, anyone receiving a bag can verify it, inspect its contents, and analyze it using straightforward Python/pandas without any custom dependencies.