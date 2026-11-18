use crate::error::Error;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
    Panic,
}

impl Level {
    pub fn parse(text: &str) -> Option<Level> {
        let text = text.trim();
        let lower = text.to_ascii_lowercase();

        let level = match lower.as_str() {
            "trace" | "trc" | "t" => Level::Trace,
            "debug" | "dbg" | "d" => Level::Debug,
            "info" | "inf" | "i" | "informational" | "notice" => Level::Info,
            "warn" | "warning" | "wrn" | "w" => Level::Warn,
            "error" | "err" | "e" => Level::Error,
            "fatal" | "ftl" | "crit" | "critical" | "alert" | "emerg" | "emergency" => Level::Fatal,
            "panic" => Level::Panic,
            _ => {
                if let Ok(number) = text.parse::<i64>() {
                    return Level::from_number(number);
                }
                return None;
            }
        };

        Some(level)
    }

    pub fn from_number(number: i64) -> Option<Level> {
        let level = match number {
            0..=2 => Level::Fatal,
            3 => Level::Error,
            4 => Level::Warn,
            5..=6 => Level::Info,
            7 => Level::Debug,
            10 => Level::Trace,
            20 => Level::Debug,
            30 => Level::Info,
            40 => Level::Warn,
            50 => Level::Error,
            60..=70 => Level::Fatal,
            _ => return None,
        };

        Some(level)
    }

    pub fn label(self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
            Level::Fatal => "FATAL",
            Level::Panic => "PANIC",
        }
    }

    pub fn color_code(self) -> &'static str {
        match self {
            Level::Trace => "90",
            Level::Debug => "36",
            Level::Info => "32",
            Level::Warn => "33",
            Level::Error => "31",
            Level::Fatal | Level::Panic => "1;31",
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Format {
    Json,
    Logfmt,
    Plain,
    Otel,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Format::Json => f.write_str("json"),
            Format::Logfmt => f.write_str("logfmt"),
            Format::Plain => f.write_str("plain"),
            Format::Otel => f.write_str("otel"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ExportFormat {
    Formatted,
    Raw,
    Json,
    Csv,
    Prometheus,
}

impl ExportFormat {
    pub fn parse(text: &str) -> Result<Self, Error> {
        match text {
            "formatted" => Ok(ExportFormat::Formatted),
            "raw" => Ok(ExportFormat::Raw),
            "json" => Ok(ExportFormat::Json),
            "csv" => Ok(ExportFormat::Csv),
            "prometheus" => Ok(ExportFormat::Prometheus),
            other => Err(Error::query(format!(
                "invalid output format '{other}' (expected formatted, raw, json, csv, or prometheus)"
            ))),
        }
    }
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportFormat::Formatted => f.write_str("formatted"),
            ExportFormat::Raw => f.write_str("raw"),
            ExportFormat::Json => f.write_str("json"),
            ExportFormat::Csv => f.write_str("csv"),
            ExportFormat::Prometheus => f.write_str("prometheus"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RedactRules {
    pub rules: Vec<(String, Regex, String)>,
}

impl RedactRules {
    pub fn new(rules: Vec<(String, Regex, String)>) -> Self {
        Self { rules }
    }

    pub fn apply(&self, entry: &mut Entry) {
        if self.rules.is_empty() {
            return;
        }

        if let Some(mut text) = entry.message.take() {
            for (_, re, replace) in &self.rules {
                text = re.replace_all(&text, replace).into_owned();
            }
            entry.message = Some(text);
        }
        if let Some(mut text) = entry.logger.take() {
            for (_, re, replace) in &self.rules {
                text = re.replace_all(&text, replace).into_owned();
            }
            entry.logger = Some(text);
        }
        if let Some(mut text) = entry.caller.take() {
            for (_, re, replace) in &self.rules {
                text = re.replace_all(&text, replace).into_owned();
            }
            entry.caller = Some(text);
        }

        for (field_name, re, replace) in &self.rules {
            if field_name == "*" {
                for value in entry.fields.values_mut() {
                    if let Value::String(mut text) = value.clone() {
                        text = re.replace_all(&text, replace).into_owned();
                        *value = Value::String(text);
                    }
                }
            } else if let Some(value) = entry.fields.get_mut(field_name) {
                if let Value::String(mut text) = value.clone() {
                    text = re.replace_all(&text, replace).into_owned();
                    *value = Value::String(text);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Entry {
    pub source: String,
    pub line: u64,
    pub timestamp: Option<DateTime<Utc>>,
    pub level: Option<Level>,
    pub level_raw: Option<String>,
    pub message: Option<String>,
    pub logger: Option<String>,
    pub caller: Option<String>,
    pub fields: Map<String, Value>,
    pub consumed: Vec<String>,
    pub raw: String,
    pub format: Format,
    pub redacted: bool,
}

impl Serialize for Entry {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.fields.len() + 8))?;
        map.serialize_entry("source", &self.source)?;
        map.serialize_entry("line", &self.line)?;
        map.serialize_entry(
            "timestamp",
            &self.timestamp.map(|t| t.to_rfc3339()).unwrap_or_default(),
        )?;
        map.serialize_entry("level", &self.level)?;
        map.serialize_entry("level_raw", &self.level_raw)?;
        map.serialize_entry("message", &self.message)?;
        map.serialize_entry("logger", &self.logger)?;
        map.serialize_entry("caller", &self.caller)?;
        map.serialize_entry("fields", &self.fields)?;
        map.serialize_entry("format", &self.format)?;
        map.serialize_entry("redacted", &self.redacted)?;
        map.end()
    }
}

impl Entry {
    pub fn is_consumed(&self, key: &str) -> bool {
        self.consumed.iter().any(|consumed| consumed == key)
    }

    pub fn redact(&mut self, rules: &RedactRules) {
        if self.redacted {
            return;
        }
        rules.apply(self);
        self.redacted = true;
    }

    pub fn flatten(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        if let Some(ts) = &self.timestamp {
            pairs.push(("timestamp".to_string(), ts.to_rfc3339()));
        }
        if let Some(level) = &self.level {
            pairs.push(("level".to_string(), level.label().to_string()));
        }
        if let Some(msg) = &self.message {
            pairs.push(("message".to_string(), msg.clone()));
        }
        if let Some(logger) = &self.logger {
            pairs.push(("logger".to_string(), logger.clone()));
        }
        Self::flatten_value(&Value::Object(self.fields.clone()), "", &mut pairs);
        pairs
    }

    fn flatten_value(value: &Value, prefix: &str, pairs: &mut Vec<(String, String)>) {
        match value {
            Value::Object(map) => {
                for (k, v) in map {
                    let key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    Self::flatten_value(v, &key, pairs);
                }
            }
            Value::Array(arr) => {
                for (i, v) in arr.iter().enumerate() {
                    let key = format!("{prefix}[{i}]");
                    Self::flatten_value(v, &key, pairs);
                }
            }
            Value::String(s) => pairs.push((prefix.to_string(), s.clone())),
            Value::Number(n) => pairs.push((prefix.to_string(), n.to_string())),
            Value::Bool(b) => pairs.push((prefix.to_string(), b.to_string())),
            Value::Null => pairs.push((prefix.to_string(), "null".to_string())),
        }
    }

    pub fn prometheus_lines(&self, metric_name: &str) -> Vec<String> {
        let mut lines = Vec::new();
        let labels: Vec<String> = self
            .fields
            .iter()
            .filter(|(k, _)| {
                matches!(
                    k.as_str(),
                    "service" | "namespace" | "environment" | "component"
                )
            })
            .map(|(k, v)| format!("{k}=\"{v}\""))
            .collect();
        let label_str = if labels.is_empty() {
            String::new()
        } else {
            format!("{{{}}}", labels.join(","))
        };

        let ms = self
            .timestamp
            .map_or(0, |t| t.timestamp_nanos_opt().unwrap_or(0) / 1_000_000);
        lines.push(format!("{metric_name}_total{label_str} 1 {ms}"));
        lines
    }
}
