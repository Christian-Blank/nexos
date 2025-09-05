# Start New Task — Next → Current (v0)

> Purpose: Move the top plan from `docs/project/next/` to `current/`, create & start a task immediately, write its task_id into the plan front‑matter, and implement with tight guardrails.  
> Input: none (selects first `PLAN-*.md` under `docs/project/next/`).  
> Output: one implementation commit (the divider commit happens in the finalize workflow).

---

## 0) Pre‑flight (required)

- Read:
  - @/.clinerules/compiled.md
  - @/docs/PLAYBOOK.md

- Run:
  ```bash
  cargo run -p nexoscli -- dev init --yes
  cargo run -p nexoscli -- dev doctor --ci --checks graph,plugins,config,events,tools --require plugins,graph
  ```

- Choose a scope label (e.g., workflow-plugin, dev-plugin, task-plugin, observability, cline-ingest). Respect .clinerules/scope.json. If you must go beyond scope: stop, open CP/ADR.

---

## 1) Pick plan with the lowest number & extract ids

```bash
mkdir -p docs/project/{next,current,done,archived}

PLAN=$(ls -1 docs/project/next/PLAN-*.md | head -n1)
test -n "$PLAN" || { echo "No plan in docs/project/next"; exit 1; }

BASENAME=$(basename "$PLAN")
PLAN_ID=$(echo "$BASENAME" | sed -E 's/^PLAN-([0-9]+)_.*/\1/')
PLAN_SLUG=$(echo "$BASENAME" | sed -E 's/^PLAN-[0-9]+_(.+)\.md/\1/')
```

---

## 2) Create + start task first (ensure full correlation)

Create task using plan title (H1) if present, else slug; then start the task.

```bash
# Create task from plan title or slug
TITLE=$(grep -m1 '^# ' "$PLAN" | sed 's/^# *//') || true
if [ -z "$TITLE" ]; then TITLE="$PLAN_SLUG"; fi

TASK_JSON=$(cargo run -p nexoscli -- task new --title "$TITLE" --output json)
TASK_ID=$(echo "$TASK_JSON" | jq -r '.data.task_id // .data.id // .meta.context.task_id // empty')
test -n "$TASK_ID" || { echo "Could not extract TASK_ID"; echo "$TASK_JSON"; exit 1; }

# Start task now for proper correlation
cargo run -p nexoscli -- task start --task "$TASK_ID"
```

Why now? Starting early ensures all subsequent commands carry {task_id, session_id} in events/envelopes.

---

## 3) Plan front‑matter + move to current/

- Merge front‑matter (create if missing): status=in-progress, plan_id, slug, task_id, started_at.

```bash
NOW=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
python3 - <<'PY'
from pathlib import Path
import sys, yaml

p = Path(sys.argv[1])
plan_id, slug, task_id, now = sys.argv[2:]

txt = p.read_text()
fm, body = {}, txt
if txt.startswith('---'):
    _, rest = txt.split('---', 1)
    head, body = (rest.split('---', 1) if '---' in rest else (rest, ''))
    fm = yaml.safe_load(head) or {}

fm.update({
  "status": "in-progress",
  "plan_id": plan_id,
  "slug": slug,
  "task_id": task_id,
  "started_at": now,
  "source": "next"
})

out = '---\n' + yaml.safe_dump(fm, sort_keys=False) + '---\n' + body.lstrip()
p.write_text(out)
PY
"$PLAN" "$PLAN_ID" "$PLAN_SLUG" "$TASK_ID" "$NOW"

git mv "$PLAN" "docs/project/current/$BASENAME"
```

---

## 4) Implementation loop (minimal diffs, high determinism)

- Read the plan fully (context + acceptance criteria).
- Prefer generators over hand‑rolled; keep diffs ≤ ~300 LOC or ≤ 6 files.
- For shell commands use observability:
  ```bash
  cargo run -p nexoscli -- term run -- <your command>
  ```
- Validate frequently:
  ```bash
  cargo build --workspace
  cargo test  --workspace
  cargo run -p nexoscli -- dev docs build
  cargo run -p nexoscli -- dev telemetry report --since 24h
  ```
- If contracts changed: write/attach ADR (docs/ADRs/ADR-YYYYMMDD-*.md) and update snapshots via the dedicated PR.

---

## 5) Implementation commit (not the divider)

Make sure the full context can be grasped from the commit message only, without needing to view all diffs.

```bash
git add --all
git commit -m "feat(<scope>): implement PLAN-$PLAN_ID $PLAN_SLUG ($TASK_ID)

- Summary of what/why (plan link)
- Evidence (E2E tests / envelopes)
- Notes (if ADR or CP)"
```

Stop here. The divider & plan finalization happens in the Finalize workflow.
