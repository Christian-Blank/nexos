mod telemetry;

use clap::{Arg, ArgAction, Command};
use clap_complete::{generate, shells::{Bash, Zsh, Fish}};
use clap_mangen::Man;
use tracing::{info, span, Level};
use std::io;

use nexoscli_core::{
    plugin::{CommandContext, OutputFormat},
    providers::Providers,
    registry::PluginRegistry,
    config::Config,
};

fn main() -> anyhow::Result<()> {
    // 0) Working dir and config
    let cwd = std::env::current_dir()?;
    let config = std::sync::Arc::new(Config::load(&cwd));

    // 1) Register plugins
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(nexoscli_plugin_issue::IssuePlugin::new()));
    registry.register(Box::new(nexoscli_plugin_dev::DevPlugin::new()));
    registry.register(Box::new(nexoscli_plugin_graph::GraphPlugin::new()));
    registry.register(Box::new(nexoscli_plugin_demo::DemoPlugin::new()));
    registry.register(Box::new(nexoscli_plugin_bundle::BundlePlugin::new()));
    registry.register(Box::new(nexoscli_plugin_cline::ClinePlugin::new()));
    registry.register(Box::new(nexoscli_plugin_term::TermPlugin::new()));
    registry.register(Box::new(nexoscli_plugin_rules::RulesPlugin::new()));
    registry.register(Box::new(nexoscli_plugin_replay::ReplayPlugin::new()));
    registry.register(Box::new(nexoscli_plugin_task::TaskPlugin::new()));
    registry.register(Box::new(nexoscli_plugin_workflow::WorkflowPlugin::new()));

    // 2) Build the full app with config-driven default output
    let app = compose_app(&registry, &config);
    let matches = app.clone().get_matches();

    // 3) Globals
    let verbose = matches.get_count("verbose") as u8;
    let output = match matches.get_one::<String>("output").map(|s| s.as_str()) {
        Some("json") => OutputFormat::Json,
        Some("yaml") => OutputFormat::Yaml,
        _ => OutputFormat::Text,
    };

    // 4) Telemetry
    telemetry::init_tracing("nexoscli", env!("CARGO_PKG_VERSION"), verbose);

    // 5) Providers and context
    let providers = Providers::new(cwd).arc();
    let ctx = CommandContext::new(providers.clone(), config.clone(), verbose, output);

    // 6) Root span
    let run_span = span!(
        Level::INFO,
        "run",
        run_id = %providers.run_id,
        cli_output = matches.get_one::<String>("output").map(|s| s.as_str()).unwrap_or("text")
    );
    let _run_g = run_span.enter();

    // 7) Route
    match matches.subcommand() {
        Some(("version", _)) => {
            info!("version");
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        }
        Some(("plugins", _)) => {
            info!("plugins");
            for p in registry.iter() {
                println!("{} v{} - {}", p.name(), p.version(), p.describe());
            }
        }
        Some(("tools", m)) => {
            if let Some(("export", em)) = m.subcommand() {
                let filter = em.get_one::<String>("plugin").map(|s| s.as_str());
                let mut specs = Vec::new();
                for p in registry.iter() {
                    if filter.is_none() || filter == Some(p.name()) {
                        specs.extend(p.tool_specs());
                    }
                }
                let exit = ctx.ok(
                    "core",
                    "tools.export",
                    &serde_json::json!({
                        "tools": specs,
                        "toolset": "nexoscli",
                        "version": env!("CARGO_PKG_VERSION")
                    })
                );
                telemetry::shutdown_tracing();
                std::process::exit(exit);
            }
        }
        Some(("completions", m)) => {
            let shell = m.get_one::<String>("shell").unwrap().as_str();
            let mut cmd = compose_app(&registry, &config);
            match shell {
                "bash" => generate(Bash, &mut cmd, "nexoscli", &mut io::stdout()),
                "zsh"  => generate(Zsh,  &mut cmd, "nexoscli", &mut io::stdout()),
                "fish" => generate(Fish, &mut cmd, "nexoscli", &mut io::stdout()),
                _ => unreachable!(),
            }
        }
        Some(("man", _)) => {
            let man = Man::new(compose_app(&registry, &config));
            man.render(&mut io::stdout())?;
        }
        Some((plugin_name, sub_matches)) => {
            let exec_span = span!(Level::INFO, "execute", plugin = plugin_name);
            let _exec_g = exec_span.enter();
            if let Some(plugin) = registry.get(plugin_name) {
                let code = plugin.execute(sub_matches, &ctx)?;
                info!(exit_code = code, "done");
                telemetry::shutdown_tracing();
                std::process::exit(code);
            } else {
                eprintln!("Unknown plugin: {}", plugin_name);
                telemetry::shutdown_tracing();
                std::process::exit(2);
            }
        }
        None => {
            let mut app = compose_app(&registry, &config);
            app.print_help().unwrap();
            println!();
        }
    }

    telemetry::shutdown_tracing();
    Ok(())
}

fn compose_app(registry: &PluginRegistry, cfg: &Config) -> Command {
    let default_output = match cfg.output.default {
        OutputFormat::Json => "json",
        OutputFormat::Yaml => "yaml",
        OutputFormat::Text => "text",
    };

    let app = Command::new("nexoscli")
        .about("System CLI with plugin architecture")
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Increase verbosity (-v, -vv, -vvv)")
                .action(ArgAction::Count)
                .global(true),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .help("Output format")
                .value_parser(["text", "json", "yaml"])
                .default_value(default_output)
                .global(true),
        )
        .subcommand(Command::new("version").about("Show version"))
        .subcommand(Command::new("plugins").about("List registered plugins"))
        .subcommand(
            Command::new("tools")
                .about("Tools & schemas")
                .subcommand(
                    Command::new("export")
                        .about("Export tools manifest (JSON)")
                        .arg(
                            Arg::new("plugin")
                                .long("plugin")
                                .value_name("NAME")
                                .help("Filter to one plugin"),
                        ),
                ),
        )
        .subcommand(
            Command::new("completions")
                .about("Generate shell completions")
                .arg(Arg::new("shell").value_parser(["bash", "zsh", "fish"]).required(true)),
        )
        .subcommand(Command::new("man").about("Print man page to stdout"));

    // add plugin subcommands
    registry.augment_all(app)
}
