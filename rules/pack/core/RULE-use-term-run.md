---
id: RULE-use-term-run
title: "Run shell commands via `nexoscli term run`"
audience: cline
scope: global
status: Active
version: 0.1.0
last_reviewed: 2025-09-01T00:00:00Z
principle: "Observability, Determinism"
owner: "rules-dri@example.com"
---
- Prefer: `nexoscli term run -- <cmd>` to capture `dur_ms`, `exit_code`, and output digests.
- Exceptions: interactive TTY or long-running servers; justify in the plan.
