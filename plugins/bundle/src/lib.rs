use clap::{Arg, ArgMatches, Command};
use flate2::{write::GzEncoder, Compression};
use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};
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
                {
                    let mut tar = TarBuilder::new(&mut enc);

                    // manifest.json
                    let mbytes = serde_json::to_vec_pretty(&manifest)?;
                    let mut header = tar::Header::new_gnu();
                    header.set_size(mbytes.len() as u64);
                    header.set_mode(0o644);
                    header.set_cksum();
                    tar.append_data(&mut header, "manifest.json", mbytes.as_slice())?;

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
                } // tar is dropped here
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
                            let n = std::io::Read::read(&mut e, &mut buf)?;
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
                let _exit = ctx.ok("bundle","verify",&serde_json::json!({
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
