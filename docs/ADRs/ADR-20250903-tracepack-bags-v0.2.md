# ADR-20250903 — NEXBags (TracePack v0.2) surfaces in replay plugin

Status: Accepted  
Date: 2025-09-03  
DRI: Cline  
Scope: replay (plugins/replay/**, docs/**, nexoscli-bin/tests/**)

## Context

Phase 1 delivered a tamper‑evident event ledger (event.v0.3) and a per-run manifest (v0.2). Next step is to produce portable, analyzable bundles (“NEXBags”) akin to ROS bag files, containing:

- Hash-chained events (event.v0.3) with verification
- (Reserved) terminal session artifacts (command records, stdout/stderr checksums, git status, patches)
- Optional file-watch topic
- Normalized Cline tasks within the run time window
- A bag-level manifest for counts, topics, integrity flag, and references

All changes must be deterministic, observable via envelopes, and respect scope fences.

## Decision

Introduce a new command group under the replay plugin:

- nexoscli replay bag build
- nexoscli replay bag verify
- nexoscli replay bag info
- nexoscli replay bag cat

These commands operate on the NEXBag (TracePack v0.2) format (directory or .tgz). No protected core files were changed.

## Command Surfaces

1) bag build

- Args:
  - --run-id <RUN-ID> (required)
  - --out <path> (required; dir or .tgz/.tar.gz)
- Behavior:
  - Stage under .nexoscli/tracepacks/bag-<run>-<ts>/
  - topics/events.ndjson is written from per-run events (preferred) or filtered global logs
  - run_manifest.json snapshot is included if present
  - cline/tasks/* are optionally included by mtime window around [first_event, last_event] ± 300s (best-effort)
  - Integrity verification is run inline and chain_ok emitted in the bag manifest
  - On success, ctx.ok("replay","bag.build", ...) envelope is emitted

2) bag verify

- Args:
  - --bag <path> (dir or .tgz)
- Behavior:
  - Reads topics/events.ndjson and recomputes content_hash (over canonical subset) and chain_hash(prev+content_hash)
  - Emits ctx.ok("replay","bag.verify", {ok, chain_ok, count, first_seq?, last_seq?, issues[]})

3) bag info

- Args:
  - --bag <path>
- Behavior:
  - Reads manifest.json (bag_manifest_v0.2) and returns summary:
    - topics, versions, time_range?, counts
  - Emits ctx.ok("replay","bag.info", ...)

4) bag cat

- Args:
  - --bag <path>, --topic <name> (e.g., events), optional --lines N
- Behavior:
  - Streams a topic file (e.g., topics/events.ndjson) and emits ctx.ok("replay","bag.cat", {ok, topic, lines, bytes})
  - Note: For convenience, `--topic events` implies `topics/events.ndjson`

## Bag Manifest (v0.2)

A new schema is introduced at docs/SPECS/bag_manifest_v0.2.schema.json. Minimal fields:

- version: "0.2"
- created_at (RFC3339), format: "dir"|"tgz"
- counts: { events, term_commands?, files_watch?, cline_tasks? }
- time_range?: { start, end }
- topics: { events: { path: "topics/events.ndjson", schema: "event_v0.3" }, ... }
- chain_ok: boolean
- run_manifest_path?: "run_manifest.json"

Additionally, groundwork schemas are added for future topics:
- docs/SPECS/term_record_v0.1.schema.json (reserved for terminal capture in a future scope)
- docs/SPECS/file_watch_event_v0.1.schema.json (optional topic)

## Determinism and Integrity

- Event integrity follows event.v0.3:
  - content_hash computed over canonical subset: { plugin, command, exit_code, dur_ms, data }
  - chain_hash = sha256(prev + content_hash), prev included and verified
- Stable file layout and canonical JSON serialization for manifests
- Prefer per-run events file to avoid selection ambiguity
- When falling back to global logs, selection filters strictly on run_id

## Observability

All new commands emit JSON envelopes via ctx.ok/ctx.err with actionable fields:
- bag.build: { run_id, format, out, counts, time_range?, chain_ok }
- bag.verify: { ok, chain_ok, count, first_seq?, last_seq?, issues[] }
- bag.info: { ok, format, topics, versions, counts, time_range? }
- bag.cat: { ok, topic, lines, bytes }

## Backwards Compatibility

- Existing replay extract remains intact
- NEXBags are additive; the legacy TracePack directory format is not removed
- docs/TRACEPACK.md updated with NEXBags usage and format

## Alternatives Considered

- Introducing bags under a new top-level plugin (rejected; replay is the natural home)
- Extending extract with verification (accepted as complementary; bag flow is clearer for topicized future)

## Test Plan

E2E tests added under nexoscli-bin/tests/:
- bag_build_verify_info.rs: build dir/tgz, verify integrity, info summary, cat topic
- bag_cline_inclusion.rs: ensures cline/tasks inclusion by time window is present when applicable

All workspace tests pass locally.

## Migration and Future Work

- Terminal capture and file-watch topics are reserved for Phase B (new scope/CP)
- CI/Doctor can gain a check to verify bag integrity over recent runs
- A future “update snapshots” PR should be used if docs/contracts/** snapshots drift (none hand-edited)

## Scope and Compliance

- Edits confined to: plugins/replay/**, docs/**, nexoscli-bin/tests/**
- No changes to protected files or core contract surfaces
- Generators not required (extending existing replay plugin)
