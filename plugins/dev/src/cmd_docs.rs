use clap::{Arg, ArgAction, ArgMatches, Command};
use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn subcommand() -> Command {
    Command::new("commands")
        .about("Generate CLI reference/catalog and update plugin READMEs")
        .arg(Arg::new("out-md").long("out-md").required(false).value_name("PATH"))
        .arg(Arg::new("out-json").long("out-json").required(false).value_name("PATH"))
        .arg(Arg::new("apply-readmes").long("apply-readmes").action(ArgAction::SetTrue))
        .arg(Arg::new("check-parity").long("check-parity").action(ArgAction::SetTrue)
            .help("Ensure all ToolSpecs appear in catalog and vice-versa; exit non-zero on mismatch"))
}

/// Execute `dev docs commands`
pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let out_md = m
        .get_one::<String>("out-md")
        .cloned()
        .unwrap_or_else(|| "docs/CLI_REFERENCE.md".to_string());
    let out_json = m.get_one::<String>("out-json").cloned();
    let apply = m.get_flag("apply-readmes");
    let check_parity = m.get_flag("check-parity");

    // 1) Introspect live clap tree
    let (code, tree) = run_cli_json(&cwd, &["__introspect", "clap-json"])?;
    if code != 0 {
        let exit = ctx.err(
            "dev",
            "docs.commands",
            "introspect.failed",
            "introspection command returned non-zero",
            Some("Ensure '__introspect clap-json' is wired; rebuild the workspace"),
            2,
        );
        return Ok(exit);
    }

    // Tools export (for toolspec mapping and schemas)
    let (_ct, tools) = run_cli_json(&cwd, &["tools", "export"])?;

    // 2) Render CLI_REFERENCE.md (deterministic)
    if let Some(parent) = Path::new(&out_md).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let md = render_markdown(&tree)?;
    fs::write(&out_md, md.as_bytes())?;
    let mut written = vec![out_md.clone()];

    // 3) Render commands catalog JSON if requested
    let mut catalog_value: Option<Value> = None;
    if let Some(json_path) = out_json.as_ref() {
        if let Some(parent) = Path::new(json_path).parent() {
            let _ = fs::create_dir_all(parent);
        }
        // Preserve generated_at if file already exists to keep catalog deterministic across regenerations
        let mut existing_generated_at: Option<String> = None;
        if Path::new(json_path).exists() {
            if let Ok(prev_txt) = fs::read_to_string(json_path) {
                if let Ok(prev_val) = serde_json::from_str::<Value>(&prev_txt) {
                    existing_generated_at = prev_val
                        .get("generated_at")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string());
                }
            }
        }
        let version = tree.get("version").and_then(|v| v.as_str()).unwrap_or(env!("CARGO_PKG_VERSION"));
        let cat = build_catalog_value(&tree, &tools, version, existing_generated_at.as_deref())?;
        let text = serde_json::to_string_pretty(&cat)?;
        fs::write(json_path, text.as_bytes())?;
        written.push(json_path.clone());
        catalog_value = Some(cat);
    }

    // 4) Update plugin README AUTO blocks if requested
    let mut updated_readmes: Vec<String> = vec![];
    if apply {
        let plugins_dir = cwd.join("plugins");
        if plugins_dir.exists() {
            for entry in fs::read_dir(&plugins_dir)? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let plugin = entry.file_name().to_string_lossy().to_string();
                let readme = entry.path().join("README.md");
                let body = render_plugin_commands_block(&plugin, &tree)?;
                let changed = ensure_commands_block(&readme, &body)?;
                if changed {
                    updated_readmes.push(readme.display().to_string());
                }
            }
        }
    }

    // 5) Parity check (ToolSpec names vs catalog toolspec)
    if check_parity {
        let cat = if let Some(v) = catalog_value.as_ref() {
            v.clone()
        } else {
            // If --out-json wasn't provided, build catalog in-memory
            let version = tree.get("version").and_then(|v| v.as_str()).unwrap_or(env!("CARGO_PKG_VERSION"));
            build_catalog_value(&tree, &tools, version, None)?
        };

        let ts_names: BTreeSet<String> = tools["data"]["tools"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|t| t.get("name").and_then(|x| x.as_str()).map(|s| s.to_string()))
            .collect();

        let cat_names: BTreeSet<String> = cat["commands"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|c| c.get("toolspec").and_then(|t| t.get("name")).and_then(|x| x.as_str()).map(|s| s.to_string()))
            .collect();

        let only_in_tools: Vec<String> = ts_names.difference(&cat_names).cloned().collect();
        let only_in_catalog: Vec<String> = cat_names.difference(&ts_names).cloned().collect();
        if !only_in_tools.is_empty() || !only_in_catalog.is_empty() {
            let msg = format!(
                "toolspec/catalog mismatch: only_in_tools={:?}; only_in_catalog={:?}",
                only_in_tools, only_in_catalog
            );
            let exit = ctx.err("dev","docs.commands.parity","catalog.toolspec.mismatch",&msg, Some("Ensure command path → toolspec name mapping matches <plugin>.<subpath>"), 2);
            return Ok(exit);
        }
    }

    let summary = format!(
        "wrote {}{}; updated_readmes={}",
        out_md,
        out_json.as_ref().map(|s| format!(",{}", s)).unwrap_or_default(),
        updated_readmes.len()
    );
    let exit = ctx.ok(
        "dev",
        "docs.commands",
        &serde_json::json!({
            "written": written,
            "updated_readmes": updated_readmes,
            "summary": summary
        }),
    );
    Ok(exit)
}

// ---------------- rendering (PLAN‑54) ----------------

fn render_markdown(tree: &Value) -> anyhow::Result<String> {
    let name = tree.get("name").and_then(|v| v.as_str()).unwrap_or("nexoscli");
    let about = tree.get("about").and_then(|v| v.as_str()).unwrap_or("");
    let version = tree
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or(env!("CARGO_PKG_VERSION"));

    let mut out = String::new();
    out.push_str("# NEXOS CLI Reference (generated)\n\n");
    if !about.is_empty() {
        out.push_str(&format!("{} — v{}\n\n", about, version));
    } else {
        out.push_str(&format!("{} v{}\n\n", name, version));
    }
    out.push_str("_Generated by `nexoscli dev docs commands`._\n\n");

    // Command Tree (nested)
    out.push_str("## Command Tree\n\n");
    out.push_str(&render_tree(tree));

    // Sections per top-level group
    if let Some(subs) = tree.get("subcommands").and_then(|v| v.as_array()) {
        let mut items: Vec<&Value> = subs.iter().collect();
        items.sort_by(|a, b| path_of(a).cmp(&path_of(b)));

        for top in items {
            let top_path = path_of(top);
            out.push_str(&format!("\n## {}\n\n", top_path));
            render_sections_recursive(top, &mut out);
        }
    }

    Ok(out)
}

fn render_tree(tree: &Value) -> String {
    let mut rows: Vec<(String, String, usize)> = Vec::new();
    if let Some(subs) = tree.get("subcommands").and_then(|v| v.as_array()) {
        for node in subs {
            collect_tree_rows(node, &mut rows);
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::new();
    for (path, about, depth) in rows {
        let indent = "  ".repeat(depth.saturating_sub(1));
        let anchor = anchor_for(&path);
        let suffix = if about.is_empty() { String::new() } else { format!(" — {}", about) };
        out.push_str(&format!("{}- [{}](#{}){}\n", indent, path, anchor, suffix));
    }
    out.push('\n');
    out
}

fn collect_tree_rows(node: &Value, rows: &mut Vec<(String, String, usize)>) {
    let path = path_of(node);
    let about = node.get("about").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let depth = path.split_whitespace().count();
    rows.push((path.clone(), about, depth));
    if let Some(subs) = node.get("subcommands").and_then(|v| v.as_array()) {
        for sc in subs {
            collect_tree_rows(sc, rows);
        }
    }
}

fn render_sections_recursive(node: &Value, out: &mut String) {
    if is_leaf(node) {
        let path = path_of(node);
        let about = node.get("about").and_then(|v| v.as_str()).unwrap_or("");
        out.push_str(&format!("### {}\n\n", path));
        if !about.is_empty() {
            out.push_str(&format!("{}\n\n", about));
        }
        if let Some(usage) = node.get("usage").and_then(|v| v.as_str()) {
            out.push_str("Usage:\n\n```\n");
            out.push_str(usage);
            out.push_str("\n```\n\n");
        }
        out.push_str(&render_flags_table(node));
        out.push('\n');
    } else if let Some(subs) = node.get("subcommands").and_then(|v| v.as_array()) {
        let mut items: Vec<&Value> = subs.iter().collect();
        items.sort_by(|a, b| path_of(a).cmp(&path_of(b)));
        for sc in items {
            render_sections_recursive(sc, out);
        }
    }
}

fn render_flags_table(node: &Value) -> String {
    let mut out = String::new();
    let args = node.get("args").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if args.is_empty() {
        out.push_str("_No flags._\n");
        return out;
    }
    out.push_str("| Flag | Type | Required | Default | Description |\n");
    out.push_str("|------|------|----------|---------|-------------|\n");
    for a in args {
        let long = a.get("long").and_then(|v| v.as_str());
        let short = a.get("short").and_then(|v| v.as_str());
        let flag = match (short, long) {
            (Some(s), Some(l)) => format!("-{}, --{}", s, l),
            (Some(s), None) => format!("-{}", s),
            (None, Some(l)) => format!("--{}", l),
            _ => String::from("(arg)"),
        };
        let kind = a.get("kind").and_then(|v| v.as_str()).unwrap_or("value");
        let required = a.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
        let default = a.get("default").and_then(|v| v.as_str()).unwrap_or("");
        let help = a.get("help").and_then(|v| v.as_str()).unwrap_or("");
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            flag,
            kind,
            if required { "yes" } else { "no" },
            if default.is_empty() { "—" } else { default },
            escape_pipes(help)
        ));
    }
    out.push('\n');
    out
}

fn escape_pipes(s: &str) -> String {
    s.replace('|', "\\|")
}

fn is_leaf(node: &Value) -> bool {
    node.get("subcommands")
        .and_then(|v| v.as_array())
        .map(|a| a.is_empty())
        .unwrap_or(true)
}

fn path_of(node: &Value) -> String {
    node.get("path")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn anchor_for(path: &str) -> String {
    path.to_lowercase().replace(' ', "-")
}

// ---------------- Catalog builder (PLAN‑55) ----------------

fn build_catalog_value(tree: &Value, tools: &Value, version: &str, existing_generated_at: Option<&str>) -> anyhow::Result<Value> {
    // Precompute ToolSpec map: name -> { args_schema, result_schema }
    let mut tool_schemas: BTreeMap<String, (Value, Value)> = BTreeMap::new();
    if let Some(arr) = tools.get("data").and_then(|d| d.get("tools")).and_then(|t| t.as_array()) {
        for t in arr {
            if let Some(name) = t.get("name").and_then(|x| x.as_str()) {
                let args_schema = t.get("args_schema").cloned().unwrap_or(json!({}));
                let result_schema = t.get("result_schema").cloned().unwrap_or(json!({}));
                tool_schemas.insert(name.to_string(), (args_schema, result_schema));
            }
        }
    }

    // Gather command entries
    let mut commands: Vec<Value> = Vec::new();
    gather_commands_recursive(tree, &mut commands, &tool_schemas);

    // Deterministic sort by path
    commands.sort_by(|a, b| {
        a.get("path")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .cmp(b.get("path").and_then(|x| x.as_str()).unwrap_or(""))
    });

    // Build schemas map for referenced toolspecs only
    let mut schemas_pairs: Vec<(String, Value)> = Vec::new();
    let mut used_tools: BTreeSet<String> = BTreeSet::new();
    for c in &commands {
        if let Some(ts) = c.get("toolspec") {
            if let Some(n) = ts.get("name").and_then(|x| x.as_str()) {
                used_tools.insert(n.to_string());
            }
        }
    }
    for name in used_tools {
        if let Some((args_s, res_s)) = tool_schemas.get(&name) {
            schemas_pairs.push((format!("{}.args.json", name), args_s.clone()));
            schemas_pairs.push((format!("{}.result.json", name), res_s.clone()));
        }
    }
    schemas_pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut schemas_obj = Map::new();
    for (k, v) in schemas_pairs {
        schemas_obj.insert(k, v);
    }

    let generated_at = existing_generated_at
        .map(|s| s.to_string())
        .unwrap_or_else(|| OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_else(|_| "1970-01-01T00:00:00Z".into()));
    let out = json!({
        "schema": "commands_catalog.v1",
        "generated_at": generated_at,
        "root": "nexoscli",
        "version": version,
        "commands": commands,
        "schemas": Value::Object(schemas_obj),
    });
    Ok(out)
}

fn gather_commands_recursive(node: &Value, out: &mut Vec<Value>, tool_schemas: &BTreeMap<String, (Value, Value)>) {
    if is_leaf(node) {
        let path = path_of(node);
        if path.is_empty() {
            return;
        }
        let plugin = path.split_whitespace().next().unwrap_or("-");
        let about = node.get("about").and_then(|v| v.as_str());
        let usage = node.get("usage").and_then(|v| v.as_str());
        let args = node.get("args").and_then(|v| v.as_array()).cloned().unwrap_or_default();

        // Normalize args
        let mut norm_args: Vec<Value> = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let long = a.get("long").and_then(|x| x.as_str());
            let short = a.get("short").and_then(|x| x.as_str());
            let name = long.or(short).unwrap_or_else(|| {
                // synthetic name
                ""
            });
            let kind = a.get("kind").and_then(|x| x.as_str()).unwrap_or("value");
            let required = a.get("required").and_then(|x| x.as_bool()).unwrap_or(false);
            let default = a.get("default").and_then(|x| x.as_str());
            let poss = a.get("possible_values").and_then(|x| x.as_array());
            let help = a.get("help").and_then(|x| x.as_str());

            norm_args.push(json!({
                "name": if name.is_empty() { format!("arg{}", i+1) } else { name.to_string() },
                "long": long,
                "short": short,
                "kind": kind,
                "required": required,
                "default": default,
                "possible_values": if let Some(arr) = poss { if arr.is_empty() { Value::Null } else { Value::Array(arr.clone()) } } else { Value::Null },
                "help": help
            }));
        }

        // Guess toolspec name: join path tokens with '.'
        let ts_name = path.split_whitespace().collect::<Vec<_>>().join(".");
        let toolspec = if tool_schemas.contains_key(&ts_name) {
            Some(json!({
                "name": ts_name,
                "args_schema_ref": format!("schemas/{}.args.json", ts_name),
                "result_schema_ref": format!("schemas/{}.result.json", ts_name),
            }))
        } else {
            None
        };

        let entry = json!({
            "path": path,
            "plugin": plugin,
            "about": about,
            "usage": usage,
            "args": norm_args,
            "toolspec": toolspec
        });
        out.push(entry);
    } else if let Some(subs) = node.get("subcommands").and_then(|v| v.as_array()) {
        for sc in subs {
            gather_commands_recursive(sc, out, tool_schemas);
        }
    }
}

// ---------------- README AUTO block updater ----------------

fn ensure_commands_block(readme: &Path, body: &str) -> anyhow::Result<bool> {
    let begin = "<!-- BEGIN: AUTO (id=commands) -->";
    let end = "<!-- END: AUTO -->";

    if !readme.exists() {
        // Create minimal README with the AUTO block
        if let Some(parent) = readme.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut txt = String::new();
        let plugin = readme
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("plugin");
        txt.push_str(&format!("# {} plugin\n\n", plugin));
        txt.push_str("## Commands\n\n");
        txt.push_str(begin);
        txt.push('\n');
        txt.push_str(body);
        txt.push('\n');
        txt.push_str(end);
        txt.push('\n');
        fs::write(readme, txt.as_bytes())?;
        return Ok(true);
    }

    let txt = fs::read_to_string(readme)?;
    if let (Some(s), Some(e)) = (txt.find(begin), txt.find(end)) {
        if e > s {
            let head = &txt[..s + begin.len()];
            let tail = &txt[e..];
            let mut new_txt = String::new();
            new_txt.push_str(head);
            new_txt.push('\n');
            new_txt.push_str(body);
            new_txt.push('\n');
            new_txt.push_str(tail);
            if new_txt != txt {
                fs::write(readme, new_txt.as_bytes())?;
                return Ok(true);
            } else {
                return Ok(false);
            }
        }
    }

    // No block: append a new section deterministically
    let mut new_txt = txt.clone();
    if !new_txt.ends_with('\n') {
        new_txt.push('\n');
    }
    new_txt.push_str("\n## Commands\n\n");
    new_txt.push_str(begin);
    new_txt.push('\n');
    new_txt.push_str(body);
    new_txt.push('\n');
    new_txt.push_str(end);
    new_txt.push('\n');
    fs::write(readme, new_txt.as_bytes())?;
    Ok(true)
}

fn render_plugin_commands_block(plugin: &str, tree: &Value) -> anyhow::Result<String> {
    // Collect leaf commands whose path begins with "<plugin> "
    let mut paths: Vec<(String, String)> = Vec::new(); // (path, about)
    collect_leaf_paths(tree, &mut paths);
    let mut items: Vec<(String, String)> = paths
        .into_iter()
        .filter(|(p, _)| p == plugin || p.starts_with(&format!("{} ", plugin)))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));

    if items.is_empty() {
        return Ok("_No commands found for this plugin._".into());
    }

    let mut out = String::new();
    for (p, about) in items {
        let line = if about.is_empty() {
            format!("- {}", p)
        } else {
            format!("- {} — {}", p, about)
        };
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out.trim_end().to_string())
}

fn collect_leaf_paths(node: &Value, out: &mut Vec<(String, String)>) {
    if let Some(subs) = node.get("subcommands").and_then(|v| v.as_array()) {
        if subs.is_empty() {
            // leaf
            let p = path_of(node);
            let about = node.get("about").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if !p.is_empty() {
                out.push((p, about));
            }
        } else {
            for sc in subs {
                collect_leaf_paths(sc, out);
            }
        }
    }
}

// ---------------- local CLI helpers ----------------

fn run_cli_json(cwd: &Path, args: &[&str]) -> anyhow::Result<(i32, Value)> {
    let exe = std::env::current_exe()?;
    let mut a: Vec<&str> = args.to_vec();
    if !a.iter().any(|x| *x == "--output") {
        a.push("--output");
        a.push("json");
    }
    let out = std::process::Command::new(exe).current_dir(cwd).args(&a).output()?;
    let code = out.status.code().unwrap_or(-1);
    match parse_json_resilient(&out.stdout) {
        Some(val) => Ok((code, val)),
        None => {
            let stdout_head = String::from_utf8_lossy(&out.stdout).chars().take(300).collect::<String>();
            let stderr_head = String::from_utf8_lossy(&out.stderr).chars().take(300).collect::<String>();
            Err(anyhow::anyhow!(
                "stdout not JSON for {:?}; code={}. stdout_head={:?} stderr_head={:?}",
                a, code, stdout_head, stderr_head
            ))
        }
    }
}

fn parse_json_resilient(bytes: &[u8]) -> Option<Value> {
    // Strip UTF-8 BOM if present
    let bytes = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    };
    // Fast path
    if let Ok(v) = serde_json::from_slice::<Value>(bytes) {
        return Some(v);
    }
    // Trim whitespace and retry
    let s = String::from_utf8_lossy(bytes);
    let trimmed = s.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Some(v);
    }
    // Try last JSON-looking line (tolerate benign logs before JSON)
    for line in trimmed.lines().rev() {
        let l = line.trim();
        if l.starts_with('{') || l.starts_with('[') {
            if let Ok(v) = serde_json::from_str::<Value>(l) {
                return Some(v);
            }
        }
    }
    None
}
