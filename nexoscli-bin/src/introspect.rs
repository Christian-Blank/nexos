use clap::{Arg, ArgAction, Command};
use serde_json::{json, Value};

/// Build a deterministic JSON representation of a clap::Command tree and print it to stdout.
/// Returns exit code (0 on success).
pub fn run_clap_json_from_app(mut app: Command) -> anyhow::Result<i32> {
    // Root metadata
    let root_name = app.get_name().to_string();
    let root_about = app.get_about().map(|s| s.to_string());
    let root_version = env!("CARGO_PKG_VERSION").to_string();

    // Collect top-level subcommands (skip hidden)
    let mut subs: Vec<Command> = app
        .get_subcommands()
        .filter(|c| !c.is_hide_set())
        .cloned()
        .collect();
    subs.sort_by(|a, b| a.get_name().cmp(b.get_name()));

    let mut jsubs: Vec<Value> = Vec::new();
    for sc in subs {
        if sc.get_name() == "__introspect" {
            // Never include the introspection command in the output
            continue;
        }
        jsubs.push(walk_command(&sc, &[])?);
    }

    let out = json!({
        "name": root_name,
        "about": root_about,
        "version": root_version,
        "subcommands": jsubs
    });

    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(0)
}

fn walk_command(cmd: &Command, parents: &[String]) -> anyhow::Result<Value> {
    let name = cmd.get_name().to_string();
    let about = cmd.get_about().map(|s| s.to_string());
    let mut path: Vec<String> = parents.to_vec();
    path.push(name.clone());
    let path_str = path.join(" ");

    // Arguments (deterministic sort)
    let mut args_vec: Vec<(String, &Arg)> = cmd
        .get_arguments()
        .map(|a| (arg_key(a), a))
        .collect();
    args_vec.sort_by(|(ka, _), (kb, _)| ka.cmp(kb));

    let mut jargs: Vec<Value> = Vec::new();
    for (_k, a) in args_vec {
        jargs.push(render_arg(a));
    }

    // Subcommands (deterministic sort)
    let mut subs: Vec<Command> = cmd.get_subcommands().filter(|c| !c.is_hide_set()).cloned().collect();
    subs.sort_by(|a, b| a.get_name().cmp(b.get_name()));
    let mut jsubs: Vec<Value> = Vec::new();
    for sc in subs {
        if sc.get_name() == "__introspect" {
            continue;
        }
        jsubs.push(walk_command(&sc, &path)?);
    }

    // Stable, simple usage synthesis (avoid clap internals)
    let usage = format!("nexoscli {} [flags]", path_str);

    Ok(json!({
        "path": path_str,
        "about": about,
        "usage": usage,
        "args": jargs,
        "subcommands": jsubs
    }))
}

fn arg_key(a: &Arg) -> String {
    if let Some(l) = a.get_long() {
        return format!("long:{}", l);
    }
    if let Some(s) = a.get_short() {
        return format!("short:{}", s);
    }
    // Fallback to help or placeholder to ensure stable sort
    if let Some(h) = a.get_help() {
        return format!("help:{}", h);
    }
    "z".repeat(8)
}

fn render_arg(a: &Arg) -> Value {
    let long = a.get_long().map(|s| s.to_string());
    let short = a.get_short().map(|c| c.to_string());
    let kind = if is_flag(a) {
        "flag"
    } else if is_multi(a) {
        "multi"
    } else {
        "value"
    };

    let required = a.is_required_set();

    // Default values
    let vals = a.get_default_values();
    let joined = vals
        .iter()
        .map(|os| os.to_string_lossy().to_string())
        .collect::<Vec<String>>()
        .join(",");
    let default = if joined.is_empty() { None } else { Some(joined) };

    // Possible values (best-effort; clap's ValueParser is type-erased, so we leave it empty unless fixed sets are surfaced)
    let possible_values: Vec<String> = Vec::new();

    let help = a.get_help().map(|s| s.to_string());

    json!({
        "long": long,
        "short": short,
        "kind": kind,
        "required": required,
        "default": default,
        "possible_values": possible_values,
        "help": help
    })
}

fn is_flag(a: &Arg) -> bool {
    matches!(a.get_action(), ArgAction::SetTrue | ArgAction::Count)
}

fn is_multi(a: &Arg) -> bool {
    // Best-effort: treat Append action as multi-value; otherwise assume single value.
    matches!(a.get_action(), ArgAction::Append)
}
