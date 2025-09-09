use clap::{ArgMatches, Command};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;

mod cmd_check;
mod cmd_impact;

pub struct GraphPlugin;

impl GraphPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for GraphPlugin {
    fn name(&self) -> &'static str {
        "graph"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn describe(&self) -> &'static str {
        "Execution graph utilities (check, impact)"
    }

    fn augment_commands(&self, app: Command) -> Command {
        app.subcommand(
            Command::new("graph")
                .about("Graph tools")
                .subcommand(cmd_check::subcommand())
                .subcommand(cmd_impact::subcommand()),
        )
    }

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match sub_matches.subcommand() {
            Some(("check", m)) => cmd_check::run(m, ctx),
            Some(("impact", m)) => cmd_impact::run(m, ctx),
            _ => Ok(2),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "graph.check".to_string(),
                about: "Validate docs/exec_graph.yaml (acyclic, unique ids)".to_string(),
                args_schema: schema_for!(cmd_check::GraphCheckArgs),
                result_schema: schema_for!(CheckSummary),
            },
            ToolSpec {
                name: "graph.impact".to_string(),
                about: "List nodes impacted (downstream) by the given id".to_string(),
                args_schema: schema_for!(cmd_impact::GraphImpactArgs),
                result_schema: schema_for!(cmd_impact::GraphImpactResult),
            },
        ]
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExecGraph {
    #[allow(dead_code)]
    id: String,
    nodes: Vec<Node>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Node {
    id: String,
    #[serde(default)]
    #[serde(rename = "type")]
    #[allow(dead_code)]
    kind: Option<String>,
    #[serde(default)]
    deps: Vec<String>, // dependencies this node relies on
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub(crate) struct CheckSummary {
    node_count: usize,
    unique_ids: bool,
    acyclic: bool,
    errors: Vec<String>,
}

pub(crate) fn load_graph(path: &str) -> anyhow::Result<ExecGraph> {
    let bytes = fs::read(PathBuf::from(path))?;
    Ok(serde_yaml::from_slice(&bytes)?)
}

pub(crate) fn check_graph(g: &ExecGraph) -> CheckSummary {
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
    let mut indegree: HashMap<&str, usize> = g
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.deps.len()))
        .collect();

    let mut q: Vec<&str> = indegree
        .iter()
        .filter(|(_, &v)| v == 0)
        .map(|(k, _)| *k)
        .collect();
    let mut processed = 0usize;
    while let Some(u) = q.pop() {
        processed += 1;
        if let Some(dependents) = rev.get(u) {
            for &v in dependents {
                if let Some(x) = indegree.get_mut(v) {
                    if *x > 0 {
                        *x -= 1;
                        if *x == 0 {
                            q.push(v);
                        }
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

pub(crate) fn impact(g: &ExecGraph, root: &str) -> Vec<String> {
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
