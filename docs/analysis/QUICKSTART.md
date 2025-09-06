# Analyst QUICKSTART (v0)

Purpose: Help analysts load TracePack/NEXBag data quickly into Python (Pandas) and DuckDB, and discover bag metadata via the dataset card (README.md) and manifest.json.

This guide assumes you have produced a bag with:
- events.ndjson (root or topics/events.ndjson)
- manifest.json
- README.md (dataset card generated on extract, or re-generated via verify --fix-readme)

Tip: If your bag is an archive (.tgz), extract it first: `tar -xzf tracebag.tgz`

---

## 1) Inspect the dataset card (README.md)

The dataset card is a self-describing overview written by the extractor and re-writable by the verifier:

- Sections (deterministic order):
  - Contents & Schemas (present artifacts and their schema IDs)
  - Counts (manifest.counts pretty-printed)
  - Labels (manifest.labels pretty-printed)
  - Tooling (manifest.tools/env/git pretty-printed)
  - Quick loads (Pandas and DuckDB snippets)

Re-generate the card from the latest manifest/files if needed:
```bash
nexoscli replay verify --path ./bag --fix-readme
```

---

## 2) Load events.ndjson with Pandas

```python
import pandas as pd
from pathlib import Path

bag = Path("./bag")  # path to your bag directory
# prefer root events.ndjson, fallback to topics/events.ndjson
events_path = bag/"events.ndjson"
if not events_path.exists():
    events_path = bag/"topics"/"events.ndjson"

df = pd.read_json(events_path, lines=True)

print(df.head())
print("columns:", df.columns.tolist())
print("rows:", len(df))
```

Flatten nested columns (optional):
```python
# Flatten attrs.* into top-level columns when present
if "attrs" in df.columns:
    attrs = pd.json_normalize(df["attrs"]).add_prefix("attrs.")
    df = pd.concat([df.drop(columns=["attrs"]), attrs], axis=1)

print(df.filter(regex="^attrs\\.").head())
```

Filter and simple aggregations:
```python
# Filter by plugin/command
term_runs = df[(df["plugin"] == "term") & (df["command"] == "run")]
print("term.run rows:", len(term_runs))

# Group by plugin counts
print(df.groupby("plugin").size().sort_values(ascending=False))
```

---

## 3) Load events.ndjson with DuckDB

```sql
-- From a SQL client (duckdb CLI or Python duckdb.connect)
-- Replace with your actual path
SELECT * FROM read_json_auto('bag/events.ndjson') LIMIT 5;
```

In Python:
```python
import duckdb
from pathlib import Path

bag = Path("./bag")
ev = bag/"events.ndjson"
if not ev.exists():
    ev = bag/"topics"/"events.ndjson"

con = duckdb.connect()
con.execute(f"CREATE VIEW events AS SELECT * FROM read_json_auto('{ev.as_posix()}')")
print(con.execute("SELECT COUNT(*) FROM events").fetchall())
print(con.execute("SELECT plugin, COUNT(*) AS c FROM events GROUP BY 1 ORDER BY c DESC").fetchdf())
```

---

## 4) Work with labels

Bag-level labels live in manifest.labels and are injected into each events row under `attrs.labels`.

Update or add labels:
```bash
# CLI pairs
nexoscli replay annotate --bag ./bag --labels exp_id=EXP-001 --labels treatment=A --labels env=dev

# From file (flat JSON) and override/extend with CLI
nexoscli replay annotate --bag ./bag --from-file labels.json --labels author=qa
```

Verify injection (first event row):
```bash
head -n 1 bag/events.ndjson
# ... "attrs":{"labels":{"exp_id":"EXP-001","treatment":"A","env":"dev", ...}, ...}
```

In Pandas:
```python
import json
from pathlib import Path

bag = Path("./bag")
manifest = json.loads((bag/"manifest.json").read_text())
print("labels:", manifest.get("labels", {}))

df = pd.read_json((bag/"events.ndjson" if (bag/"events.ndjson").exists() else bag/"topics"/"events.ndjson"), lines=True)
labels = pd.json_normalize(df.get("attrs", {})).get("labels")
# Alternatively:
lbl = (
    pd.json_normalize(df["attrs"])
    if "attrs" in df.columns
    else pd.DataFrame()
)
if not lbl.empty and "labels" in lbl.columns:
    labs = pd.json_normalize(lbl["labels"]).add_prefix("label.")
    df = pd.concat([df, labs], axis=1)
    print(df.filter(regex="^label\\.", axis=1).head())
```

---

## 5) Quick manifest queries

```bash
# Counts
jq '.counts' bag/manifest.json

# Labels
jq '.labels' bag/manifest.json

# Tooling snapshots
jq '.tools, .env, .git' bag/manifest.json
```

---

## 6) CSV export (for spreadsheets)

```bash
# Export events/fileops/annotations into CSVs under bag/exports/csv
nexoscli replay export --bag ./bag --what all --as csv
ls ./bag/exports/csv
```

---

## 7) Tips

- Prefer reading from README.md to quickly understand what’s inside the bag.
- Labels are idempotent; re-running annotate with the same K=V does not corrupt files.
- Very large NDJSON: Pandas and DuckDB both support streaming/efficient readers; consider filtering early.
- When in doubt about paths, fall back to:
  - events: bag/events.ndjson or bag/topics/events.ndjson
  - fileops: bag/fileops/fileops.ndjson or bag/fileops.ndjson

---
