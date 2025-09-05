# Rules for cline

## RULE-use-term-run — Run shell commands via `nexoscli term run`


- Prefer: `nexoscli term run -- <cmd>` to capture `dur_ms`, `exit_code`, and output digests.
- Exceptions: interactive TTY or long-running servers; justify in the plan.

