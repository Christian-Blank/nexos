Awesome—let’s ship (A) the read‑only GitHub adapter and (B) TracePack bundles as small, additive slices. I’m giving you drop‑in code, tests, and acceptance steps that match your current tree.

⸻

0) Workspace deps (one edit)

Root Cargo.toml → add shared crates:

[workspace.dependencies]
reqwest = { version = "0.12", default-features = false, features = ["json", "blocking", "rustls-tls"] }
tar = "0.4"
flate2 = "1.0"
sha2 = "0.10"

(You already use sha2 for event content hashes; leave it if present.)

⸻

(A) Read‑only GitHub adapter for issue import

1) Extend the issue plugin Cargo

plugins/issue/Cargo.toml

[dependencies]
# ... existing ...
reqwest = { workspace = true }   # NEW

2) Add GitHub import flags and handler

Edit plugins/issue/src/lib.rs:

(i) imports

At the top, add:

use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, ETAG, IF_NONE_MATCH, USER_AGENT};

(ii) Extend the import subcommand declaration

Find the existing import subcommand block and replace it with:

.subcommand(
    Command::new("import")
      .about("Import issues from external sources (read-only)")
      .arg(Arg::new("from").long("from").required(true).value_parser(["file","github"]))
      .arg(Arg::new("path").long("path").value_name("FILE").required_if_eq("from","file"))
      .arg(Arg::new("repo").long("repo").value_name("owner/name").required_if_eq("from","github"))
      .arg(Arg::new("state").long("state").value_parser(["open","all"]).default_value("open"))
      .arg(Arg::new("labels").long("labels").value_name("CSV").required(false))
      .arg(Arg::new("since").long("since").value_name("ISO8601").required(false))
      .arg(Arg::new("limit").long("limit").value_parser(clap::value_parser!(u32)).default_value("100"))
      .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
      // test helper: feed a GitHub JSON fixture to avoid network in CI
      .arg(Arg::new("fixture").long("fixture").value_name("FILE").required(false))
)

(iii) Extend the execute() match arm for import

Find the Some(("import", m)) => { ... } branch and replace its body with:

Some(("import", m)) => {
    let root = std::env::current_dir()?;
    let dir = root.join(".nexoscli").join("issues");
    std::fs::create_dir_all(&dir)?;

    let from = m.get_one::<String>("from").unwrap().as_str();
    match from {
        "file" => {
            let path = m.get_one::<String>("path").unwrap();
            // (existing file import) – keep your previous implementation, but route success via ctx.ok
            let bytes = std::fs::read(path)?;
            let items: serde_json::Value = serde_json::from_slice(&bytes)?;
            let arr = items.as_array().ok_or_else(|| anyhow::anyhow!("expected JSON array"))?;
            let mut created: Vec<String> = Vec::new();
            for v in arr {
                let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
                if title.is_empty() { continue; }
                let body = v.get("body").and_then(|x| x.as_str()).map(|s| s.to_string());
                let labels: Vec<String> = if let Some(lv) = v.get("labels") {
                    if let Some(a) = lv.as_array() {
                        a.iter().filter_map(|x| x.as_str()).map(|s| s.to_string()).collect()
                    } else if let Some(s) = lv.as_str() {
                        s.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
                    } else { vec![] }
                } else { vec![] };

                let id = next_id(&dir)?;
                let now_s = now();
                let mut issue = Issue {
                    id: id.clone(),
                    title,
                    body,
                    labels,
                    state: IssueState::Open,
                    created_at: now_s.clone(),
                    updated_at: now_s,
                };
                // allow optional source passthrough
                if let Some(src) = v.get("source") {
                    let mut value = serde_json::to_value(&issue)?;
                    value["source"] = src.clone();
                    std::fs::write(dir.join(format!("{id}.json")), serde_json::to_vec_pretty(&value)?)?;
                } else {
                    std::fs::write(dir.join(format!("{id}.json")), serde_json::to_vec_pretty(&issue)?)?;
                }
                created.push(id);
            }
            let exit = ctx.ok("issue","import",&serde_json::json!({"from":"file","path":path,"created":created}));
            return Ok(exit);
        }
        "github" => {
            let repo = m.get_one::<String>("repo").unwrap();
            let state = m.get_one::<String>("state").map(|s| s.as_str()).unwrap_or("open");
            let labels_csv = m.get_one::<String>("labels").map(|s| s.as_str());
            let since = m.get_one::<String>("since").map(|s| s.as_str());
            let limit = *m.get_one::<u32>("limit").unwrap_or(&100);
            let dry = m.get_flag("dry-run");
            let fixture = m.get_one::<String>("fixture").map(|s| s.as_str());

            let created = import_github(repo, state, labels_csv, since, limit, dry, fixture, &dir)?;
            let exit = ctx.ok("issue","import",&serde_json::json!({
                "from":"github","repo":repo,"dry_run":dry,"created":created
            }));
            return Ok(exit);
        }
        _ => {
            return Ok(ctx.err(
                "issue","import",
                "issue.import.unsupported",
                "only --from file|github are supported",
                Some("use: issue import --from file --path <json> OR --from github --repo owner/name"),
                2
            ));
        }
    }
}

(iv) Add the GitHub import helper inside the same file:

#[derive(serde::Deserialize)]
struct GhLabel { name: String }
#[derive(serde::Deserialize)]
struct GhIssue {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    html_url: String,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    labels: Vec<GhLabel>,
    // PRs appear in /issues; we skip those by checking for "pull_request" key
    #[serde(default)]
    pull_request: Option<serde_json::Value>,
}

fn import_github(
    repo: &str,
    state: &str,
    labels_csv: Option<&str>,
    since: Option<&str>,
    limit: u32,
    dry_run: bool,
    fixture: Option<&str>,
    out_dir: &std::path::Path,
) -> anyhow::Result<Vec<String>> {
    // load JSON: either from fixture or GitHub API
    let gh_issues: Vec<GhIssue> = if let Some(path) = fixture {
        let bytes = std::fs::read(path)?;
        serde_json::from_slice(&bytes)?
    } else {
        let url = format!("https://api.github.com/repos/{repo}/issues");
        let mut q = vec![("state", state.to_string()), ("per_page", limit.min(100).to_string())];
        if let Some(lbl) = labels_csv { if !lbl.is_empty() { q.push(("labels".into(), lbl.into())); } }
        if let Some(s) = since { q.push(("since".into(), s.into())); }

        let token = std::env::var("GITHUB_TOKEN").ok();
        let client = Client::builder().build()?;
        let mut req = client.get(&url)
            .header(USER_AGENT, "nexoscli/0.1")
            .header(ACCEPT, "application/vnd.github+json")
            .query(&q);

        if let Some(t) = token { req = req.header(AUTHORIZATION, format!("Bearer {t}")); }

        // Optional ETag cache
        let cache_dir = out_dir.parent().unwrap_or_else(|| std::path::Path::new(".")).join(".import").join("github");
        std::fs::create_dir_all(&cache_dir).ok();
        let cache_key = repo.replace("/", "_");
        let cache_file = cache_dir.join(format!("{cache_key}.json"));
        let mut etag_to_save: Option<String> = None;
        if let Ok(c) = std::fs::read_to_string(&cache_file) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&c) {
                if let Some(et) = v.get("etag").and_then(|x| x.as_str()) {
                    req = req.header(IF_NONE_MATCH, et);
                }
            }
        }

        let resp = req.send()?;
        if resp.status().as_u16() == 304 {
            // nothing new
            return Ok(vec![]);
        }
        if let Some(et) = resp.headers().get(ETAG).and_then(|v| v.to_str().ok()) {
            etag_to_save = Some(et.to_string());
        }
        let mut issues: Vec<GhIssue> = resp.json()?;
        // Drop PRs (they include pull_request key)
        issues.retain(|i| i.pull_request.is_none());
        // Save minimal cache
        if let Some(etag) = etag_to_save {
            let meta = serde_json::json!({"etag": etag, "fetched": now()});
            let _ = std::fs::write(&cache_file, serde_json::to_vec_pretty(&meta)?);
        }
        issues
    };

    // Map & (optionally) persist
    let (owner, name) = repo.split_once('/').ok_or_else(|| anyhow::anyhow!("repo must be owner/name"))?;
    let mut created = vec![];
    for gi in gh_issues.into_iter().take(limit as usize) {
        let id = format!("ISS-GH_{}_{}_{}", owner, name, gi.number);
        if dry_run {
            created.push(id);
            continue;
        }
        let labels: Vec<String> = gi.labels.into_iter().map(|l| l.name).collect();
        let mut obj = serde_json::json!({
            "id": id,
            "title": gi.title,
            "body": gi.body,
            "labels": labels,
            "state": if gi.state == "open" { "open" } else { "closed" },
            "created_at": gi.created_at,
            "updated_at": gi.updated_at,
            "source": {
                "origin": "github",
                "repo": repo,
                "number": gi.number,
                "url": gi.html_url
            }
        });
        let path = out_dir.join(format!("{id}.json"));
        std::fs::write(path, serde_json::to_vec_pretty(&obj)?)?;
        created.push(obj["id"].as_str().unwrap().to_string());
    }
    Ok(created)
}

Behavior notes
	•	No writes to GitHub; read‑only.
	•	Idempotent with ETag (no new files if nothing changed).
	•	Offline CI: pass --fixture tests/fixtures/github_issues.json to avoid network.

⸻

(B) TracePack bundles: bundle build / bundle verify

We’ll add a new plugin: plugins/bundle.

1) Add the plugin crate

plugins/bundle/Cargo.toml

[package]
name = "nexoscli-plugin-bundle"
version = "0.1.0"
edition = "2021"

[dependencies]
nexoscli-core = { path = "../../nexoscli-core" }
anyhow = { workspace = true }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
serde_yaml = { workspace = true }
clap = { workspace = true }
time = { workspace = true }
tar = { workspace = true }
flate2 = { workspace = true }
sha2 = { workspace = true }

plugins/bundle/src/lib.rs

use clap::{Arg, ArgAction, ArgMatches, Command};
use flate2::{write::GzEncoder, Compression};
use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write, path::{Path, PathBuf}};
use tar::{Builder as TarBuilder, Archive};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub struct BundlePlugin;
impl BundlePlugin { pub fn new() -> Self { Self } }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BundleBuildArgs {
    node_id: String,
    #[serde(default)]
    out: Option<String>,
    #[serde(default)]
    include: Option<String>, // csv: issues,docs
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BundleBuildResult {
    path: String,
    inputs: Vec<String>,
    checksums: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BundleVerifyArgs {
    path: String,
    #[serde(default)]
    against: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BundleVerifyResult {
    manifest_ok: bool,
    archive_ok: bool,
    against_ok: Option<bool>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    node_id: String,
    created_at: String,
    generator: Generator,
    inputs: Vec<String>,
    checksums: BTreeMap<String, String>,
}
#[derive(Debug, Serialize, Deserialize)]
struct Generator { cli: String, version: String }

impl Plugin for BundlePlugin {
    fn name(&self) -> &'static str { "bundle" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "TracePack bundles (build/verify)" }

    fn augment_commands(&self, app: Command) -> Command {
        app.subcommand(
            Command::new("bundle").about("TracePack bundles")
              .subcommand(
                Command::new("build")
                  .about("Build a bundle (.tgz) with manifest and inputs")
                  .arg(Arg::new("node_id").required(true))
                  .arg(Arg::new("out").long("out").value_name("DIR"))
                  .arg(Arg::new("include").long("include").value_name("CSV").help("issues,docs"))
              )
              .subcommand(
                Command::new("verify")
                  .about("Verify a bundle")
                  .arg(Arg::new("path").required(true))
                  .arg(Arg::new("against").long("against").value_name("DIR").help("Also verify current filesystem files match manifest"))
              )
        )
    }

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match sub_matches.subcommand() {
            Some(("build", m)) => {
                let node_id = m.get_one::<String>("node_id").unwrap();
                let out = m.get_one::<String>("out").map(|s| s.as_str()).unwrap_or(".nexoscli/bundles");
                let includes = m.get_one::<String>("include").map(|s| s.as_str()).unwrap_or("issues,docs");
                let root = std::env::current_dir()?;

                let mut files: Vec<String> = vec![];
                if includes.contains("docs") {
                    let p = root.join("docs").join("exec_graph.yaml");
                    if p.exists() { files.push(path_rel(&root, &p)); }
                }
                if includes.contains("issues") {
                    let idir = root.join(".nexoscli").join("issues");
                    if idir.exists() {
                        for e in walk_json(&idir)? { files.push(path_rel(&root, &e)); }
                    }
                }

                let checksums = hash_map(&root, &files)?;
                let manifest = Manifest {
                    node_id: node_id.clone(),
                    created_at: OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
                    generator: Generator { cli: "nexoscli".into(), version: env!("CARGO_PKG_VERSION").into() },
                    inputs: files.clone(),
                    checksums: checksums.clone(),
                };

                let out_dir = root.join(out);
                fs::create_dir_all(&out_dir)?;
                let fname = format!("bundle-{}-{}.tgz", sanitize(node_id), ts_compact());
                let out_path = out_dir.join(fname);

                // write tar.gz
                let f = fs::File::create(&out_path)?;
                let mut enc = GzEncoder::new(f, Compression::default());
                let mut tar = TarBuilder::new(&mut enc);

                // manifest.json
                let mbytes = serde_json::to_vec_pretty(&manifest)?;
                tar.append_data(&mut tar::Header::new_gnu(), "manifest.json", mbytes.as_slice())?;

                // files under files/<path>
                for rel in &files {
                    let src = root.join(rel);
                    let mut header = tar::Header::new_gnu();
                    let meta = fs::metadata(&src)?;
                    header.set_size(meta.len());
                    header.set_mode(0o644);
                    header.set_cksum();
                    let mut f = fs::File::open(&src)?;
                    let arc_path = Path::new("files").join(rel);
                    tar.append_data(&mut header, arc_path, &mut f)?;
                }
                tar.finish()?;
                enc.finish()?;

                let exit = ctx.ok("bundle","build",&serde_json::json!({
                    "path": out_path.display().to_string(),
                    "inputs": files,
                    "checksums": checksums
                }));
                Ok(exit)
            }
            Some(("verify", m)) => {
                let path = m.get_one::<String>("path").unwrap();
                let against = m.get_one::<String>("against").map(|s| s.as_str());
                let root = std::env::current_dir()?;

                let f = fs::File::open(path)?;
                let dec = flate2::read::GzDecoder::new(f);
                let mut ar = Archive::new(dec);

                // extract manifest.json into memory and also compute hashes of archived files
                let mut manifest: Option<Manifest> = None;
                let mut arc_hashes: BTreeMap<String, String> = BTreeMap::new();
                for e in ar.entries()? {
                    let mut e = e?;
                    let p = e.path()?;
                    let pstr = p.to_string_lossy().to_string();
                    if pstr == "manifest.json" {
                        let mut buf = Vec::new();
                        std::io::copy(&mut e, &mut buf)?;
                        manifest = Some(serde_json::from_slice(&buf)?);
                    } else if pstr.starts_with("files/") {
                        let rel = pstr.trim_start_matches("files/").to_string();
                        let mut hasher = Sha256::new();
                        let mut buf = [0u8; 8192];
                        loop {
                            let n = e.read(&mut buf)?;
                            if n == 0 { break; }
                            hasher.update(&buf[..n]);
                        }
                        arc_hashes.insert(rel, format!("sha256:{:x}", hasher.finalize()));
                    }
                }

                let mut errors = vec![];
                let mf = match manifest {
                    Some(m) => m,
                    None => {
                        let exit = ctx.err("bundle","verify","bundle.manifest.missing","manifest.json not found in archive", None, 2);
                        return Ok(exit);
                    }
                };

                // archive_ok: archived file hashes match manifest
                let mut archive_ok = true;
                for (rel, want) in &mf.checksums {
                    match arc_hashes.get(rel) {
                        Some(got) if got == want => {}
                        Some(got) => { archive_ok = false; errors.push(format!("mismatch: {} expected {} got {}", rel, want, got)); }
                        None => { archive_ok = false; errors.push(format!("missing in archive: {}", rel)); }
                    }
                }

                // against_ok: current filesystem matches manifest (optional)
                let mut against_ok = None;
                if let Some(dir) = against {
                    let base = Path::new(dir);
                    let mut ok = true;
                    for (rel, want) in &mf.checksums {
                        let p = base.join(rel);
                        if !p.exists() {
                            ok = false; errors.push(format!("against missing: {}", rel)); continue;
                        }
                        let got = hash_file(&p)?;
                        if got != *want { ok = false; errors.push(format!("against mismatch: {} expected {} got {}", rel, want, got)); }
                    }
                    against_ok = Some(ok);
                }

                let manifest_ok = true; // schema is trivial for v0
                let exit_code = if archive_ok && against_ok.unwrap_or(true) { 0 } else { 2 };
                let exit = ctx.ok("bundle","verify",&serde_json::json!({
                    "manifest_ok": manifest_ok,
                    "archive_ok": archive_ok,
                    "against_ok": against_ok,
                    "errors": errors
                }));
                if exit_code != 0 { std::process::exit(exit_code); }
                Ok(0)
            }
            _ => Ok(2),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "bundle.build".to_string(),
                about: "Build a TracePack bundle".to_string(),
                args_schema: schema_for!(BundleBuildArgs),
                result_schema: schema_for!(BundleBuildResult),
            },
            ToolSpec {
                name: "bundle.verify".to_string(),
                about: "Verify a TracePack bundle".to_string(),
                args_schema: schema_for!(BundleVerifyArgs),
                result_schema: schema_for!(BundleVerifyResult),
            },
        ]
    }
}

fn path_rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().to_string()
}
fn walk_json(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut v = vec![];
    for e in fs::read_dir(dir)? {
        let e = e?;
        if e.file_type()?.is_file() && e.path().extension().map(|x| x=="json").unwrap_or(false) {
            v.push(e.path());
        }
    }
    Ok(v)
}
fn ts_compact() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap().replace([':', '-'], "").replace('+', "")
}
fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}
fn hash_file(p: &Path) -> anyhow::Result<String> {
    let mut h = Sha256::new();
    let mut f = fs::File::open(p)?;
    std::io::copy(&mut f, &mut h)?;
    Ok(format!("sha256:{:x}", h.finalize()))
}
fn hash_map(root: &Path, files: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
    let mut m = BTreeMap::new();
    for rel in files {
        m.insert(rel.clone(), hash_file(&root.join(rel))?);
    }
    Ok(m)
}

2) Register the plugin

Root Cargo.toml workspace members:

[workspace]
members = [
  "nexoscli-core","nexoscli-bin",
  "plugins/issue","plugins/dev","plugins/graph","plugins/demo",
  "plugins/bundle"              # NEW
]
resolver = "2"

nexoscli-bin/Cargo.toml dependencies:

[dependencies]
# ... existing ...
nexoscli-plugin-bundle = { path = "../plugins/bundle" }   # NEW

nexoscli-bin/src/main.rs registration:

registry.register(Box::new(nexoscli_plugin_bundle::BundlePlugin::new())); // NEW


⸻

(A)+(B) Tests (offline and deterministic)

Add the following new test files under nexoscli-bin/tests/.
(You already have dev_init.rs, issue_flow.rs, issue_import.rs—keep them.)

These tests run in temp dirs and never touch your repo.

1) helpers.rs (shared)

If you don’t have a helpers file yet, add the one I provided earlier. (It includes tmp(), run_json(), run_raw(), assert_event_for_run(), and optional event export.)

2) issue_github_import.rs

mod helpers; use helpers::*;
use serde_json::json;

#[test]
fn github_import_from_fixture_dry_run_and_apply() {
    let t = tmp();

    // Seed a fake GitHub payload
    let fixture = r#"
    [
      {
        "number": 101,
        "title": "Fixture One",
        "body": "body one",
        "state": "open",
        "html_url": "https://example/1",
        "created_at": "2024-12-01T00:00:00Z",
        "updated_at": "2024-12-02T00:00:00Z",
        "labels": [{"name":"bug"},{"name":"p0"}]
      },
      {
        "number": 102,
        "title": "Fixture Two",
        "state": "closed",
        "html_url": "https://example/2",
        "created_at": "2024-12-03T00:00:00Z",
        "updated_at": "2024-12-04T00:00:00Z",
        "labels": []
      }
    ]
    "#;
    std::fs::create_dir_all(t.path().join("tests")).unwrap();
    std::fs::write(t.path().join("tests/gh.json"), fixture).unwrap();

    // dry-run
    let v = run_json(t.path(), &[
        "issue","import","--from","github",
        "--repo","owner/repo",
        "--fixture","tests/gh.json",
        "--dry-run"
    ]);
    assert!(v["data"]["created"].as_array().unwrap().len() == 2);

    // apply
    let v2 = run_json(t.path(), &[
        "issue","import","--from","github",
        "--repo","owner/repo",
        "--fixture","tests/gh.json"
    ]);
    let created = v2["data"]["created"].as_array().unwrap();
    assert_eq!(created.len(), 2);
    // file names exist
    let p1 = t.path().join(".nexoscli/issues").join("ISS-GH_owner_repo_101.json");
    let p2 = t.path().join(".nexoscli/issues").join("ISS-GH_owner_repo_102.json");
    assert!(p1.exists() && p2.exists());

    export_events(t.path(), "issue_github_import");
}

3) bundle_e2e.rs

mod helpers; use helpers::*;
use serde_yaml;

#[test]
fn bundle_build_and_verify_against_current() {
    let t = tmp();

    // Prepare minimal repo: issues + docs/exec_graph.yaml
    let _ = run_json(t.path(), &["dev","init","--yes"]);
    // two local issues
    let _ = run_json(t.path(), &["issue","new","--title","A"]);
    let _ = run_json(t.path(), &["issue","new","--title","B"]);
    // graph
    std::fs::create_dir_all(t.path().join("docs")).unwrap();
    let g = serde_yaml::to_string(&serde_yaml::yaml!({
        "id": "EG-test",
        "nodes": [
            { "id": "A", "deps": [] },
            { "id": "B", "deps": ["A"] }
        ]
    })).unwrap();
    std::fs::write(t.path().join("docs/exec_graph.yaml"), g).unwrap();

    // build
    let v = run_json(t.path(), &["bundle","build","EG-test","--include","issues,docs"]);
    let bpath = v["data"]["path"].as_str().unwrap();
    assert!(std::path::Path::new(bpath).exists());

    // verify (archive itself)
    let v2 = run_json(t.path(), &["bundle","verify", bpath]);
    assert!(v2["data"]["archive_ok"].as_bool().unwrap());

    // verify against current filesystem
    let v3 = run_json(t.path(), &["bundle","verify", bpath, "--against", "."]);
    assert!(v3["data"]["against_ok"].as_bool().unwrap());

    // corrupt a file and fail against
    // change one issue file to break checksum
    let issues_dir = t.path().join(".nexoscli/issues");
    let mut one = std::fs::read_dir(&issues_dir).unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().map(|x| x=="json").unwrap_or(false))
        .unwrap().path();
    std::fs::write(&one, br#"{"id":"tampered"}"#).unwrap();

    let (code, out, _err) = run_raw(t.path(), &["bundle","verify", bpath, "--against", ".", "--output", "json"]);
    assert!(code != 0, "verify --against should fail after tamper");
    let v4: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v4["data"]["against_ok"].as_bool() == Some(false));

    export_events(t.path(), "bundle_build_verify");
}


⸻

Wire the plugin into the bin

You already registered dev/graph/demo/issue. Add bundle (as shown above), rebuild, and run tests:

cargo build --workspace
cargo test --workspace


⸻

Acceptance (manual quick probes)

# GitHub import from a fixture
cargo nex -- issue import --from github --repo owner/repo --fixture tests/gh.json --dry-run --output json
cargo nex -- issue import --from github --repo owner/repo --fixture tests/gh.json --output json
ls .nexoscli/issues/ISS-GH_owner_repo_*.json

# (Optional) live call — needs GITHUB_TOKEN
GITHUB_TOKEN=... cargo nex -- issue import --from github --repo rust-lang/rfcs --state open --limit 10 --output json

# Bundle build/verify
cargo nex -- bundle build EG-sample --include issues,docs --output json
cargo nex -- bundle verify .nexoscli/bundles/bundle-EG_sample-*.tgz --output json
cargo nex -- bundle verify .nexoscli/bundles/bundle-EG_sample-*.tgz --against . --output json


⸻

Pin future (leave until needed)

Add to docs/ROADMAP_NEXT.md under “Ready‑to‑pull”:
	•	GitHub pagination + incremental sync: persist since and last_imported_number in .nexoscli/.import/github/<repo>.json, support --since last to continue.
	•	Label/assignee mapping policy: optional transform table at .nexoscli/mapping/github.toml.
	•	Bundles: add --include docs/ADRs/**, --include prompts/**, and a --notes free‑text field that lands in manifest.
	•	Bundles: bundle inspect to print manifest and file list.
	•	Bundles: bundle diff <a.tgz> <b.tgz> to show inputs + checksum deltas.

⸻

Blind‑spot check (candor)
	•	Keep fixture‑based tests for GitHub to avoid flaky CI. The live path is optional and safe.
	•	Bundle verify without --against will always pass if the archive is intact (by design). The --against . path is the useful guard for drift.
	•	Id strategy: imported IDs are prefixed (ISS-GH_owner_repo_<number>) so they never collide with local ISS-000x