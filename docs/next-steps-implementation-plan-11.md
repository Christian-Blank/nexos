Love it. Let’s lock in guard‑rails + self‑documentation so both humans and AI can use the system confidently, and so “rules” evolve by evidence, not vibes.

Below are drop‑in slices that fit your current tree:
	•	(1) Rules plugin — initialize a minimal rules pack, lint it, and export audience‑specific bundles (AI/Cline/Humans).
	•	(2) Doctor integration — add a rules gate to dev doctor --ci (soft by default; enforce via --require rules).
	•	(3) Docs builder — dev docs build compiles a single source of truth from the live CLI + rules into docs/CLI_REFERENCE.md and docs/RULES.md.
	•	(4) Workflow doc — a tiny “Rule Sweep” workflow so every effort ends with a repeatable rule review.
	•	(5) Tests — E2E tests prove the surfaces (init → lint → export → docs build → doctor).

These are intentionally small and additive. You can paste them and run.

⸻

0) Workspace additions

Root Cargo.toml (shared deps)

[workspace.dependencies]
serde_yaml = "0.9"
walkdir = "2"
regex = "1"


⸻

1) New plugin: plugins/rules

plugins/rules/Cargo.toml

[package]
name = "nexoscli-plugin-rules"
version = "0.1.0"
edition = "2021"

[dependencies]
nexoscli-core = { path = "../../nexoscli-core" }
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = { workspace = true }
schemars = "0.8"
walkdir = { workspace = true }
regex = { workspace = true }
time = { version = "0.3", features = ["parsing","formatting"] }
clap = { workspace = true }

plugins/rules/src/lib.rs

use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use nexoscli_core::{Plugin, CommandContext, CommandOutput, Result, HealthStatus};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fs, path::{Path, PathBuf}};
use walkdir::WalkDir;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub struct RulesPlugin;
impl RulesPlugin { pub fn new() -> Self { Self } }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct RuleFront {
    id: String,                    // RULE-<slug>
    title: String,
    audience: String,              // human | agent | cline
    scope: String,                 // global | plugin:<name> | path:<glob> | workflow:<name>
    status: String,                // Active | Deprecated
    version: String,               // 0.1.0
    last_reviewed: String,         // RFC3339
    principle: Option<String>,     // link/id of principle
    owner: Option<String>,         // person/email
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct LintIssue { file: String, message: String }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct LintResult { ok: bool, issues: Vec<LintIssue>, files: usize }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ExportArgs { audience: String, out: Option<String> }

impl Plugin for RulesPlugin {
    fn name(&self) -> &'static str { "rules" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn description(&self) -> &'static str { "Rules management (init/lint/export)" }

    fn register_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
            ClapCommand::new("rules").about(self.description())
                .subcommand(
                    ClapCommand::new("init")
                        .about("Create minimal rules pack")
                        .arg(Arg::new("force").long("force").action(ArgAction::SetTrue))
                )
                .subcommand(
                    ClapCommand::new("lint")
                        .about("Validate rules/*.md front-matter")
                )
                .subcommand(
                    ClapCommand::new("export")
                        .about("Export audience-specific bundle")
                        .arg(Arg::new("for").long("for").required(true).value_parser(["human","agent","cline"]))
                        .arg(Arg::new("out").long("out").value_name("PATH"))
                )
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<CommandOutput> {
        match m.subcommand() {
            Some(("init", mm)) => {
                let force = mm.get_flag("force");
                let root = std::env::current_dir()?;
                init_rules(&root, force)?;
                let out = json!({"created": true});
                let code = ctx.ok("rules","init",&out);
                Ok(CommandOutput { message: None, data: Some(out), exit_code: code })
            }
            Some(("lint", _)) => {
                let root = std::env::current_dir()?;
                let res = lint_rules(&root)?;
                let code = if res.ok { ctx.ok("rules","lint",&res) } else { ctx.err("rules","lint","rules.lint.failed","rules lint issues", None, 2) };
                Ok(CommandOutput { message: None, data: Some(serde_json::to_value(&res)?), exit_code: code })
            }
            Some(("export", mm)) => {
                let audience = mm.get_one::<String>("for").unwrap().to_string();
                let out_path = mm.get_one::<String>("out").cloned()
                    .unwrap_or_else(|| {
                        match audience.as_str() {
                            "cline" => ".clinerules/compiled.md".into(),
                            "human" => "docs/RULES.md".into(),
                            "agent" => ".nexoscli/rules/for-agents.md".into(),
                            _ => "rules_export.md".into()
                        }
                    });
                let root = std::env::current_dir()?;
                let md = export_rules(&root, &audience)?;
                if let Some(dir) = Path::new(&out_path).parent() { fs::create_dir_all(dir)?; }
                fs::write(&out_path, md.as_bytes())?;
                let out = json!({"audience": audience, "out": out_path});
                let code = ctx.ok("rules","export",&out);
                Ok(CommandOutput { message: None, data: Some(out), exit_code: code })
            }
            _ => Err(anyhow::anyhow!("unknown rules command").into()),
        }
    }

    fn health_check(&self) -> Result<HealthStatus> { Ok(HealthStatus::Healthy) }

    fn tool_specs(&self) -> Vec<nexoscli_core::plugin::ToolSpec> {
        vec![
            nexoscli_core::plugin::ToolSpec {
                name: "rules.lint".into(),
                about: "Validate rules front-matter".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(LintResult),
            },
            nexoscli_core::plugin::ToolSpec {
                name: "rules.export".into(),
                about: "Export rules bundle".into(),
                args_schema: schema_for!(ExportArgs),
                result_schema: schema_for!(serde_json::Value),
            }
        ]
    }
}

/* ---------- impl ---------- */

fn init_rules(root: &Path, force: bool) -> anyhow::Result<()> {
    let rules_dir = root.join("rules");
    let tmpl_dir  = rules_dir.join("templates");
    let pack_dir  = rules_dir.join("pack").join("core");
    if force && rules_dir.exists() { fs::remove_dir_all(&rules_dir)?; }
    fs::create_dir_all(&tmpl_dir)?;
    fs::create_dir_all(&pack_dir)?;
    // template
    let tmpl = r#"---
id: RULE-<slug>
title: "<short imperative rule>"
audience: human
scope: global
status: Active
version: 0.1.0
last_reviewed: 2025-09-01T00:00:00Z
principle: "SSOT | Contracts over intentions"
owner: "rules-dri@example.com"
---

# Intent
Explain *why* this rule exists in 2–4 sentences.

# Rule
The precise instruction as a checklist:
- [ ] Step 1 …
- [ ] Step 2 …

# Enforcement
- Gate: dev doctor / rules lint / CI
- Evidence: events / coverage

# Exceptions
When it's acceptable to bypass (and what to do then).
"#;
    fs::write(tmpl_dir.join("rule.md"), tmpl)?;

    // minimal core pack (three rules)
    let r1 = r#"---
id: RULE-use-term-run
title: "Run shell commands via `nexoscli term run`"
audience: cline
scope: global
status: Active
version: 0.1.0
last_reviewed: 2025-09-01T00:00:00Z
principle: "Observability, Determinism"
owner: "rules-dri@example.com"
---
- Prefer: `nexoscli term run -- <cmd>` to capture `dur_ms`, `exit_code`, and output digests.
- Exceptions: interactive TTY or long-running servers; justify in the plan.
"#;
    let r2 = r#"---
id: RULE-post-effort-rule-sweep
title: "After each effort, run a Rule Sweep"
audience: human
scope: workflow:post-effort
status: Active
version: 0.1.0
last_reviewed: 2025-09-01T00:00:00Z
principle: "Emergent governance"
owner: "rules-dri@example.com"
---
**Run**: `nexoscli dev telemetry report --since 7d` and `nexoscli dev doctor --ci`.  
If recurring errors or friction appear, add/change/deprecate a rule via `rules/templates/rule.md`.  
Open a proposal in `rules/proposals/CP-YYYYMMDD-*.md` and link acceptance/evidence.
"#;
    let r3 = r#"---
id: RULE-cline-import
title: "Keep Cline tasks mirrored"
audience: agent
scope: plugin:cline
status: Active
version: 0.1.0
last_reviewed: 2025-09-01T00:00:00Z
principle: "SSOT, Replayability"
owner: "rules-dri@example.com"
---
- Nightly (or after major efforts): `nexoscli cline import --since last`.
- Ensure `dev doctor --ci --checks cline` is green; if not, mount storage or set `NEXOS_CLINE_STORAGE`.
"#;
    fs::write(pack_dir.join("RULE-use-term-run.md"), r1)?;
    fs::write(pack_dir.join("RULE-post-effort-rule-sweep.md"), r2)?;
    fs::write(pack_dir.join("RULE-cline-import.md"), r3)?;
    // proposals dir
    fs::create_dir_all(rules_dir.join("proposals"))?;
    Ok(())
}

fn lint_rules(root: &Path) -> anyhow::Result<LintResult> {
    let dir = root.join("rules");
    let mut issues = Vec::<LintIssue>::new();
    let mut seen_ids = std::collections::HashSet::<String>::new();
    let mut files = 0usize;

    for ent in WalkDir::new(&dir).into_iter().flatten() {
        let p = ent.path();
        if p.is_dir() { continue; }
        if p.extension().and_then(|x| x.to_str()) != Some("md") { continue; }
        files += 1;
        let (front, _body) = parse_frontmatter(p)?;
        match front {
            None => issues.push(LintIssue{ file: p.display().to_string(), message: "missing front-matter".into() }),
            Some(f) => {
                // required fields
                if f.id.trim().is_empty() { issues.push(msg(p,"empty id")); }
                if !seen_ids.insert(f.id.clone()) { issues.push(msg(p,"duplicate id")); }
                if !["human","agent","cline"].contains(&f.audience.as_str()) { issues.push(msg(p,"invalid audience")); }
                if !["Active","Deprecated"].contains(&f.status.as_str()) { issues.push(msg(p,"invalid status")); }
                // RFC3339
                if OffsetDateTime::parse(&f.last_reviewed, &Rfc3339).is_err() {
                    issues.push(msg(p,"last_reviewed not RFC3339"));
                }
            }
        }
    }

    Ok(LintResult{ ok: issues.is_empty(), issues, files })
}

fn export_rules(root: &Path, audience: &str) -> anyhow::Result<String> {
    let dir = root.join("rules");
    let mut rows: Vec<(RuleFront, String, PathBuf)> = Vec::new(); // (front, body, path)

    for ent in WalkDir::new(&dir).into_iter().flatten() {
        let p = ent.path();
        if p.is_dir() { continue; }
        if p.extension().and_then(|x| x.to_str()) != Some("md") { continue; }
        if let (Some(front), body) = parse_frontmatter(p)? {
            if front.status == "Active" && (front.audience == audience || (audience=="human" && front.audience=="agent")) {
                rows.push((front, body, p.to_path_buf()));
            }
        }
    }

    rows.sort_by(|a,b| a.0.id.cmp(&b.0.id));
    let mut out = String::new();
    out.push_str(&format!("# Rules for {}\n\n", audience));
    for (f, body, _p) in rows {
        out.push_str(&format!("## {} — {}\n\n", f.id, f.title));
        out.push_str(&body);
        if !body.ends_with('\n') { out.push('\n'); }
        out.push('\n');
    }
    Ok(out)
}

fn parse_frontmatter(p: &Path) -> anyhow::Result<(Option<RuleFront>, String)> {
    let txt = fs::read_to_string(p)?;
    if !txt.starts_with("---") {
        return Ok((None, txt));
    }
    let end = txt[3..].find("\n---").map(|i| i+3).ok_or_else(|| anyhow::anyhow!("unterminated front-matter in {}", p.display()))?;
    let fm = &txt[3..end];
    let body = &txt[end+4..];
    let front: RuleFront = serde_yaml::from_str(fm)?;
    Ok((Some(front), body.to_string()))
}

fn msg(p: &Path, m: &str) -> LintIssue {
    LintIssue{ file: p.display().to_string(), message: m.into() }
}


⸻

2) Register plugin + update workspace

Root Cargo.toml:

[workspace]
members = [
  "nexoscli-core","nexoscli-bin",
  "plugins/dev","plugins/graph","plugins/demo","plugins/issue","plugins/bundle","plugins/cline","plugins/term",
  "plugins/rules"     # NEW
]
resolver = "2"

nexoscli-bin/Cargo.toml:

[dependencies]
# ...existing...
nexoscli-plugin-rules = { path = "../plugins/rules" }

nexoscli-bin/src/main.rs (plugin registration):

registry.register(Box::new(nexoscli_plugin_rules::RulesPlugin::new())); // NEW


⸻

3) Extend Doctor: add rules check (soft; enforceable)

In plugins/dev/src/lib.rs, in your doctor --ci implementation, add:

// rules (soft by default)
if want.contains("rules") {
    let (code_lint, _v1) = run_cli_json(&cwd, &["rules","lint"])?;
    let ok_lint = code_lint == 0;
    // also ensure export works for cline & human
    let (code_ec, _v2) = run_cli_json(&cwd, &["rules","export","--for","cline","--out",".clinerules/compiled.md"])?;
    let (code_eh, _v3) = run_cli_json(&cwd, &["rules","export","--for","human","--out","docs/RULES.md"])?;
    let ok = ok_lint && code_ec == 0 && code_eh == 0;
    checks.push(DoctorCiCheck{
        name: "rules".into(),
        ok,
        detail: if ok { "lint & export OK".into() } else { "rules lint/export failed".into() },
        severity: "info".into(),
        fix: Some("Run `nexoscli rules init` then `nexoscli rules lint`".into()),
    });
}

And allow enforcement:

// already added earlier for cline; reuse same `--require` logic:
// e.g., nexoscli dev doctor --ci --checks rules,cline --require rules


⸻

4) Docs builder: dev docs build

In plugins/dev/src/lib.rs, add the subcommand and helpers.

Add to augment_commands():

.subcommand(
  Command::new("docs").about("Documentation utilities")
    .subcommand(
      Command::new("build")
        .about("Generate CLI reference and Rules doc")
        .arg(Arg::new("open").long("open").action(ArgAction::SetTrue))
    )
)

Add text runner helper alongside run_cli_json():

fn run_cli_text(cwd: &std::path::Path, args: &[&str]) -> anyhow::Result<(i32, String)> {
    let exe = std::env::current_exe()?;
    let out = std::process::Command::new(exe).current_dir(cwd).args(args).output()?;
    let code = out.status.code().unwrap_or(-1);
    Ok((code, String::from_utf8_lossy(&out.stdout).to_string()))
}

Implement the "docs build" branch:

Some(("docs", dm)) => {
  if let Some(("build", m)) = dm.subcommand() {
    let cwd = std::env::current_dir()?;
    // 1) Top-level help
    let (_c1, help) = run_cli_text(&cwd, &["--help"])?;
    // 2) tools export (machine specs)
    let (_c2, tools) = run_cli_json(&cwd, &["tools","export"])?;
    // 3) rules export (humans)
    let (_c3, _v3) = run_cli_json(&cwd, &["rules","export","--for","human","--out","docs/RULES.md"])?;

    std::fs::create_dir_all("docs")?;
    let mut md = String::new();
    md.push_str("# NEXOS CLI Reference (generated)\n\n");
    md.push_str("## Top-level Help\n\n```\n");
    md.push_str(&help);
    md.push_str("\n```\n\n");

    md.push_str("## Tool Specs (for agents)\n\n");
    let tools_arr = tools["data"]["tools"].as_array().cloned().unwrap_or_default();
    for t in tools_arr {
        let name = t["name"].as_str().unwrap_or("-");
        let about = t["about"].as_str().unwrap_or("-");
        md.push_str(&format!("### `{}`\n\n{}\n\n", name, about));
    }
    std::fs::write("docs/CLI_REFERENCE.md", md.as_bytes())?;

    // Optionally open
    if m.get_flag("open") {
        #[cfg(target_os="macos")] { let _ = std::process::Command::new("open").arg("docs/CLI_REFERENCE.md").status(); }
        #[cfg(target_os="linux")] { let _ = std::process::Command::new("xdg-open").arg("docs/CLI_REFERENCE.md").status(); }
    }

    let out = serde_json::json!({"written": ["docs/CLI_REFERENCE.md", "docs/RULES.md"]});
    let code = ctx.ok("dev","docs.build",&out);
    return Ok(CommandOutput{ message: None, data: Some(out), exit_code: code });
  }
  Ok(CommandOutput{ message: None, data: None, exit_code: 2 })
}


⸻

5) Docs: “Rule Sweep” workflow (human view)

Create docs/WORKFLOWS.md:

# Core Workflows (v0)

## 1) After each effort — Rule Sweep (10–15 min)

**Goal**: Keep the system simple and accurate by adding/changing/removing rules based on evidence.

**Steps**
1. Run telemetry: `nexoscli dev telemetry report --since 7d --output table`  
   – Note recurring errors (top error codes) and slow commands (p90).
2. Run doctor: `nexoscli dev doctor --ci --checks rules,cline,plugins,graph --output json`  
   – Ensure green. If not, open issues; don’t bypass.
3. If evidence suggests a rule change:
   - Copy `rules/templates/rule.md` → `rules/pack/<area>/RULE-<slug>.md`
   - Write the rule in 1 screen; include *Intent*, *Rule*, *Enforcement*, *Exceptions*.
   - Link the principle and add owner (DRI).
4. Validate & publish:
   - `nexoscli rules lint`
   - `nexoscli rules export --for human`
   - `nexoscli rules export --for cline`
   - Commit with message: `rules: add RULE-<slug> (evidence: …)`

(You can tweak wording; the point is a stable loop.)

⸻

6) Tests (drop‑in under nexoscli-bin/tests/)

rules_init_lint_export.rs

mod helpers; use helpers::*;
use std::fs;

#[test]
fn rules_init_lint_export() {
    let t = tmp();
    // init
    let _ = run_json(t.path(), &["rules","init","--force"]);
    assert!(t.path().join("rules/pack/core/RULE-use-term-run.md").exists());

    // lint
    let v = run_json(t.path(), &["rules","lint"]);
    assert!(v["data"]["ok"].as_bool().unwrap());

    // export for cline
    let v2 = run_json(t.path(), &["rules","export","--for","cline","--out",".clinerules/compiled.md"]);
    assert!(t.path().join(".clinerules/compiled.md").exists());
    let body = fs::read_to_string(t.path().join(".clinerules/compiled.md")).unwrap();
    assert!(body.contains("RULE-use-term-run"));

    // export for humans (docs)
    let _ = run_json(t.path(), &["rules","export","--for","human","--out","docs/RULES.md"]);
    assert!(t.path().join("docs/RULES.md").exists());
}

dev_docs_build.rs

mod helpers; use helpers::*;

#[test]
fn dev_docs_build_generates_cli_reference() {
    let t = tmp();
    // make sure rules export doesn't fail during docs build
    let _ = run_json(t.path(), &["rules","init","--force"]);
    let v = run_json(t.path(), &["dev","docs","build"]);
    assert!(v["data"]["written"].as_array().unwrap().len() >= 2);
    assert!(t.path().join("docs/CLI_REFERENCE.md").exists());
    assert!(t.path().join("docs/RULES.md").exists());
}

doctor_ci_rules.rs

mod helpers; use helpers::*;

#[test]
fn doctor_ci_includes_rules_check() {
    let t = tmp();
    // rules missing -> check still informational, ok=true
    let v = run_json(t.path(), &["dev","doctor","--ci","--checks","rules"]);
    assert!(v["data"]["ok"].as_bool().unwrap());

    // now init rules and require them
    let _ = run_json(t.path(), &["rules","init","--force"]);
    let v2 = run_json(t.path(), &["dev","doctor","--ci","--checks","rules","--require","rules"]);
    assert!(v2["data"]["ok"].as_bool().unwrap());
}

Run tests:

cargo test --workspace


⸻

7) Quick manual acceptance

# Seed rules
nexoscli rules init --force

# Validate & export
nexoscli rules lint
nexoscli rules export --for cline --out .clinerules/compiled.md
nexoscli rules export --for human --out docs/RULES.md

# Build docs (CLI + rules)
nexoscli dev docs build
open docs/CLI_REFERENCE.md  # macOS
# or xdg-open on Linux

# Doctor (soft), then strict
nexoscli dev doctor --ci --checks rules,cline
nexoscli dev doctor --ci --checks rules --require rules


⸻

8) What you just gained
	•	Single source of truth for usage: docs/CLI_REFERENCE.md is generated from the live CLI and tool specs; drift can’t creep in.
	•	Rules, not folklore: small, linted rules in rules/pack/* with audiences (human/agent/cline), exported into .clinerules for AI and docs/ for humans.
	•	A stable loop: every effort ends with a Rule Sweep; doctor + telemetry provide evidence; rules change via a tiny PR.

⸻

9) Suggested follow‑ups (pin; add them to ROADMAP_NEXT.md as possible future tasks)
	•	rules propose: scaffold rules/proposals/CP-YYYYMMDD-*.md from telemetry (top error codes) and recent events.
	•	rules drift: detect stale last_reviewed and page the rules DRI.
	•	dev docs build --watch: rebuild on plugin/rules changes.
	•	Cline: cline tail watcher (ingest on change) and a rules check that ensures .clinerules/compiled.md is present before running Cline tasks.

⸻

Direct candor: you don’t need a grand “rules engine.” The smallest stable loop is enough: (1) rules as files, (2) lint + export, (3) a short rule‑sweep workflow. Everything else can grow by emergence.
