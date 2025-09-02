# TracePack

This bundle captures a single `nexoscli` run for analysis and replay.

## Layout
- `manifest.json` — metadata (counts, git range, per-file hashes)
- `events.ndjson` — newline-delimited events for the run (schema v0.2)
- `patches/` — git patches captured by `term run` (seq-####-*.patch)
- `status/`  — `git status --porcelain` snapshots (seq-####-*.status)

## Notes
- All relative paths are within this bundle.
- Apply patches **only** on a throwaway branch. Use `replay plan/run` for safety.
