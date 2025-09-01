use clap::{Arg, ArgAction, Command};
use nexoscli_core::{
    plugin::{CommandContext, OutputFormat},
    providers::Providers,
    registry::PluginRegistry,
};
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    // 1) Register plugins
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(nexoscli_plugin_issue::IssuePlugin::new()));

    // 2) Build the full app (globals + plugin subcommands), then parse once
    let mut app = compose_app(&registry);
    let matches = app.clone().get_matches();

    // 3) Globals
    let verbose = matches.get_count("verbose") as u8;
    let output = match matches.get_one::<String>("output").map(|s| s.as_str()) {
        Some("json") => OutputFormat::Json,
        Some("yaml") => OutputFormat::Yaml,
        _ => OutputFormat::Text,
    };

    // 4) Logging
    let level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(level.parse().unwrap()))
        .with_writer(std::io::stderr)
        .init();

    // 5) Context
    let cwd = std::env::current_dir()?;
    let providers = Providers::new(cwd).arc();
    let ctx = CommandContext::new(providers, verbose, output);

    // 6) Route
    match matches.subcommand() {
        Some(("version", _)) => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        }
        Some(("plugins", _)) => {
            for p in registry.iter() {
                println!("{} v{} - {}", p.name(), p.version(), p.describe());
            }
        }
        Some((plugin_name, sub_matches)) => {
            if let Some(plugin) = registry.get(plugin_name) {
                let code = plugin.execute(sub_matches, &ctx)?;
                std::process::exit(code);
            } else {
                eprintln!("Unknown plugin: {}", plugin_name);
                std::process::exit(2);
            }
        }
        None => {
            app.print_help().unwrap();
            println!();
        }
    }

    Ok(())
}

fn compose_app(registry: &PluginRegistry) -> Command {
    let app = Command::new("nexoscli")
        .about("System CLI with plugin architecture")
        // Global flags (work anywhere in the command line)
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
                .default_value("text")
                .global(true),
        )
        .subcommand(Command::new("version").about("Show version"))
        .subcommand(Command::new("plugins").about("List registered plugins"));

    registry.augment_all(app)
}
