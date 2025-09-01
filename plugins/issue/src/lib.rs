use clap::{Arg, ArgAction, ArgMatches, Command};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, ETAG, IF_NONE_MATCH, USER_AGENT};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum IssueState {
    Open,
    Closed,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct Issue {
    id: String,
    title: String,
    body: Option<String>,
    labels: Vec<String>,
    state: IssueState,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct IssueNewArgs {
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    labels: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct IssueListArgs {
    #[serde(default)]
    state: Option<String>, // "open"|"closed"|"all"
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct IssueListResult {
    items: Vec<Issue>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ShowArgs {
    id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct CloseArgs {
    id: String,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct CloseResult {
    id: String,
    state: String,
}

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

fn now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}

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
                .subcommand(
                    Command::new("new")
                        .about("Create a new issue")
                        .arg(
                            Arg::new("title")
                                .long("title")
                                .short('t')
                                .required(true)
                                .value_name("TEXT"),
                        )
                        .arg(Arg::new("body").long("body").required(false).value_name("TEXT"))
                        .arg(
                            Arg::new("labels")
                                .long("labels")
                                .value_delimiter(',')
                                .action(ArgAction::Append),
                        ),
                )
                .subcommand(
                    Command::new("list")
                        .about("List issues")
                        .arg(
                            Arg::new("state")
                                .long("state")
                                .value_parser(["open", "closed", "all"])
                                .default_value("all"),
                        ),
                )
                .subcommand(
                    Command::new("show")
                        .about("Show an issue")
                        .arg(Arg::new("id").required(true)),
                )
                .subcommand(
                    Command::new("close")
                        .about("Close an issue")
                        .arg(Arg::new("id").required(true))
                        .arg(Arg::new("reason").long("reason").required(false).value_name("TEXT")),
                )
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
                ),
        )
    }

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match sub_matches.subcommand() {
            Some(("new", m)) => {
                let title = m.get_one::<String>("title").unwrap().to_string();
                let body = m.get_one::<String>("body").cloned();
                let labels = m
                    .get_many::<String>("labels")
                    .map(|it| it.cloned().collect())
                    .unwrap_or_else(Vec::new);

                let root = std::env::current_dir()?;
                let dir = root.join(".nexoscli").join("issues");
                std::fs::create_dir_all(&dir)?;
                let id = next_id(&dir)?;
                let now_s = now();
                let issue = Issue {
                    id: id.clone(),
                    title,
                    body,
                    labels,
                    state: IssueState::Open,
                    created_at: now_s.clone(),
                    updated_at: now_s,
                };
                let path = dir.join(format!("{id}.json"));
                fs::write(&path, serde_json::to_vec_pretty(&issue)?)?;

                let exit = ctx.ok("issue", "new", &issue);
                Ok(exit)
            }
            Some(("list", m)) => {
                let state = m.get_one::<String>("state").map(|s| s.as_str()).unwrap_or("all");
                let root = std::env::current_dir()?;
                let dir = root.join(".nexoscli").join("issues");
                let mut items: Vec<Issue> = Vec::new();
                if dir.exists() {
                    for entry in fs::read_dir(&dir)? {
                        let entry = entry?;
                        if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                            let bytes = fs::read(entry.path())?;
                            if let Ok(iss) = serde_json::from_slice::<Issue>(&bytes) {
                                items.push(iss);
                            }
                        }
                    }
                }
                let filtered: Vec<&Issue> = match state {
                    "open" => items.iter().filter(|i| matches!(i.state, IssueState::Open)).collect(),
                    "closed" => items.iter().filter(|i| matches!(i.state, IssueState::Closed)).collect(),
                    _ => items.iter().collect(),
                };
                let exit = ctx.ok("issue", "list", &serde_json::json!({ "items": filtered }));
                Ok(exit)
            }
            Some(("show", m)) => {
                let id = m.get_one::<String>("id").unwrap();
                let root = std::env::current_dir()?;
                let path = root.join(".nexoscli").join("issues").join(format!("{id}.json"));
                let bytes = fs::read(&path)?;
                let issue: Issue = serde_json::from_slice(&bytes)?;
                let exit = ctx.ok("issue", "show", &issue);
                Ok(exit)
            }
            Some(("close", m)) => {
                let id = m.get_one::<String>("id").unwrap().to_string();
                let root = std::env::current_dir()?;
                let path = root.join(".nexoscli").join("issues").join(format!("{id}.json"));
                let mut issue: Issue = serde_json::from_slice(&fs::read(&path)?)?;
                issue.state = IssueState::Closed;
                issue.updated_at = now();
                fs::write(&path, serde_json::to_vec_pretty(&issue)?)?;
                let exit = ctx.ok("issue", "close", &serde_json::json!({ "id": issue.id, "state": "closed" }));
                Ok(exit)
            }
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
                            let issue = Issue {
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
            _ => Ok(2),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "issue.new".to_string(),
                about: "Create a new local issue".to_string(),
                args_schema: schema_for!(IssueNewArgs),
                result_schema: schema_for!(Issue),
            },
            ToolSpec {
                name: "issue.list".to_string(),
                about: "List issues (optionally by state)".to_string(),
                args_schema: schema_for!(IssueListArgs),
                result_schema: schema_for!(IssueListResult),
            },
            ToolSpec {
                name: "issue.show".to_string(),
                about: "Show a single issue by ID".to_string(),
                args_schema: schema_for!(ShowArgs),
                result_schema: schema_for!(Issue),
            },
            ToolSpec {
                name: "issue.close".to_string(),
                about: "Close an issue by ID".to_string(),
                args_schema: schema_for!(CloseArgs),
                result_schema: schema_for!(CloseResult),
            },
        ]
    }
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

fn next_id(dir: &PathBuf) -> std::io::Result<String> {
    let counter_path = dir.join(".counter");
    let mut n: u64 = if counter_path.exists() {
        std::fs::read_to_string(&counter_path)?.trim().parse().unwrap_or(0)
    } else {
        0
    };
    n += 1;
    std::fs::write(&counter_path, n.to_string())?;
    Ok(format!("ISS-{n:04}"))
}
