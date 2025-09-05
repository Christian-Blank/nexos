Below is Implementation Plan #34 — “Schemas bundle & validators (v0)”.
It adds a schemas API and a validator to the replay plugin, so every bag (TracePack) can be checked against canonical, versioned JSON Schemas. This is a P0 step toward a usable, analyzable v0.

⸻

What you get
	•	Canonical schemas (embedded):
	•	nexos/events@0.3 (lines in events.ndjson)
	•	nexos/fileops@0.1 (reserved for #35, validates when present)
	•	CLI:
	•	nexoscli replay schema list – list available schemas & versions
	•	nexoscli replay schema get --name events --version 0.3 [--out path] – print or persist schema JSON
	•	nexoscli replay validate --bag <DIR> [--what events|fileops|all] – validate NDJSON artifacts in a bag dir
	•	Validator details:
	•	JSON Schema validation (using jsonschema crate)
	•	Count rows validated, list invalid lines with concise errors
	•	Summarize per‑artifact results (ok, lines, valid, invalid, errors)
	•	Docs:
	•	docs/EVENTS_SCHEMA.md – human‑readable schema & examples
	•	docs/FILEOPS_SCHEMA.md – reserved now; basic doc + JSON Schema included (will be exercised by #35)
	•	E2E tests:
	•	Smoke test that makes a run, extracts a bag, and validates it end‑to‑end

Scope is limited to directories as bag inputs (same layout as replay extract --out <dir>). If you prefer .tgz, unzip first—the validator keeps v0 simple and robust.

⸻

Patch 1 — Add dependency

plugins/replay/Cargo.toml (append under [dependencies])

jsonschema = "0.17"

Uses the widely adopted jsonschema validator. No feature flags needed.

⸻

Patch 2 — Add schemas module

plugins/replay/src/schemas.rs (new file)

use serde_json::json;

#[derive(Clone, Debug)]
pub struct SchemaInfo {
    pub name: &'static str,      // "events" | "fileops"
    pub version: &'static str,   // "0.3" | "0.1"
    pub id: &'static str,        // "nexos/events@0.3"
    pub description: &'static str,
}

pub fn list() -> Vec<SchemaInfo> {
    vec![
        SchemaInfo{
            name: "events",
            version: "0.3",
            id: "nexos/events@0.3",
            description: "Stable event envelope lines (events.ndjson); RFC3339 ts; ok/exit_code; attrs/context.",
        },
        SchemaInfo{
            name: "fileops",
            version: "0.1",
            id: "nexos/fileops@0.1",
            description: "Repository file operations (fileops.ndjson); op/path/patch/digest_after.",
        },
    ]
}

pub fn get_schema_json(name: &str, version: &str) -> Option<serde_json::Value> {
    match (name, version) {
        ("events", "0.3") => Some(events_03()),
        ("fileops", "0.1") => Some(fileops_01()),
        _ => None,
    }
}

/* ---------- concrete schemas ---------- */

fn events_03() -> serde_json::Value {
    // JSON Schema Draft 7 compatible
    json!({
      "$schema": "http://json-schema.org/draft-07/schema#",
      "$id": "nexos/events@0.3",
      "type": "object",
      "additionalProperties": true,
      "required": ["ts","run_id","plugin","command","exit_code","ok"],
      "properties": {
        "ts": { "type": "string", "format": "date-time" },
        "run_id": { "type": "string", "minLength": 1 },
        "plugin": { "type": "string", "minLength": 1 },
        "command": { "type": "string", "minLength": 1 },
        "exit_code": { "type": "integer" },
        "ok": { "type": "boolean" },
        "dur_ms": { "type": "integer", "minimum": 0 },
        "args_hash": { "type": "string" },
        "content_hash": { "type": "string" },
        "error_code": { "type": "string" },
        "context": {
          "type": "object",
          "additionalProperties": true,
          "properties": {
            "task_id": { "type": "string" },
            "session_id": { "type": "string" },
            "stage": { "type": "string" }
          }
        },
        "attrs": {
          "type": "object",
          "additionalProperties": true
        }
      }
    })
}

fn fileops_01() -> serde_json::Value {
    json!({
      "$schema": "http://json-schema.org/draft-07/schema#",
      "$id": "nexos/fileops@0.1",
      "type": "object",
      "additionalProperties": true,
      "required": ["op","path"],
      "properties": {
        "schema": { "type": "string", "const": "nexos/fileops@0.1" },
        "ts": { "type": "string", "format": "date-time" },
        "run_id": { "type": "string" },
        "op": { "type": "string", "enum": ["create","modify","delete","rename"] },
        "path": { "type": "string", "minLength": 1 },
        "bytes": { "type": "integer", "minimum": 0 },
        "digest_after": { "type": "string", "pattern": "^sha256:[0-9a-fA-F]{64}$" },
        "patch": {
          "type": "object",
          "additionalProperties": true,
          "required": ["sha256"],
          "properties": {
            "sha256": { "type": "string", "pattern": "^sha256:[0-9a-fA-F]{64}$" },
            "lines": { "type": "integer", "minimum": 0 }
          }
        },
        "source": { "type": "string" },
        "context": {
          "type": "object",
          "additionalProperties": true,
          "properties": {
            "task_id": { "type": "string" },
            "session_id": { "type": "string" },
            "stage": { "type": "string" }
          }
        }
      }
    })
}


⸻

Patch 3 — Extend the replay plugin with schema & validate

Open plugins/replay/src/lib.rs and:
	1.	Add imports near the top:

use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use jsonschema::{JSONSchema};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fs, io::{BufRead, BufReader}, path::{Path, PathBuf}};
use walkdir::WalkDir;

mod schemas; // NEW

	2.	Add result structs (place with other public CLI data types if you keep them):

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SchemaRow { name: String, version: String, id: String, description: String }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SchemaListResult { schemas: Vec<SchemaRow> }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SchemaGetArgs { name: String, version: String, #[serde(default)] out: Option<String> }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SchemaGetResult { name: String, version: String, id: String, bytes: usize, written: Option<String> }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ValidateArgs {
    bag: String,
    #[serde(default)] what: Option<String>, // events | fileops | all
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ArtifactReport {
    ok: bool,
    lines: u64,
    valid: u64,
    invalid: u64,
    #[serde(default)]
    errors: Vec<String>, // first N error messages with line numbers
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ValidateResult {
    bag: String,
    summary_ok: bool,
    events: Option<ArtifactReport>,
    fileops: Option<ArtifactReport>,
}

	3.	Augment the CLI surface inside augment_commands:

// inside `replay` command tree
.subcommand(
  ClapCommand::new("schema").about("Schemas bundle")
    .subcommand(ClapCommand::new("list").about("List available schemas"))
    .subcommand(
      ClapCommand::new("get").about("Get schema JSON")
        .arg(Arg::new("name").long("name").required(true).value_parser(["events","fileops"]))
        .arg(Arg::new("version").long("version").required(true).value_name("N.N"))
        .arg(Arg::new("out").long("out").value_name("PATH"))
    )
)
.subcommand(
  ClapCommand::new("validate").about("Validate bag artifacts against schemas")
    .arg(Arg::new("bag").long("bag").required(true).value_name("DIR"))
    .arg(Arg::new("what").long("what").value_parser(["events","fileops","all"]).default_value("all"))
)

	4.	Handle the new commands in execute:

match sub_matches.subcommand() {
    // ...
    Some(("schema", sm)) => {
        match sm.subcommand() {
            Some(("list", _)) => {
                let rows: Vec<SchemaRow> = schemas::list().into_iter().map(|s| SchemaRow{
                    name: s.name.to_string(),
                    version: s.version.to_string(),
                    id: s.id.to_string(),
                    description: s.description.to_string(),
                }).collect();
                let exit = ctx.ok("replay","schema.list",&SchemaListResult{ schemas: rows });
                Ok(exit)
            }
            Some(("get", gm)) => {
                let name = gm.get_one::<String>("name").unwrap().to_string();
                let version = gm.get_one::<String>("version").unwrap().to_string();
                let outp = gm.get_one::<String>("out").cloned();
                let schema = schemas::get_schema_json(&name, &version)
                    .ok_or_else(|| anyhow::anyhow!("unknown schema {}/{}", name, version))?;
                let bytes = serde_json::to_vec_pretty(&schema)?.len();
                let written = if let Some(path) = outp.clone() {
                    if let Some(dir) = Path::new(&path).parent() { fs::create_dir_all(dir)?; }
                    fs::write(&path, serde_json::to_vec_pretty(&schema)?)?;
                    Some(path)
                } else { None };
                let id = schemas::list().into_iter()
                    .find(|s| s.name==name && s.version==version).map(|s| s.id).unwrap_or("");
                let exit = ctx.ok("replay","schema.get",&SchemaGetResult{
                    name, version, id: id.to_string(), bytes, written
                });
                Ok(exit)
            }
            _ => Ok(ctx.err("replay","schema","replay.schema.unknown","unknown schema subcommand",None,2)),
        }
    }

    Some(("validate", vm)) => {
        let bag = vm.get_one::<String>("bag").unwrap().to_string();
        let what = vm.get_one::<String>("what").map(|s| s.as_str()).unwrap_or("all");
        let bagp = PathBuf::from(&bag);
        if !bagp.is_dir() {
            return Ok(ctx.err("replay","validate","replay.validate.nodir","bag path is not a directory",None,2));
        }

        let mut events_rep: Option<ArtifactReport> = None;
        let mut fileops_rep: Option<ArtifactReport> = None;

        if what=="events" || what=="all" {
            let p = bagp.join("events.ndjson");
            if p.exists() {
                let schema = schemas::get_schema_json("events","0.3").unwrap();
                events_rep = Some(validate_ndjson(&p, &schema)?);
            }
        }
        if what=="fileops" || what=="all" {
            let p = bagp.join("fileops.ndjson");
            if p.exists() {
                let schema = schemas::get_schema_json("fileops","0.1").unwrap();
                fileops_rep = Some(validate_ndjson(&p, &schema)?);
            }
        }

        let ok_summary = events_rep.as_ref().map(|r| r.ok).unwrap_or(true)
            && fileops_rep.as_ref().map(|r| r.ok).unwrap_or(true);

        let out = ValidateResult{
            bag,
            summary_ok: ok_summary,
            events: events_rep,
            fileops: fileops_rep,
        };
        let exit = if ok_summary {
            ctx.ok("replay","validate",&out)
        } else {
            ctx.err("replay","validate","replay.validate.failed","validation failed",None,2)
        };
        Ok(exit)
    }

    // ...
}

	5.	Add helper function at bottom of the file:

fn validate_ndjson(path: &Path, schema_json: &serde_json::Value) -> anyhow::Result<ArtifactReport> {
    use jsonschema::output::BasicOutput;
    let compiled = JSONSchema::compile(schema_json)
        .map_err(|e| anyhow::anyhow!("invalid schema: {}", e))?;

    let f = fs::File::open(path)?;
    let reader = BufReader::new(f);
    let mut lines: u64 = 0;
    let mut valid: u64 = 0;
    let mut invalid: u64 = 0;
    let mut errors: Vec<String> = vec![];

    for (idx, line) in reader.lines().enumerate() {
        let line_no = (idx + 1) as u64;
        let l = match line {
            Ok(s) if !s.trim().is_empty() => s,
            Ok(_) => { continue; }
            Err(e) => {
                invalid += 1;
                errors.push(format!("line {}: io error: {}", line_no, e));
                continue;
            }
        };
        lines += 1;
        match serde_json::from_str::<serde_json::Value>(&l) {
            Ok(v) => {
                let result = compiled.apply(&v);
                if result.is_valid() {
                    valid += 1;
                } else {
                    invalid += 1;
                    // grab a compact first error
                    if errors.len() < 10 {
                        let out = BasicOutput::from(result);
                        if let Some(first) = out.errors.first() {
                            errors.push(format!("line {}: {}", line_no, first.error));
                        } else {
                            errors.push(format!("line {}: schema violation", line_no));
                        }
                    }
                }
            }
            Err(e) => {
                invalid += 1;
                if errors.len() < 10 {
                    errors.push(format!("line {}: json parse error: {}", line_no, e));
                }
            }
        }
    }

    Ok(ArtifactReport{
        ok: invalid == 0,
        lines, valid, invalid, errors
    })
}

	6.	(Optional) Tool specs for agents (if your plugin implements tool_specs()):

// in `tool_specs()`:
ToolSpec{
  name: "replay.schema.list".into(),
  about: "List available schemas".into(),
  args_schema: schema_for!(serde_json::Value),
  result_schema: schema_for!(SchemaListResult),
},
ToolSpec{
  name: "replay.schema.get".into(),
  about: "Get schema JSON".into(),
  args_schema: schema_for!(SchemaGetArgs),
  result_schema: schema_for!(SchemaGetResult),
},
ToolSpec{
  name: "replay.validate".into(),
  about: "Validate bag artifacts (events/fileops) against schemas".into(),
  args_schema: schema_for!(ValidateArgs),
  result_schema: schema_for!(ValidateResult),
},

If your replay plugin already exposes tool specs, just extend that vector with the above.

⸻

Patch 4 — Docs (schemas)

docs/EVENTS_SCHEMA.md (new)

# Events Schema — `nexos/events@0.3`

Each line in `events.ndjson` represents one CLI event with a stable envelope.

## Required fields
- `ts` *(RFC3339)* — event timestamp
- `run_id` *(string)* — unique id for the `nexoscli` run
- `plugin` *(string)*
- `command` *(string)*
- `exit_code` *(int)* — process exit code
- `ok` *(bool)* — `exit_code == 0`

## Common optional fields
- `dur_ms` *(int ≥ 0)* — duration in milliseconds
- `args_hash`, `content_hash` *(strings)* — integrity digests
- `error_code` *(string)* — normalized error code (if any)
- `context.task_id`, `context.session_id`, `context.stage`
- `attrs` *(object)* — tool-specific attributes

## JSON Schema
See also:
```bash
nexoscli replay schema get --name events --version 0.3

**`docs/FILEOPS_SCHEMA.md`** (new)

```markdown
# FileOps Schema — `nexos/fileops@0.1`

Each line in `fileops.ndjson` describes a repository file operation correlated to a run.

## Required fields
- `op` — one of `create|modify|delete|rename`
- `path` — repo-relative path

## Recommended fields
- `ts` *(RFC3339)*, `run_id` *(string)*
- `bytes` *(int ≥ 0)* — new file size after op (if known)
- `digest_after` *(sha256:...)* — content digest after op
- `patch.sha256` *(sha256:...)* — patch content digest
- `patch.lines` *(int ≥ 0)*
- `source` *(string)* — e.g. `term.run`
- `context.task_id|session_id|stage`

## JSON Schema
```bash
nexoscli replay schema get --name fileops --version 0.1

Note: FileOps normalization lands in Plan #35. The validator will accept existing fileops.ndjson when present and fail fast otherwise.

---

## Patch 5 — E2E tests

**`nexoscli-bin/tests/replay_schema_validate.rs`** (new)

```rust
mod helpers; use helpers::*;
use std::fs;

#[test]
fn replay_schema_validate_events() {
    let t = tmp();

    // init
    let _ = run_json(t.path(), &["dev","init","--yes"]);

    // produce a trivial event/run
    let v = run_json(t.path(), &["term","run","--","echo","hello"]);
    let run_id = v["meta"]["run_id"].as_str().expect("run_id");

    // extract a bag dir
    let outdir = t.path().join("bag");
    let _ = run_json(t.path(), &["replay","extract","--run-id", run_id, "--out", outdir.to_str().unwrap() ]);

    // schema list
    let lst = run_json(t.path(), &["replay","schema","list"]);
    assert!(lst["data"]["schemas"].as_array().unwrap().iter()
        .any(|s| s["id"].as_str()==Some("nexos/events@0.3")));

    // schema get
    let schema_path = t.path().join("events.schema.json");
    let _ = run_json(t.path(), &["replay","schema","get","--name","events","--version","0.3","--out", schema_path.to_str().unwrap()]);
    assert!(schema_path.exists());

    // validate (events present; fileops may be absent pre-#35)
    let val = run_json(t.path(), &["replay","validate","--bag", outdir.to_str().unwrap()]);
    assert!(val["data"]["summary_ok"].as_bool().unwrap_or(false));
    assert!(val["data"]["events"]["ok"].as_bool().unwrap());
}

Uses the existing test helpers (same pattern as your other E2E tests).

⸻

Smoke run (manual)

# Build
cargo build --workspace

# Make a run and extract a bag
./target/debug/nexoscli dev init --yes
./target/debug/nexoscli term run -- echo "hi"
RUN_ID=$(jq -r .meta.run_id < .nexoscli/log/events.jsonl | tail -n 1)  # or capture from the previous envelope
./target/debug/nexoscli replay extract --run-id "$RUN_ID" --out ./bag-demo

# List schemas
./target/debug/nexoscli replay schema list --output json

# Show events schema
./target/debug/nexoscli replay schema get --name events --version 0.3

# Validate the bag
./target/debug/nexoscli replay validate --bag ./bag-demo --output json

Expected: summary_ok=true, events.ok=true. (fileops may be null until #35 normalizer is added or if your current bag doesn’t include it.)

⸻

Notes & guardrails
	•	Directory bags only in v0 (keeps validator simple and fast). If you need .tgz, untar first; we can add tar.gz reading later without changing the interface.
	•	Forgiving additional props: Schemas accept extra fields to avoid breaking older logs; correctness hinges on required core fields and formats.
	•	Error budget: Validator reports up to 10 first errors per artifact to keep envelopes small and useful.

⸻

Why this step matters (tightens the loop)
	•	Establishes a single source of truth for observability schemas.
	•	Turns drift into a detected failure (replay validate → CI gate ready).
	•	Unblocks #35 (fileops normalizer) and #36 (export)—analysts can trust outputs.
