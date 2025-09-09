use clap::{ArgMatches, Args, Command, FromArgMatches};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Args)]
pub struct ImportArgs {
    #[arg(long, value_parser = ["file", "github"])]
    pub from: String,
    #[arg(long)]
    pub path: Option<String>, // required when from=file
    #[arg(long)]
    pub repo: Option<String>, // required when from=github (owner/name)
    #[arg(long, value_parser = ["open", "all"], default_value = "open")]
    pub state: String,
    #[arg(long)]
    pub labels: Option<String>, // CSV
    #[arg(long)]
    pub since: Option<String>, // ISO8601
    #[arg(long, default_value_t = 100)]
    pub limit: u32,
    #[arg(long)]
    pub dry_run: bool,
    // test helper: feed a GitHub JSON fixture to avoid network in CI
    #[arg(long)]
    pub fixture: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImportResult {
    pub from: String,
    pub path: Option<String>,
    pub repo: Option<String>,
    pub dry_run: bool,
    pub created: Vec<String>,
}

pub fn subcommand() -> Command {
    ImportArgs::augment_args(
        Command::new("import").about("Import issues from external sources (read-only)"),
    )
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args = ImportArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let root = std::env::current_dir()?;
    let dir = root.join(".nexoscli").join("issues");
    std::fs::create_dir_all(&dir)?;

    match args.from.as_str() {
        "file" => {
            let Some(path) = args.path.as_ref() else {
                return Ok(ctx.err(
                    "issue",
                    "import",
                    "issue.import.missing_path",
                    "missing --path when --from file",
                    Some("use: issue import --from file --path <json>"),
                    2,
                ));
            };
            let bytes = fs::read(path)?;
            let items: serde_json::Value = serde_json::from_slice(&bytes)?;
            let arr = items
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("expected JSON array"))?;
            let mut created: Vec<String> = Vec::new();
            for v in arr {
                let title = v
                    .get("title")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                if title.is_empty() {
                    continue;
                }
                let body = v
                    .get("body")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string());
                let labels: Vec<String> = if let Some(lv) = v.get("labels") {
                    if let Some(a) = lv.as_array() {
                        a.iter()
                            .filter_map(|x| x.as_str())
                            .map(|s| s.to_string())
                            .collect()
                    } else if let Some(s) = lv.as_str() {
                        s.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };

                let id = super::next_id(&dir)?;
                let now_s = super::now();
                let issue = super::Issue {
                    id: id.clone(),
                    title,
                    body,
                    labels,
                    state: super::IssueState::Open,
                    created_at: now_s.clone(),
                    updated_at: now_s,
                };
                // optional source passthrough
                if let Some(src) = v.get("source") {
                    let mut value = serde_json::to_value(&issue)?;
                    value["source"] = src.clone();
                    fs::write(
                        dir.join(format!("{id}.json")),
                        serde_json::to_vec_pretty(&value)?,
                    )?;
                } else {
                    fs::write(
                        dir.join(format!("{id}.json")),
                        serde_json::to_vec_pretty(&issue)?,
                    )?;
                }
                created.push(id);
            }
            let res = ImportResult {
                from: "file".to_string(),
                path: Some(path.clone()),
                repo: None,
                dry_run: false,
                created,
            };
            Ok(ctx.ok("issue", "import", &res))
        }
        "github" => {
            let Some(repo) = args.repo.as_ref() else {
                return Ok(ctx.err(
                    "issue",
                    "import",
                    "issue.import.missing_repo",
                    "missing --repo when --from github",
                    Some("use: issue import --from github --repo owner/name"),
                    2,
                ));
            };
            let created = super::import_github(
                repo,
                &args.state,
                args.labels.as_deref(),
                args.since.as_deref(),
                args.limit,
                args.dry_run,
                args.fixture.as_deref(),
                Path::new(&dir),
            )?;
            let res = ImportResult {
                from: "github".to_string(),
                path: None,
                repo: Some(repo.clone()),
                dry_run: args.dry_run,
                created,
            };
            Ok(ctx.ok("issue", "import", &res))
        }
        _ => Ok(ctx.err(
            "issue",
            "import",
            "issue.import.unsupported",
            "only --from file|github are supported",
            Some("use: issue import --from file --path <json> OR --from github --repo owner/name"),
            2,
        )),
    }
}
