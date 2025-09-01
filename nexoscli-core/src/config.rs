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

impl Default for OutputConfig {
    fn default() -> Self {
        Self { default: default_output_format() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
