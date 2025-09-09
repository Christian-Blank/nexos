use clap::{ArgMatches, Args, Command, FromArgMatches};
use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::Args)]
pub struct GraphCheckArgs {
    #[arg(long = "file", value_name = "PATH", default_value = "docs/exec_graph.yaml")]
    pub file: String,
}

pub fn subcommand() -> Command {
    GraphCheckArgs::augment_args(
        Command::new("check")
            .about("Validate docs/exec_graph.yaml (acyclic, unique ids)"),
    )
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args =
        GraphCheckArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let g = super::load_graph(&args.file)?;
    let summary = super::check_graph(&g);
    let exit = if summary.errors.is_empty() {
        ctx.ok("graph", "check", &summary)
    } else {
        ctx.err(
            "graph",
            "check",
            "graph.invalid",
            "graph has errors",
            Some("fix errors listed in data.errors"),
            2,
        )
    };
    Ok(exit)
}
