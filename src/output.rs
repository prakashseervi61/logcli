use crate::error::Error;
use crate::model::Entry;
use chrono::format::{Item, StrftimeItems};
use chrono::{DateTime, Local, Utc};
use chrono_tz::Tz;
use serde_json::Value;
use std::fs::File;
use std::io::{self, BufWriter, IsTerminal, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

impl ColorMode {
    pub fn parse(text: &str) -> Result<Self, Error> {
        match text {
            "auto" => Ok(ColorMode::Auto),
            "always" => Ok(ColorMode::Always),
            "never" => Ok(ColorMode::Never),
            other => Err(Error::usage(format!(
                "invalid color mode '{other}' (expected auto, always, or never)"
            ))),
        }
    }

    pub fn enabled(self, tty_output: bool) -> bool {
        match self {
            ColorMode::Never => false,
            ColorMode::Always => true,
            ColorMode::Auto => {
                tty_output
                    && std::env::var_os("NO_COLOR").map_or(true, |value| value.is_empty())
                    && std::env::var("TERM").map_or(true, |term| term != "dumb")
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OutputTz {
    Utc,
    Local,
    Named(Tz),
}

impl OutputTz {
    pub fn parse(zone: Option<&str>, local: bool) -> Result<Self, Error> {
        if local {
            return Ok(OutputTz::Local);
        }

        match zone {
            None | Some("UTC") | Some("utc") => Ok(OutputTz::Utc),
            Some("local") | Some("Local") => Ok(OutputTz::Local),
            Some(name) => name
                .parse::<Tz>()
                .map(OutputTz::Named)
                .map_err(|_| Error::usage(format!("unknown time zone '{name}'"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expansion {
    Never,
    Inline,
    Auto,
    Always,
}

impl Expansion {
    pub fn parse(text: &str) -> Result<Self, Error> {
        match text {
            "never" => Ok(Expansion::Never),
            "inline" => Ok(Expansion::Inline),
            "auto" => Ok(Expansion::Auto),
            "always" => Ok(Expansion::Always),
            other => Err(Error::usage(format!(
                "invalid expansion mode '{other}' (expected never, inline, auto, or always)"
            ))),
        }
    }
}

pub struct FormatOptions {
    pub color: bool,
    pub timezone: OutputTz,
    pub time_format: String,
    pub hide: Vec<String>,
    pub hide_empty: bool,
    pub expansion: Expansion,
    pub raw: bool,
    pub trailing: u8,
}

pub struct Formatter {
    color: bool,
    timezone: OutputTz,
    time_format: String,
    hide: HideRules,
    hide_empty: bool,
    expansion: Expansion,
    raw: bool,
    trailing: u8,
}

impl Formatter {
    pub fn new(options: FormatOptions) -> Result<Self, Error> {
        let items: Vec<Item> = StrftimeItems::new(&options.time_format).collect();
        if items.iter().any(|item| matches!(item, Item::Error)) {
            return Err(Error::usage(format!(
                "invalid time format '{}'",
                options.time_format
            )));
        }

        Ok(Self {
            color: options.color,
            timezone: options.timezone,
            time_format: options.time_format,
            hide: HideRules::new(&options.hide),
            hide_empty: options.hide_empty,
            expansion: options.expansion,
            raw: options.raw,
            trailing: options.trailing,
        })
    }

    pub fn write(&self, entry: &Entry, out: &mut dyn Write) -> Result<(), Error> {
        if self.raw {
            out.write_all(entry.raw.as_bytes())?;
            out.write_all(&[self.trailing])?;
            return Ok(());
        }

        let time = match entry.timestamp.as_ref() {
            Some(timestamp) => self.render_time(timestamp),
            None => "-".to_string(),
        };
        if self.color {
            write!(out, "\x1b[90m{time}\x1b[0m ")?;
        } else {
            write!(out, "{time} ")?;
        }

        match entry.level {
            Some(level) => {
                if self.color {
                    write!(
                        out,
                        "\x1b[{}m{:<5}\x1b[0m ",
                        level.color_code(),
                        level.label()
                    )?;
                } else {
                    write!(out, "{:<5} ", level.label())?;
                }
            }
            None => write!(out, "{:<5} ", "-")?,
        }

        if let Some(logger) = &entry.logger {
            if self.color {
                write!(out, "\x1b[35m{logger}\x1b[0m ")?;
            } else {
                write!(out, "{logger} ")?;
            }
        }

        if let Some(message) = &entry.message {
            write!(out, "{}", render_text(message, self.expansion))?;
        }

        for (key, value) in &entry.fields {
            if entry.is_consumed(key) || self.hide.is_hidden(key) {
                continue;
            }
            if self.hide_empty && is_empty(value) {
                continue;
            }

            write!(out, " ")?;
            if self.color {
                write!(out, "\x1b[2m{key}\x1b[0m=")?;
            } else {
                write!(out, "{key}=")?;
            }
            write!(out, "{}", render_value(value, self.expansion))?;
        }

        out.write_all(&[self.trailing])?;
        Ok(())
    }

    fn render_time(&self, timestamp: &DateTime<Utc>) -> String {
        match &self.timezone {
            OutputTz::Utc => timestamp.format(&self.time_format).to_string(),
            OutputTz::Local => timestamp
                .with_timezone(&Local)
                .format(&self.time_format)
                .to_string(),
            OutputTz::Named(zone) => timestamp
                .with_timezone(zone)
                .format(&self.time_format)
                .to_string(),
        }
    }
}

struct HideRules {
    all: bool,
    hidden: Vec<String>,
    reveals: Vec<String>,
}

impl HideRules {
    fn new(rules: &[String]) -> Self {
        let mut all = false;
        let mut hidden = Vec::new();
        let mut reveals = Vec::new();

        for rule in rules {
            if rule == "*" {
                all = true;
            } else if let Some(key) = rule.strip_prefix('!') {
                reveals.push(key.to_string());
            } else {
                hidden.push(rule.clone());
            }
        }

        Self {
            all,
            hidden,
            reveals,
        }
    }

    fn is_hidden(&self, key: &str) -> bool {
        if self.reveals.iter().any(|reveal| reveal == "*") {
            return false;
        }
        if self.reveals.iter().any(|reveal| reveal == key) {
            return false;
        }
        self.all || self.hidden.iter().any(|hidden| hidden == key)
    }
}

fn render_text(text: &str, expansion: Expansion) -> String {
    match expansion {
        Expansion::Never => text.replace('\n', "\\n").replace('\r', "\\r"),
        Expansion::Inline | Expansion::Auto | Expansion::Always => text.replace('\n', "\n    "),
    }
}

fn render_value(value: &Value, expansion: Expansion) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    };

    render_text(&text, expansion)
}

fn is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(map) => map.is_empty(),
        _ => false,
    }
}

pub fn open_output(path: Option<&Path>) -> Result<Box<dyn Write>, Error> {
    match path {
        Some(path) if path != Path::new("-") => {
            let file = File::create(path)
                .map_err(|err| Error::io(Some(&path.display().to_string()), err))?;
            Ok(Box::new(BufWriter::new(file)))
        }
        _ => Ok(Box::new(BufWriter::new(io::stdout()))),
    }
}

pub fn is_tty_output(path: Option<&Path>) -> bool {
    match path {
        Some(path) if path != Path::new("-") => false,
        _ => io::stdout().is_terminal(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_rules_support_reveal_and_hide_all() {
        let rules = HideRules::new(&["*".to_string(), "!provider".to_string()]);
        assert!(rules.is_hidden("request_id"));
        assert!(!rules.is_hidden("provider"));

        let rules = HideRules::new(&["request_id".to_string()]);
        assert!(rules.is_hidden("request_id"));
        assert!(!rules.is_hidden("provider"));
    }

    #[test]
    fn expansion_never_escapes_newlines() {
        assert_eq!(render_text("a\nb", Expansion::Never), "a\\nb");
        assert_eq!(render_text("a\nb", Expansion::Inline), "a\n    b");
    }
}
