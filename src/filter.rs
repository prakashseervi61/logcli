use crate::error::Error;
use crate::model::{Entry, Level};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::{Map, Value};

pub struct Filters {
    min_level: Option<Level>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    fields: Vec<FieldFilter>,
}

impl Filters {
    pub fn new(
        min_level: Option<Level>,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
        raw_filters: &[String],
    ) -> Result<Self, Error> {
        let fields = raw_filters
            .iter()
            .map(|raw| FieldFilter::parse(raw))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            min_level,
            since,
            until,
            fields,
        })
    }

    pub fn matches(&self, entry: &Entry) -> bool {
        if let Some(min_level) = self.min_level {
            match entry.level {
                Some(level) if level >= min_level => {}
                _ => return false,
            }
        }

        if let Some(since) = &self.since {
            match &entry.timestamp {
                Some(timestamp) if timestamp >= since => {}
                _ => return false,
            }
        }

        if let Some(until) = &self.until {
            match &entry.timestamp {
                Some(timestamp) if timestamp <= until => {}
                _ => return false,
            }
        }

        self.fields.iter().all(|filter| filter.matches(entry))
    }
}

struct FieldFilter {
    path: String,
    op: Op,
    value: String,
    include_absent: bool,
    regex: Option<Regex>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Contains,
    NotContains,
    Regex,
    NotRegex,
}

const OPERATORS: &[(&str, Op)] = &[
    ("!~~=", Op::NotRegex),
    ("~~=", Op::Regex),
    ("!~=", Op::NotContains),
    ("~=", Op::Contains),
    ("!=", Op::Ne),
    ("=", Op::Eq),
];

impl FieldFilter {
    fn parse(raw: &str) -> Result<Self, Error> {
        let mut best: Option<(usize, &'static str, Op)> = None;

        for &(symbol, op) in OPERATORS {
            if let Some(index) = raw.find(symbol) {
                let replace = match best {
                    None => true,
                    Some((best_index, best_symbol, _)) => {
                        index < best_index
                            || (index == best_index && symbol.len() > best_symbol.len())
                    }
                };
                if replace {
                    best = Some((index, symbol, op));
                }
            }
        }

        let (index, symbol, op) = best.ok_or_else(|| {
            Error::usage(format!(
                "invalid filter '{raw}': expected key=value, key!=value, key~=text, or key~~=regex"
            ))
        })?;

        let mut path = raw[..index].trim();
        if path.is_empty() {
            return Err(Error::usage(format!(
                "invalid filter '{raw}': empty field name"
            )));
        }

        let include_absent = path.ends_with('?');
        if include_absent {
            path = &path[..path.len() - 1];
        }

        let path = path.strip_prefix('.').unwrap_or(path);
        let path = unquote(path);

        let value = unquote(raw[index + symbol.len()..].trim());

        let regex = match op {
            Op::Regex | Op::NotRegex => Some(
                Regex::new(&value)
                    .map_err(|err| Error::query(format!("invalid regex '{value}': {err}")))?,
            ),
            _ => None,
        };

        Ok(Self {
            path: path.to_string(),
            op,
            value,
            include_absent,
            regex,
        })
    }

    fn matches(&self, entry: &Entry) -> bool {
        let values = resolve(&entry.fields, &self.path);

        if values.is_empty() {
            return self.include_absent;
        }

        match self.op {
            Op::Eq => values.iter().any(|value| self.equals(value)),
            Op::Ne => values.iter().all(|value| !self.equals(value)),
            Op::Contains => values
                .iter()
                .filter_map(|value| scalar_text(value))
                .any(|text| text.contains(&self.value)),
            Op::NotContains => values
                .iter()
                .filter_map(|value| scalar_text(value))
                .all(|text| !text.contains(&self.value)),
            Op::Regex => {
                let regex = self.regex.as_ref().expect("regex compiled at parse time");
                values
                    .iter()
                    .filter_map(|value| scalar_text(value))
                    .any(|text| regex.is_match(&text))
            }
            Op::NotRegex => {
                let regex = self.regex.as_ref().expect("regex compiled at parse time");
                values
                    .iter()
                    .filter_map(|value| scalar_text(value))
                    .all(|text| !regex.is_match(&text))
            }
        }
    }

    fn equals(&self, value: &Value) -> bool {
        if let (Some(left), Ok(right)) = (as_number(value), self.value.parse::<f64>()) {
            return left == right;
        }

        match value {
            Value::Bool(flag) => self.value == flag.to_string(),
            Value::Null => self.value.eq_ignore_ascii_case("null"),
            Value::String(text) => text == &self.value,
            Value::Number(_) => false,
            other => serde_json::to_string(other)
                .map(|text| text == self.value)
                .unwrap_or(false),
        }
    }
}

/// Resolve a dotted path, supporting arrays via `[]` (any element) and `[n]` (index).
fn resolve<'a>(fields: &'a Map<String, Value>, path: &str) -> Vec<&'a Value> {
    if let Some(value) = fields.get(path) {
        return vec![value];
    }

    let mut segments = path.split('.');
    let first = match segments.next() {
        Some(first) => first,
        None => return Vec::new(),
    };

    let rest: Vec<&str> = segments.collect();
    let mut results = Vec::new();

    if let Some(value) = fields.get(first) {
        walk(value, &rest, &mut results);
    }

    results
}

fn walk<'a>(value: &'a Value, segments: &[&str], results: &mut Vec<&'a Value>) {
    if segments.is_empty() {
        results.push(value);
        return;
    }

    let segment = segments[0];
    let rest = &segments[1..];

    match value {
        Value::Object(map) => {
            if let Some(next) = map.get(segment) {
                walk(next, rest, results);
            }
        }
        Value::Array(items) => {
            if segment == "[]" {
                for item in items {
                    walk(item, rest, results);
                }
            } else if let Some(index) = parse_index(segment) {
                if let Some(next) = items.get(index) {
                    walk(next, rest, results);
                }
            }
        }
        _ => {}
    }
}

fn parse_index(segment: &str) -> Option<usize> {
    let inner = segment.strip_prefix('[')?.strip_suffix(']')?;
    if inner.is_empty() {
        return None;
    }
    inner.parse().ok()
}

fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse::<f64>().ok().filter(|number| number.is_finite()),
        _ => None,
    }
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Null => Some("null".to_string()),
        _ => None,
    }
}

fn unquote(text: &str) -> String {
    let text = text.trim();
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        serde_json::from_str::<String>(text).unwrap_or_else(|_| text[1..text.len() - 1].to_string())
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Format;
    use serde_json::json;

    fn entry() -> Entry {
        let mut fields = Map::new();
        fields.insert("status".to_string(), json!(500));
        fields.insert("message".to_string(), json!("connection refused"));
        fields.insert(
            "request".to_string(),
            json!({"method": "GET", "path": "/users"}),
        );
        fields.insert(
            "span".to_string(),
            json!([{"name": "sp0001"}, {"name": "sp0002"}]),
        );

        Entry {
            source: "test".to_string(),
            line: 1,
            timestamp: None,
            level: Some(Level::Error),
            level_raw: Some("error".to_string()),
            message: Some("connection refused".to_string()),
            logger: None,
            caller: None,
            fields,
            consumed: Vec::new(),
            raw: String::new(),
            format: Format::Json,
        }
    }

    fn matches(filter: &str) -> bool {
        let filters = Filters::new(None, None, None, &[filter.to_string()]).unwrap();
        filters.matches(&entry())
    }

    #[test]
    fn equality_matches_numbers_as_strings() {
        assert!(matches("status=500"));
        assert!(matches("status=\"500\""));
        assert!(!matches("status=404"));
    }

    #[test]
    fn nested_paths_resolve() {
        assert!(matches("request.method=GET"));
        assert!(!matches("request.method=POST"));
    }

    #[test]
    fn array_paths_are_existential() {
        assert!(matches("span.[].name=sp0002"));
        assert!(matches("span.[0].name=sp0001"));
        assert!(!matches("span.[1].name=sp0001"));
    }

    #[test]
    fn absent_modifier_includes_missing_fields() {
        assert!(!matches("missing=value"));
        assert!(matches("missing?=value"));
        assert!(matches("missing?!=value"));
        assert!(!matches("missing!=value"));
    }

    #[test]
    fn substring_and_regex() {
        assert!(matches("message~=refused"));
        assert!(!matches("message~=timeout"));
        assert!(matches("message~~=^connection"));
        assert!(!matches("message!~~=^connection"));
    }

    #[test]
    fn invalid_filter_is_an_error() {
        assert!(Filters::new(None, None, None, &["nope".to_string()]).is_err());
        assert!(Filters::new(None, None, None, &["message~~=(unclosed".to_string()]).is_err());
    }
}
