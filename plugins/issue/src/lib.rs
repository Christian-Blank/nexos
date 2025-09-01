use clap::{Arg, ArgAction, ArgMatches, Command};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tracing::{span, Level};

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
                        .about("Import issues (read-only adapter)")
                        .arg(Arg::new("from").long("from").required(true).value_parser(["file"]))
                        .arg(Arg::new("path").long("path").required(true).value_name("FILE"))
                        .arg(Arg::new("id-strategy").long("id-strategy").value_parser(["next","keep"]).default_value("next"))
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

                ctx.events.write("issue", "new", &issue, 0)?;
                ctx.render(&serde_json::json!({ "data": issue, "errors": [], "meta": { "ok": true } }));
                Ok(0)
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
                ctx.render(&serde_json::json!({ "data": { "items": filtered }, "errors": [], "meta": { "count": filtered.len() } }));
                Ok(0)
            }
            Some(("show", m)) => {
                let id = m.get_one::<String>("id").unwrap();
                let root = std::env::current_dir()?;
                let path = root.join(".nexoscli").join("issues").join(format!("{id}.json"));
                let bytes = fs::read(&path)?;
                let issue: Issue = serde_json::from_slice(&bytes)?;
                ctx.render(&serde_json::json!({ "data": issue, "errors": [], "meta": {} }));
                Ok(0)
            }
            Some(("close", m)) => {
                let id = m.get_one::<String>("id").unwrap().to_string();
                let root = std::env::current_dir()?;
                let path = root.join(".nexoscli").join("issues").join(format!("{id}.json"));
                let mut issue: Issue = serde_json::from_slice(&fs::read(&path)?)?;
                issue.state = IssueState::Closed;
                issue.updated_at = now();
                fs::write(&path, serde_json::to_vec_pretty(&issue)?)?;
                ctx.events.write("issue", "close", &serde_json::json!({ "id": id }), 0)?;
                ctx.render(&serde_json::json!({ "data": { "id": issue.id, "state": "closed" }, "errors": [], "meta": {} }));
                Ok(0)
            }
            Some(("import", m)) => {
                let from = m.get_one::<String>("from").unwrap().as_str();
                let path = m.get_one::<String>("path").unwrap();
                let id_strategy = m.get_one::<String>("id-strategy").unwrap().as_str();

                if from != "file" {
                    ctx.render(&serde_json::json!({"data": null, "errors":[{"code":"issue.import.unsupported","message":"only --from file is supported in v0"}], "meta":{}}));
                    return Ok(2);
                }
                
                let run = span!(Level::INFO, "issue.import", from, path, id_strategy);
                let _g = run.enter();
                
                let root = std::env::current_dir()?;
                let dir = root.join(".nexoscli").join("issues");
                std::fs::create_dir_all(&dir)?;

                // Expect a JSON array: [{title, body?, labels?}] (id? if --id-strategy keep)
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

                    let id = if id_strategy == "keep" {
                        v.get("id").and_then(|x| x.as_str()).map(|s| s.to_string()).unwrap_or_else(|| next_id(&dir).unwrap())
                    } else {
                        next_id(&dir)?
                    };

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
                    created.push(id.clone());
                }

                ctx.events.write("issue", "import", &serde_json::json!({"from":"file","path":path,"created":created}), 0)?;
                ctx.render(&serde_json::json!({"data":{"created":created}, "errors":[], "meta":{}}));
                Ok(0)
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
