---
id: RULE-cline-import
title: "Keep Cline tasks mirrored"
audience: agent
scope: plugin:cline
status: Active
version: 0.1.0
last_reviewed: 2025-09-01T00:00:00Z
principle: "SSOT, Replayability"
owner: "rules-dri@example.com"
---
- Nightly (or after major efforts): `nexoscli cline import --since last`.
- Ensure `dev doctor --ci --checks cline` is green; if not, mount storage or set `NEXOS_CLINE_STORAGE`.
