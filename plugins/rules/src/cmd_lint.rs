use clap::{ArgMatches, Args, Command, FromArgMatches};
use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::Args)]
pub struct RulesLintArgs {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RulesLintResult {
    pub files: u32,
    pub issues: Vec<String>,
    pub ok: bool,
}

pub fn subcommand() -> Command {
    RulesLintArgs::augment_args(
        Command::new("lint").about("Validate rules/*.md front-matter"),
    )
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let _args =
        RulesLintArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let root = std::env::current_dir()?;
    let res = super::lint_rules(&root)?;
    // Map internal LintResult -> public RulesLintResult
    let issues: Vec<String> = res
        .issues
        .iter()
        .map(|i| format!("{}: {}", i.file, i.message))
        .collect();
    let summary = RulesLintResult {
        files: res.files as u32,
        issues,
        ok: res.ok,
    };
    let exit = if summary.ok {
        ctx.ok("rules", "lint", &summary)
    } else {
        // Keep existing behavior: emit an error envelope with exit_code=2
        ctx.err(
            "rules",
            "lint",
            "rules.lint.failed",
            "rules lint issues",
            None,
            2,
        )
    };
    Ok(exit)
}
