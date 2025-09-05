---
Implemented in: commit bfa50794244240252a34dcdc3724e12f9b2aeab feat(observability): implement Plan 26 Contract Freeze — schema tags + schema exporter (ADR-20250903-contract-schema-tags, TASK-20250903-plan-26-contract-freeze-schema-tags-schema-exporter-track-a)
---
Perfect—this is Track A (“Contract Freeze & Schema Registry”). Below is a drop‑in implementation pack you can commit now:
	•	ADR to allow schema tags in protected files.
	•	Core: add schema tags + derive JSON Schemas from the actual types (single source of truth).
	•	Dev plugin: nexoscli dev schemas export --out docs/SPECS/ to regenerate specs.
	•	Emitters: add schema to events, bag manifests, and term records.
	•	Tests: one E2E for schema export.
	•	Docs: tiny SPEC readme.

I’ve written the code to be minimal and additive; it won’t change command flags or outputs except for the new "schema" field in emitted JSON records.

⸻

1) ADR — allow schema tags in protected files

docs/ADRs/ADR-20250903-contract-schema-tags.md

# ADR-20250903 — Schema tags & schema export

**Status:** Accepted  
**Context:** We need a single source of truth for public contracts (events, bag manifest, term records, file watch).  
**Decision:** 
1) Add a `"schema"` string field to every JSON record the CLI emits (events, bag manifest, term record, file-watch).  
2) Derive JSON Schemas from the actual Rust types via `schemars` and expose `nexoscli dev schemas export`.  
3) Treat schema changes as contracts: additive changes are minor; breaking changes require an upconverter and a new schema version string.

**Consequences:**  
- Protected files touched: `nexoscli-core/src/event.rs` (derive + field), `plugins/replay`, `plugins/term`.  
- CI will gate on `dev schemas export` output being committed.

**Versioning policy:**  
- Events: `"event.v0.3"`  
- Term record: `"term_record.v0.1"`  
- File watch event: `"file_watch_event.v0.1"`  
- Bag manifest: `"bag_manifest.v0.2"`  


⸻

2) Core: contracts + schema tags

2.1 Add schemars to nexoscli-core

nexoscli-core/Cargo.toml (add if not present)

[dependencies]
# ...
schemars = { workspace = true, features = ["derive"] }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }

2.2 Event: add schema + derive JsonSchema

Note: I’m showing only the surgical additions. Keep all existing fields.

nexoscli-core/src/event.rs (patch)

use schemars::JsonSchema; // NEW

// Add JsonSchema to the derive list
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct Event {
    // NEW: contract tag (defaults to event.v0.3 if not supplied)
    #[serde(default = "event_schema_tag")]
    pub schema: &'static str,

    // ... keep your existing fields (ts, run_id, plugin, command, args_hash, content_hash, exit_code, dur_ms, attrs, meta, etc.)
}

// NEW: constant + default function
pub const EVENT_SCHEMA_V03: &str = "event.v0.3";
fn event_schema_tag() -> &'static str { EVENT_SCHEMA_V03 }

Wherever you construct an Event today, you don’t need to touch anything—serde(default) will populate "schema": "event.v0.3" automatically when the line is serialized.

2.3 Shared contract types (for schema export & re‑use)

nexoscli-core/src/contracts.rs (new)

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const TERM_RECORD_SCHEMA_V01: &str = "term_record.v0.1";
pub const FILE_WATCH_EVENT_SCHEMA_V01: &str = "file_watch_event.v0.1";
pub const BAG_MANIFEST_SCHEMA_V02: &str = "bag_manifest.v0.2";

/// Term command execution record written by the term plugin.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TermRecordV01 {
    #[serde(default = "schema_term_record")]
    pub schema: &'static str,
    pub run_id: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub started_at: String,   // RFC3339
    pub finished_at: String,  // RFC3339
    pub exit_code: i32,
    pub dur_ms: u64,
    pub stdout_sha256: Option<String>,
    pub stderr_sha256: Option<String>,
}
fn schema_term_record() -> &'static str { TERM_RECORD_SCHEMA_V01 }

/// File system change event (if/when file-watch is enabled).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op")] // "create" | "modify" | "delete" | "rename"
pub enum FileWatchEventV01 {
    #[serde(rename = "create")] Create { #[serde(default = "schema_file_watch")] schema: &'static str, ts: String, path: String, size: Option<u64>, sha256: Option<String> },
    #[serde(rename = "modify")] Modify { #[serde(default = "schema_file_watch")] schema: &'static str, ts: String, path: String, size: Option<u64>, sha256: Option<String> },
    #[serde(rename = "delete")] Delete { #[serde(default = "schema_file_watch")] schema: &'static str, ts: String, path: String },
    #[serde(rename = "rename")] Rename { #[serde(default = "schema_file_watch")] schema: &'static str, ts: String, from: String, to: String },
}
fn schema_file_watch() -> &'static str { FILE_WATCH_EVENT_SCHEMA_V01 }

/// NEXBag/TracePack manifest (v0.2).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BagManifestV02 {
    #[serde(default = "schema_bag_manifest")]
    pub schema: &'static str,
    pub version: String,             // "0.2"
    pub kind: String,                // "tracepack"
    pub run_id: String,
    pub created_at: String,          // RFC3339
    pub time_range: TimeRange,
    pub counts: Counts,
    pub git: Option<GitRange>,
    pub plugins: Option<std::collections::BTreeMap<String, u64>>,
    pub commands: Option<std::collections::BTreeMap<String, u64>>,
    pub patches: Option<Vec<PatchEntry>>,
    pub status_files: Option<Vec<StatusEntry>>,
    pub cline_tasks: Option<Vec<ClineEntry>>,
}
fn schema_bag_manifest() -> &'static str { BAG_MANIFEST_SCHEMA_V02 }

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TimeRange { pub from: String, pub to: String }
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Counts { pub events_lines: u64, pub steps: Option<u64>, pub patches: Option<u64>, pub status: Option<u64>, pub cline: Option<u64> }
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GitRange { pub from: String, pub to: String }
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PatchEntry { pub seq: u32, pub src: String, pub dst: String, pub sha256: String, pub bytes: u64 }
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusEntry { pub seq: u32, pub src: String, pub dst: String, pub sha256: String, pub bytes: u64 }
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClineEntry { pub id: String, pub dst: String, pub sha256: String, pub bytes: u64, pub created_at: String, pub messages: u64 }

nexoscli-core/src/lib.rs (expose contracts)

pub mod contracts;
pub use contracts::*;


⸻

3) Dev plugin: schemas export

Add a new subcommand under the dev plugin that writes JSON Schemas for the four contracts.

plugins/dev/Cargo.toml (ensure schemars dep)

[dependencies]
# ...
schemars = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }

plugins/dev/src/lib.rs (patch; only the new bits shown)

use schemars::schema_for;
use serde_json::to_string_pretty;

impl Plugin for DevPlugin {
    fn augment_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
            ClapCommand::new("dev").about("Developer utilities")
              // ... existing subcommands
              .subcommand(
                 ClapCommand::new("schemas")
                   .about("Export contract JSON Schemas")
                   .subcommand(
                      ClapCommand::new("export")
                        .about("Write JSON Schemas to a directory")
                        .arg(Arg::new("out").long("out").required(true).value_name("DIR"))
                   )
              )
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match m.subcommand() {
            // ... existing subcommands
            Some(("schemas", sm)) => match sm.subcommand() {
                Some(("export", esm)) => {
                    let out = esm.get_one::<String>("out").unwrap();
                    let dir = std::path::Path::new(out);
                    std::fs::create_dir_all(dir)?;

                    // 1) Event (from real core type)
                    let ev = schema_for!(nexoscli_core::event::Event);
                    std::fs::write(dir.join("event_v0.3.schema.json"), to_string_pretty(&ev)?)?;

                    // 2) Term record
                    let tr = schema_for!(nexoscli_core::contracts::TermRecordV01);
                    std::fs::write(dir.join("term_record_v0.1.schema.json"), to_string_pretty(&tr)?)?;

                    // 3) File watch event
                    let fw = schema_for!(nexoscli_core::contracts::FileWatchEventV01);
                    std::fs::write(dir.join("file_watch_event_v0.1.schema.json"), to_string_pretty(&fw)?)?;

                    // 4) Bag manifest
                    let bm = schema_for!(nexoscli_core::contracts::BagManifestV02);
                    std::fs::write(dir.join("bag_manifest_v0.2.schema.json"), to_string_pretty(&bm)?)?;

                    let code = ctx.ok("dev","schemas.export",
                        &serde_json::json!({"out": out, "files":["event_v0.3.schema.json","term_record_v0.1.schema.json","file_watch_event_v0.1.schema.json","bag_manifest_v0.2.schema.json"]})
                    );
                    Ok(code)
                }
                _ => Ok(ctx.err("dev","schemas","dev.schemas.unknown","unknown schemas command", None, 2))
            },
            _ => { /* existing */ }
        }
    }
}


⸻

4) Emitters: add schema fields (A2)

4.1 Events (core) — already handled

Because Event now has a defaulted schema field, every event line written will include:

"schema": "event.v0.3"

No further changes needed.

4.2 Bag manifest (replay/bag builder)

In the code that creates the manifest (likely in plugins/replay/src/lib.rs when building a bag):

use nexoscli_core::contracts::{BagManifestV02, BAG_MANIFEST_SCHEMA_V02, TimeRange, Counts /* ... */};

let manifest = BagManifestV02{
    schema: BAG_MANIFEST_SCHEMA_V02,
    version: "0.2".into(),
    kind: "tracepack".into(),
    run_id,
    created_at: now_rfc3339(),
    time_range: TimeRange{ from: first_ts, to: last_ts },
    counts, git, plugins, commands, patches, status_files, cline_tasks,
};
// write to topics/manifest.json

4.3 Term record (term plugin)

Where you persist per‑command records under .nexoscli/term/:

use nexoscli_core::contracts::{TermRecordV01, TERM_RECORD_SCHEMA_V01};

let rec = TermRecordV01{
  schema: TERM_RECORD_SCHEMA_V01,
  run_id: run_id.clone(),
  cmd: cmd.clone(),
  args: argv.clone(),
  cwd: std::env::current_dir().ok().map(|p| p.display().to_string()),
  started_at, finished_at,
  exit_code,
  dur_ms,
  stdout_sha256,
  stderr_sha256,
};
std::fs::write(path, serde_json::to_vec_pretty(&rec)?)?;

4.4 File watch (optional, when you wire it)

Wherever your watcher publishes JSON, use FileWatchEventV01::* constructors so the "schema" tag is carried.

⸻

5) Tests — schema export E2E

nexoscli-bin/tests/dev_schemas_export.rs

mod helpers; use helpers::*;
use std::fs;

#[test]
fn dev_schemas_export_writes_all_files() {
    let t = tmp();

    // export schemas
    let v = run_json(t.path(), &["dev","schemas","export","--out","docs/SPECS"]);
    assert_eq!(v["data"]["out"], "docs/SPECS");

    // files exist
    let out = t.path().join("docs/SPECS");
    assert!(out.join("event_v0.3.schema.json").exists());
    assert!(out.join("term_record_v0.1.schema.json").exists());
    assert!(out.join("file_watch_event_v0.1.schema.json").exists());
    assert!(out.join("bag_manifest_v0.2.schema.json").exists());

    // sanity: JSON parses
    for f in ["event_v0.3.schema.json","term_record_v0.1.schema.json","file_watch_event_v0.1.schema.json","bag_manifest_v0.2.schema.json"] {
        let txt = fs::read_to_string(out.join(f)).unwrap();
        let _: serde_json::Value = serde_json::from_str(&txt).unwrap();
    }
}

This mirrors your existing test helper pattern (run_json in nexoscli-bin/tests/helpers.rs).

⸻

6) Docs — SPEC index

docs/SPECS/README.md

# Contract Schemas (v0)

These files are generated by:

```bash
nexoscli dev schemas export --out docs/SPECS

	•	event_v0.3.schema.json — single line in .nexoscli/log/events.jsonl
	•	term_record_v0.1.schema.json — .nexoscli/term/<...>.json
	•	file_watch_event_v0.1.schema.json — optional file-watch events
	•	bag_manifest_v0.2.schema.json — topics/manifest.json in a NEXBag/TracePack

Do not hand-edit. Regenerate via the CLI and commit.

---

## 7) CI hook (optional now; add when ready)

You can add a simple “spec drift” job (pseudo):

```yaml
- name: Export schemas
  run: ./target/debug/nexoscli dev schemas export --out docs/SPECS

- name: Fail on drift
  run: |
    git diff --exit-code -- docs/SPECS || (echo "Schema drift: run 'nexoscli dev schemas export'" && exit 1)


⸻

8) How to verify locally (manual)

# 1) Build & run schemas export
cargo build --workspace
./target/debug/nexoscli dev schemas export --out docs/SPECS --output json | jq

# 2) See new schema tag in events
./target/debug/nexoscli task new --title "schema smoke"
tail -1 .nexoscli/log/events.jsonl | jq '.schema'   # "event.v0.3"

# 3) If you have a term record path, check schema tag
grep -R --include '*.json' '"term_record.v0.1"' .nexoscli/term/ || true

# 4) Build a bag, check manifest schema
./target/debug/nexoscli replay extract --run-id <some-run> --out demo-tracepack.tgz
tar -xOf demo-tracepack.tgz topics/manifest.json | jq '.schema'  # "bag_manifest.v0.2"


⸻

9) Notes & pitfalls
	•	Protected files: This ADR explicitly permits adding a schema field and JsonSchema derive to event.rs.
	•	Back‑compat: The new schema field is additive. Consumers ignoring unknown fields keep working.
	•	One source of truth: Schemas are derived from the same Rust types your emitters use—no hand‑written JSON Schema to drift.
	•	Next migrations: For any breaking change, bump the schema version string and add an upconverter in replay bag upgrade.

⸻