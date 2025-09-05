# Finalize Task — Verify & Divider (v1, CLI-native)

Purpose: Verify acceptance, maintain docs/rules, create divider commit, write final metadata into the plan, and move current/ → done/.
Input: the single plan in docs/project/current/PLAN-*.md.

---

## 0) Pre‑flight

nexoscli dev init --yes
nexoscli dev doctor --ci --checks graph,plugins,config,events,tools --require plugins,graph

---

## 1) Verification (acceptance criteria must hold)

cargo build --workspace
cargo test  --workspace
nexoscli dev docs build
nexoscli dev telemetry report --since 24h

If observability/replay was touched and a bag exists (optional):

# optional if relevant for the plan
nexoscli replay verify --bag <bag-dir> || true

---

## 2) Docs maintenance

- Ensure READMEs and generated docs are in sync.
- Keep plugin docs in their bounded context folder (co‑located with code).
- Re‑run:

nexoscli dev docs build

---

## 3) Rules maintenance

nexoscli rules lint
nexoscli rules export --for human --out docs/RULES.md
nexoscli rules export --for cline --out .clinerules/compiled.md

Add/update rules if the development process or surfaces changed.

---

## 4) ADR/contract check (only if public contracts changed)

- If command names/flags/schemas changed (help vs snapshots differ):
  - Add ADR (docs/ADRs/ADR-YYYYMMDD-*.md) and link in divider commit.

---

## 5) Follow‑ups (frictions/ideas/bugs)

Create small atomic issues for high‑impact items:

nexoscli issue new --title "Follow-up: <short>" --labels '["plan","followup"]' --body "<what/why>"

---

## 6) Divider commit

# Stage changes (if any) and create divider commit; allow empty if nothing changed
git add --all
git commit -m "feat(plan): finalize PLAN-<ID> <slug> (acceptance met)

- Evidence: tests, doctor, replay verify (if applicable)
- Docs: regenerated
- Rules: lint/export OK
- ADR: <id-or-n/a>
- Follow-ups: <ids-or-n/a>" || git commit --allow-empty -m "feat(plan): finalize PLAN-<ID> <slug> (acceptance met)

- Evidence: tests, doctor, replay verify (if applicable)
- Docs: regenerated
- Rules: lint/export OK
- ADR: <id-or-n/a>
- Follow-ups: <ids-or-n/a>"

PLAN_SHA=$(git rev-parse --short HEAD)

---

## 7) Finalize plan (write metadata, end task + bundle, move current → done)

# CLI-native finalize (Rust): writes status=done, implemented_in (HEAD), completed_at;
# ends task + bundles; moves file to docs/project/done/
nexoscli dev plans finalize --output json

# Commit moved plan file (mark as done with divider sha)
git add --all
git commit -m "chore(plans): mark PLAN-<ID> <slug> as done (divider=${PLAN_SHA})" || true

---

## Success checklist

- [ ] Acceptance verified (E2E/doctor/docs).
- [ ] Divider commit created.
- [ ] Plan updated (status=done, implemented_in, completed_at) and moved to done/.
- [ ] Task ended + bundle produced.

---

## Troubleshooting

- Active task prevents starting/resuming or finalize reports task still active:

  Error: Other: task 'TASK-...' is active; pause it before starting/resuming another task

  Resolutions:
  - Pause the current active task:
    nexoscli task pause
  - If needed, end the previous task explicitly (best-effort):
    nexoscli task end --task <TASK_ID> || true
  - Retry finalize:
    nexoscli dev plans finalize --output json

- No plan under current/:
  - Ensure a plan was started via:
    nexoscli dev plans start --output json
  - Or pass a specific path:
    nexoscli dev plans finalize --plan docs/project/current/PLAN-XX_slug.md --output json
