---
status: done
plan_id: '48'
slug: repo-check
task_id: TASK-20250906-one-shot-policy-idempotent
started_at: 2025-09-06T10:47:34.793389Z
source: next
completed_at: 2025-09-06T20:13:02.27501Z
implemented_in: 6e886d9
---
Awesome idea. Let’s turn “entropy control” into first‑class, policy‑driven repo safeguards you can run locally or in CI with the same CLI you already use.

Below is a drop‑in extension to your existing dev plugin that adds:
	•	dev repo check – structure/entropy checks: READMEs on major folders, no code in docs/, examples/adapters/contracts layout, tests don’t reference deprecated paths, oversized bundles, protected patterns, etc.
	•	dev repo policy init – writes a policy file to customize what “major folders” are and which file types are allowed in docs/.
	•	dev repo readmes --apply – scaffolds missing folder READMEs from templates.
	•	Integration into dev doctor --ci via a new repo check.

All commands emit your standard ctx.ok/ctx.err envelopes and are safe to wire into CI.

⸻

1) CLI surface

# one-shot policy (idempotent)
nexoscli dev repo policy init --output json

# dry-run: see all issues without modifying the tree
nexoscli dev repo check --output json

# auto-create missing folder READMEs (policy-driven)
nexoscli dev repo readmes --apply --output json

# CI gate (adds “repo” to doctor’s checks)
nexoscli dev doctor --ci --checks repo,plugins,events-schema,events-chain --output json


⸻

2) New source file: plugins/dev/src/repo.rs

Paste this file as‑is. It has zero external deps beyond your current crate set.

// plugins/dev/src/repo.rs
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::{Path, PathBuf}};
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoPolicy {
    #[serde(default)] pub version: String,                 // "v0.1"
    // Relative directories that MUST have a README.md if they exist
    #[serde(default)] pub required_readmes: Vec<String>,
    // Disallow these file extensions anywhere under docs/ (lowercased; without dot)
    #[serde(default)] pub docs_forbid_ext: Vec<String>,
    // YAML files that are allowed under docs/ (relative paths)
    #[serde(default)] pub docs_allow_yaml: Vec<String>,
    // Path substrings that are forbidden to exist (e.g., "examples/workflows")
    #[serde(default)] pub forbid_paths: Vec<String>,
    // Soft-entropy checks (warnings)
    #[serde(default)] pub warn_root_bundles_over: usize,   // count of *.tgz/zip/tar at repo root
    #[serde(default)] pub warn_large_binary_mb: u64,       // warn for files > N MB under repo root
}

impl Default for RepoPolicy {
    fn default() -> Self {
        Self {
            version: "v0.1".into(),
            required_readmes: vec![
                "docs".into(),
                "plugins".into(),
                "nexoscli-core".into(),
                "nexoscli-bin".into(),
                "rules".into(),
                "examples".into(),
                "contracts".into(),
                "adapters".into(),
                "scripts".into(),
                "testdata".into(),
            ],
            // You can still allow specific YAMLs below with docs_allow_yaml
            docs_forbid_ext: vec![
                "rs","py","sh","bash","js","jsx","ts","tsx","go","java","kt",
                "rb","pl","ps1","c","cc","cpp","h","hpp","toml","yml","yaml",
            ],
            // exceptions (relative to repo root)
            docs_allow_yaml: vec![
                "docs/exec_graph.yaml".into(),     // explicitly allowed
            ],
            forbid_paths: vec![
                "examples/workflows",                   // specs must live under examples/
                "adapters/temporal-worker",            // adapters/** instead
                "contracts/schemas",                      // schemas live under contracts/schemas
            ],
            warn_root_bundles_over: 10,
            warn_large_binary_mb: 25,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoCheckItem {
    pub name: String,
    pub ok: bool,
    #[serde(default)] pub errors: Vec<String>,
    #[serde(default)] pub warnings: Vec<String>,
    #[serde(default)] pub touched: Vec<String>,
    #[serde(default)] pub count: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoCheckResult {
    pub ok: bool,
    pub items: Vec<RepoCheckItem>,
    pub summary: String,
    #[serde(default)] pub policy_path: Option<String>,
}

pub fn write_default_policy(root: &Path, force: bool) -> anyhow::Result<PathBuf> {
    let p = root.join(".nexoscli").join("repo_policy.yaml");
    if p.exists() && !force {
        return Ok(p);
    }
    if let Some(dir) = p.parent() { fs::create_dir_all(dir)?; }
    let y = serde_yaml::to_string(&RepoPolicy::default())?;
    fs::write(&p, y.as_bytes())?;
    Ok(p)
}

pub fn load_policy(root: &Path, policy_path: Option<&Path>) -> RepoPolicy {
    let path = policy_path
        .map(|p| p.to_path_buf())
        .unwrap_or(root.join(".nexoscli").join("repo_policy.yaml"));
    if path.exists() {
        if let Ok(txt) = fs::read_to_string(&path) {
            if let Ok(pol) = serde_yaml::from_str::<RepoPolicy>(&txt) {
                return pol;
            }
        }
    }
    RepoPolicy::default()
}

pub fn check_repo(root: &Path, policy_path: Option<&Path>, fix_readmes: bool) -> anyhow::Result<RepoCheckResult> {
    let pol = load_policy(root, policy_path);
    let mut items: Vec<RepoCheckItem> = vec![];

    // A) Major folders have README.md
    let mut readme_item = RepoCheckItem { name: "readmes".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
    for d in &pol.required_readmes {
        let dir = root.join(d);
        if !dir.exists() || !dir.is_dir() { continue; }
        let readme = dir.join("README.md");
        if !readme.exists() {
            let msg = format!("missing README.md under {}", d);
            if fix_readmes {
                let content = scaffold_readme_for(d);
                if let Err(e) = fs::write(&readme, content.as_bytes()) {
                    readme_item.ok = false;
                    readme_item.errors.push(format!("{} (create failed: {})", msg, e));
                } else {
                    readme_item.warnings.push(format!("autocreated {}", readme.display()));
                    readme_item.touched.push(path_str(&readme));
                }
            } else {
                readme_item.ok = false;
                readme_item.errors.push(msg);
            }
        }
    }
    items.push(readme_item);

    // B) docs/ has no code (with configured exceptions)
    let mut docs_item = RepoCheckItem { name: "docs_nocode".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
    let docs = root.join("docs");
    if docs.exists() {
        for ent in WalkDir::new(&docs).into_iter().flatten() {
            if !ent.file_type().is_file() { continue; }
            let p = ent.path();
            let rel = rel_str(root, p);
            // allowlist for YAML
            if matches!(p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()), Some(ref ext) if pol.docs_forbid_ext.contains(ext)) {
                // allowed exceptions for YAML
                if (p.extension().unwrap_or_default() == "yaml" || p.extension().unwrap_or_default() == "yml")
                    && pol.docs_allow_yaml.iter().any(|allow| rel == allow.as_str())
                {
                    continue;
                }
                docs_item.ok = false;
                docs_item.errors.push(format!("forbidden in docs: {}", rel));
            }
        }
    }
    items.push(docs_item);

    // C) forbidden paths do not exist
    let mut ban_item = RepoCheckItem { name: "forbid_paths".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
    for frag in &pol.forbid_paths {
        let p = root.join(frag);
        if p.exists() {
            ban_item.ok = false;
            ban_item.errors.push(format!("forbidden path present: {}", frag));
        }
    }
    items.push(ban_item);

    // D) examples & adapters & contracts structure
    items.push(check_examples(root));
    items.push(check_adapters_temporal_worker(root));
    items.push(check_contracts_schemas(root));

    // E) tests must not refer to examples/workflows
    items.push(check_tests_for_old_paths(root));

    // F) soft entropy: bundles & large binaries at root
    items.push(check_root_bundles(root, pol.warn_root_bundles_over));
    items.push(check_root_large_binaries(root, pol.warn_large_binary_mb));

    // G) .clinerules/protected-patterns exists (soft warning)
    items.push(check_protected_patterns(root));

    // rollup
    let ok = items.iter().all(|i| i.ok);
    let summary = if ok { "repo OK".into() } else { "repo has violations".into() };

    Ok(RepoCheckResult {
        ok,
        items,
        summary,
        policy_path: Some(root.join(".nexoscli").join("repo_policy.yaml").display().to_string()),
    })
}

/* --------------- helpers --------------- */

fn scaffold_readme_for(dir: &str) -> String {
    format!(r#"# {dir}/

**Purpose:** What lives here and why.

**Allowed content:** Keep this folder focused. Add examples (bullets).

**Prohibited content:** List what must *not* live here (with rationale).

**Ownership / DRI:** Fill in team/owner.

**Related generators:** Document any generators that populate this folder.

> This README was scaffolded by `nexoscli dev repo readmes --apply`."#)
}

fn check_examples(root: &Path) -> RepoCheckItem {
    let mut item = RepoCheckItem { name: "examples_layout".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
    let ex = root.join("examples").join("workflows");
    if !ex.exists() {
        item.ok = false;
        item.errors.push("missing examples/workflows (move YAML specs here)".into());
    }
    item
}

fn check_adapters_temporal_worker(root: &Path) -> RepoCheckItem {
    let mut item = RepoCheckItem { name: "adapters_temporal_worker".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
    let a = root.join("adapters").join("temporal-worker");
    if !a.exists() {
        item.ok = false;
        item.errors.push("missing adapters/temporal-worker/".into());
        return item;
    }
    for need in ["pyproject.toml","README.md","worker.py","drain_outbox.py"] {
        let p = a.join(need);
        if !p.exists() {
            item.ok = false;
            item.errors.push(format!("adapters/temporal-worker missing {}", need));
        }
    }
    item
}

fn check_contracts_schemas(root: &Path) -> RepoCheckItem {
    let mut item = RepoCheckItem { name: "contracts_schemas".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
    let dir = root.join("contracts").join("schemas");
    if !dir.exists() {
        item.ok = false;
        item.errors.push("missing contracts/schemas (run: nexoscli dev schemas export --out contracts/schemas)".into());
        return item;
    }
    for must in [
        "event_v0.3.schema.json",
        "term_record_v0.1.schema.json",
        "file_watch_event_v0.1.schema.json",
        "bag_manifest_v0.2.schema.json",
    ] {
        if !dir.join(must).exists() {
            item.ok = false;
            item.errors.push(format!("contracts/schemas missing {}", must));
        }
    }
    item
}

fn check_tests_for_old_paths(root: &Path) -> RepoCheckItem {
    let mut item = RepoCheckItem { name: "tests_paths".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
    let t = root.join("nexoscli-bin").join("tests");
    if !t.exists() { return item; }
    let mut hits = 0u64;
    for ent in WalkDir::new(&t).into_iter().flatten() {
        let p = ent.path();
        if !p.is_file() || p.extension().and_then(|e| e.to_str()) != Some("rs") { continue; }
        let mut buf = String::new();
        if let Ok(mut f) = fs::File::open(p) {
            let _ = f.read_to_string(&mut buf);
            if buf.contains("examples/workflows") {
                hits += 1;
                item.ok = false;
                item.errors.push(format!("{} references examples/workflows (use examples/workflows)", rel_str(root, p)));
            }
        }
    }
    if hits > 0 { item.count = Some(hits); }
    item
}

fn check_root_bundles(root: &Path, warn_over: usize) -> RepoCheckItem {
    let mut item = RepoCheckItem { name: "root_bundles".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
    let mut cnt = 0usize;
    if let Ok(readdir) = fs::read_dir(root) {
        for e in readdir.flatten() {
            let p = e.path();
            if p.is_file() {
                let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if n.ends_with(".tgz") || n.ends_with(".tar.gz") || n.ends_with(".zip") || n.ends_with(".tar") {
                    cnt += 1;
                }
            }
        }
    }
    item.count = Some(cnt as u64);
    if cnt > warn_over {
        item.warnings.push(format!("{} bundles at repo root (> {})", cnt, warn_over));
    }
    item
}

fn check_root_large_binaries(root: &Path, warn_mb: u64) -> RepoCheckItem {
    let mut item = RepoCheckItem { name: "large_root_files".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
    let mut cnt = 0u64;
    if let Ok(readdir) = fs::read_dir(root) {
        for e in readdir.flatten() {
            let p = e.path();
            if p.is_file() {
                if let Ok(m) = fs::metadata(&p) {
                    let bytes = m.len();
                    let mb = (bytes as f64 / (1024.0*1024.0)) as f64;
                    if mb >= warn_mb as f64 {
                        item.warnings.push(format!("large file at root: {} (~{:.1} MB)", path_str(&p), mb));
                        cnt += 1;
                    }
                }
            }
        }
    }
    item.count = Some(cnt);
    item
}

fn check_protected_patterns(root: &Path) -> RepoCheckItem {
    let mut item = RepoCheckItem { name: "protected_patterns".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
    let p = root.join(".clinerules").join("protected-patterns");
    if !p.exists() {
        item.warnings.push("missing .clinerules/protected-patterns (consider enforcing generated/forbidden areas)".into());
    }
    item
}

fn rel_str(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string().replace('\\', "/")
}
fn path_str(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}


⸻

3) Minimal additions to plugins/dev/src/lib.rs

Add a module import near the top:

mod checks;
mod repo; // <— NEW

Add a new repo command group inside augment_commands:

// in fn augment_commands(&self, app: Command) -> Command {
.subcommand(
    Command::new("repo").about("Repository entropy & structure safeguards")
      .subcommand(
         Command::new("check").about("Validate structure against policy")
           .arg(Arg::new("policy").long("policy").required(false).value_name("PATH"))
           .arg(Arg::new("fix-readmes").long("fix-readmes").action(ArgAction::SetTrue)
                .help("Auto-create missing READMEs according to policy"))
      )
      .subcommand(
         Command::new("policy").about("Manage repo policy")
           .subcommand(
             Command::new("init").about("Write default .nexoscli/repo_policy.yaml")
               .arg(Arg::new("force").long("force").action(ArgAction::SetTrue))
           )
      )
      .subcommand(
         Command::new("readmes").about("Scaffold missing folder READMEs")
           .arg(Arg::new("policy").long("policy").required(false).value_name("PATH"))
           .arg(Arg::new("apply").long("apply").action(ArgAction::SetTrue))
      )
)

Handle the new subcommands in execute:

// inside match sub_matches.subcommand() {
Some(("repo", rm)) => {
    match rm.subcommand() {
        Some(("policy", pm)) => {
            if let Some(("init", im)) = pm.subcommand() {
                let root = std::env::current_dir()?;
                let force = im.get_flag("force");
                let path = crate::repo::write_default_policy(&root, force)?;
                let exit = ctx.ok("dev","repo.policy.init", &serde_json::json!({ "written": path.display().to_string(), "force": force }));
                return Ok(exit);
            }
            Ok(2)
        }
        Some(("check", cm)) => {
            let root = std::env::current_dir()?;
            let pol = cm.get_one::<String>("policy").map(|s| PathBuf::from(s));
            let fix = cm.get_flag("fix-readmes");
            let res = crate::repo::check_repo(&root, pol.as_deref(), fix)?;
            if res.ok {
                let exit = ctx.ok("dev","repo.check",&res);
                return Ok(exit);
            } else {
                let exit = ctx.err("dev","repo.check","repo.structure.invalid","Repo violations found", Some("Run with --fix-readmes; or update policy via dev repo policy init"), 2);
                return Ok(exit);
            }
        }
        Some(("readmes", rm2)) => {
            let root = std::env::current_dir()?;
            let pol = rm2.get_one::<String>("policy").map(|s| PathBuf::from(s));
            let apply = rm2.get_flag("apply");
            let res = crate::repo::check_repo(&root, pol.as_deref(), apply)?;
            // Return only the item for readmes, but keep full context in data
            let exit = ctx.ok("dev","repo.readmes",&res);
            return Ok(exit);
        }
        _ => Ok(2),
    }
}

Expose the new tools to agents (optional but consistent with your “tools export”):

fn tool_specs(&self) -> Vec<ToolSpec> {
    let mut v = vec![
        // ... existing specs ...
    ];
    v.push(ToolSpec {
        name: "dev.repo.check".to_string(),
        about: "Validate repo structure/entropy against policy".to_string(),
        args_schema: schemars::schema_for!(serde_json::Value),
        result_schema: schemars::schema_for!(serde_json::Value),
    });
    v.push(ToolSpec {
        name: "dev.repo.policy.init".to_string(),
        about: "Write default .nexoscli/repo_policy.yaml".to_string(),
        args_schema: schemars::schema_for!(serde_json::Value),
        result_schema: schemars::schema_for!(serde_json::Value),
    });
    v.push(ToolSpec {
        name: "dev.repo.readmes".to_string(),
        about: "Scaffold missing folder READMEs".to_string(),
        args_schema: schemars::schema_for!(serde_json::Value),
        result_schema: schemars::schema_for!(serde_json::Value),
    });
    v
}

Integrate the repo check into dev doctor --ci (add this block inside the if m.get_flag("ci") branch, where other checks are pushed):

// after plugins/tools/etc
if want.contains("repo") {
    let res = crate::repo::check_repo(&cwd, None, /*fix_readmes*/ false)?;
    let ok = res.ok;
    let mut detail = res.summary.clone();
    // summarize top violations (keep CI output short)
    if !ok {
        let mut viols = Vec::new();
        for it in &res.items {
            if !it.ok {
                viols.push(format!("{}: {}", it.name, it.errors.get(0).cloned().unwrap_or_default()));
            }
        }
        if !viols.is_empty() {
            detail = format!("violations → {}", viols.join(" | "));
        }
    }
    checks.push(DoctorCiCheck {
        name: "repo".into(),
        ok,
        detail,
        severity: if ok { "info".into() } else { "error".into() },
        fix: Some("Run `nexoscli dev repo check --fix-readmes` and/or `nexoscli dev repo policy init`".into()),
    });
}

Note: No other existing code needs to change. This stays additive and respects your envelope + event discipline.

⸻

4) Default policy file (generated)

nexoscli dev repo policy init writes .nexoscli/repo_policy.yaml:

version: v0.1

# Each existing directory listed here must contain a README.md
required_readmes:
  - docs
  - plugins
  - nexoscli-core
  - nexoscli-bin
  - rules
  - examples
  - contracts
  - adapters
  - scripts
  - testdata

# Forbidden code extensions in docs/ (lowercased, without dot)
docs_forbid_ext:
  - rs
  - py
  - sh
  - bash
  - js
  - jsx
  - ts
  - tsx
  - go
  - java
  - kt
  - rb
  - pl
  - ps1
  - c
  - cc
  - cpp
  - h
  - hpp
  - toml
  - yml
  - yaml

# Allowed YAML exceptions under docs/
docs_allow_yaml:
  - docs/exec_graph.yaml

# Paths that must not exist
forbid_paths:
  - examples/workflows
  - adapters/temporal-worker
  - contracts/schemas

# Soft entropy warnings
warn_root_bundles_over: 10
warn_large_binary_mb: 25

Customize as your repo evolves (e.g., allow extra YAMLs under docs/ by adding them to docs_allow_yaml).

⸻

5) GitHub Actions example

name: Repo Hygiene

on:
  pull_request:
  push:
    branches: [ main ]

jobs:
  hygiene:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install toolchain
        run: |
          rustup toolchain install stable --profile minimal
          rustup default stable
      - name: Build CLI
        run: cargo build --locked --workspace
      - name: Repo policy (idempotent)
        run: ./target/debug/nexoscli dev repo policy init --output json
      - name: Repo check (CI gate)
        run: ./target/debug/nexoscli dev doctor --ci --checks repo,plugins,events-schema,events-chain --output json


⸻

6) What it catches (and how it helps)
	•	Missing folder READMEs → quick context for every major area; templates are auto‑scaffolded.
	•	Code leaked into docs/ → prevents the slow drift you saw earlier; YAML exceptions are explicit.
	•	Bad layout (examples/workflows, adapters/temporal-worker, contracts/schemas) → flags moves you want to enforce.
	•	Examples, adapters, contracts → make sure the canonical locations exist and contain minimal required files.
	•	Test debt (examples/workflows references) → fail fast until tests are migrated to examples/workflows.
	•	Entropy warnings (too many bundles, large binaries at root, missing protected patterns) → nudges cleanup without blocking.

Everything is policy‑driven and easily tuned per repo.

⸻

7) Optional: quick README template you can reuse

When --fix-readmes/--apply is on, each missing README gets:

# <folder>/

**Purpose:** What lives here and why.

**Allowed content:** Keep this folder focused. Add examples (bullets).

**Prohibited content:** List what must *not* live here (with rationale).

**Ownership / DRI:** Fill in team/owner.

**Related generators:** Document any generators that populate this folder.

> This README was scaffolded by `nexoscli dev repo readmes --apply`.

Tune the template in scaffold_readme_for() if you want per‑folder variants.

⸻

8) Roadmap extensions (easy follow‑ons)
	•	Size budgets by folder (e.g., docs_max_mb, examples_max_files).
	•	Executable‑bit guard outside scripts/ (files with +x flagged).
	•	Generated provenance: verify that contracts/schemas/* equals a fresh dev schemas export (you already do that in CI, can be added here too).
	•	Symlink and binary‑blob guards under docs/.
	•	Owner gates: read CODEOWNERS to map folder owners into the check output.

⸻

9) Quick smoke locally

cargo build --workspace
./target/debug/nexoscli dev repo policy init
./target/debug/nexoscli dev repo check
./target/debug/nexoscli dev repo readmes --apply
./target/debug/nexoscli dev doctor --ci --checks repo --output json

If you’d like, I can also draft a small test (nexoscli-bin/tests/dev_repo_entropy.rs) that sets up a tmp repo, drops a fake docs/foo.py and asserts the check fails — but the above gets you a working, scalable safeguard you can iterate on immediately.