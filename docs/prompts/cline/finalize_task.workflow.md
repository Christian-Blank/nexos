# Finalize Task — Verify & Divider (v0)

> Purpose: Verify acceptance, maintain docs/rules, create divider commit, write final metadata into the plan, and move `current/ → done/`.  
> Input: the single plan in `docs/project/current/PLAN-*.md`.

---

## 0) Pre‑flight

```bash
cargo run -p nexoscli dev init --yes
cargo run -p nexoscli dev doctor --ci --checks graph,plugins,config,events,tools --require plugins,graph
```


⸻

1) Verification (acceptance criteria must hold)

```bash
cargo build --workspace
cargo test  --workspace
cargo run -p nexoscli dev docs build
cargo run -p nexoscli dev telemetry report --since 24h
```

If observability/replay was touched and a bag exists:

# optional if relevant for the plan
nexoscli replay verify --bag <bag-dir> || true


⸻

2) Docs maintenance
	•	Ensure READMEs and generated docs are in sync.
	•	Keep plugin docs in their bounded context folder (co‑located with code).
	•	Re‑run:

```bash
cargo run -p nexoscli dev docs build
```



⸻

3) Rules maintenance

```bash
cargo run -p nexoscli rules lint
cargo run -p nexoscli rules export --for human --out docs/RULES.md
cargo run -p nexoscli rules export --for cline --out .clinerules/compiled.md
```

Add/update rules if the development process or surfaces changed.

⸻

4) ADR/contract check (only if public contracts changed)
	•	If command names/flags/schemas changed (help vs snapshots differ):
	•	Add ADR (docs/ADRs/ADR-YYYYMMDD-*.md) and link in divider commit.

⸻

5) Follow‑ups (frictions/ideas/bugs)

Create small atomic issues for high‑impact items:

```bash
cargo run -p nexoscli issue new --title "Follow-up: <short>" --labels plan,followup --body "<what/why>"
```


⸻

6) Divider commit

```bash
git add --all
git commit -m "feat(plan): finalize PLAN-<ID> <slug> (acceptance met)

- Evidence: tests, doctor, replay verify (if applicable)
- Docs: regenerated
- Rules: lint/export OK
- ADR: <id-or-n/a>
- Follow-ups: <ids-or-n/a>"
PLAN_SHA=$(git rev-parse --short HEAD)
```

⸻

7) Mark plan done (write metadata, move current → done)

```bash
FILE=$(ls -1 docs/project/current/PLAN-*.md | head -n1)
test -n "$FILE" || { echo "No plan in current"; exit 1; }

NOW=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
python3 - <<'PY'
from pathlib import Path
import sys, yaml
p = Path(sys.argv[1]); sha=sys.argv[2]; now=sys.argv[3]
txt = p.read_text()
fm, body = {}, txt
if txt.startswith('---'):
    _, rest = txt.split('---',1)
    head, body = rest.split('---',1) if '---' in rest else (rest, '')
    fm = yaml.safe_load(head) or {}
fm.update({"status":"done","implemented_in":sha,"completed_at":now})
out = '---\n'+yaml.safe_dump(fm, sort_keys=False)+'---\n'+body.lstrip()
p.write_text(out)
PY
"$FILE" "$PLAN_SHA" "$NOW"

BASENAME=$(basename "$FILE")
git mv "$FILE" "docs/project/done/$BASENAME"
git add --all
git commit -m "chore(plans): mark $(echo $BASENAME | sed 's/.md$//') as done (divider=$PLAN_SHA)"
```

⸻

8) End task + bundle (traceability)
	•	Recover TASK_ID from the plan’s front‑matter and end the task:
```bash
TASK_ID=$(python3 - <<'PY'



from pathlib import Path, yaml
p = list(Path(“docs/project/done”).glob(“PLAN-*.md”))[-1]
fm = {}
txt = p.read_text()
if txt.startswith(’—’):
_, rest = txt.split(’—’,1)
head, _ = rest.split(’—’,1) if ‘—’ in rest else (rest, ‘’)
fm = yaml.safe_load(head) or {}
print(fm.get(“task_id”,””))
PY
)
test -n “$TASK_ID” || { echo “No task_id in plan front-matter”; exit 1; }

cargo run -p nexoscli task end
cargo run -p nexoscli task bundle –task “$TASK_ID” –out “./${TASK_ID}-bundle.tgz”
```

**Success checklist**
- [ ] Acceptance verified (E2E/doctor/docs).
- [ ] Divider commit created.
- [ ] Plan updated (status=done, implemented_in, completed_at) and moved to done/.
- [ ] Task ended + bundle produced.

This finalize flow ensures you don’t forget to end the task and that the plan file itself is the source of truth for task_id and completion metadata.
