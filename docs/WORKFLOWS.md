# Core Workflows (v0)

## Plan Lifecycle (CLI-native)

Manage plans deterministically with the dev plugin — do not hand-edit plan front‑matter or move plan files manually.

Quick start:
```
nexoscli dev init --yes
nexoscli dev plans start --output json
# implement work…
nexoscli dev plans finalize --output json
```

What happens:
- Start: creates + starts a task, updates front‑matter (status=in-progress, plan_id, slug, task_id, started_at, source=next), moves next → current
- Finalize: records implemented_in (HEAD), writes completed_at + status=done, ends task + bundle, moves current → done

Troubleshooting:
- If an active task blocks start:
  - Pause: `nexoscli task pause`
  - Or end: `nexoscli task end --task <ID> || true`
  - Retry start: `nexoscli dev plans start --output json`

References:
- docs/project/README.md — plan folders and commands
- docs/prompts/cline/start_new_task.workflow.md
- docs/prompts/cline/finalize_task.workflow.md

## 1) After each effort — Rule Sweep (10–15 min)

**Goal**: Keep the system simple and accurate by adding/changing/removing rules based on evidence.

**Steps**
1. Run telemetry: `nexoscli dev telemetry report --since 7d --output table`  
   – Note recurring errors (top error codes) and slow commands (p90).
2. Run doctor: `nexoscli dev doctor --ci --checks rules,cline,plugins,graph --output json`  
   – Ensure green. If not, open issues; don't bypass.
3. If evidence suggests a rule change:
   - Copy `rules/templates/rule.md` → `rules/pack/<area>/RULE-<slug>.md`
   - Write the rule in 1 screen; include *Intent*, *Rule*, *Enforcement*, *Exceptions*.
   - Link the principle and add owner (DRI).
4. Validate & publish:
   - `nexoscli rules lint`
   - `nexoscli rules export --for human`
   - `nexoscli rules export --for cline`
   - Commit with message: `rules: add RULE-<slug> (evidence: …)`

**Expected Duration**: 10-15 minutes  
**Frequency**: After each significant effort or milestone  
**Evidence**: Telemetry data, doctor checks, recurring friction patterns

## 2) Rule Creation Template

When creating new rules, follow this structure:

### Template Location
Copy from: `rules/templates/rule.md`
Create at: `rules/pack/<category>/RULE-<slug>.md`

### Required Front-matter
```yaml
---
id: RULE-<slug>
title: "<short imperative rule>"
audience: human | agent | cline
scope: global | plugin:<name> | path:<glob> | workflow:<name>
status: Active | Deprecated
version: 0.1.0
last_reviewed: 2025-09-01T00:00:00Z
principle: "SSOT | Contracts over intentions"
owner: "rules-dri@example.com"
---
```

### Required Sections
1. **Intent**: Why this rule exists (2–4 sentences)
2. **Rule**: Precise checklist of requirements
3. **Enforcement**: How/where it's checked (gates, evidence)
4. **Exceptions**: When to bypass and what to do

## 3) Rule Review Cycle

**Monthly**: Review all rules with `last_reviewed` > 30 days old
- Update `last_reviewed` timestamp
- Check if rule is still relevant
- Update based on new evidence
- Deprecate unused rules (set `status: Deprecated`)

**Evidence-Driven Changes**:
- Use `nexoscli dev telemetry report` for patterns
- Use `nexoscli dev doctor --ci` for compliance
- Document reasoning in commit messages

## 4) Documentation Sync

**Before releases**:
1. `nexoscli dev docs build` — Generate CLI reference and rules docs
2. `nexoscli rules export --for human` — Update human-readable rules
3. `nexoscli rules export --for cline` — Update AI/agent rules
4. Verify all exports are current and accurate

**Integration Points**:
- CI checks include rules validation
- Documentation builds are automated
- Rules are version-controlled with evidence

---

*This workflow ensures rules evolve based on evidence rather than assumptions, maintaining system coherence while enabling rapid iteration.*
