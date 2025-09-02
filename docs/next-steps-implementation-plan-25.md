Absolutely—let’s finish Observability v0 so you can produce a single, portable, analyzable “bag” (ROS‑bag style) that contains:
	•	Event ledger (tamper‑evident, hash‑chained)
	•	Terminal sessions (command, stdout/stderr logs, timings)
	•	Git changes (pre/post status, patches)
	•	Cline tasks (normalized, redacted)
	•	(Optional) File‑watch events (live file edits while you work)

Below you’ll get:
	1.	the format (TracePack v0.2 ≈ “NEXBag”) + JSON schemas,
	2.	minimal code patches to capture terminal + git deltas, hash‑chain events, build & verify a bag, and (optional) file‑watch,
	3.	docs updates (drop‑in),
	4.	tests + CI gates (what to add).

Analogy: This is indeed like ROS bags: we record topics (events, term, file_watches, cline) into a deterministic container with an index & manifest. You can “bag build”, “bag verify”, “bag info”, “bag cat”.

⸻

0) What “v0 done” means (acceptance)
	•	Every CLI run writes event.v0.3 lines with hash chaining (tamper‑evident).
	•	term run captures stdout/stderr, durations, git status (pre/post) and patches.
	•	Cline tasks are importable (already done) and included in the bag by time window.
	•	replay bag build produces a directory or .tgz with a manifest and topics/ sub‑tree.
	•	replay verify checks chain integrity + manifest, and fails on corruption.
	•	Schemas live under docs/SPECS/ and docs describe the bag.
	•	E2E tests pass; doctor can require events_integrity.

⸻

1) NEXBag / TracePack v0.2 — Format (ready to commit)

Layout (dir or .tgz):

tracepack/
  manifest.json                # bag-level manifest (v0.2)
  topics/
    events.ndjson              # event.v0.3 (hash-chained)
    term/commands.ndjson       # terminal session index (see schema)
    files/watch.ndjson         # (optional) file-watch events
  patches/
    seq-0001-<name>.patch
    ...
  status/
    seq-0001-pre.status
    seq-0001-post.status
  cline/tasks/
    2025-09-01T12-00-00Z-task_123.json
    ...
  run_manifest.json            # per-run chain summary (anchor)
  README.md

docs/SPECS/event_v0.3.schema.json (hash-chained event line) — shortened here, same as earlier proposal:

{
  "$schema":"https://json-schema.org/draft/2020-12/schema",
  "title":"nexoscli.event.v0.3",
  "type":"object",
  "required":["schema","ts","run_id","seq","plugin","command","exit_code","dur_ms","content_hash","chain_hash"],
  "properties":{
    "schema":{"const":"event.v0.3"},
    "ts":{"type":"string","format":"date-time"},
    "run_id":{"type":"string"},
    "seq":{"type":"integer","minimum":1},
    "prev":{"type":"string"},
    "content_hash":{"type":"string","pattern":"^sha256:[0-9a-f]{64}$"},
    "chain_hash":{"type":"string","pattern":"^sha256:[0-9a-f]{64}$"},
    "plugin":{"type":"string"},
    "command":{"type":"string"},
    "args_hash":{"type":"string"},
    "exit_code":{"type":"integer"},
    "dur_ms":{"type":"integer","minimum":0},
    "attrs":{"type":"object"},
    "meta":{"type":"object"}
  }
}

docs/SPECS/bag_manifest_v0.2.schema.json (bag summary):

{
  "$schema":"https://json-schema.org/draft/2020-12/schema",
  "title":"nexoscli.bag_manifest.v0.2",
  "type":"object",
  "required":["version","format","created_at","topics","counts"],
  "properties":{
    "version":{"const":"0.2"},
    "format":{"const":"nexbag"},
    "created_at":{"type":"string","format":"date-time"},
    "source":{"type":"object","properties":{"repo":{"type":"string"},"branch":{"type":"string"},"run_id":{"type":"string"}}},
    "time_range":{"type":"object","properties":{"from":{"type":"string","format":"date-time"},"to":{"type":"string","format":"date-time"}}},
    "topics":{"type":"array","items":{"type":"string"}},
    "counts":{"type":"object",
      "properties":{"events":{"type":"integer"},"term":{"type":"integer"},"patches":{"type":"integer"},"status":{"type":"integer"},"cline":{"type":"integer"},"file_watch":{"type":"integer"}}
    },
    "chain_ok":{"type":"boolean"},
    "notes":{"type":"string"}
  }
}

docs/SPECS/run_manifest_v0.2.schema.json (per‑run anchor; unchanged from earlier).

docs/SPECS/term_record_v0.1.schema.json (rows in topics/term/commands.ndjson):

{
  "$schema":"https://json-schema.org/draft/2020-12/schema",
  "title":"nexoscli.term_record.v0.1",
  "type":"object",
  "required":["schema","ts","run_id","seq","cmd","cwd","exit_code","dur_ms","stdout_bytes","stderr_bytes","files"],
  "properties":{
    "schema":{"const":"term_record.v0.1"},
    "ts":{"type":"string","format":"date-time"},
    "run_id":{"type":"string"},
    "seq":{"type":"integer","minimum":1},
    "cmd":{"type":"string"},
    "cwd":{"type":"string"},
    "exit_code":{"type":"integer"},
    "dur_ms":{"type":"integer","minimum":0},
    "stdout_bytes":{"type":"integer","minimum":0},
    "stderr_bytes":{"type":"integer","minimum":0},
    "stdout_sha256":{"type":"string"},
    "stderr_sha256":{"type":"string"},
    "files":{
      "type":"object",
      "properties":{
        "stdout":{"type":"string"},
        "stderr":{"type":"string"},
        "pre_status":{"type":"string"},
        "post_status":{"type":"string"},
        "patch":{"type":"string"}
      }
    }
  }
}

docs/SPECS/file_watch_event_v0.1.schema.json (optional):

{
  "$schema":"https://json-schema.org/draft/2020-12/schema",
  "title":"nexoscli.file_watch_event.v0.1",
  "type":"object",
  "required":["schema","ts","path","op"],
  "properties":{
    "schema":{"const":"file_watch_event.v0.1"},
    "ts":{"type":"string","format":"date-time"},
    "path":{"type":"string"},
    "op":{"type":"string","enum":["Create","Write","Remove","Rename"]},
    "size":{"type":"integer","minimum":0}
  }
}


⸻

2) Code slices (additive, minimal churn)

A) Hash‑chained events (core)

Add nexoscli-core/src/ledger.rs and call it when emitting events (same helper I proposed earlier). The one below is trimmed to essentials and aligns with your ctx.ok/ctx.err flow.

// nexoscli-core/src/ledger.rs
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Serialize, Clone)]
pub struct EventV03<'a> {
  pub schema: &'static str, pub ts: String, pub run_id: &'a str, pub seq: u64,
  pub prev: String, pub content_hash: String, pub chain_hash: String,
  pub plugin: &'a str, pub command: &'a str,
  pub args_hash: Option<String>, pub exit_code: i32, pub dur_ms: u64,
  #[serde(skip_serializing_if="Option::is_none")] pub attrs: Option<serde_json::Value>,
  #[serde(skip_serializing_if="Option::is_none")] pub meta: Option<serde_json::Value>
}

pub struct Ledger {
  run_id: String, seq: u64, last_chain_hash: String,
  events_path: PathBuf, run_path: PathBuf,
  first_seq: u64, first_hash: Option<String>, last_seq: u64,
  first_ts: Option<String>, last_ts: Option<String>,
}

impl Ledger {
  pub fn open(base: &str, run_id: &str) -> anyhow::Result<Self> {
    let root = PathBuf::from(base);
    fs::create_dir_all(root.join("log").join("runs"))?;
    Ok(Self {
      run_id: run_id.into(), seq: 0, last_chain_hash: String::new(),
      events_path: root.join("log").join("events.jsonl"),
      run_path: root.join("log").join("runs").join(format!("{}.jsonl", run_id)),
      first_seq: 0, first_hash: None, last_seq: 0, first_ts: None, last_ts: None
    })
  }
  pub fn append(&mut self, plugin: &str, command: &str, exit_code: i32, dur_ms: u64,
                args_hash: Option<String>, attrs: Option<serde_json::Value>, meta: Option<serde_json::Value>,
                canonical_for_hash: &serde_json::Value) -> anyhow::Result<()> {
    self.seq += 1;
    let ts = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
    let content_hash = sha256(&serde_json::to_vec(canonical_for_hash)?);
    let chain_hash   = sha256(format!("{}{}", self.last_chain_hash, content_hash).as_bytes());
    let ev = EventV03 {
      schema:"event.v0.3", ts: ts.clone(), run_id:&self.run_id, seq:self.seq,
      prev:self.last_chain_hash.clone(), content_hash, chain_hash: chain_hash.clone(),
      plugin, command, args_hash, exit_code, dur_ms, attrs, meta
    };
    if self.first_seq==0 { self.first_seq = 1; self.first_hash = Some(ev.chain_hash.clone()); self.first_ts = Some(ts.clone()); }
    self.last_seq = self.seq; self.last_chain_hash = chain_hash; self.last_ts = Some(ts.clone());
    let line = serde_json::to_string(&ev)? + "\n";
    append(&self.run_path, &line)?; append(&self.events_path, &line)?;
    Ok(())
  }
}
impl Drop for Ledger {
  fn drop(&mut self) {
    if self.last_seq >= self.first_seq {
      let manifest = json!({
        "version":"0.2","run_id": self.run_id,"count": (self.last_seq - self.first_seq + 1),
        "first_seq": self.first_seq,"last_seq": self.last_seq,
        "first_hash": self.first_hash,"last_hash": self.last_chain_hash,
        "time_range":{"from": self.first_ts,"to": self.last_ts},"chain_ok": true
      });
      if let Some(dir) = self.run_path.parent() {
        let mf = dir.join(format!("{}.manifest.json", self.run_id));
        let _ = fs::write(mf, serde_json::to_vec_pretty(&manifest).unwrap_or_default());
      }
    }
  }
}
fn append(p:&PathBuf,s:&str)->anyhow::Result<()>{
  if let Some(d)=p.parent(){fs::create_dir_all(d)?;}
  let mut f=std::fs::OpenOptions::new().create(true).append(true).open(p)?; f.write_all(s.as_bytes())?; Ok(())
}
fn sha256(b:&[u8])->String{
  let mut h=Sha256::new(); h.update(b); format!("sha256:{:x}",h.finalize())
}

Wire it: in your single event emission point (where ctx.ok/ctx.err writes), create & cache a Ledger (e.g., as a field on CommandContext) and call append(...) after writing the envelope. Use a canonical subset of the envelope for content_hash (plugin, command, exit_code, dur_ms, args_hash, minimal attrs/meta) to avoid noise.

⸻

B) Terminal + Git capture (plugins/term)

Add pre/post git status + patch, and write stdout/stderr logs. Keep it opt‑in default ON.

Key behaviors:
	•	Before executing: snapshot git status --porcelain=v2 → status/seq-XXXX-pre.status
	•	Run command, capture stdout/stderr → term/<seq>-stdout.txt, term/<seq>-stderr.txt
	•	After: git status --porcelain=v2 → status/seq-XXXX-post.status
	•	Generate patch: git diff --patch --no-color → patches/seq-XXXX-<slug>.patch
	•	Append a term_record.v0.1 row to topics/term/commands.ndjson

Patch (simplified):

// plugins/term/src/lib.rs  (add within your existing `term run` handler)
use sha2::{Digest, Sha256};
use std::{fs, process::Command as Proc, io::Write, path::PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

fn sha256(bytes:&[u8])->String{ let mut h=Sha256::new(); h.update(bytes); format!("sha256:{:x}",h.finalize()) }
fn now()->String{ OffsetDateTime::now_utc().format(&Rfc3339).unwrap() }

fn ensure(p:&PathBuf){ if let Some(d)=p.parent(){ let _=fs::create_dir_all(d);} }

fn git_status(path:&PathBuf)->Option<Vec<u8>>{
  let out=Proc::new("git").args(["status","--porcelain=v2"]).current_dir(path).output().ok()?;
  Some(out.stdout)
}
fn git_patch(path:&PathBuf)->Option<Vec<u8>>{
  let out=Proc::new("git").args(["diff","--patch","--no-color","--no-ext-diff","-U3"]).current_dir(path).output().ok()?;
  Some(out.stdout)
}

fn write_file(p:&PathBuf,bytes:&[u8]){ ensure(p); let _=fs::write(p,bytes); }

fn sanitize_cmd(c:&str)->String{
  let mut s=c.chars().map(|ch| if ch.is_alphanumeric(){ch}else{'-'}).collect::<String>();
  if s.len()>40 { s.truncate(40); } s
}

fn term_run_record(seq:u64, run_id:&str, cwd:&PathBuf, cmdline:&str) -> anyhow::Result<serde_json::Value> {
  let root=PathBuf::from(".nexoscli");
  // pre status
  let pre = git_status(cwd).unwrap_or_default();
  let pre_path = root.join("status").join(format!("seq-{:04}-pre.status", seq));
  write_file(&pre_path,&pre);

  // exec command
  #[cfg(target_os="windows")]
  let mut child = Proc::new("cmd").args(["/C", cmdline]).current_dir(cwd).output()?;
  #[cfg(not(target_os="windows"))]
  let mut child = Proc::new("sh").args(["-lc", cmdline]).current_dir(cwd).output()?;

  let stdout = child.stdout; let stderr = child.stderr; let exit = child.status.code().unwrap_or(-1);
  let out_path = root.join("term").join(format!("seq-{:04}-stdout.txt", seq));
  let err_path = root.join("term").join(format!("seq-{:04}-stderr.txt", seq));
  write_file(&out_path,&stdout); write_file(&err_path,&stderr);

  // post and patch
  let post = git_status(cwd).unwrap_or_default();
  let post_path = root.join("status").join(format!("seq-{:04}-post.status", seq));
  write_file(&post_path,&post);

  let patch = git_patch(cwd).unwrap_or_default();
  let patch_path = root.join("patches").join(format!("seq-{:04}-{}.patch", seq, sanitize_cmd(cmdline)));
  write_file(&patch_path,&patch);

  // term record row
  let rec = serde_json::json!({
    "schema":"term_record.v0.1",
    "ts": now(), "run_id": run_id, "seq": seq,
    "cmd": cmdline, "cwd": cwd.display().to_string(),
    "exit_code": exit, "dur_ms": 0,  // if you have a timer, fill real duration
    "stdout_bytes": stdout.len(), "stderr_bytes": stderr.len(),
    "stdout_sha256": sha256(&stdout), "stderr_sha256": sha256(&stderr),
    "files": {
      "stdout": out_path.display().to_string(),
      "stderr": err_path.display().to_string(),
      "pre_status": pre_path.display().to_string(),
      "post_status": post_path.display().to_string(),
      "patch": patch_path.display().to_string()
    }
  });
  // append to topics/term/commands.ndjson
  let term_index = root.join("topics").join("term").join("commands.ndjson");
  ensure(&term_index); let mut f=fs::OpenOptions::new().create(true).append(true).open(term_index)?;
  writeln!(f,"{}", serde_json::to_string(&rec)?)?;
  Ok(rec)
}

In your term run command, allocate a per‑process seq (e.g., from the same increment used by Ledger, or own counter). Include the run_id (from CommandContext).

Why pre/post & patch even if command fails? We want real‑world diffs—even failed commands might edit files.

⸻

C) Bag build/verify/info/cat (plugins/replay)

Extend the existing replay plugin instead of creating a new one. Keep replay extract working; add bag as a clearer alias.

CLI surface
	•	nexoscli replay bag build --run-id RUN --out <dir|.tgz> --include events,term,patches,status,cline,file_watch
	•	nexoscli replay bag verify --in <dir|.tgz>
	•	nexoscli replay bag info --in <dir|.tgz> --output json
	•	nexoscli replay bag cat --in <dir|.tgz> --topic topics/events.ndjson

Implementation notes
	•	If --out ends with .tgz, create a tarball (add tar + flate2 to the plugins/replay crate).
	•	If --in is .tgz, stream using tar::Archive.
	•	Build manifest.json (bag_manifest_v0.2) and copy in run_manifest.json.
	•	For cline, select tasks in the run’s time window ±5min (you already do this).

Sketch inside plugins/replay/src/lib.rs:

// add deps in Cargo.toml: tar = "0.4", flate2 = "1"
fn bag_build(run_id:&str, out:&str, includes:&[&str]) -> anyhow::Result<serde_json::Value> {
  let base = PathBuf::from(".nexoscli");
  let run_file = base.join("log").join("runs").join(format!("{}.jsonl", run_id));
  let run_manifest = base.join("log").join("runs").join(format!("{}.manifest.json", run_id));
  anyhow::ensure!(run_file.exists(), "run not found: {}", run_id);

  // compute counts, copy files to a temp dir (if .tgz) or directly to out dir
  // ...
  // assemble manifest.json with counts + topics list
  // write README.md
  Ok(serde_json::json!({"ok": true, "out": out }))
}

fn bag_verify(input:&str) -> anyhow::Result<serde_json::Value> {
  // open dir or .tgz; read run_manifest.json and topics/events.ndjson
  // call your existing verify_chain() on topics/events.ndjson
  Ok(serde_json::json!({"ok": true}))
}

fn bag_info(input:&str) -> anyhow::Result<serde_json::Value> {
  // summarize manifest.json + counts
  Ok(serde_json::json!({"ok": true, "summary": { /* ... */ }}))
}

fn bag_cat(input:&str, topic:&str) -> anyhow::Result<()> {
  // stream topic file to stdout
  Ok(())
}

You already have replay extract—you can refactor it to use these helpers and keep the old command as a thin wrapper.

⸻

D) (Optional) File‑watch events (plugins/fswatch)

Nice‑to‑have for v0: a foreground watcher you can run while coding.

CLI
	•	nexoscli fs watch start  (blocks; ctrl‑C to stop)
	•	nexoscli fs watch start --timeout 600 (auto stop after N seconds)
	•	Emits topics/files/watch.ndjson rows with file_watch_event.v0.1 schema.

Implementation (skeleton)
	•	New crate plugins/fswatch with notify = "6":

use clap::{Arg, ArgMatches, Command as ClapCommand};
use nexoscli_core::{plugin::{Plugin, CommandContext}, Result};
use notify::{RecommendedWatcher, Config, EventKind, RecursiveMode, Watcher};
use std::{path::PathBuf, sync::mpsc::channel, time::Duration, fs, io::Write};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub struct FsWatchPlugin;
impl FsWatchPlugin { pub fn new() -> Self { Self } }

fn now()->String{ OffsetDateTime::now_utc().format(&Rfc3339).unwrap() }

impl Plugin for FsWatchPlugin {
  fn name(&self)->&'static str{"fs"}
  fn version(&self)->&'static str{env!("CARGO_PKG_VERSION")}
  fn describe(&self)->&'static str{"File watch (write events to bag topic)"}

  fn augment_commands(&self, app: ClapCommand)->ClapCommand{
    app.subcommand(
      ClapCommand::new("fs").about("File utilities")
        .subcommand(
          ClapCommand::new("watch").about("Watch filesystem and emit events")
            .arg(Arg::new("path").long("path").value_name("DIR").default_value("."))
            .arg(Arg::new("timeout").long("timeout").value_parser(clap::value_parser!(u64)))
        )
    )
  }

  fn execute(&self, m:&ArgMatches, ctx:&CommandContext)->Result<i32>{
    match m.subcommand() {
      Some(("watch", sm))=>{
        let path = PathBuf::from(sm.get_one::<String>("path").unwrap());
        let timeout = sm.get_one::<u64>("timeout").cloned();
        let (tx, rx) = std::sync::mpsc::channel();

        let mut w: RecommendedWatcher = Watcher::new(tx, Config::default())?;
        w.watch(&path, RecursiveMode::Recursive)?;

        let topic = PathBuf::from(".nexoscli/topics/files/watch.ndjson");
        if let Some(d)=topic.parent(){ let _ = fs::create_dir_all(d); }
        let mut f = fs::OpenOptions::new().create(true).append(true).open(&topic)?;

        let start = std::time::Instant::now();
        loop {
          if let Ok(ev)=rx.recv_timeout(Duration::from_millis(500)) {
            if let Ok(e)=ev {
              let op = match e.kind {
                EventKind::Create(_) => "Create",
                EventKind::Modify(_) => "Write",
                EventKind::Remove(_) => "Remove",
                EventKind::Rename{..} => "Rename",
                _ => "Write"
              };
              for p in e.paths {
                let size = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                let row = serde_json::json!({"schema":"file_watch_event.v0.1","ts":now(),
                  "path":p.display().to_string(),"op":op,"size":size});
                writeln!(f,"{}", serde_json::to_string(&row).unwrap()).ok();
              }
            }
          }
          if let Some(ms)=timeout {
            if start.elapsed() >= Duration::from_millis(ms) { break; }
          }
        }
        Ok(ctx.ok("fs","watch",&serde_json::json!({"ok":true,"path": path})))
      }
      _=>Ok(2)
    }
  }
}

Register plugin in nexoscli-bin/src/main.rs and workspace/Cargo.

This is optional; term + git patches already give you strong file‑change observability for v0.

⸻

3) Docs (drop‑ins)

Update docs/TRACEPACK.md → describe NEXBag/TracePack v0.2 as above and add a short “ROS bag analogy”:
	•	Topics = events, term/commands, files/watch, cline/tasks
	•	Manifests = manifest.json + run_manifest.json
	•	“Build/Verify/Info/Cat” commands with examples

Update docs/OBSERVABILITY.md → add Event v0.3 fields and a 4‑line example. Note where term logs + patches are stored locally, and that bag build pulls them under a single root.

Create docs/SPECS/ with the JSON schemas above.

⸻

4) Tests (E2E, add under nexoscli-bin/tests/)
	1.	envelope_hash_chain.rs
	•	Run two simple commands (nexoscli issue new ... twice).
	•	Assert .nexoscli/log/runs/<RUN>.jsonl has seq=1,2 and chain_hash == sha256(prev + content_hash).
	2.	term_capture.rs
	•	Inside a temp git repo, create/modify a file via nexoscli term run -- 'printf hi >> foo.txt'.
	•	Assert status/seq-0001-pre.status, ...post.status, patches/seq-0001-*.patch, term/seq-0001-stdout.txt exist and non‑empty.
	3.	bag_build_verify_info.rs
	•	nexoscli replay bag build --run-id RUN --out bag.tgz --include events,term,patches,status.
	•	nexoscli replay bag verify --in bag.tgz returns { ok:true }.
	•	nexoscli replay bag info --in bag.tgz --output json returns counts > 0.
	4.	cline_inclusion.rs
	•	Set NEXOS_CLINE_TEST_ROOT and import a small task.
	•	bag build --include cline → tar contains cline/tasks/*.json; manifest counts reflect it.

(If you enable fswatch) add fs_watch.rs: start watcher with --timeout 1500, touch a file, assert a row exists in topics/files/watch.ndjson.

⸻

5) CI guardrails
	•	Doctor: add events_integrity check that runs:

nexoscli dev telemetry report --since 24h --metrics --output json
jq -r '.data.runs[].run_id' report.json | head -n 3 | while read RUN; do
  nexoscli replay verify --run-id "$RUN" || exit 1
done


	•	Bag in CI (optional on failures):

nexoscli replay bag build --run-id "$RUN" --out artifacts/tracepack.tgz --include events,term,patches,status,cline



⸻

6) Day‑to‑day usage (what you run)

# Normal dev (recommended)
nexoscli task start --task TASK-...
nexoscli term run -- 'cargo test --workspace'
nexoscli term run -- 'cargo clippy'
nexoscli cline import --since 24h

# Build a bag for this CLI run
RUN_ID=$(jq -r '.meta.run_id' .nexoscli/log/runs/*.manifest.json | tail -n1)
nexoscli replay bag build --run-id "$RUN_ID" --include events,term,patches,status,cline --out tracepack.tgz
nexoscli replay bag verify --in tracepack.tgz
nexoscli replay bag info   --in tracepack.tgz --output json

You can now archive/share tracepack.tgz like a ROS bag and do offline analysis.

⸻

7) Why this nails Observability v0
	•	End‑to‑end provenance: Hash‑chained events + per‑run anchors → tamper‑evident history.
	•	Repro & forensics: Terminal logs + git diffs + status snapshots capture what changed and how.
	•	AI/analytics‑ready: Stable schemas; JSONL topics; deterministic bag.
	•	Cline parity: Context imported & co‑packed; easy correlation.
	•	Temporal‑ready: All of this is engine‑agnostic. When you flip to Temporal, its History is just an additional topic; your bag stays the SSOT.

⸻

8) Small issue backlog to open now
	•	core: add Ledger and wire into event emission (hash‑chain v0.3)
	•	term: add pre/post status, patch, stdout/err logs, term_record rows
	•	replay: implement bag build|verify|info|cat (alias extract to bag build)
	•	cline: ensure --since window is used by bag builder for inclusion
	•	docs: add docs/SPECS/ + update OBSERVABILITY.md, TRACEPACK.md
	•	tests: 4 E2Es above (+1 watcher if enabled)
	•	ci: doctor events_integrity gate

⸻

If you want, I can tailor the CommandContext hook (exact spot to call Ledger::append) to your current nexoscli-core/src/envelope.rs once you paste that file; the patch is small (<30 LOC) and will make the chain live immediately.