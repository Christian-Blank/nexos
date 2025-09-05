You’re absolutely right: anything that requires inline Python or complex shell heredocs inside Cline prompts is brittle. The right fix is to move those steps into nexoscli so Cline (or any human) can just run one or two robust CLI commands. Below I give you a compact, production‑ready slice that:
	•	Adds plan lifecycle commands to the dev plugin (dev plans …) to find the next plan, start it (create & start a task, update plan front‑matter, move next → current), and finalize it (write implemented commit, end task, bundle, move current → done).
	•	Keeps all logic in Rust (no Python scripts), with stable JSON envelopes and testable behavior.
	•	Works with your current tree (docs/project/{next,current,done,archived} and both PLAN-*.md / plan-*.md naming).
	•	Preserves your guardrails (ctx.ok/ctx.err envelopes, task correlation, minimal diffs).

⸻

Plan 34 — Plan lifecycle CLI (start/finalize)

Goal: Replace ad‑hoc scripts in prompts with first‑class CLI guardrails.

1) Add a dependency (dev plugin)

plugins/dev/Cargo.toml — add serde_yaml if it’s not already available from workspace:

[dependencies]
# ...existing...
serde_yaml = { workspace = true }

(If you don’t have a workspace pin, serde_yaml = "0.9" is fine.)

⸻

2) Extend the dev plugin with plans commands

plugins/dev/src/lib.rs — add the following changes:
	•	Top use additions (near the other uses):

use serde_yaml; // add
use std::ffi::OsStr;

	•	New args/results structs (place near other structs):

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanStartArgs {
    #[serde(default)]
    plan: Option<String>,
    #[serde(default)]
    scope: Option<String>,   // informational; used in commit messages downstream
    #[serde(default)]
    dry_run: bool,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanStartResult {
    picked_plan: String,
    moved_from: String,
    moved_to: String,
    task_id: String,
    title: String,
    plan_id: String,
    slug: String,
    updated_frontmatter: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanFinalizeArgs {
    #[serde(default)]
    plan: Option<String>,          // if omitted, single plan under current is assumed
    #[serde(default)]
    bundle_out: Option<String>,    // default: <TASK_ID>-bundle.tgz in repo root
    #[serde(default)]
    implemented_in: Option<String> // override the commit sha to record; default: HEAD
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanFinalizeResult {
    plan: String,
    moved_to: String,
    task_id: Option<String>,
    implemented_in: String,
    bundle_out: Option<String>,
    frontmatter: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanListArgs {
    #[serde(default)]
    r#where: Option<String>, // "next"|"current"|"done"|"archived"| "all"
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanRow {
    folder: String,
    file: String,
    plan_id: Option<String>,
    slug: Option<String>,
    title: Option<String>,
    status: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanListResult {
    total: usize,
    items: Vec<PlanRow>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanStatusArgs {
    plan: String,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanStatusResult {
    plan: String,
    frontmatter: serde_json::Value,
    title: Option<String>,
    plan_id: Option<String>,
    slug: Option<String>,
    status: Option<String>,
}

	•	Augment commands (inside augment_commands under the existing .subcommand(Command::new("dev")…) block), add a “plans” group:

// dev plans (project plan lifecycle)
.subcommand(
    Command::new("plans").about("Project plan lifecycle")
      .subcommand(
        Command::new("list").about("List plans")
          .arg(Arg::new("where").long("where").value_parser(["next","current","done","archived","all"]).default_value("all"))
      )
      .subcommand(
        Command::new("next").about("Show the next plan candidate under docs/project/next")
      )
      .subcommand(
        Command::new("status").about("Show plan front-matter and derived ids")
          .arg(Arg::new("plan").long("plan").required(true).value_name("PATH"))
      )
      .subcommand(
        Command::new("start").about("Start a plan: create+start task, update front-matter, move next→current")
          .arg(Arg::new("plan").long("plan").required(false).value_name("PATH"))
          .arg(Arg::new("scope").long("scope").required(false).value_name("LABEL"))
          .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
      )
      .subcommand(
        Command::new("finalize").about("Finalize a plan: mark done, end task, bundle, move current→done")
          .arg(Arg::new("plan").long("plan").required(false).value_name("PATH"))
          .arg(Arg::new("bundle-out").long("bundle-out").required(false).value_name("PATH"))
          .arg(Arg::new("implemented-in").long("implemented-in").required(false).value_name("SHA"))
      )
)

	•	Execute branch (inside execute(&self, sub_matches, ctx)), add a new arm for "plans" with handlers:

            // ---------------- dev plans ----------------
            Some(("plans", pm)) => {
                match pm.subcommand() {
                    Some(("list", m)) => {
                        let which = m.get_one::<String>("where").cloned().unwrap_or_else(|| "all".to_string());
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        let items = list_plans(&root, &which)?;
                        let out = PlanListResult{ total: items.len(), items };
                        let exit = ctx.ok("dev","plans.list",&out);
                        Ok(exit)
                    }
                    Some(("next", _m)) => {
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        let p = pick_next_plan(&root).ok_or_else(|| anyhow::anyhow!("no plan under docs/project/next"))?;
                        let meta = read_plan_meta(&p)?;
                        let (plan_id, slug) = derive_id_slug(&p);
                        let out = serde_json::json!({
                            "plan": p.to_string_lossy(),
                            "plan_id": plan_id,
                            "slug": slug,
                            "title": meta.title,
                            "status": meta.status
                        });
                        let exit = ctx.ok("dev","plans.next",&out);
                        Ok(exit)
                    }
                    Some(("status", m)) => {
                        let plan = PathBuf::from(m.get_one::<String>("plan").unwrap());
                        let meta = read_plan_meta(&plan)?;
                        let (plan_id, slug) = derive_id_slug(&plan);
                        let out = PlanStatusResult {
                            plan: plan.display().to_string(),
                            frontmatter: meta.front_json(),
                            title: meta.title.clone(),
                            plan_id,
                            slug,
                            status: meta.status.clone(),
                        };
                        let exit = ctx.ok("dev","plans.status",&out);
                        Ok(exit)
                    }
                    Some(("start", m)) => {
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        let plan_opt = m.get_one::<String>("plan").cloned();
                        let dry = m.get_flag("dry-run");
                        let plan_path = match plan_opt {
                            Some(p) => PathBuf::from(p),
                            None => pick_next_plan(&root).ok_or_else(|| anyhow::anyhow!("no plan under docs/project/next"))?,
                        };
                        let (plan_id, slug) = derive_id_slug(&plan_path);
                        let meta0 = read_plan_meta(&plan_path)?;
                        if let Some(s) = meta0.status.as_deref() {
                            if s.eq_ignore_ascii_case("in-progress") || s.eq_ignore_ascii_case("done") {
                                anyhow::bail!("plan status is '{}', cannot start", s);
                            }
                        }
                        // derive title
                        let title = meta0.title.clone().unwrap_or_else(|| slug.clone());

                        // create + start task via CLI (keeps envelopes consistent)
                        let (c_new, v_new) = run_cli_json(&root, &["task","new","--title", &title])?;
                        anyhow::ensure!(c_new==0, "task new failed");
                        let task_id = v_new["data"]["task_id"].as_str()
                            .or_else(|| v_new["data"]["id"].as_str())
                            .or_else(|| v_new["meta"]["context"]["task_id"].as_str())
                            .unwrap_or("").to_string();
                        anyhow::ensure!(!task_id.is_empty(), "could not extract task_id");
                        let (c_start, _v_start) = run_cli_json(&root, &["task","start","--task",&task_id])?;
                        anyhow::ensure!(c_start==0, "task start failed");

                        // update front-matter
                        let now = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
                        let mut meta = meta0.clone();
                        meta.status = Some("in-progress".into());
                        meta.plan_id = Some(plan_id.clone());
                        meta.slug = Some(slug.clone());
                        meta.task_id = Some(task_id.clone());
                        meta.started_at = Some(now);
                        meta.source = Some("next".into());

                        if dry {
                            let out = PlanStartResult {
                                picked_plan: plan_path.display().to_string(),
                                moved_from: plan_path.display().to_string(),
                                moved_to: format!("docs/project/current/{}", plan_path.file_name().unwrap().to_string_lossy()),
                                task_id, title, plan_id, slug,
                                updated_frontmatter: meta.front_json(),
                            };
                            let exit = ctx.ok("dev","plans.start.dryrun",&out);
                            return Ok(exit);
                        }

                        write_plan_meta(&plan_path, &meta)?;
                        // move next -> current
                        let dst = root.join("docs").join("project").join("current")
                            .join(plan_path.file_name().unwrap());
                        std::fs::rename(&plan_path, &dst)?;

                        let out = PlanStartResult {
                            picked_plan: dst.display().to_string(),
                            moved_from: format!("docs/project/next/{}", dst.file_name().unwrap().to_string_lossy()),
                            moved_to: dst.display().to_string(),
                            task_id, title, plan_id, slug,
                            updated_frontmatter: meta.front_json(),
                        };
                        let exit = ctx.ok("dev","plans.start",&out);
                        Ok(exit)
                    }
                    Some(("finalize", m)) => {
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        let plan_opt = m.get_one::<String>("plan").cloned();
                        let plan_path = match plan_opt {
                            Some(p) => PathBuf::from(p),
                            None => pick_single_current(&root)?
                        };
                        let mut meta = read_plan_meta(&plan_path)?;
                        let task_id = meta.task_id.clone();

                        // implemented_in (commit sha)
                        let impl_in = if let Some(x) = m.get_one::<String>("implemented-in") {
                            x.clone()
                        } else {
                            git_rev_parse_head().unwrap_or_default()
                        };
                        // end task + bundle if we have a task_id
                        let mut bundle_out = None;
                        if let Some(tid) = &task_id {
                            let _ = run_cli_json(&root, &["task","end"]); // best-effort, non-fatal
                            let out_path = if let Some(path) = m.get_one::<String>("bundle-out") {
                                path.clone()
                            } else {
                                format!("{}-bundle.tgz", tid)
                            };
                            let (_c_b, _v_b) = run_cli_json(&root, &["task","bundle","--task",tid,"--out",&out_path])?;
                            bundle_out = Some(out_path);
                        }

                        // write front-matter and move current -> done
                        let now = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
                        meta.status = Some("done".into());
                        if !impl_in.is_empty() { meta.implemented_in = Some(impl_in.clone()); }
                        meta.completed_at = Some(now);
                        write_plan_meta(&plan_path, &meta)?;
                        let dst = root.join("docs").join("project").join("done")
                            .join(plan_path.file_name().unwrap());
                        std::fs::rename(&plan_path, &dst)?;

                        let out = PlanFinalizeResult {
                            plan: dst.display().to_string(),
                            moved_to: dst.display().to_string(),
                            task_id,
                            implemented_in: impl_in,
                            bundle_out,
                            frontmatter: meta.front_json(),
                        };
                        let exit = ctx.ok("dev","plans.finalize",&out);
                        Ok(exit)
                    }
                    _ => Ok(2)
                }
            }

	•	Helpers (add at bottom near helper section):

#[derive(Clone)]
struct PlanMeta {
    front: serde_yaml::Mapping,
    title: Option<String>,
    status: Option<String>,
    plan_id: Option<String>,
    slug: Option<String>,
    task_id: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    implemented_in: Option<String>,
    source: Option<String>,
}
impl PlanMeta {
    fn front_json(&self) -> serde_json::Value {
        // serde_yaml::Mapping -> JSON value (lossy-friendly)
        serde_json::to_value(&self.front.clone()).unwrap_or(serde_json::json!({}))
    }
}

fn ensure_project_plan_dirs(root: &Path) -> anyhow::Result<()> {
    for d in ["docs/project/next","docs/project/current","docs/project/done","docs/project/archived"] {
        let p = root.join(d);
        std::fs::create_dir_all(&p)?;
    }
    Ok(())
}

fn list_plans(root: &Path, which: &str) -> anyhow::Result<Vec<PlanRow>> {
    use std::fs;
    let mut out = vec![];
    let folders: Vec<&str> = match which {
        "next" => vec!["docs/project/next"],
        "current" => vec!["docs/project/current"],
        "done" => vec!["docs/project/done"],
        "archived" => vec!["docs/project/archived"],
        _ => vec!["docs/project/next","docs/project/current","docs/project/done","docs/project/archived"],
    };
    for f in folders {
        let dir = root.join(f);
        if !dir.exists() { continue; }
        for e in fs::read_dir(&dir)? {
            let e = e?;
            if !e.file_type()?.is_file() { continue; }
            if e.path().extension() != Some(OsStr::new("md")) { continue; }
            let file = e.file_name().to_string_lossy().to_string();
            if !file.to_lowercase().starts_with("plan-") { continue; }
            let meta = read_plan_meta(&e.path()).ok();
            let (pid, slug) = derive_id_slug(&e.path());
            out.push(PlanRow{
                folder: f.to_string(),
                file,
                plan_id: pid,
                slug,
                title: meta.as_ref().and_then(|m| m.title.clone()),
                status: meta.and_then(|m| m.status),
            });
        }
    }
    Ok(out)
}

fn pick_next_plan(root: &Path) -> Option<PathBuf> {
    let dir = root.join("docs/project/next");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension()==Some(OsStr::new("md")))
        .filter(|p| p.file_name().map(|s| s.to_string_lossy().to_lowercase().starts_with("plan-")).unwrap_or(false))
        .collect();
    files.sort();
    files.into_iter().next()
}

fn pick_single_current(root: &Path) -> anyhow::Result<PathBuf> {
    let dir = root.join("docs/project/current");
    let files: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension()==Some(OsStr::new("md")))
        .filter(|p| p.file_name().map(|s| s.to_string_lossy().to_lowercase().starts_with("plan-")).unwrap_or(false))
        .collect();
    match files.len() {
        0 => anyhow::bail!("no plan under docs/project/current"),
        1 => Ok(files[0].clone()),
        _ => anyhow::bail!("multiple plans under docs/project/current; pass --plan"),
    }
}

fn derive_id_slug(p: &Path) -> (Option<String>, Option<String>) {
    let base = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
    // Accept PLAN-<id>_<slug>.md or plan-<id>_<slug>.md
    let s = base.trim_end_matches(".md");
    let s = if let Some(rest) = s.strip_prefix("PLAN-").or_else(|| s.strip_prefix("plan-")) { rest } else { return (None,None); };
    // split at first '_' (preferred) else '-' after digits
    let mut id = String::new();
    let mut slug = String::new();
    for (i, ch) in s.char_indices() {
        if ch.is_ascii_digit() {
            id.push(ch);
            continue;
        }
        // first separator after digits
        let rest = &s[i+1..];
        slug = rest.trim_start_matches('_').trim_start_matches('-').to_string();
        break;
    }
    if id.is_empty() { return (None,None); }
    if slug.is_empty() {
        // fallback: everything after digits if any
        if let Some(pos) = s.find('_').or_else(|| s.find('-')) {
            slug = s[pos+1..].to_string();
        }
    }
    (Some(id), Some(slug))
}

fn read_plan_meta(p: &Path) -> anyhow::Result<PlanMeta> {
    let txt = std::fs::read_to_string(p)?;
    let mut front = serde_yaml::Mapping::new();
    let mut body = txt.as_str();
    let mut title = None;

    if txt.starts_with("---") {
        if let Some(idx) = txt[3..].find("\n---") {
            let head = &txt[3..3+idx];
            let rest = &txt[3+idx+4..];
            body = rest;
            if let Ok(v) = serde_yaml::from_str::<serde_yaml::Mapping>(head) {
                front = v;
            }
        }
    }
    // derive title from first '# ' in body
    for line in body.lines() {
        let l = line.trim_start();
        if let Some(t) = l.strip_prefix("# ") {
            title = Some(t.trim().to_string());
            break;
        }
    }
    let get = |k: &str| front.get(&serde_yaml::Value::String(k.into()))
        .and_then(|v| match v {
            serde_yaml::Value::String(s) => Some(s.clone()),
            _ => None
        });
    Ok(PlanMeta{
        front: front.clone(),
        title,
        status: get("status"),
        plan_id: get("plan_id"),
        slug: get("slug"),
        task_id: get("task_id"),
        started_at: get("started_at"),
        completed_at: get("completed_at"),
        implemented_in: get("implemented_in"),
        source: get("source"),
    })
}

fn write_plan_meta(p: &Path, meta: &PlanMeta) -> anyhow::Result<()> {
    let txt = std::fs::read_to_string(p)?;
    let mut front = meta.front.clone();
    let set = |m: &mut serde_yaml::Mapping, k: &str, v: Option<String>| {
        if let Some(val) = v {
            m.insert(serde_yaml::Value::String(k.into()), serde_yaml::Value::String(val));
        }
    };
    set(&mut front, "status", meta.status.clone());
    set(&mut front, "plan_id", meta.plan_id.clone());
    set(&mut front, "slug", meta.slug.clone());
    set(&mut front, "task_id", meta.task_id.clone());
    set(&mut front, "started_at", meta.started_at.clone());
    set(&mut front, "completed_at", meta.completed_at.clone());
    set(&mut front, "implemented_in", meta.implemented_in.clone());
    set(&mut front, "source", meta.source.clone());

    let body = if txt.starts_with("---") {
        if let Some(idx) = txt[3..].find("\n---") {
            &txt[3+idx+4..]
        } else { &txt }
    } else { &txt };

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&serde_yaml::to_string(&front)?);
    out.push_str("---\n");
    out.push_str(body.trim_start());
    std::fs::write(p, out.as_bytes())?;
    Ok(())
}

fn git_rev_parse_head() -> Option<String> {
    use std::process::Command;
    let out = Command::new("git").args(["rev-parse","--short","HEAD"]).output().ok()?;
    if !out.status.success() { return None; }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

	•	Tool specs (so agents can call them directly). Add to tool_specs():

// dev.plans.start
ToolSpec {
    name: "dev.plans.start".to_string(),
    about: "Start a plan: create+start task, update front-matter, move next→current".to_string(),
    args_schema: schema_for!(PlanStartArgs),
    result_schema: schema_for!(PlanStartResult),
},
// dev.plans.finalize
ToolSpec {
    name: "dev.plans.finalize".to_string(),
    about: "Finalize a plan: mark done, end task, bundle, move current→done".to_string(),
    args_schema: schema_for!(PlanFinalizeArgs),
    result_schema: schema_for!(PlanFinalizeResult),
},
// dev.plans.list
ToolSpec {
    name: "dev.plans.list".to_string(),
    about: "List plans in docs/project/{next,current,done,archived}".to_string(),
    args_schema: schema_for!(PlanListArgs),
    result_schema: schema_for!(PlanListResult),
},
// dev.plans.status
ToolSpec {
    name: "dev.plans.status".to_string(),
    about: "Show plan front-matter and derived ids".to_string(),
    args_schema: schema_for!(PlanStatusArgs),
    result_schema: schema_for!(PlanStatusResult),
},
// dev.plans.next
ToolSpec {
    name: "dev.plans.next".to_string(),
    about: "Show the next plan candidate (docs/project/next)".to_string(),
    args_schema: schema_for!(serde_json::Value),
    result_schema: schema_for!(serde_json::Value),
},

Note: We reuse run_cli_json() you already have to call task new|start|end|bundle so everything emits proper envelopes and stays observable.

⸻

3) Quick smoke

cargo build --workspace

# List plans
./target/debug/nexoscli dev plans list --where next --output json

# Start next plan (creates+starts task, writes front-matter, moves next→current)
./target/debug/nexoscli dev plans start --output json

# Implement your work... then finalize:
./target/debug/nexoscli dev plans finalize --output json

# Inspect results
./target/debug/nexoscli dev plans list --where done --output json

What it does:
	•	start:
	•	Picks first docs/project/next/PLAN-*.md (or pass --plan).
	•	Creates task (task new) and starts it (task start).
	•	Updates front‑matter: status=in-progress, plan_id, slug, task_id, started_at, source=next.
	•	Moves file to docs/project/current/.
	•	finalize:
	•	Uses the only plan under current/ by default (or --plan).
	•	Records implemented_in = git rev-parse --short HEAD (or --implemented-in).
	•	Ends task (task end) and bundles (task bundle --out <TASK_ID>-bundle.tgz), if task_id present.
	•	Updates front‑matter: status=done, implemented_in, completed_at.
	•	Moves file to docs/project/done/.

⸻

4) Update your Cline workflows (tiny change)
	•	Start: replace the whole plan‑specific shell/python with:

nexoscli dev init --yes
nexoscli dev doctor --ci --checks graph,plugins,config,events,tools --require plugins,graph
nexoscli dev plans start --output json

	•	Finalize: replace the verification shell block that edits YAML by hand with:

cargo build --workspace
cargo test  --workspace
nexoscli dev docs build
nexoscli rules lint
nexoscli rules export --for human --out docs/RULES.md
nexoscli rules export --for cline --out .clinerules/compiled.md

nexoscli dev plans finalize --output json

You can still wrap these in your prompt text, but no inline Python anymore. Cline just runs these commands.

⸻

5) (Optional) Add an E2E test

nexoscli-bin/tests/dev_plan_lifecycle.rs (skeleton):

mod helpers; use helpers::*;
use std::fs;

#[test]
fn dev_plans_start_and_finalize() {
    let t = tmp();
    // Seed a next plan
    let next = t.path().join("docs/project/next");
    fs::create_dir_all(&next).unwrap();
    fs::write(next.join("PLAN-99_example.md"), "# Example\n\nBody").unwrap();

    // start
    let v = run_json(t.path(), &["dev","plans","start"]);
    let task_id = v["data"]["task_id"].as_str().unwrap();
    assert!(t.path().join("docs/project/current/PLAN-99_example.md").exists());

    // finalize
    let v2 = run_json(t.path(), &["dev","plans","finalize"]);
    assert!(t.path().join("docs/project/done/PLAN-99_example.md").exists());
    assert_eq!(v2["data"]["task_id"].as_str().unwrap(), task_id);
}


⸻

Why this solves your Cline pain
	•	No heredoc Python, no interactive shells—just CLI calls.
	•	Front‑matter parsing/patching is inside Rust, tested, and reliable.
	•	Works even if Cline slightly mangles text; the CLI handles the heavy‑lifting.
	•	Keeps everything observable (envelopes, events, task correlation).

⸻
