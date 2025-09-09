use clap::{ArgMatches, Args, Command, FromArgMatches};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::Args)]
pub struct ImportArgs {
    #[arg(long, value_name = "PATH|auto")]
    pub storage: Option<String>, // explicit path or "auto"
    #[arg(long, value_name = "WINDOW")]
    pub since: Option<String>,   // "24h" | "7d" | RFC3339
    #[arg(long, value_name = "N")]
    pub limit: Option<u64>,
    #[arg(long)]
    pub dry_run: bool,
}

pub fn subcommand() -> Command {
    ImportArgs::augment_args(Command::new("import").about("Import tasks (read-only)"))
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args = ImportArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let since_ts = super::parse_since(args.since.as_ref());
    let limit = args.limit.unwrap_or(0);
    let dry = args.dry_run;

    let storage = match args.storage.as_deref() {
        Some(s) if s != "auto" => PathBuf::from(s),
        _ => {
            let (_cands, selected, _hints) = super::locate_all();
            selected.ok_or_else(|| anyhow::anyhow!("no Cline tasks dir found; run `nexoscli cline locate` or pass --storage <path>"))?
        }
    };
    if !storage.exists() {
        return Err(anyhow::anyhow!("storage not found: {}", storage.display()).into());
    }

    // ingest
    let outdir = PathBuf::from(".nexoscli").join("cline").join("tasks");
    let mut summary = super::import_tasks(&storage, since_ts, &outdir, limit, dry)?;
    summary.persisted_dir = Some(outdir.display().to_string());

    let exit = if dry {
        ctx.ok("cline","import.dryrun",&summary)
    } else {
        ctx.ok("cline","import",&summary)
    };
    Ok(exit)
}
