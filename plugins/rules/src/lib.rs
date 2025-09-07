use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use nexoscli_core::plugin::{Plugin, CommandContext, ToolSpec};
use nexoscli_core::Result;
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
    fn describe(&self) -> &'static str { "Rules management (init/lint/export)" }

    fn augment_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
            ClapCommand::new("rules").about(self.describe())
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

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match sub_matches.subcommand() {
            Some(("init", m)) => {
                let force = m.get_flag("force");
                let root = std::env::current_dir()?;
                init_rules(&root, force)?;
                let out = json!({"created": true});
                let code = ctx.ok("rules","init",&out);
                Ok(code)
            }
            Some(("lint", _)) => {
                let root = std::env::current_dir()?;
                let res = lint_rules(&root)?;
                let code = if res.ok { 
                    ctx.ok("rules","lint",&res)
                } else { 
                    ctx.err("rules","lint","rules.lint.failed","rules lint issues", None, 2)
                };
                Ok(code)
            }
            Some(("export", m)) => {
                let audience = m.get_one::<String>("for").unwrap().to_string();
                let out_path = m.get_one::<String>("out").cloned()
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
                Ok(code)
            }
            _ => Ok(2),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "rules.lint".into(),
                about: "Validate rules front-matter".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(LintResult),
            },
            ToolSpec {
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
    let end = txt[3..].find("\n---").map(|i| i + 3).ok_or_else(|| anyhow::anyhow!("unterminated front-matter in {}", p.display()))?;
    let fm = &txt[3..end];
    let body = &txt[end + 4..];

    // First parse as generic YAML to allow filtering non-rule docs (e.g., readme.v0)
    let yaml: serde_yaml::Value = serde_yaml::from_str(fm)?;

    // Skip files that declare a different schema (like readme.v0) or that don't declare an id
    if let serde_yaml::Value::Mapping(map) = &yaml {
        if let Some(schema_val) = map.get(&serde_yaml::Value::String("schema".into())) {
            if schema_val.as_str() == Some("readme.v0") {
                return Ok((None, body.to_string()));
            }
        }
        if !map.contains_key(&serde_yaml::Value::String("id".into())) {
            return Ok((None, body.to_string()));
        }
    }

    // Deserialize into our RuleFront schema
    let front: RuleFront = serde_yaml::from_value(yaml)?;
    Ok((Some(front), body.to_string()))
}

fn msg(p: &Path, m: &str) -> LintIssue {
    LintIssue{ file: p.display().to_string(), message: m.into() }
}
