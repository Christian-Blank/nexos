Love the momentum. Let’s ship the next three small-but-foundational slices that make everything cleaner for humans + AI and set us up for multi‑domain work:
	1.	Standard output & error envelope (one helper → no more ad‑hoc JSON in plugins)
	2.	Graph plugin (graph check + graph impact) to make the repo’s execution DAG first‑class
	3.	Dev init: optional config (--with-config) to bootstrap .nexoscli/config.toml

All three are additive, low‑risk, and give immediate value.

⸻

A) Standardize outputs & errors (Envelope)

Goal: Every command returns the same shape:

{ "data": <T or null>, "errors": [ { "code", "message", "suggestion?" } ], "meta": { "run_id", "plugin", "command" } }

…rendered as JSON / YAML / Text via the same helper. It also writes an event once (no more duplicate events.write() calls scattered across plugins).

1) Add envelope helpers to core

nexoscli-core/src/envelope.rs (new)

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    pub run_id: String,
    pub plugin: String,
    pub command: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorItem {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Envelope<T: Serialize> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    pub errors: Vec<ErrorItem>,
    pub meta: Meta,
}

impl<T: Serialize> Envelope<T> {
    pub fn ok(run_id: &str, plugin: &str, command: &str, data: T) -> Self {
        Self {
            data: Some(data),
            errors: vec![],
            meta: Meta {
                run_id: run_id.to_string(),
                plugin: plugin.to_string(),
                command: command.to_string(),
            },
        }
    }

    pub fn err(
        run_id: &str,
        plugin: &str,
        command: &str,
        code: &str,
        message: &str,
        suggestion: Option<String>,
    ) -> Envelope<serde_json::Value> {
        Envelope {
            data: None,
            errors: vec![ErrorItem {
                code: code.to_string(),
                message: message.to_string(),
                suggestion,
            }],
            meta: Meta {
                run_id: run_id.to_string(),
                plugin: plugin.to_string(),
                command: command.to_string(),
            },
        }
    }
}

Wire it up: export the module.

nexoscli-core/src/lib.rs

pub mod command;        // if present
pub mod error;
pub mod event;
pub mod plugin;
pub mod providers;
pub mod registry;
pub mod envelope;       // NEW

pub use error::*;
pub use event::*;
pub use plugin::*;

2) Add ok() / err() helpers on CommandContext

nexoscli-core/src/plugin.rs — add imports + methods

use crate::envelope::Envelope;
// ... existing imports ...

impl CommandContext {
    /// Render & event-write a success envelope in one go.
    pub fn ok<T: serde::Serialize>(&self, plugin: &str, command: &str, data: T) -> i32 {
        // write canonical event first
        let _ = self.events.write(plugin, command, &data, 0);

        // render a stable envelope
        let env = Envelope::ok(&self.providers.run_id, plugin, command, data);
        match self.output {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&env).unwrap()),
            OutputFormat::Yaml => println!("{}", serde_yaml::to_string(&env).unwrap()),
            OutputFormat::Text => println!("{}", serde_json::to_string_pretty(&env).unwrap()),
        }
        0
    }

    /// Render & event-write a failure envelope—use for recoverable/expected errors.
    pub fn err(
        &self,
        plugin: &str,
        command: &str,
        code: &str,
        message: &str,
        suggestion: Option<&str>,
        exit_code: i32,
    ) -> i32 {
        let env = Envelope::err(
            &self.providers.run_id,
            plugin,
            command,
            code,
            message,
            suggestion.map(|s| s.to_string()),
        );
        let _ = self.events.write(
            plugin,
            command,
            &serde_json::json!({"error": { "code": code, "message": message }}),
            exit_code,
        );

        match self.output {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&env).unwrap()),
            OutputFormat::Yaml => println!("{}", serde_yaml::to_string(&env).unwrap()),
            OutputFormat::Text => println!("{}", serde_json::to_string_pretty(&env).unwrap()),
        }
        exit_code
    }
}

3) Use the helpers in plugins

Replace the manual ctx.events.write(...) + ctx.render(...) pairs with one call:
	•	ctx.ok("issue","new", issue)
	•	ctx.ok("issue","list", json!({ "items": filtered }))
	•	ctx.err("issue","import", "issue.import.unsupported", "...", Some("Use --from file"), 2)

plugins/issue/src/lib.rs — change each subcommand arm:

// NEW (inside "new")
let exit = ctx.ok("issue", "new", &issue);
return Ok(exit);

// NEW (inside "list")
let exit = ctx.ok("issue", "list", &serde_json::json!({ "items": filtered }));
return Ok(exit);

// NEW (inside "show")
let exit = ctx.ok("issue", "show", &issue);
return Ok(exit);

// NEW (inside "close")
let exit = ctx.ok("issue", "close", &serde_json::json!({ "id": issue.id, "state": "closed" }));
return Ok(exit);

// NEW (import) success
let exit = ctx.ok("issue", "import", &serde_json::json!({ "from":"file","path": path, "created": created }));
return Ok(exit);

// NEW (import) unsupported
return Ok(ctx.err(
    "issue","import",
    "issue.import.unsupported",
    "only --from file is supported in v0",
    Some("use: issue import --from file --path <json>"),
    2
));

plugins/dev/src/lib.rs — use the helpers:

// plan
let exit = ctx.ok("dev","init.plan", &serde_json::json!({ "actions": actions }));
return Ok(exit);

// apply
let exit = ctx.ok("dev","init.apply", &serde_json::json!({ "actions": actions }));
return Ok(exit);

This centralizes JSON I/O, event writing, and meta/run_id inject—zero drift.

Acceptance (after build):

cargo run --manifest-path nexoscli-bin/Cargo.toml -- --output json issue new --title "env test"
# Output now has {data,errors,meta} with run_id/plugin/command


⸻

B) Graph plugin — graph check & graph impact

Goal: make docs/exec_graph.yaml queryable and lintable from the CLI so humans and AI can trust it.

1) Add the plugin to the workspace

Root Cargo.toml

[workspace]
members = [
  "nexoscli-core","nexoscli-bin",
  "plugins/issue","plugins/dev","plugins/graph"   # NEW
]
resolver = "2"

nexoscli-bin/Cargo.toml

[dependencies]
# ...existing...
nexoscli-plugin-graph = { path = "../plugins/graph" }   # NEW

nexoscli-bin/src/main.rs — register it:

let mut registry = PluginRegistry::new();
registry.register(Box::new(nexoscli_plugin_issue::IssuePlugin::new()));
registry.register(Box::new(nexoscli_plugin_dev::DevPlugin::new()));
registry.register(Box::new(nexoscli_plugin_graph::GraphPlugin::new()));  // NEW

2) Create the plugin

plugins/graph/Cargo.toml

[package]
name = "nexoscli-plugin-graph"
version = "0.1.0"
edition = "2021"

[dependencies]
nexoscli-core = { path = "../../nexoscli-core" }
anyhow = { workspace = true }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
serde_yaml = { workspace = true }
clap = { workspace = true }

plugins/graph/src/lib.rs

use clap::{Arg, ArgMatches, Command};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;

#[derive(Debug, Deserialize)]
struct ExecGraph {
    id: String,
    nodes: Vec<Node>,
}
#[derive(Debug, Deserialize)]
struct Node {
    id: String,
    #[serde(default)]
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    deps: Vec<String>, // dependencies this node relies on
}

pub struct GraphPlugin;
impl GraphPlugin { pub fn new() -> Self { Self } }

impl Plugin for GraphPlugin {
    fn name(&self) -> &'static str { "graph" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "Execution graph utilities (check, impact)" }

    fn augment_commands(&self, app: Command) -> Command {
        app.subcommand(
            Command::new("graph").about("Graph tools")
              .subcommand(
                Command::new("check")
                  .about("Validate docs/exec_graph.yaml (acyclic, unique ids)")
                  .arg(Arg::new("file").long("file").value_name("PATH").default_value("docs/exec_graph.yaml"))
              )
              .subcommand(
                Command::new("impact")
                  .about("List nodes impacted (downstream) by the given id")
                  .arg(Arg::new("id").required(true))
                  .arg(Arg::new("file").long("file").value_name("PATH").default_value("docs/exec_graph.yaml"))
              )
        )
    }

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match sub_matches.subcommand() {
            Some(("check", m)) => {
                let path = m.get_one::<String>("file").unwrap();
                let g = load_graph(path)?;
                let summary = check_graph(&g);
                let exit = if summary.errors.is_empty() {
                    ctx.ok("graph","check", &summary)
                } else {
                    ctx.err("graph","check", "graph.invalid", "graph has errors", Some("fix errors listed in data.errors"), 2)
                };
                Ok(exit)
            }
            Some(("impact", m)) => {
                let path = m.get_one::<String>("file").unwrap();
                let id = m.get_one::<String>("id").unwrap();
                let g = load_graph(path)?;
                let impacted = impact(&g, id);
                let exit = ctx.ok("graph","impact", &serde_json::json!({ "id": id, "impacted": impacted }));
                Ok(exit)
            }
            _ => Ok(2),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> { vec![] }
}

fn load_graph(path: &str) -> anyhow::Result<ExecGraph> {
    let bytes = fs::read(PathBuf::from(path))?;
    Ok(serde_yaml::from_slice(&bytes)?)
}

#[derive(serde::Serialize)]
struct CheckSummary {
    node_count: usize,
    unique_ids: bool,
    acyclic: bool,
    errors: Vec<String>,
}

fn check_graph(g: &ExecGraph) -> CheckSummary {
    let mut errors = vec![];
    // unique ids
    let mut seen = HashSet::new();
    for n in &g.nodes {
        if !seen.insert(&n.id) {
            errors.push(format!("duplicate id: {}", n.id));
        }
    }
    let unique_ids = errors.is_empty();

    // acyclic (Kahn)
    let mut indeg: HashMap<&str, usize> = HashMap::new();
    let mut rev: HashMap<&str, Vec<&str>> = HashMap::new(); // dep -> dependents
    for n in &g.nodes {
        indeg.entry(n.id.as_str()).or_insert(0);
    }
    for n in &g.nodes {
        for d in &n.deps {
            if indeg.get(d.as_str()).is_none() {
                errors.push(format!("unknown dep '{}' referenced by '{}'", d, n.id));
                indeg.insert(d.as_str(), 0);
            }
            *indeg.get_mut(n.id.as_str()).unwrap() += 0; // ensure present
            rev.entry(d.as_str()).or_default().push(n.id.as_str());
        }
    }
    // compute indegree = number of deps each node has
    let mut indegree: HashMap<&str, usize> = g.nodes.iter().map(|n| (n.id.as_str(), n.deps.len())).collect();

    let mut q: Vec<&str> = indegree.iter().filter(|(_, &v)| v == 0).map(|(k, _)| *k).collect();
    let mut processed = 0usize;
    while let Some(u) = q.pop() {
        processed += 1;
        if let Some(dependents) = rev.get(u) {
            for &v in dependents {
                if let Some(x) = indegree.get_mut(v) {
                    if *x > 0 {
                        *x -= 1;
                        if *x == 0 { q.push(v); }
                    }
                }
            }
        }
    }
    let acyclic = processed == indegree.len();
    if !acyclic {
        errors.push("cycle detected (toposort incomplete)".to_string());
    }

    CheckSummary {
        node_count: g.nodes.len(),
        unique_ids,
        acyclic,
        errors,
    }
}

fn impact(g: &ExecGraph, root: &str) -> Vec<String> {
    // downstream = who depends on root: build reverse adjacency dep->dependents
    let mut rev: HashMap<&str, Vec<&str>> = HashMap::new();
    for n in &g.nodes {
        for d in &n.deps {
            rev.entry(d.as_str()).or_default().push(n.id.as_str());
        }
    }
    let mut out = vec![];
    let mut seen = HashSet::new();
    let mut stack = vec![root];
    while let Some(u) = stack.pop() {
        if let Some(dependents) = rev.get(u) {
            for &v in dependents {
                if seen.insert(v) {
                    out.push(v.to_string());
                    stack.push(v);
                }
            }
        }
    }
    out.sort();
    out
}

Acceptance:

# Check the repo graph
cargo nex -- graph check --output json

# Impact of a node (example id from your exec_graph.yaml)
cargo nex -- graph impact F0.1-syscli-issue-local --output json


⸻

C) dev init --with-config (optional config bootstrap)

Add a flag that writes a minimal .nexoscli/config.toml if it doesn’t exist.

plugins/dev/src/lib.rs — augment the command & handler:

// add flag
.arg(Arg::new("with-config").long("with-config").action(ArgAction::SetTrue).help("Create .nexoscli/config.toml with defaults"))

// in execute():
let with_config = m.get_flag("with-config");
// plan:
if with_config {
    let cfg = dot.join("config.toml");
    if !cfg.exists() {
        actions.push(format!("write_file {}", cfg.display()));
    }
}
// apply:
if with_config {
    let cfg = dot.join("config.toml");
    if !cfg.exists() {
        let content = r#"
[output]
default = "json"

[telemetry]
enabled = false
endpoint = ""
"#;
        fs::write(&cfg, content.trim_start())?;
    }
}

Acceptance:

cargo nex -- dev init --yes --with-config --output json
cat .nexoscli/config.toml

(We can wire a typed loader later; the point now is to standardize the location & shape.)

⸻

Run it all

# Build
cargo build --workspace

# Tests (existing + new if you added any)
cargo test --workspace

# Try the envelope (note meta/run_id present)
cargo nex -- --output json issue new --title "envelope"
cargo nex -- --output json issue list

# Graph lints
cargo nex -- graph check --output json
cargo nex -- graph impact F0.1-syscli-issue-local --output json

# Dev init with config
cargo nex -- dev init --yes --with-config --output json
cat .nexoscli/config.toml

# Event log still the ground truth
cat .nexoscli/log/events.jsonl
