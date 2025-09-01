Absolutely—let’s finish the OTEL-ready nucleus with the last three slices you called out, and park the rest cleanly:
	1.	Span lifecycle helpers (RAII guards: parse, validate, persist, render)
	2.	Typed config loader with env overrides (NEXOS_*) and per‑plugin sections; wired into CommandContext
	3.	Doctor command (nexoscli dev doctor) with “ready” signal + checks

I’m giving you drop‑in files / small edits so Cline can apply fast. After that, you’ll also get a tiny ROADMAP_NEXT.md to pin future steps (e.g., GitHub import).

⸻

0) Workspace dependencies

Root Cargo.toml – add the toml crate (if not already):

[workspace.dependencies]
toml = "0.8"


⸻

1) Core: typed config loader + RAII span helpers + envelope already used

1a) New file — nexoscli-core/src/config.rs

use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, path::{Path, PathBuf}};

use crate::plugin::OutputFormat;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    /// Free-form per-plugin configuration (keyed by plugin name)
    #[serde(default)]
    pub plugins: HashMap<String, toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_output_format")]
    pub default: OutputFormat,
}
fn default_output_format() -> OutputFormat { OutputFormat::Json }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub endpoint: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output: OutputConfig { default: default_output_format() },
            telemetry: TelemetryConfig { enabled: false, endpoint: None },
            plugins: HashMap::new(),
        }
    }
}

impl Config {
    /// Load from .nexoscli/config.toml under `cwd`, then apply NEXOS_* env overrides.
    pub fn load(cwd: &Path) -> Self {
        let path = cwd.join(".nexoscli").join("config.toml");
        let mut cfg = if path.exists() {
            match std::fs::read_to_string(&path).ok().and_then(|s| toml::from_str::<Config>(&s).ok()) {
                Some(c) => c,
                None => Config::default(),
            }
        } else {
            Config::default()
        };
        cfg.apply_env_overrides();
        cfg
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(v) = env::var("NEXOS_OUTPUT_DEFAULT") {
            self.output.default = match v.to_ascii_lowercase().as_str() {
                "json" => OutputFormat::Json,
                "yaml" => OutputFormat::Yaml,
                "text" => OutputFormat::Text,
                _ => self.output.default,
            };
        }
        if let Ok(v) = env::var("NEXOS_TELEMETRY_ENABLED") {
            self.telemetry.enabled = matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "on");
        }
        if let Ok(v) = env::var("NEXOS_TELEMETRY_ENDPOINT") {
            if !v.is_empty() {
                self.telemetry.endpoint = Some(v);
                // enabling endpoint implies enabled unless explicitly disabled
                if env::var("NEXOS_TELEMETRY_ENABLED").is_err() {
                    self.telemetry.enabled = true;
                }
            }
        }
    }

    /// Helper for plugin-specific config block as TOML value.
    pub fn plugin_block(&self, plugin: &str) -> Option<&toml::Value> {
        self.plugins.get(plugin)
    }

    /// Return location of repo config file (for doctor).
    pub fn config_path(cwd: &Path) -> PathBuf {
        cwd.join(".nexoscli").join("config.toml")
    }
}

Notes
	•	Defaults to output.default = json.
	•	Env overrides: NEXOS_OUTPUT_DEFAULT, NEXOS_TELEMETRY_ENABLED, NEXOS_TELEMETRY_ENDPOINT.
	•	plugins is a free‑form map (we can type individual sections later when needed).

1b) Update core mod export — nexoscli-core/src/lib.rs

Add the new module:

pub mod command;   // if present
pub mod error;
pub mod event;
pub mod plugin;
pub mod providers;
pub mod registry;
pub mod envelope;
pub mod config;    // NEW

1c) Update nexoscli-core/Cargo.toml

Add toml to this crate:

[dependencies]
toml = { workspace = true }

(If it already exists, skip.)

1d) RAII span helpers (phase lifecycle) — add to nexoscli-core/src/plugin.rs
	1.	Make OutputFormat serializable & friendly for config:

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat { Text, Json, Yaml }

	2.	Add config into CommandContext and add phase helpers:

use crate::config::Config;
use tracing::{span, Level};
use tracing::span::Entered;

// ...

pub struct CommandContext {
    pub config: std::sync::Arc<Config>,           // NEW
    pub providers: std::sync::Arc<Providers>,
    pub events: EventWriter,
    pub verbosity: u8,
    pub output: OutputFormat,
}

impl CommandContext {
    pub fn new(
        providers: std::sync::Arc<Providers>,
        config: std::sync::Arc<Config>,           // NEW
        verbosity: u8,
        output: OutputFormat,
    ) -> Self {
        let events = EventWriter::new(providers.clone());
        Self { config, providers, events, verbosity, output }
    }

    // ---- Phase lifecycle guards (RAII) ----
    pub fn phase<'a>(&'a self, name: &'static str) -> Entered<'a> {
        let sp = span!(Level::DEBUG, name);
        sp.enter()
    }
    pub fn parse<'a>(&'a self)    -> Entered<'a> { self.phase("parse") }
    pub fn validate<'a>(&'a self) -> Entered<'a> { self.phase("validate") }
    pub fn persist<'a>(&'a self)  -> Entered<'a> { self.phase("persist") }
    pub fn render<'a>(&'a self)   -> Entered<'a> { self.phase("render") }

    // (your existing ok()/err() helpers remain unchanged)
}


⸻

2) Bin: load config early and pass into context; default output from config

2a) nexoscli-bin/src/main.rs — minimal diffs
	•	Imports:

use nexoscli_core::config::Config;            // NEW

	•	Load config before composing the CLI, so we can use it as the default for --output:

fn main() -> anyhow::Result<()> {
    // 0) Working dir and config
    let cwd = std::env::current_dir()?;
    let config = std::sync::Arc::new(Config::load(&cwd));

    // 1) Register plugins
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(nexoscli_plugin_issue::IssuePlugin::new()));
    registry.register(Box::new(nexoscli_plugin_dev::DevPlugin::new()));
    registry.register(Box::new(nexoscli_plugin_graph::GraphPlugin::new()));

    // 2) Build the full app with config-driven default output
    let app = compose_app(&registry, &config);          // CHANGED
    let matches = app.clone().get_matches();

    // 3) Globals
    let verbose = matches.get_count("verbose") as u8;
    let output = match matches.get_one::<String>("output").map(|s| s.as_str()) {
        Some("json") => OutputFormat::Json,
        Some("yaml") => OutputFormat::Yaml,
        _ => OutputFormat::Text,
    };

    // 4) Telemetry, providers, context
    telemetry::init_tracing("nexoscli", env!("CARGO_PKG_VERSION"), verbose);
    let providers = Providers::new(cwd).arc();
    let ctx = CommandContext::new(providers.clone(), config.clone(), verbose, output);   // CHANGED

    // 5) Spans and routing (unchanged) ...

	•	compose_app now receives config to set the default value of --output:

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
                .default_value(default_output)      // CHANGED
                .global(true),
        )
        // ... rest unchanged
        ;

    registry.augment_all(app)
}

	•	completions/man call sites: when you recompose the app, pass &config there too (wherever you call compose_app a second time).

⸻

3) Dev plugin: doctor command + tool spec

Edit plugins/dev/src/lib.rs (keeping your existing init, plugin new, plugins check) and add:

3a) Add types for doctor

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DoctorCheck {
    name: String,
    ok: bool,
    detail: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DoctorResult {
    ready: bool,
    checks: Vec<DoctorCheck>,
}

3b) Wire a new subcommand in augment_commands()

Add another subcommand under Command::new("dev"):

.subcommand(
    Command::new("doctor")
      .about("Verify local setup (dotdir, event log write, config, OTEL envs)")
)

3c) Implement the branch in execute()

Add another match arm:

Some(("doctor", _)) => {
    use nexoscli_core::providers::Clock;
    let cwd = std::env::current_dir()?;
    let dot = cwd.join(".nexoscli");
    let log = dot.join("log").join("events.jsonl");

    let mut checks = Vec::new();

    // dotdir present
    let dot_ok = dot.exists();
    checks.push(DoctorCheck {
        name: "dotdir_exists".into(),
        ok: dot_ok,
        detail: dot.display().to_string(),
    });

    // event log write (append via event writer)
    let event_ok = ctx.events.write(
        "dev",
        "doctor.ping",
        &serde_json::json!({"ping": true, "ts": Clock::now_rfc3339()}),
        0,
    ).is_ok();
    checks.push(DoctorCheck {
        name: "event_log_writable".into(),
        ok: event_ok,
        detail: log.display().to_string(),
    });

    // config load
    let cfg = &ctx.config;
    let cfg_detail = format!(
        "output.default={:?}, telemetry.enabled={}, endpoint={:?}",
        cfg.output.default, cfg.telemetry.enabled, cfg.telemetry.endpoint
    );
    checks.push(DoctorCheck {
        name: "config_loaded".into(),
        ok: true,
        detail: cfg_detail,
    });

    // OTEL env (optional)
    let otel = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    checks.push(DoctorCheck {
        name: "otel_endpoint_env_present".into(),
        ok: otel.is_some(),
        detail: otel.unwrap_or_else(|| "(not set; optional)".into()),
    });

    let ready = dot_ok && event_ok; // config and OTEL are not required for 'ready'
    let exit = ctx.ok("dev", "doctor", &DoctorResult { ready, checks });
    return Ok(exit);
}

3d) Add tool spec

In tool_specs() push:

ToolSpec {
    name: "dev.doctor".to_string(),
    about: "Verify local setup (dotdir, event log write, config, OTEL envs)".to_string(),
    args_schema: schema_for!(struct DoctorArgs{}),       // empty args
    result_schema: schema_for!(DoctorResult),
}

Add the empty DoctorArgs type near the others:

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DoctorArgs {}


⸻

4) (Optional, nice) use phase spans in issue plugin

This is optional but demonstrates the RAII guards. Edit plugins/issue/src/lib.rs inside each branch:

// new
Some(("new", m)) => {
    { let _p = ctx.parse();
      /* read clap args into locals */ }
    { let _p = ctx.persist();
      /* write files */ }
    { let _p = ctx.render();
      let exit = ctx.ok("issue","new",&issue);
      return Ok(exit);
    }
}

Do the same pattern in list/show/close/import (cheap; improves traces immediately).

⸻

5) Build & acceptance

# Build
cargo build --workspace

# Config: try env override (won't change CLI if you pass --output, but shows in doctor)
NEXOS_OUTPUT_DEFAULT=yaml cargo run --manifest-path nexoscli-bin/Cargo.toml -- dev doctor --output json

# Doctor should say "ready": true, and list checks
cargo run --manifest-path nexoscli-bin/Cargo.toml -- dev doctor --output json | jq

# With OTEL (inside your devcontainer or local collector), see nested spans:
RUST_LOG=info cargo run --manifest-path nexoscli-bin/Cargo.toml --features otel -- --output json issue new --title "span demo"
# Open Jaeger UI: service= nexoscli; spans: run -> execute -> parse/persist/render (if you added them)


⸻

6) Pin the rest — add a small roadmap note

Create docs/ROADMAP_NEXT.md (drop‑in):

# NexOS CLI — Pinned Roadmap (next bites)

> We pause here. The items below are queued and will be pulled when evidence appears.

## Ready-to-pull

1) **Read-only GitHub adapter**
   - `issue import --from github --repo org/name [--state open|all]`
   - **Non-goals**: mutation, rate-limit gymnastics. Straight mapping to local issues.
   - **Acceptance**: 1:1 field map; deterministic transforms logged to events; dry-run supported.

2) **Span lifecycle guard (typed)**
   - Current RAII spans are untyped. Introduce `Phase("parse" | "validate" | "persist" | "render")` enum + helper:
     - `ctx.phase(Phase::Parse).entered()`
   - Emit `duration_ms` as span attr (computed by Drop).

3) **Typed per-plugin config**
   - Promote selected `plugins.<name>` TOML blocks to structs as needed (when >2 fields used).
   - Policy: only type a block when two commands read the same fields.

4) **Doctor: CI mode**
   - `dev doctor --ci` returns non-zero on any failing readiness check.
   - Adds check: `plugins check` OK, `graph check` OK.

## Principles (unchanged)
- Ground truth is `events.jsonl`.
- OTEL is an optional mirror; spans follow `run -> execute -> (parse, validate, persist, render)`.
- Default output comes from `.nexoscli/config.toml` or `NEXOS_OUTPUT_DEFAULT`, overridden by CLI flag.


⸻

That’s it

You now have:
	•	Config (typed, env‑overrideable) injected into CommandContext
	•	Phase spans (RAII) for consistent traces
	•	Doctor signal for “is this repo ready?” checks
