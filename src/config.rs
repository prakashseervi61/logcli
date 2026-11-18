use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Declarative configuration for logcli.
///
/// Loaded from `logcli.yaml`, `logcli.json`, or `logcli.toml` in the current
/// directory (or `--config`). CLI flags override config values.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    /// Default input files (used when none passed on command line).
    pub files: Option<Vec<PathBuf>>,
    /// Default input format: auto, json, logfmt, plain, otel.
    pub input_format: Option<String>,
    /// Default level threshold.
    pub level: Option<String>,
    /// Default time range start.
    pub since: Option<String>,
    /// Default time range end.
    pub until: Option<String>,
    /// Default field filters (ANDed).
    pub filters: Option<Vec<String>>,
    /// Default query expressions (ANDed).
    pub queries: Option<Vec<String>>,
    /// Output format: formatted, raw, json, csv, prometheus.
    pub output_format: Option<String>,
    /// Output file (default stdout).
    pub output: Option<PathBuf>,
    /// Maximum accepted entry size.
    pub max_entry_size: Option<String>,
    /// Time zone for output.
    pub time_zone: Option<String>,
    /// Use local time zone.
    pub local: Option<bool>,
    /// Hide fields.
    pub hide: Option<Vec<String>>,
    /// Hide null/empty fields.
    pub hide_empty: Option<bool>,
    /// Field expansion mode.
    pub expansion: Option<String>,
    /// Sort chronologically.
    pub sort: Option<bool>,
    /// Follow mode (tail new lines).
    pub follow: Option<bool>,
    /// Value sets referenced by `in @name`.
    pub sets: Option<HashMap<String, Vec<String>>>,
    /// PII redaction rules.
    pub redact: Option<Vec<RedactRule>>,
    /// Alert rules evaluated against matched entries.
    pub alerts: Option<Vec<AlertRule>>,
    /// TUI defaults.
    pub tui: Option<TuiConfig>,
}

/// A PII redaction rule. Matches a regex and replaces with a mask.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedactRule {
    /// Field name to redact (e.g. "email", "message", "*" for all string fields).
    pub field: String,
    /// Regex pattern to match within the field value.
    pub pattern: String,
    /// Replacement text (default "[REDACTED]").
    #[serde(default = "default_redact_replacement")]
    pub replace: String,
}

fn default_redact_replacement() -> String {
    "[REDACTED]".to_string()
}

/// An alert rule. Evaluated after filtering; if any entry matches, the process
/// exits with the configured exit code and prints the alert.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlertRule {
    /// Human-readable alert name.
    pub name: String,
    /// Query expression (same syntax as `-q`).
    pub query: String,
    /// Minimum count of matching entries to trigger (default 1).
    #[serde(default = "default_one")]
    pub threshold: u64,
    /// Exit code when triggered (default 2).
    #[serde(default = "default_exit_code")]
    pub exit_code: i32,
}

fn default_one() -> u64 {
    1
}

fn default_exit_code() -> i32 {
    2
}

/// TUI-specific configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TuiConfig {
    /// Default sort mode: none, time, level.
    pub sort: Option<String>,
    /// Default follow mode.
    pub follow: Option<bool>,
    /// Default color theme: default, monokai, solarized.
    pub theme: Option<String>,
}

impl Config {
    /// Discover and load config from the given path, or auto-discover in the
    /// current directory. Returns `Ok(None)` when no config file exists.
    pub fn load(config_path: Option<&Path>) -> Result<Option<Self>, Error> {
        let candidates: Vec<PathBuf> = match config_path {
            Some(p) => vec![p.to_path_buf()],
            None => vec![
                PathBuf::from("logcli.json"),
                PathBuf::from("logcli.yaml"),
                PathBuf::from("logcli.yml"),
            ],
        };

        for path in &candidates {
            if path.exists() {
                let text = std::fs::read_to_string(path)
                    .map_err(|err| Error::io(Some(path.to_string_lossy().as_ref()), err))?;
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let config = match ext {
                    "json" => serde_json::from_str(&text).map_err(|err| {
                        Error::query(format!("invalid config '{}': {err}", path.display()))
                    })?,
                    "yaml" | "yml" => {
                        return Err(Error::query(format!(
                            "YAML config '{}' requires serde_yaml; use JSON config instead",
                            path.display()
                        )))
                    }
                    _ => continue,
                };
                return Ok(Some(config));
            }
        }

        Ok(None)
    }

    /// Compile PII redaction rules into regex matchers.
    pub fn redact_matchers(&self) -> Result<Vec<(String, regex::Regex, String)>, Error> {
        let mut out = Vec::new();
        for rule in self.redact.as_deref().unwrap_or(&[]) {
            let re = regex::Regex::new(&rule.pattern).map_err(|err| {
                Error::query(format!("invalid redact pattern '{}': {err}", rule.pattern))
            })?;
            out.push((rule.field.clone(), re, rule.replace.clone()));
        }
        Ok(out)
    }

    /// Compile alert rules into query expressions.
    pub fn alert_queries(&self) -> Vec<(String, u64, i32, String)> {
        self.alerts
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|a| (a.name.clone(), a.threshold, a.exit_code, a.query.clone()))
            .collect()
    }
}

/// Result of evaluating alert rules against a set of matched entries.
#[derive(Debug, Default)]
pub struct AlertResult {
    pub triggered: Vec<(String, u64)>, // (name, count)
    pub exit_code: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_matchers_compile() {
        let config = Config {
            redact: Some(vec![RedactRule {
                field: "email".to_string(),
                pattern: r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}".to_string(),
                replace: "[EMAIL]".to_string(),
            }]),
            ..Default::default()
        };
        let matchers = config.redact_matchers().unwrap();
        assert_eq!(matchers.len(), 1);
        assert!(matchers[0].1.is_match("alice@example.com"));
        assert!(!matchers[0].1.is_match("not an email"));
    }

    #[test]
    fn alert_queries_compile() {
        let config = Config {
            alerts: Some(vec![
                AlertRule {
                    name: "errors".to_string(),
                    query: "level = error".to_string(),
                    threshold: 5,
                    exit_code: 3,
                },
                AlertRule {
                    name: "fatal".to_string(),
                    query: "level = fatal".to_string(),
                    threshold: 1,
                    exit_code: 2,
                },
            ]),
            ..Default::default()
        };
        let alerts = config.alert_queries();
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].0, "errors");
        assert_eq!(alerts[0].1, 5);
        assert_eq!(alerts[0].2, 3);
    }
}
