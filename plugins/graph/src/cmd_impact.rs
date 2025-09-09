use clap::{ArgMatches, Args, Command, FromArgMatches};
use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::Args)]
pub struct GraphImpactArgs {
    #[arg(value_name = "ID")]
    pub id: String,
    #[arg(long = "file", value_name = "PATH", default_value = "docs/exec_graph.yaml")]
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphImpactResult {
    pub id: String,
    pub impacted: Vec<String>,
}

pub fn subcommand() -> Command {
    GraphImpactArgs::augment_args(
        Command::new("impact").about("List nodes impacted (downstream) by the given id"),
    )
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args =
        GraphImpactArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let g = super::load_graph(&args.file)?;
    let impacted = super::impact(&g, &args.id);
    let exit = ctx.ok(
        "graph",
        "impact",
        &GraphImpactResult {
            id: args.id,
            impacted,
        },
    );
    Ok(exit)
}
