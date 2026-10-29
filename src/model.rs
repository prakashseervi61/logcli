use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
        // Syslog severities (0-7) and common pino/bunyan levels (10-60).
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Logfmt,
    Plain,
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
}

impl Entry {
    pub fn is_consumed(&self, key: &str) -> bool {
        self.consumed.iter().any(|consumed| consumed == key)
    }
}
