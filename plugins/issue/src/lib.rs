use clap::{ArgMatches, Command};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, ETAG, IF_NONE_MATCH, USER_AGENT};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;

mod cmd_close;
mod cmd_import;
mod cmd_list;
mod cmd_new;
mod cmd_show;

pub struct IssuePlugin;
impl IssuePlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for IssuePlugin {
    fn name(&self) -> &'static str {
        "issue"
    }
    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    fn describe(&self) -> &'static str {
        "Local issues (new|list|show|close) with JSON outputs"
    }

    fn augment_commands(&self, app: Command) -> Command {
        app.subcommand(
            Command::new("issue")
                .about("Issue management")
                .subcommand(cmd_new::subcommand())
                .subcommand(cmd_list::subcommand())
                .subcommand(cmd_show::subcommand())
                .subcommand(cmd_close::subcommand())
                .subcommand(cmd_import::subcommand()),
        )
    }

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match sub_matches.subcommand() {
            Some(("new", m)) => cmd_new::run(m, ctx),
            Some(("list", m)) => cmd_list::run(m, ctx),
            Some(("show", m)) => cmd_show::run(m, ctx),
            Some(("close", m)) => cmd_close::run(m, ctx),
            Some(("import", m)) => cmd_import::run(m, ctx),
            _ => Ok(2),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "issue.new".to_string(),
                about: "Create a new local issue".to_string(),
                args_schema: schema_for!(cmd_new::IssueNewArgs),
                result_schema: schema_for!(Issue),
            },
            ToolSpec {
                name: "issue.list".to_string(),
                about: "List issues (optionally by state)".to_string(),
                args_schema: schema_for!(cmd_list::IssueListArgs),
                result_schema: schema_for!(cmd_list::IssueListResult),
            },
            ToolSpec {
                name: "issue.show".to_string(),
                about: "Show a single issue by ID".to_string(),
                args_schema: schema_for!(cmd_show::ShowArgs),
                result_schema: schema_for!(Issue),
            },
            ToolSpec {
                name: "issue.close".to_string(),
                about: "Close an issue by ID".to_string(),
                args_schema: schema_for!(cmd_close::CloseArgs),
                result_schema: schema_for!(cmd_close::CloseResult),
            },
            // Intentionally omit ToolSpec for issue.import (not agent-exposed)
        ]
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum IssueState {
    Open,
    Closed,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub(crate) struct Issue {
    pub id: String,
    pub title: String,
    pub body: Option<String>,
    pub labels: Vec<String>,
    pub state: IssueState,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(serde::Deserialize)]
struct GhLabel {
    name: String,
}
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

pub(crate) fn now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}

pub(crate) fn import_github(
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
        if let Some(lbl) = labels_csv {
            if !lbl.is_empty() {
                q.push(("labels".into(), lbl.into()));
            }
        }
        if let Some(s) = since {
            q.push(("since".into(), s.into()));
        }

        let token = std::env::var("GITHUB_TOKEN").ok();
        let client = Client::builder().build()?;
        let mut req = client
            .get(&url)
            .header(USER_AGENT, "nexoscli/0.1")
            .header(ACCEPT, "application/vnd.github+json")
            .query(&q);

        if let Some(t) = token {
            req = req.header(AUTHORIZATION, format!("Bearer {t}"));
        }

        // Optional ETag cache
        let cache_dir = out_dir
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(".import")
            .join("github");
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
    let (owner, name) = repo
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("repo must be owner/name"))?;
    let mut created = vec![];
    for gi in gh_issues.into_iter().take(limit as usize) {
        let id = format!("ISS-GH_{}_{}_{}", owner, name, gi.number);
        if dry_run {
            created.push(id);
            continue;
        }
        let labels: Vec<String> = gi.labels.into_iter().map(|l| l.name).collect();
        let obj = serde_json::json!({
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

pub(crate) fn next_id(dir: &PathBuf) -> std::io::Result<String> {
    let counter_path = dir.join(".counter");
    let mut n: u64 = if counter_path.exists() {
        std::fs::read_to_string(&counter_path)?
            .trim()
            .parse()
            .unwrap_or(0)
    } else {
        0
    };
    n += 1;
    std::fs::write(&counter_path, n.to_string())?;
    Ok(format!("ISS-{n:04}"))
}
