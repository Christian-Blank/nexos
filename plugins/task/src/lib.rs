use clap::{Arg, ArgMatches, Command as ClapCommand, ValueHint};
use nexoscli_core::{Plugin, CommandContext, Result, engine::{engine_from, EngineKind}};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::{Path, PathBuf}};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use walkdir::WalkDir;

const ALLOWED_STAGES: &[&str] = &["planning","implementation","verification","done","blocked"];
const TASKS_DIR: &str = ".nexoscli/tasks";
const CURRENT_FILE: &str = ".nexoscli/task/current";

pub struct TaskPlugin;
impl TaskPlugin { pub fn new() -> Self { Self } }

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct Session { 
  id: String,            // NEW: stable identifier for the session
  start: String, 
  end: Option<String> 
}

fn gen_session_id() -> String {
  use sha2::{Digest, Sha256};
  let seed = format!("{}-{}", time::OffsetDateTime::now_utc().unix_timestamp_nanos(), std::process::id());
  let d = Sha256::digest(seed.as_bytes());
  format!("SES-{}", &hex::encode(d)[..12])
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct TaskRecord {
  id: String,
  title: String,
  stage: String,        // planning|implementation|verification|done|blocked
  status: String,       // new|active|paused|ended
  created_at: String,
  started_at: Option<String>,
  ended_at: Option<String>,
  sessions: Vec<Session>,
  notes: Vec<String>,
}

impl TaskRecord {
  fn path(id: &str) -> PathBuf { PathBuf::from(TASKS_DIR).join(id).join("task.json") }
  fn load(id: &str) -> anyhow::Result<Self> {
    let p = Self::path(id);
    let s = fs::read_to_string(&p)?;
    Ok(serde_json::from_str(&s)?)
  }
  fn save(&self) -> anyhow::Result<()> {
    let dir = PathBuf::from(TASKS_DIR).join(&self.id);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("task.json"), serde_json::to_vec_pretty(self)?)?;
    Ok(())
  }
}

impl Plugin for TaskPlugin {
  fn name(&self) -> &'static str { "task" }
  fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
  fn describe(&self) -> &'static str { "Tasks: new/start/pause/resume/stage/status/list/end/bundle" }

  fn augment_commands(&self, app: ClapCommand) -> ClapCommand {
    app.subcommand(
      ClapCommand::new("task").about(self.describe())
        .subcommand(
          ClapCommand::new("new").about("Create a new task")
            .arg(Arg::new("title").long("title").required(true))
            .arg(Arg::new("id").long("id").value_hint(ValueHint::Other).help("Optional explicit id (defaults to TASK-yyyymmdd-<slug>)"))
        )
        .subcommand(
          ClapCommand::new("start").about("Start a task (make it active)")
            .arg(Arg::new("task").long("task"))
            .arg(Arg::new("name").long("name"))
            .arg(engine_arg())
        )
        .subcommand(ClapCommand::new("pause").about("Pause the active task").arg(Arg::new("task").long("task")))
        .subcommand(ClapCommand::new("resume").about("Resume a paused task")
            .arg(Arg::new("task").long("task"))
            .arg(Arg::new("name").long("name")))
        .subcommand(
          ClapCommand::new("stage").about("Set the stage for a task")
            .arg(Arg::new("stage").required(true).value_parser(["planning","implementation","verification","done","blocked"]))
            .arg(Arg::new("task").long("task"))
            .arg(engine_arg())
        )
        .subcommand(ClapCommand::new("status").about("Show task status").arg(Arg::new("task").long("task")))
        .subcommand(ClapCommand::new("list").about("List tasks"))
        .subcommand(
          ClapCommand::new("end").about("End the current (or given) task")
            .arg(Arg::new("task").long("task"))
            .arg(engine_arg())
        )
        .subcommand(
          ClapCommand::new("bundle").about("Build a TracePack bundle for a task window")
            .arg(Arg::new("task").long("task"))
            .arg(Arg::new("name").long("name").required(false))
            .arg(Arg::new("out").long("out").required(false))
            .arg(Arg::new("include").long("include").default_value("all")
                 .help("CSV from: events,patches,status,cline,all"))
        )
    )
  }

  fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    match m.subcommand() {
      Some(("new", sm)) => {
        let title = sm.get_one::<String>("title").unwrap().to_string();
        let id = sm.get_one::<String>("id").map(|s| s.to_string())
          .unwrap_or_else(|| format!("TASK-{}-{}", today(), slugify(&title)));
        let rec = TaskRecord {
          id: id.clone(), title,
          stage: "planning".into(), status: "new".into(),
          created_at: now(),
          ..Default::default()
        };
        rec.save()?;
        let out = json!({"id": rec.id, "stage": rec.stage, "status": rec.status});
        Ok(ctx.ok("task","new",&out))
      }

      Some(("start", sm)) => {
        let engine_kind = engine_kind_from(sm);
        let id = resolve_task_id(sm.get_one::<String>("task"), sm.get_one::<String>("name"))?;
        ensure_no_unpaused_active(ctx)?;
        
        // Try to load existing task, or create a new one if it doesn't exist
        let mut rec = match TaskRecord::load(&id) {
          Ok(rec) => rec,
          Err(_) => {
            // Task doesn't exist, create a new one
            let title = sm.get_one::<String>("name").cloned()
              .or_else(|| sm.get_one::<String>("task").cloned())
              .unwrap_or_else(|| id.clone());
            TaskRecord {
              id: id.clone(),
              title,
              stage: "planning".into(),
              status: "new".into(),
              created_at: now(),
              ..Default::default()
            }
          }
        };
        
        // update
        if rec.started_at.is_none() { rec.started_at = Some(now()); }
        rec.status = "active".into();
        if rec.stage.is_empty() { rec.stage = "implementation".into(); }
        let sid = gen_session_id();
        rec.sessions.push(Session{ id: sid.clone(), start: now(), end: None });
        rec.save()?;
        // mark current
        set_current(&rec.id)?;
        set_current_session(&sid)?;
        
        // Engine integration
        let eng = engine_from(engine_kind);
        if matches!(engine_kind, EngineKind::TemporalStub) {
            let attrs = temporal_attrs_for_task(&rec);
            let input = serde_json::json!({"action":"start"});
            eng.start("task", &rec.id, attrs, input)?;
            // NOTE: we deliberately **do not** run hooks for Temporal here
        } else {
            // Local engine: behavior unchanged (hooks run on stage transitions)
        }
        
        let out = json!({"id": rec.id, "stage": rec.stage, "status": rec.status});
        Ok(ctx.ok("task","start",&out))
      }

      Some(("pause", sm)) => {
        let id = sm.get_one::<String>("task").cloned().or_else(current_id);
        let id = id.ok_or_else(|| anyhow::anyhow!("no active task; pass --task"))?;
        let mut rec = TaskRecord::load(&id)?;
        if rec.status != "active" {
          return Ok(ctx.err("task","pause","task.pause.not_active",&format!("task '{}' is not active", id),None,1));
        }
        if let Some(last) = rec.sessions.last_mut() {
          if last.end.is_none() { last.end = Some(now()); }
        }
        rec.status = "paused".into();
        rec.save()?;
        clear_current()?;
        clear_current_session()?;        // NEW
        let out = json!({"id": rec.id, "status": rec.status});
        Ok(ctx.ok("task","pause",&out))
      }

      Some(("resume", sm)) => {
        let id = resolve_task_id(sm.get_one::<String>("task"), sm.get_one::<String>("name"))?;
        ensure_no_unpaused_active(ctx)?;
        let mut rec = TaskRecord::load(&id)?;
        rec.status = "active".into();
        let sid = gen_session_id();
        rec.sessions.push(Session{ id: sid.clone(), start: now(), end: None });
        rec.save()?;
        set_current(&rec.id)?;
        set_current_session(&sid)?;      // NEW
        let out = json!({"id": rec.id, "status": rec.status});
        Ok(ctx.ok("task","resume",&out))
      }

      Some(("stage", sm)) => {
        let engine_kind = engine_kind_from(sm);
        let stage = sm.get_one::<String>("stage").unwrap().to_string();
        let id = sm.get_one::<String>("task").cloned().or_else(current_id)
          .ok_or_else(|| anyhow::anyhow!("no task specified; pass --task or have one active"))?;
        let mut rec = TaskRecord::load(&id)?;
        rec.stage = stage.clone();
        rec.save()?;
        
        // Engine integration
        let eng = engine_from(engine_kind);
        if matches!(engine_kind, EngineKind::TemporalStub) {
            let attrs = temporal_attrs_for_task(&rec);
            let payload = serde_json::json!({"action":"stage_enter","stage": stage});
            eng.signal("task", &rec.id, "stage_enter", attrs, payload)?;
            // **Skip** gates/hooks under Temporal for now (the worker will own them)
        } else {
            // Local: gates/hooks as you already implemented
            // TODO: Add gate evaluation and hook execution for local engine
            // For now, this preserves existing behavior (no gates/hooks implemented yet)
        }
        
        let out = json!({"id": rec.id, "stage": rec.stage});
        Ok(ctx.ok("task","stage",&out))
      }

      Some(("status", sm)) => {
        let id = sm.get_one::<String>("task").cloned().or_else(current_id)
          .ok_or_else(|| anyhow::anyhow!("no task specified; pass --task or have one active"))?;
        let rec = TaskRecord::load(&id)?;
        let counts = counts_for_task(&id);
        let out = json!({
          "id": rec.id, "title": rec.title, "stage": rec.stage, "status": rec.status,
          "created_at": rec.created_at, "started_at": rec.started_at, "ended_at": rec.ended_at,
          "sessions": rec.sessions, "counts": counts
        });
        Ok(ctx.ok("task","status",&out))
      }

      Some(("list", _)) => {
        let mut items = vec![];
        let d = PathBuf::from(TASKS_DIR);
        if d.exists() {
          for ent in WalkDir::new(&d).min_depth(1).max_depth(1).into_iter().flatten() {
            if ent.path().is_dir() {
              let p = ent.path().join("task.json");
              if p.exists() {
                if let Some(rec) = fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str::<TaskRecord>(&s).ok()) {
                  items.push(rec);
                }
              }
            }
          }
        }
        let out = json!({"items": items});
        Ok(ctx.ok("task","list",&out))
      }

      Some(("end", sm)) => {
        let engine_kind = engine_kind_from(sm);
        let id = sm.get_one::<String>("task").cloned().or_else(current_id)
          .ok_or_else(|| anyhow::anyhow!("no task specified; pass --task or have one active"))?;
        let mut rec = TaskRecord::load(&id)?;
        if let Some(last) = rec.sessions.last_mut() { if last.end.is_none() { last.end = Some(now()); } }
        rec.status = "ended".into();
        if rec.stage != "done" { rec.stage = "done".into(); }
        rec.ended_at = Some(now());
        rec.save()?;
        
        // Engine integration
        let eng = engine_from(engine_kind);
        if matches!(engine_kind, EngineKind::TemporalStub) {
            let attrs = temporal_attrs_for_task(&rec);
            let payload = serde_json::json!({"action":"end"});
            eng.signal("task", &rec.id, "end", attrs, payload)?;
        }
        
        let out = json!({"id": rec.id, "status": rec.status, "stage": rec.stage});
        // Clear current and session AFTER generating the response so this command gets task context
        let result = ctx.ok("task","end",&out);
        clear_current().ok();
        clear_current_session().ok();
        Ok(result)
      }

      Some(("bundle", sm)) => {
        let id = resolve_task_id(sm.get_one::<String>("task"), sm.get_one::<String>("name"))?;
        let out_path = sm.get_one::<String>("out")
          .map(|s| s.to_string())
          .unwrap_or_else(|| default_out_for(&id, sm.get_one::<String>("name")));
        let include = sm.get_one::<String>("include").unwrap().to_string();

        let (events, first_ts, last_ts) = collect_task_events(&id)?;
        let mut counts = BundleCounts::default();
        counts.events_lines = events.lines().count() as u64;

        // staging
        let stage_dir = PathBuf::from(".nexoscli").join("tracepacks").join(format!("{}", &id));
        fs::create_dir_all(&stage_dir)?;
        // write events.ndjson and task.json
        fs::write(stage_dir.join("events.ndjson"), events.as_bytes())?;
        fs::create_dir_all(stage_dir.join("task"))?;
        fs::copy(TaskRecord::path(&id), stage_dir.join("task").join("task.json"))?;

        // patches/status from events payloads
        let mut manifest_patches = vec![];
        let mut manifest_status  = vec![];
        if include == "all" || include.contains("patches") || include.contains("status") {
            for line in events.lines() {
              if let Ok(v) = serde_json::from_str::<Value>(line) {
                if v.get("plugin").and_then(|x| x.as_str()) == Some("term")
                  && v.get("command").and_then(|x| x.as_str()) == Some("run") {
                    if include == "all" || include.contains("patches") {
                      if let Some(p) = v.pointer("/data/patch_path").and_then(|x| x.as_str()) {
                        let src = Path::new(p);
                        if src.exists() {
                          let dst_dir = stage_dir.join("patches");
                          fs::create_dir_all(&dst_dir)?;
                          let dst = dst_dir.join(format!("seq-{:04}-{}", v.get("seq").and_then(|x| x.as_u64()).unwrap_or(0), src.file_name().unwrap().to_string_lossy()));
                          fs::copy(src, &dst).ok();
                          counts.patches += 1;
                          let body = fs::read(&dst).unwrap_or_default();
                          manifest_patches.push(json!({"seq": v["seq"], "dst": format!("patches/{}", dst.file_name().unwrap().to_string_lossy()), "sha256": sha256(&body), "bytes": body.len()}));
                        }
                      }
                    }
                    if include == "all" || include.contains("status") {
                      if let Some(p) = v.pointer("/data/status_path").and_then(|x| x.as_str()) {
                        let src = Path::new(p);
                        if src.exists() {
                          let dst_dir = stage_dir.join("status");
                          fs::create_dir_all(&dst_dir)?;
                          let dst = dst_dir.join(format!("seq-{:04}-{}", v.get("seq").and_then(|x| x.as_u64()).unwrap_or(0), src.file_name().unwrap().to_string_lossy()));
                          fs::copy(src, &dst).ok();
                          counts.status += 1;
                          let body = fs::read(&dst).unwrap_or_default();
                          manifest_status.push(json!({"seq": v["seq"], "dst": format!("status/{}", dst.file_name().unwrap().to_string_lossy()), "sha256": sha256(&body), "bytes": body.len()}));
                        }
                      }
                    }
                }
              }
            }
        }

        // cline tasks by time window (+/-5m pad)
        let mut manifest_cline = vec![];
        if include == "all" || include.contains("cline") {
          let pad = time::Duration::seconds(300);
          let from = first_ts.as_ref().and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()).map(|t| t - pad);
          let to   = last_ts.as_ref().and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()).map(|t| t + pad);
          let src = PathBuf::from(".nexoscli").join("cline").join("tasks");
          if src.exists() {
            let dst = stage_dir.join("cline").join("tasks");
            fs::create_dir_all(&dst)?;
            for ent in WalkDir::new(&src).min_depth(1).max_depth(2).into_iter().flatten() {
              let p = ent.path();
              if p.is_dir() || p.extension().and_then(|x| x.to_str()) != Some("json") { continue; }
              if within_mtime(p, from, to) {
                let raw = fs::read(p).unwrap_or_default();
                let id  = serde_json::from_slice::<Value>(&raw)
                          .ok().and_then(|v| v.get("id").and_then(|x| x.as_str()).map(|s| s.to_string()))
                          .unwrap_or_else(|| format!("anon-{}", &sha256(&raw)[7..19]));
                let base = format!("{}-{}.json", "task", id.replace(|c: char| !c.is_ascii_alphanumeric(), "_"));
                let dstf = dst.join(base);
                fs::write(&dstf, &raw)?;
                counts.cline += 1;
                manifest_cline.push(json!({"id": id, "dst": format!("cline/tasks/{}", dstf.file_name().unwrap().to_string_lossy()), "sha256": sha256(&raw), "bytes": raw.len()}));
              }
            }
          }
        }

        // manifest - get session id from task record
        let rec_for_manifest = TaskRecord::load(&id)?;
        let manifest_session_id = rec_for_manifest.sessions.last().map(|s| s.id.clone())
            .or_else(|| current_session_id());

        let manifest = json!({
          "version":"0.1","kind":"tracepack.task",
          "task_id": id,
          "session_id": manifest_session_id,       // <- use last session or current
          "created_at": now(),
          "time_range": { "from": first_ts, "to": last_ts },
          "counts": { "events_lines": counts.events_lines, "patches": counts.patches, "status": counts.status, "cline": counts.cline },
          "patches": manifest_patches,
          "status_files": manifest_status,
          "cline_tasks": manifest_cline
        });
        fs::write(stage_dir.join("manifest.json"), serde_json::to_vec_pretty(&manifest)?)?;
        fs::write(stage_dir.join("README.md"), TASK_README.as_bytes())?;

        // output: dir or tgz
        let final_out = if out_path.ends_with(".tgz") || out_path.ends_with(".tar.gz") {
          let parent = Path::new(&out_path).parent().unwrap_or(Path::new("."));
          fs::create_dir_all(parent)?;
          let f = std::fs::File::create(&out_path)?;
          let enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
          let mut tar = tar::Builder::new(enc);
          for ent in WalkDir::new(&stage_dir).into_iter().flatten() {
            let p = ent.path();
            if p.is_dir() { continue; }
            let rel = p.strip_prefix(&stage_dir).unwrap();
            tar.append_path_with_name(p, rel)?;
          }
          tar.into_inner()?.finish()?;
          out_path
        } else {
          if Path::new(&out_path) != stage_dir {
            if Path::new(&out_path).exists() { 
              return Ok(ctx.err("task","bundle","task.bundle.exists",&format!("destination exists: {}", out_path),Some("use different --out path or remove existing"),1));
            }
            fs::create_dir_all(Path::new(&out_path).parent().unwrap_or(Path::new(".")))?;
            fs::rename(&stage_dir, &out_path)?;
          }
          out_path
        };

        let out = json!({"task": id, "out": final_out});
        Ok(ctx.ok("task","bundle",&out))
      }

      _ => Ok(ctx.err("task","unknown","task.unknown","unknown task command",Some("use: new|start|pause|resume|stage|status|list|end|bundle"),2)),
    }
  }
}

// -------- helpers --------

fn today() -> String { OffsetDateTime::now_utc().format(&time::macros::format_description!("[year][month][day]")).unwrap() }
fn now() -> String { OffsetDateTime::now_utc().format(&Rfc3339).unwrap() }

fn slugify(s: &str) -> String {
  let t = s.to_lowercase().chars()
    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
    .collect::<String>();
  let t = regex::Regex::new("-+").unwrap().replace_all(&t, "-").to_string();
  t.trim_matches('-').to_string()
}

fn resolve_task_id(id_opt: Option<&String>, name_opt: Option<&String>) -> anyhow::Result<String> {
  if let Some(id) = id_opt { return Ok(id.clone()); }
  if let Some(name) = name_opt {
    let slug = slugify(name);
    // try to find an existing task whose id ends with the slug
    let d = PathBuf::from(TASKS_DIR);
    if d.exists() {
      for ent in WalkDir::new(&d).min_depth(1).max_depth(1).into_iter().flatten() {
        if ent.path().is_dir() {
          let tid = ent.file_name().to_string_lossy().to_string();
          if tid.ends_with(&slug) { return Ok(tid); }
        }
      }
    }
    // not found: create a fresh TaskRecord
    let id = format!("TASK-{}-{}", today(), slug);
    let rec = TaskRecord{
      id: id.clone(), title: name.to_string(),
      stage: "planning".into(), status: "new".into(),
      created_at: now(), ..Default::default()
    };
    rec.save()?;
    return Ok(id);
  }
  // fallback to active
  if let Some(cur) = current_id() { return Ok(cur); }
  Err(anyhow::anyhow!("no task specified; pass --task or --name"))
}

fn default_out_for(id: &str, name_opt: Option<&String>) -> String {
  if let Some(name) = name_opt {
    let slug = slugify(name);
    format!("{}.tgz", slug)
  } else {
    format!("{}.tgz", id)
  }
}

fn set_current(id: &str) -> anyhow::Result<()> {
  let p = PathBuf::from(".nexoscli").join("task");
  fs::create_dir_all(&p)?;
  fs::write(p.join("current"), id.as_bytes())?;
  Ok(())
}
fn clear_current() -> anyhow::Result<()> {
  let p = PathBuf::from(".nexoscli").join("task").join("current");
  let _ = fs::remove_file(p);
  Ok(())
}
fn current_id() -> Option<String> {
  fs::read_to_string(PathBuf::from(CURRENT_FILE)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn set_current_session(id: &str) -> anyhow::Result<()> {
  let p = PathBuf::from(".nexoscli").join("task");
  fs::create_dir_all(&p)?;
  fs::write(p.join("current_session"), id.as_bytes())?;
  Ok(())
}
fn clear_current_session() -> anyhow::Result<()> {
  let p = PathBuf::from(".nexoscli").join("task").join("current_session");
  let _ = fs::remove_file(p);
  Ok(())
}
fn current_session_id() -> Option<String> {
  fs::read_to_string(PathBuf::from(".nexoscli").join("task").join("current_session"))
    .ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn ensure_no_unpaused_active(_ctx: &CommandContext) -> anyhow::Result<()> {
  if let Some(id) = current_id() {
    // if a "current" exists, we consider it unpaused → block
    return Err(anyhow::anyhow!(format!("task '{}' is active; pause it before starting/resuming another task", id)));
  }
  Ok(())
}

#[derive(Default)]
struct BundleCounts { events_lines: u64, patches: u64, status: u64, cline: u64 }

fn within_mtime(p: &Path, from: Option<OffsetDateTime>, to: Option<OffsetDateTime>) -> bool {
  if let Ok(meta) = fs::metadata(p) {
    if let Ok(m) = meta.modified() {
      let t = OffsetDateTime::from(m);
      if let Some(f) = from { if t < f { return false; } }
      if let Some(x) = to   { if t > x { return false; } }
    }
  }
  true
}

fn counts_for_task(task_id: &str) -> serde_json::Value {
  // scan events from the actual events.jsonl file
  let events_file = PathBuf::from(".nexoscli").join("log").join("events.jsonl");
  let mut events = 0u64; let mut patches = 0u64; let mut cline = 0u64;
  if events_file.exists() {
    let txt = fs::read_to_string(&events_file).unwrap_or_default();
    for line in txt.lines() {
      if let Ok(v) = serde_json::from_str::<Value>(line) {
        if v.get("context").and_then(|c| c.get("task_id")).and_then(|x| x.as_str()) == Some(task_id) {
          events += 1;
          if v.get("plugin").and_then(|x| x.as_str()) == Some("term")
            && v.get("command").and_then(|x| x.as_str()) == Some("run")
            && v.pointer("/data/patch_path").is_some() { patches += 1; }
          if v.get("plugin").and_then(|x| x.as_str()) == Some("cline") { cline += 1; }
        }
      }
    }
  }
  json!({"events": events, "patches": patches, "cline": cline})
}

fn collect_task_events(task_id: &str) -> anyhow::Result<(String, Option<String>, Option<String>)> {
  let events_file = PathBuf::from(".nexoscli").join("log").join("events.jsonl");
  let mut out = String::new();
  let mut first: Option<String> = None;
  let mut last:  Option<String> = None;
  if events_file.exists() {
    let txt = fs::read_to_string(&events_file).unwrap_or_default();
    for line in txt.lines() {
      if let Ok(v) = serde_json::from_str::<Value>(line) {
        if v.get("context").and_then(|c| c.get("task_id")).and_then(|x| x.as_str()) == Some(task_id) {
          out.push_str(line); out.push('\n');
          if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) {
            if first.as_ref().map(|s| ts < s.as_str()).unwrap_or(true) { first = Some(ts.to_string()); }
            if last.as_ref().map(|s| ts > s.as_str()).unwrap_or(true) { last = Some(ts.to_string()); }
          }
        }
      }
    }
  }
  Ok((out, first, last))
}

fn sha256(b: &[u8]) -> String {
  use sha2::{Digest, Sha256};
  format!("sha256:{:x}", Sha256::digest(b))
}

fn engine_arg() -> clap::Arg {
    clap::Arg::new("engine")
        .long("engine")
        .value_parser(["local","temporal"])
        .default_value("local")
        .help("Execution engine for this action")
}

fn engine_kind_from(matches: &clap::ArgMatches) -> EngineKind {
    match matches.get_one::<String>("engine").map(|s| s.as_str()) {
        Some("temporal") => EngineKind::TemporalStub,
        _ => EngineKind::Local,
    }
}

fn git_context() -> serde_json::Value {
    let mut repo = String::new(); let mut branch = String::new();
    if let Ok(out) = std::process::Command::new("git").args(["rev-parse","--show-toplevel"]).output() {
        if out.status.success() { repo = String::from_utf8_lossy(&out.stdout).trim().to_string(); }
    }
    if let Ok(out) = std::process::Command::new("git").args(["rev-parse","--abbrev-ref","HEAD"]).output() {
        if out.status.success() { branch = String::from_utf8_lossy(&out.stdout).trim().to_string(); }
    }
    serde_json::json!({ "repo": if repo.is_empty(){serde_json::Value::Null}else{serde_json::Value::String(repo)},
                        "branch": if branch.is_empty(){serde_json::Value::Null}else{serde_json::Value::String(branch)} })
}

fn temporal_attrs_for_task(task: &TaskRecord) -> serde_json::Value {
    // **pre-Temporal** mapping (you can formalize in a mapping file later)
    let ctx = git_context();
    let sid = current_session_id();
    serde_json::json!({
      "task_id": task.id, "stage": task.stage, "status": task.status,
      "session_id": sid, "repo": ctx.get("repo"), "branch": ctx.get("branch")
    })
}

const TASK_README: &str = r#"# Task TracePack

This bundle captures a single task's history:
- `task/task.json` — task ledger (id, stage, sessions)
- `events.ndjson`  — all events tagged with this task_id
- `patches/`       — git diffs captured via `nexoscli term run`
- `status/`        — `git status --porcelain` snapshots
- `cline/tasks/`   — normalized Cline tasks in the time window (+/- 5m)

Use `nexoscli replay plan/run` for patch-level replay on a safe branch.
"#;
