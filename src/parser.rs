use crate::model::{Entry, Format, Level};
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::{Map, Value};

const TIMESTAMP_KEYS: &[&str] = &["timestamp", "time", "ts", "@timestamp", "date", "datetime"];
const LEVEL_KEYS: &[&str] = &[
    "level",
    "lvl",
    "severity",
    "severity_text",
    "log.level",
    "@level",
];
const MESSAGE_KEYS: &[&str] = &["message", "msg", "log", "@message"];
const LOGGER_KEYS: &[&str] = &["logger", "source", "service", "@logger"];
const CALLER_KEYS: &[&str] = &["caller", "file", "source.file"];

/// Parse a single raw line into a normalized entry.
///
/// Returns `None` when the line is invalid for the requested format
/// (auto-detection treats malformed JSON as invalid, not as plain text).
pub fn parse_entry(
    raw: &str,
    source: &str,
    line: u64,
    forced_format: &str,
    allow_prefix: bool,
) -> Option<Entry> {
    if raw.trim().is_empty() {
        return None;
    }

    let mut format = Format::Plain;
    let mut fields = Map::new();
    let mut plain_message = None;

    match forced_format {
        "auto" => {
            let looks_like_json = raw.trim_start().starts_with('{');
            if let Some(parsed) = parse_json(raw, allow_prefix) {
                fields = parsed;
                format = Format::Json;
            } else if looks_like_json {
                return None;
            } else if let Some(parsed) = parse_logfmt(raw) {
                fields = parsed;
                format = Format::Logfmt;
            } else {
                plain_message = Some(raw.to_string());
            }
        }
        "json" => {
            fields = parse_json(raw, allow_prefix)?;
            format = Format::Json;
        }
        "logfmt" => match parse_logfmt(raw) {
            Some(parsed) => {
                fields = parsed;
                format = Format::Logfmt;
            }
            None => plain_message = Some(raw.to_string()),
        },
        "plain" => plain_message = Some(raw.to_string()),
        _ => return None,
    }

    Some(normalize(fields, plain_message, raw, source, line, format))
}

fn normalize(
    fields: Map<String, Value>,
    plain_message: Option<String>,
    raw: &str,
    source: &str,
    line: u64,
    format: Format,
) -> Entry {
    let mut entry = Entry {
        source: source.to_string(),
        line,
        timestamp: None,
        level: None,
        level_raw: None,
        message: None,
        logger: None,
        caller: None,
        fields,
        consumed: Vec::new(),
        raw: raw.to_string(),
        format,
    };

    for key in TIMESTAMP_KEYS {
        let parsed = lookup(&entry.fields, key).and_then(value_timestamp);
        if let Some(timestamp) = parsed {
            entry.timestamp = Some(timestamp);
            entry.consumed.push((*key).to_string());
            break;
        }
    }

    if let Some((key, value)) = find_key(&entry.fields, LEVEL_KEYS) {
        entry.level = level_value(value);
        entry.level_raw = value_text_opt(value);
        entry.consumed.push(key.to_string());
    }

    if let Some((key, value)) = find_key(&entry.fields, MESSAGE_KEYS) {
        entry.message = message_value(value);
        entry.consumed.push(key.to_string());
    }

    if let Some((key, Value::String(text))) = find_key(&entry.fields, LOGGER_KEYS) {
        entry.logger = Some(text.clone());
        entry.consumed.push(key.to_string());
    }

    if let Some((key, Value::String(text))) = find_key(&entry.fields, CALLER_KEYS) {
        entry.caller = Some(text.clone());
        entry.consumed.push(key.to_string());
    }

    if entry.message.is_none() {
        entry.message = plain_message.or_else(|| {
            if entry.fields.is_empty() {
                Some(raw.to_string())
            } else {
                None
            }
        });
    }

    entry
}

fn find_key<'a>(
    fields: &'a Map<String, Value>,
    keys: &[&'static str],
) -> Option<(&'static str, &'a Value)> {
    for key in keys {
        if let Some(value) = lookup(fields, key) {
            return Some((key, value));
        }
    }
    None
}

fn lookup<'a>(fields: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    if let Some(value) = fields.get(key) {
        return Some(value);
    }

    let mut parts = key.split('.');
    let first = parts.next()?;
    let mut current = fields.get(first)?;

    for part in parts {
        current = current.as_object()?.get(part)?;
    }

    Some(current)
}

fn value_timestamp(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::String(text) => parse_timestamp(text),
        Value::Number(number) => number.as_i64().and_then(unix_timestamp),
        _ => None,
    }
}

fn parse_timestamp(text: &str) -> Option<DateTime<Utc>> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Some(parsed.with_timezone(&Utc));
    }

    const DATETIME_FORMATS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ];

    for format in DATETIME_FORMATS {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(text, format) {
            return Some(Utc.from_utc_datetime(&parsed));
        }
    }

    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        let midnight = date.and_hms_opt(0, 0, 0)?;
        return Some(Utc.from_utc_datetime(&midnight));
    }

    if (10..=19).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit()) {
        if let Ok(number) = text.parse::<i64>() {
            return unix_timestamp(number);
        }
    }

    None
}

fn unix_timestamp(number: i64) -> Option<DateTime<Utc>> {
    if number < 0 {
        return None;
    }

    let magnitude = number.unsigned_abs();
    let (seconds, nanos) = if magnitude < 100_000_000_000 {
        (number, 0)
    } else if magnitude < 100_000_000_000_000 {
        (number / 1_000, ((number % 1_000) * 1_000_000) as u32)
    } else if magnitude < 100_000_000_000_000_000 {
        (number / 1_000_000, ((number % 1_000_000) * 1_000) as u32)
    } else {
        (number / 1_000_000_000, (number % 1_000_000_000) as u32)
    };

    Utc.timestamp_opt(seconds, nanos).single()
}

fn level_value(value: &Value) -> Option<Level> {
    match value {
        Value::String(text) => Level::parse(text),
        Value::Number(number) => number.as_i64().and_then(Level::from_number),
        _ => None,
    }
}

fn value_text_opt(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn message_value(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        other => serde_json::to_string(other).ok(),
    }
}

fn parse_json(raw: &str, allow_prefix: bool) -> Option<Map<String, Value>> {
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(raw) {
        return Some(map);
    }

    if !allow_prefix {
        return None;
    }

    for (index, _) in raw.match_indices('{') {
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&raw[index..]) {
            return Some(map);
        }
    }

    None
}

fn parse_logfmt(raw: &str) -> Option<Map<String, Value>> {
    let bytes = raw.as_bytes();
    let mut index = 0usize;
    let mut map = Map::new();

    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let key_start = index;
        while index < bytes.len() && bytes[index] != b'=' && !bytes[index].is_ascii_whitespace() {
            index += 1;
        }

        if index >= bytes.len() || bytes[index] != b'=' {
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            continue;
        }

        let key = &raw[key_start..index];
        index += 1;

        let value = if index < bytes.len() && bytes[index] == b'"' {
            index += 1;
            let mut collected = Vec::new();

            while index < bytes.len() {
                match bytes[index] {
                    b'\\' if index + 1 < bytes.len() => {
                        let escaped = bytes[index + 1];
                        match escaped {
                            b'"' => collected.push(b'"'),
                            b'\\' => collected.push(b'\\'),
                            b'n' => collected.push(b'\n'),
                            b't' => collected.push(b'\t'),
                            b'r' => collected.push(b'\r'),
                            other => {
                                collected.push(b'\\');
                                collected.push(other);
                            }
                        }
                        index += 2;
                    }
                    b'"' => {
                        index += 1;
                        break;
                    }
                    other => {
                        collected.push(other);
                        index += 1;
                    }
                }
            }

            String::from_utf8_lossy(&collected).into_owned()
        } else {
            let value_start = index;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            raw[value_start..index].to_string()
        };

        if !key.is_empty() {
            map.insert(key.to_string(), Value::String(value));
        }
    }

    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logfmt_handles_quotes_and_escapes() {
        let map = parse_logfmt(r#"level=info msg="hello \"world\"" path=/a/b"#).unwrap();
        assert_eq!(map["msg"], Value::String("hello \"world\"".to_string()));
        assert_eq!(map["path"], Value::String("/a/b".to_string()));
    }

    #[test]
    fn logfmt_ignores_stray_tokens() {
        let map = parse_logfmt("hello level=warn msg=test").unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map["level"], Value::String("warn".to_string()));
    }

    #[test]
    fn rfc3339_timestamp_parses() {
        let timestamp = parse_timestamp("2026-10-28T10:00:00Z").unwrap();
        assert_eq!(timestamp.to_rfc3339(), "2026-10-28T10:00:00+00:00");
    }

    #[test]
    fn unix_millis_timestamp_parses() {
        let timestamp = parse_timestamp("1793181602000").unwrap();
        assert_eq!(timestamp.timestamp(), 1_793_181_602);
    }

    #[test]
    fn numeric_levels_parse() {
        assert_eq!(Level::parse("40"), Some(Level::Warn));
        assert_eq!(Level::from_number(50), Some(Level::Error));
        assert_eq!(Level::parse("nope"), None);
    }

    #[test]
    fn invalid_json_is_rejected_in_auto_mode() {
        assert!(parse_entry("{\"broken\":", "test", 1, "auto", false).is_none());
    }

    #[test]
    fn plain_text_falls_back() {
        let entry = parse_entry("just a plain line", "test", 1, "auto", false).unwrap();
        assert_eq!(entry.message.as_deref(), Some("just a plain line"));
        assert_eq!(entry.format, Format::Plain);
    }
}
