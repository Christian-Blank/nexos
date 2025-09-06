use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoPolicy {
    #[serde(default)]
    pub version: String, // "v0.1"
    // Relative directories that MUST have a README.md if they exist
    #[serde(default)]
    pub required_readmes: Vec<String>,
    // Disallow these file extensions anywhere under docs/ (lowercased; without dot)
    #[serde(default)]
    pub docs_forbid_ext: Vec<String>,
    // YAML files that are allowed under docs/ (relative paths)
    #[serde(default)]
    pub docs_allow_yaml: Vec<String>,
    // Path substrings that are forbidden to exist (legacy paths)
    #[serde(default)]
    pub forbid_paths: Vec<String>,
    // Soft-entropy checks (warnings)
    #[serde(default)]
    pub warn_root_bundles_over: usize, // count of *.tgz/zip/tar at repo root
    #[serde(default)]
    pub warn_large_binary_mb: u64, // warn for files > N MB under repo root
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
                "rs".into(), "py".into(), "sh".into(), "bash".into(), "js".into(), "jsx".into(), "ts".into(), "tsx".into(), "go".into(), "java".into(), "kt".into(), "rb".into(), "pl".into(), "ps1".into(), "c".into(), "cc".into(),
                "cpp".into(), "h".into(), "hpp".into(), "toml".into(), "yml".into(), "yaml".into(),
            ],
            // exceptions (relative to repo root)
            docs_allow_yaml: vec!["docs/exec_graph.yaml".into()],
            // Legacy doc-based paths to forbid (post-PLAN-47 canonicalization)
            forbid_paths: vec![
                "docs/workflows".into(),
                "docs/temporal_worker".into(),
                "docs/SPECS".into(),
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
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub touched: Vec<String>,
    #[serde(default)]
    pub count: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoCheckResult {
    pub ok: bool,
    pub items: Vec<RepoCheckItem>,
    pub summary: String,
    #[serde(default)]
    pub policy_path: Option<String>,
}

pub fn write_default_policy(root: &Path, force: bool) -> anyhow::Result<PathBuf> {
    let p = root.join(".nexoscli").join("repo_policy.yaml");
    if p.exists() && !force {
        return Ok(p);
    }
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir)?;
    }
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

pub fn check_repo(
    root: &Path,
    policy_path: Option<&Path>,
    fix_readmes: bool,
) -> anyhow::Result<RepoCheckResult> {
    let pol = load_policy(root, policy_path);
    let mut items: Vec<RepoCheckItem> = vec![];

    // A) Major folders have README.md
    let mut readme_item = RepoCheckItem {
        name: "readmes".into(),
        ok: true,
        errors: vec![],
        warnings: vec![],
        touched: vec![],
        count: None,
    };
    for d in &pol.required_readmes {
        let dir = root.join(d);
        if !dir.exists() || !dir.is_dir() {
            continue;
        }
        let readme = dir.join("README.md");
        if !readme.exists() {
            let msg = format!("missing README.md under {}", d);
            if fix_readmes {
                let content = scaffold_readme_for(d);
                if let Err(e) = fs::write(&readme, content.as_bytes()) {
                    readme_item.ok = false;
                    readme_item
                        .errors
                        .push(format!("{} (create failed: {})", msg, e));
                } else {
                    readme_item
                        .warnings
                        .push(format!("autocreated {}", readme.display()));
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
    let mut docs_item = RepoCheckItem {
        name: "docs_nocode".into(),
        ok: true,
        errors: vec![],
        warnings: vec![],
        touched: vec![],
        count: None,
    };
    let docs = root.join("docs");
    if docs.exists() && docs.is_dir() {
        for ent in WalkDir::new(&docs).into_iter().flatten() {
            if !ent.file_type().is_file() {
                continue;
            }
            let p = ent.path();
            let rel = rel_str(root, p);
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase());
            if let Some(ext) = ext {
                if pol.docs_forbid_ext.contains(&ext) {
                    // YAML exceptions
                    if (ext == "yaml" || ext == "yml")
                        && pol.docs_allow_yaml.iter().any(|allow| rel == allow.as_str())
                    {
                        continue;
                    }
                    docs_item.ok = false;
                    docs_item
                        .errors
                        .push(format!("forbidden in docs: {}", rel));
                }
            }
        }
    }
    items.push(docs_item);

    // C) forbidden legacy paths do not exist
    let mut ban_item = RepoCheckItem {
        name: "forbid_paths".into(),
        ok: true,
        errors: vec![],
        warnings: vec![],
        touched: vec![],
        count: None,
    };
    for frag in &pol.forbid_paths {
        let p = root.join(frag);
        if p.exists() {
            ban_item.ok = false;
            ban_item
                .errors
                .push(format!("forbidden path present: {}", frag));
        }
    }
    items.push(ban_item);

    // D) examples & adapters & contracts structure
    items.push(check_examples(root));
    items.push(check_adapters_temporal_worker(root));
    items.push(check_contracts_schemas(root));

    // E) tests must not refer to legacy doc paths
    items.push(check_tests_for_legacy_paths(root));

    // F) soft entropy: bundles & large binaries at root
    items.push(check_root_bundles(root, pol.warn_root_bundles_over));
    items.push(check_root_large_binaries(root, pol.warn_large_binary_mb));

    // G) .clinerules/protected-patterns.md exists (soft warning)
    items.push(check_protected_patterns(root));

    // rollup
    let ok = items.iter().all(|i| i.ok);
    let summary = if ok {
        "repo OK".into()
    } else {
        "repo has violations".into()
    };

    Ok(RepoCheckResult {
        ok,
        items,
        summary,
        policy_path: Some(
            root.join(".nexoscli")
                .join("repo_policy.yaml")
                .display()
                .to_string(),
        ),
    })
}

/* --------------- helpers --------------- */

fn scaffold_readme_for(dir: &str) -> String {
    format!(
        r#"# {dir}/

**Purpose:** What lives here and why.

**Allowed content:** Keep this folder focused. Add examples (bullets).

**Prohibited content:** List what must *not* live here (with rationale).

**Ownership / DRI:** Fill in team/owner.

**Related generators:** Document any generators that populate this folder.

> This README was scaffolded by `nexoscli dev repo readmes --apply`."#
    )
}

fn check_examples(root: &Path) -> RepoCheckItem {
    let mut item = RepoCheckItem {
        name: "examples_layout".into(),
        ok: true,
        errors: vec![],
        warnings: vec![],
        touched: vec![],
        count: None,
    };
    let ex = root.join("examples").join("workflows");
    if !ex.exists() {
        item.ok = false;
        item.errors
            .push("missing examples/workflows (move YAML specs here)".into());
    }
    item
}

fn check_adapters_temporal_worker(root: &Path) -> RepoCheckItem {
    let mut item = RepoCheckItem {
        name: "adapters_temporal_worker".into(),
        ok: true,
        errors: vec![],
        warnings: vec![],
        touched: vec![],
        count: None,
    };
    let a = root.join("adapters").join("temporal-worker");
    if !a.exists() {
        item.ok = false;
        item.errors.push("missing adapters/temporal-worker/".into());
        return item;
    }
    for need in ["pyproject.toml", "README.md", "worker.py", "drain_outbox.py"] {
        let p = a.join(need);
        if !p.exists() {
            item.ok = false;
            item.errors
                .push(format!("adapters/temporal-worker missing {}", need));
        }
    }
    item
}

fn check_contracts_schemas(root: &Path) -> RepoCheckItem {
    let mut item = RepoCheckItem {
        name: "contracts_schemas".into(),
        ok: true,
        errors: vec![],
        warnings: vec![],
        touched: vec![],
        count: None,
    };
    let dir = root.join("contracts").join("schemas");
    if !dir.exists() {
        item.ok = false;
        item.errors.push(
            "missing contracts/schemas (run: nexoscli dev schemas export --out contracts/schemas)".into(),
        );
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
            item.errors
                .push(format!("contracts/schemas missing {}", must));
        }
    }
    item
}

fn check_tests_for_legacy_paths(root: &Path) -> RepoCheckItem {
    let mut item = RepoCheckItem {
        name: "tests_paths".into(),
        ok: true,
        errors: vec![],
        warnings: vec![],
        touched: vec![],
        count: None,
    };
    let t = root.join("nexoscli-bin").join("tests");
    if !t.exists() {
        return item;
    }
    let mut hits = 0u64;
    for ent in WalkDir::new(&t).into_iter().flatten() {
        let p = ent.path();
        if !p.is_file() || p.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let mut buf = String::new();
        if let Ok(mut f) = fs::File::open(p) {
            let _ = f.read_to_string(&mut buf);
            if buf.contains("docs/workflows") {
                hits += 1;
                item.ok = false;
                item.errors.push(format!(
                    "{} references docs/workflows (use examples/workflows)",
                    rel_str(root, p)
                ));
            }
        }
    }
    if hits > 0 {
        item.count = Some(hits);
    }
    item
}

fn check_root_bundles(root: &Path, warn_over: usize) -> RepoCheckItem {
    let mut item = RepoCheckItem {
        name: "root_bundles".into(),
        ok: true,
        errors: vec![],
        warnings: vec![],
        touched: vec![],
        count: None,
    };
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
        item.warnings
            .push(format!("{} bundles at repo root (> {})", cnt, warn_over));
    }
    item
}

fn check_root_large_binaries(root: &Path, warn_mb: u64) -> RepoCheckItem {
    let mut item = RepoCheckItem {
        name: "large_root_files".into(),
        ok: true,
        errors: vec![],
        warnings: vec![],
        touched: vec![],
        count: None,
    };
    let mut cnt = 0u64;
    if let Ok(readdir) = fs::read_dir(root) {
        for e in readdir.flatten() {
            let p = e.path();
            if p.is_file() {
                if let Ok(m) = fs::metadata(&p) {
                    let bytes = m.len();
                    let mb = (bytes as f64 / (1024.0 * 1024.0)) as f64;
                    if mb >= warn_mb as f64 {
                        item.warnings.push(format!(
                            "large file at root: {} (~{:.1} MB)",
                            path_str(&p),
                            mb
                        ));
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
    let mut item = RepoCheckItem {
        name: "protected_patterns".into(),
        ok: true,
        errors: vec![],
        warnings: vec![],
        touched: vec![],
        count: None,
    };
    let p = root
        .join(".clinerules")
        .join("protected-patterns.md"); // note: .md extension
    if !p.exists() {
        item.warnings.push(
            "missing .clinerules/protected-patterns.md (consider enforcing generated/forbidden areas)".into(),
        );
    }
    item
}

fn rel_str(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .display()
        .to_string()
        .replace('\\', "/")
}
fn path_str(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}
