use clap::{ArgMatches, Args, Command, FromArgMatches};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::Args)]
pub struct CorrelateArgs {
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long, default_value = "5m")]
    pub pad: String,
    #[arg(long = "run-id")]
    pub run_id: Option<String>,
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub write: bool,
}

pub fn subcommand() -> Command {
    CorrelateArgs::augment_args(Command::new("correlate").about("Correlate Cline tasks to runs"))
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args = CorrelateArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let dot = std::env::current_dir()?.join(".nexoscli");
    let pad = crate::correlation::parse_pad(Some(&args.pad));
    // best-effort ensure AIM streams exist (side-effect tolerated; matches previous behavior)
    let _ = crate::normalize::normalize_since(&dot, None, false, None);

    if let Some(run) = args.run_id.as_ref() {
        let rec = crate::correlation::correlate_run(&dot, run, pad)?;
        // attach message stats
        let msg_stats = super::correlate_messages_for_run(&dot, &rec);
        let mut v = serde_json::to_value(&rec)?;
        if let Some(obj) = v.as_object_mut() {
            obj.insert("msg_stats".into(), msg_stats);
        }
        if args.write {
            crate::correlation::persist_record(&dot, &rec)?;
            // persist sibling messages summary
            let corr_dir = dot.join("cline").join("corr");
            std::fs::create_dir_all(&corr_dir).ok();
            std::fs::write(
                corr_dir.join(format!("{}.messages.json", rec.run_id)),
                serde_json::to_vec_pretty(&v)?,
            )?;
        }
        return Ok(ctx.ok("cline", "correlate.one", &v));
    } else {
        let since_ts = crate::correlation::parse_since(args.since.as_ref());
        let write = args.write;
        let (sum, recs) = crate::correlation::correlate_since(&dot, since_ts, pad, write)?;
        // compute message coverage
        let mut with_msgs: u64 = 0;
        for r in &recs {
            let stats = super::correlate_messages_for_run(&dot, r);
            if stats
                .get("messages_in_window")
                .and_then(|x| x.as_u64())
                .unwrap_or(0)
                > 0
            {
                with_msgs += 1;
            }
        }
        let msg_cov = if recs.is_empty() {
            1.0
        } else {
            with_msgs as f64 / recs.len() as f64
        };
        return Ok(ctx.ok(
            "cline",
            "correlate",
            &serde_json::json!({ "summary": sum, "count": recs.len(), "message_coverage": msg_cov }),
        ));
    }
}
