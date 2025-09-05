---
id: RULE-post-effort-rule-sweep
title: "After each effort, run a Rule Sweep"
audience: human
scope: workflow:post-effort
status: Active
version: 0.1.0
last_reviewed: 2025-09-01T00:00:00Z
principle: "Emergent governance"
owner: "rules-dri@example.com"
---
**Run**: `nexoscli dev telemetry report --since 7d` and `nexoscli dev doctor --ci`.  
If recurring errors or friction appear, add/change/deprecate a rule via `rules/templates/rule.md`.  
Open a proposal in `rules/proposals/CP-YYYYMMDD-*.md` and link acceptance/evidence.
