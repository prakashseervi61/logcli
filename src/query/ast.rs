use crate::model::{Entry, Level};
use regex::Regex;
use serde_json::Value;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Level,
    Message,
    Logger,
    Caller,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Name(String),
    AnyIndex,
    Index(usize),
}

/// A parsed field path.
///
/// `raw` is used for the exact-key-first rule: if the source object contains a
/// literal key equal to the whole path, it wins over dotted traversal.
#[derive(Debug, Clone)]
pub struct Path {
    pub raw: String,
    pub segments: Vec<Segment>,
    pub role: Option<Role>,
    pub exact_only: bool,
    pub absent: bool,
}

impl Path {
    /// Parse an unquoted or quoted path. A trailing `?` on an unquoted path
    /// enables the absent modifier; quoted paths pass `absent` explicitly.
    pub fn parse(text: &str, quoted: bool, absent: bool) -> Result<Self, String> {
        let mut body = text;
        let mut absent = absent;

        if !quoted {
            if let Some(stripped) = body.strip_suffix('?') {
                absent = true;
                body = stripped;
            }
        }

        if body.is_empty() {
            return Err("empty field path".to_string());
        }

        let leading_dot = body.starts_with('.');
        let body = body.strip_prefix('.').unwrap_or(body);
        if body.is_empty() {
            return Err("empty field path".to_string());
        }

        if quoted {
            return Ok(Self {
                raw: body.to_string(),
                segments: vec![Segment::Name(body.to_string())],
                role: None,
                exact_only: true,
                absent,
            });
        }

        let segments = parse_segments(body)?;
        let role = if !leading_dot && segments.len() == 1 {
            match &segments[0] {
                Segment::Name(name) => match name.as_str() {
                    "level" => Some(Role::Level),
                    "message" => Some(Role::Message),
                    "logger" => Some(Role::Logger),
                    "caller" => Some(Role::Caller),
                    _ => None,
                },
                _ => None,
            }
        } else {
            None
        };

        if role.is_none() && !leading_dot && is_reserved(body) {
            return Err(format!(
                "reserved word '{body}' must be quoted or prefixed with '.' to be used as a field"
            ));
        }

        Ok(Self {
            raw: body.to_string(),
            segments,
            role,
            exact_only: false,
            absent,
        })
    }

    /// Parse a `-f key=value` path, which may be quoted and may end in `?`.
    pub fn from_filter(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.starts_with('"') {
            let end = find_string_end(text)?;
            let key: String = serde_json::from_str(&text[..end])
                .map_err(|error| format!("invalid quoted field name: {error}"))?;
            let rest = text[end..].trim();
            let absent = match rest {
                "" => false,
                "?" => true,
                other => return Err(format!("unexpected text '{other}' after quoted field name")),
            };
            if key.is_empty() {
                return Err("empty field path".to_string());
            }
            return Ok(Self {
                raw: key.clone(),
                segments: vec![Segment::Name(key)],
                role: None,
                exact_only: true,
                absent,
            });
        }

        Self::parse(text, false, false)
    }

    fn resolve<'a>(&self, entry: &'a Entry) -> Vec<Resolved<'a>> {
        if let Some(role) = self.role {
            return match role {
                Role::Level => entry
                    .level
                    .map(|level| {
                        vec![Resolved::Level {
                            level,
                            raw: entry.level_raw.as_deref(),
                        }]
                    })
                    .unwrap_or_default(),
                Role::Message => entry
                    .message
                    .as_deref()
                    .map(|text| vec![Resolved::Text(text)])
                    .unwrap_or_default(),
                Role::Logger => entry
                    .logger
                    .as_deref()
                    .map(|text| vec![Resolved::Text(text)])
                    .unwrap_or_default(),
                Role::Caller => entry
                    .caller
                    .as_deref()
                    .map(|text| vec![Resolved::Text(text)])
                    .unwrap_or_default(),
            };
        }

        let mut values = Vec::new();

        if let Some(value) = entry.fields.get(&self.raw) {
            values.push(value);
        } else if !self.exact_only {
            if let Some(Segment::Name(first)) = self.segments.first() {
                if let Some(root) = entry.fields.get(first) {
                    walk(root, &self.segments[1..], &mut values);
                }
            }
        }

        values.into_iter().map(Resolved::Value).collect()
    }
}

fn parse_segments(body: &str) -> Result<Vec<Segment>, String> {
    let mut segments = Vec::new();

    for part in body.split('.') {
        if part.is_empty() {
            return Err("empty path segment".to_string());
        }

        if part == "[]" {
            segments.push(Segment::AnyIndex);
        } else if part.starts_with('[') {
            let inner = part
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
                .ok_or_else(|| format!("unclosed '[' in path segment '{part}'"))?;
            if inner.is_empty() {
                segments.push(Segment::AnyIndex);
            } else if let Ok(index) = inner.parse::<usize>() {
                segments.push(Segment::Index(index));
            } else {
                return Err(format!("invalid array index '[{inner}]'"));
            }
        } else if part.contains('[') || part.contains(']') {
            return Err(format!("invalid path segment '{part}'"));
        } else {
            segments.push(Segment::Name(part.to_string()));
        }
    }

    if !matches!(segments.first(), Some(Segment::Name(_))) {
        return Err("path must start with a field name".to_string());
    }

    Ok(segments)
}

fn walk<'a>(value: &'a Value, segments: &[Segment], out: &mut Vec<&'a Value>) {
    if segments.is_empty() {
        out.push(value);
        return;
    }

    match &segments[0] {
        Segment::Name(name) => {
            if let Value::Object(map) = value {
                if let Some(next) = map.get(name) {
                    walk(next, &segments[1..], out);
                }
            }
        }
        Segment::AnyIndex => {
            if let Value::Array(items) = value {
                for item in items {
                    walk(item, &segments[1..], out);
                }
            }
        }
        Segment::Index(index) => {
            if let Value::Array(items) = value {
                if let Some(next) = items.get(*index) {
                    walk(next, &segments[1..], out);
                }
            }
        }
    }
}

fn find_string_end(text: &str) -> Result<usize, String> {
    let mut escaped = false;

    for (index, character) in text.char_indices() {
        if index == 0 {
            continue;
        }
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' => return Ok(index + 1),
            _ => {}
        }
    }

    Err("unterminated string literal".to_string())
}

fn is_reserved(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "and"
            | "or"
            | "not"
            | "in"
            | "exists"
            | "exist"
            | "like"
            | "contain"
            | "match"
            | "eq"
            | "ne"
            | "gt"
            | "ge"
            | "lt"
            | "le"
            | "true"
            | "false"
            | "null"
    )
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}

impl Literal {
    pub fn text(&self) -> String {
        match self {
            Literal::Str(text) => text.clone(),
            Literal::Num(number) => number.to_string(),
            Literal::Bool(true) => "true".to_string(),
            Literal::Bool(false) => "false".to_string(),
            Literal::Null => "null".to_string(),
        }
    }
}

#[derive(Debug)]
pub enum Op {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Contains,
    NotContains,
    Regex(Regex),
    NotRegex(Regex),
    Like(Regex),
    NotLike(Regex),
}

#[derive(Debug)]
pub enum SetRef {
    Inline(Vec<Literal>),
    File { path: String },
    Stdin { offset: usize, query: String },
}

#[derive(Debug)]
pub enum Expr {
    Bool(bool),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Compare {
        path: Path,
        op: Op,
        value: Literal,
    },
    Exists {
        path: Path,
    },
    In {
        path: Path,
        set: SetRef,
        negated: bool,
    },
}

impl Expr {
    pub fn matches(&self, entry: &Entry) -> bool {
        self.eval(entry) == Truth::True
    }

    /// Rough cost class used to order top-level AND terms (QUERY_SPEC §8).
    pub fn cost(&self) -> u8 {
        match self {
            Expr::Bool(_) => 0,
            Expr::Compare { op, .. } => match op {
                Op::Eq | Op::Ne | Op::Gt | Op::Ge | Op::Lt | Op::Le => 1,
                Op::Contains | Op::NotContains => 3,
                Op::Regex(_) | Op::NotRegex(_) | Op::Like(_) | Op::NotLike(_) => 4,
            },
            Expr::Exists { .. } => 2,
            Expr::In { .. } => 1,
            Expr::Not(inner) => inner.cost(),
            Expr::And(left, right) | Expr::Or(left, right) => left.cost().max(right.cost()),
        }
    }

    fn eval(&self, entry: &Entry) -> Truth {
        match self {
            Expr::Bool(true) => Truth::True,
            Expr::Bool(false) => Truth::False,
            Expr::And(left, right) => {
                let left = left.eval(entry);
                if left == Truth::False {
                    return Truth::False;
                }
                left.and(right.eval(entry))
            }
            Expr::Or(left, right) => {
                let left = left.eval(entry);
                if left == Truth::True {
                    return Truth::True;
                }
                left.or(right.eval(entry))
            }
            Expr::Not(inner) => inner.eval(entry).not(),
            Expr::Exists { path } => {
                if path.resolve(entry).is_empty() {
                    Truth::False
                } else {
                    Truth::True
                }
            }
            Expr::Compare { path, op, value } => compare(path, op, value, entry),
            Expr::In { path, set, negated } => {
                let values = path.resolve(entry);
                if values.is_empty() {
                    return if path.absent {
                        Truth::True
                    } else {
                        Truth::Unknown
                    };
                }

                let candidates = match set {
                    SetRef::Inline(values) => values,
                    // resolve_sets() replaces file/stdin references before evaluation.
                    SetRef::File { .. } | SetRef::Stdin { .. } => return Truth::False,
                };

                let any = values
                    .iter()
                    .any(|value| candidates.iter().any(|candidate| value.equals(candidate)));

                if (any && !negated) || (!any && *negated) {
                    Truth::True
                } else {
                    Truth::False
                }
            }
        }
    }
}

fn compare(path: &Path, op: &Op, literal: &Literal, entry: &Entry) -> Truth {
    let values = path.resolve(entry);

    if values.is_empty() {
        return if path.absent {
            Truth::True
        } else {
            Truth::Unknown
        };
    }

    let matched = match op {
        Op::Eq => values.iter().any(|value| value.equals(literal)),
        Op::Ne => values.iter().all(|value| !value.equals(literal)),
        Op::Gt => values
            .iter()
            .filter_map(|value| value.order(literal))
            .any(|order| order == Ordering::Greater),
        Op::Ge => values
            .iter()
            .filter_map(|value| value.order(literal))
            .any(|order| order != Ordering::Less),
        Op::Lt => values
            .iter()
            .filter_map(|value| value.order(literal))
            .any(|order| order == Ordering::Less),
        Op::Le => values
            .iter()
            .filter_map(|value| value.order(literal))
            .any(|order| order != Ordering::Greater),
        Op::Contains => {
            let needle = literal.text();
            values
                .iter()
                .filter_map(Resolved::text)
                .any(|text| text.contains(&needle))
        }
        Op::NotContains => {
            let needle = literal.text();
            values
                .iter()
                .filter_map(Resolved::text)
                .all(|text| !text.contains(&needle))
        }
        Op::Regex(regex) => values
            .iter()
            .filter_map(Resolved::text)
            .any(|text| regex.is_match(&text)),
        Op::NotRegex(regex) => values
            .iter()
            .filter_map(Resolved::text)
            .all(|text| !regex.is_match(&text)),
        Op::Like(regex) => values
            .iter()
            .filter_map(Resolved::text)
            .any(|text| regex.is_match(&text)),
        Op::NotLike(regex) => values
            .iter()
            .filter_map(Resolved::text)
            .all(|text| !regex.is_match(&text)),
    };

    if matched {
        Truth::True
    } else {
        Truth::False
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Truth {
    True,
    False,
    Unknown,
}

impl Truth {
    fn not(self) -> Truth {
        match self {
            Truth::True => Truth::False,
            Truth::False => Truth::True,
            Truth::Unknown => Truth::Unknown,
        }
    }

    fn and(self, other: Truth) -> Truth {
        match (self, other) {
            (Truth::False, _) | (_, Truth::False) => Truth::False,
            (Truth::Unknown, _) | (_, Truth::Unknown) => Truth::Unknown,
            _ => Truth::True,
        }
    }

    fn or(self, other: Truth) -> Truth {
        match (self, other) {
            (Truth::True, _) | (_, Truth::True) => Truth::True,
            (Truth::Unknown, _) | (_, Truth::Unknown) => Truth::Unknown,
            _ => Truth::False,
        }
    }
}

enum Resolved<'a> {
    Level { level: Level, raw: Option<&'a str> },
    Text(&'a str),
    Value(&'a Value),
}

impl Resolved<'_> {
    fn equals(&self, literal: &Literal) -> bool {
        match self {
            Resolved::Level { level, .. } => {
                literal_level(literal).is_some_and(|other| *level == other)
            }
            Resolved::Text(text) => text_equals(text, literal),
            Resolved::Value(value) => value_equals(value, literal),
        }
    }

    fn order(&self, literal: &Literal) -> Option<Ordering> {
        match self {
            Resolved::Level { level, .. } => literal_level(literal).map(|other| level.cmp(&other)),
            Resolved::Text(text) => match literal {
                Literal::Str(other) => Some(text.as_bytes().cmp(other.as_bytes())),
                Literal::Num(other) => text
                    .parse::<f64>()
                    .ok()
                    .filter(|number| number.is_finite())
                    .and_then(|number| number.partial_cmp(other)),
                Literal::Bool(_) | Literal::Null => None,
            },
            Resolved::Value(value) => value_order(value, literal),
        }
    }

    fn text(&self) -> Option<String> {
        match self {
            Resolved::Level { level, raw } => Some(
                raw.map(str::to_string)
                    .unwrap_or_else(|| level.label().to_ascii_lowercase()),
            ),
            Resolved::Text(text) => Some((*text).to_string()),
            Resolved::Value(Value::String(text)) => Some(text.clone()),
            Resolved::Value(Value::Number(number)) => Some(number.to_string()),
            Resolved::Value(Value::Bool(flag)) => Some(flag.to_string()),
            Resolved::Value(Value::Null) => Some("null".to_string()),
            Resolved::Value(Value::Array(_)) | Resolved::Value(Value::Object(_)) => None,
        }
    }
}

fn text_equals(text: &str, literal: &Literal) -> bool {
    match literal {
        Literal::Str(other) => text == other,
        Literal::Num(other) => text
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
            .is_some_and(|number| number == *other),
        Literal::Bool(_) | Literal::Null => false,
    }
}

fn value_equals(value: &Value, literal: &Literal) -> bool {
    match value {
        Value::String(text) => text_equals(text, literal),
        Value::Number(number) => match literal {
            Literal::Num(other) => number.as_f64() == Some(*other),
            Literal::Str(other) => number.as_f64().is_some_and(|left| {
                other
                    .parse::<f64>()
                    .ok()
                    .filter(|right| right.is_finite())
                    .is_some_and(|right| left == right)
            }),
            Literal::Bool(_) | Literal::Null => false,
        },
        Value::Bool(flag) => matches!(literal, Literal::Bool(other) if other == flag),
        Value::Null => matches!(literal, Literal::Null),
        Value::Array(_) | Value::Object(_) => false,
    }
}

fn value_order(value: &Value, literal: &Literal) -> Option<Ordering> {
    match (value, literal) {
        (Value::Number(left), Literal::Num(right)) => left.as_f64()?.partial_cmp(right),
        (Value::String(left), Literal::Num(right)) => left
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())?
            .partial_cmp(right),
        (Value::Number(left), Literal::Str(right)) => {
            let right = right.parse::<f64>().ok()?;
            left.as_f64()?.partial_cmp(&right)
        }
        (Value::String(left), Literal::Str(right)) => Some(left.as_bytes().cmp(right.as_bytes())),
        _ => None,
    }
}

fn literal_level(literal: &Literal) -> Option<Level> {
    match literal {
        Literal::Str(text) => Level::parse(text),
        Literal::Num(number) if number.fract() == 0.0 && *number >= 0.0 => {
            Level::from_number(*number as i64)
        }
        _ => None,
    }
}

/// Compare two literals. Used to fold comparisons that contain no field path.
pub fn literal_compare(left: &Literal, right: &Literal, op: &Op) -> bool {
    match op {
        Op::Eq => literal_equals(left, right),
        Op::Ne => !literal_equals(left, right),
        Op::Gt => literal_order(left, right) == Some(Ordering::Greater),
        Op::Ge => literal_order(left, right) != Some(Ordering::Less),
        Op::Lt => literal_order(left, right) == Some(Ordering::Less),
        Op::Le => literal_order(left, right) != Some(Ordering::Greater),
        Op::Contains => left.text().contains(&right.text()),
        Op::NotContains => !left.text().contains(&right.text()),
        Op::Regex(regex) => regex.is_match(&left.text()),
        Op::NotRegex(regex) => !regex.is_match(&left.text()),
        Op::Like(regex) => regex.is_match(&left.text()),
        Op::NotLike(regex) => !regex.is_match(&left.text()),
    }
}

fn literal_equals(left: &Literal, right: &Literal) -> bool {
    match (left, right) {
        (Literal::Str(left), Literal::Str(right)) => left == right,
        (Literal::Num(left), Literal::Num(right)) => left == right,
        (Literal::Str(text), Literal::Num(number)) | (Literal::Num(number), Literal::Str(text)) => {
            text.parse::<f64>()
                .ok()
                .filter(|parsed| parsed.is_finite())
                .is_some_and(|parsed| parsed == *number)
        }
        (Literal::Bool(left), Literal::Bool(right)) => left == right,
        (Literal::Null, Literal::Null) => true,
        _ => false,
    }
}

fn literal_order(left: &Literal, right: &Literal) -> Option<Ordering> {
    match (left, right) {
        (Literal::Num(left), Literal::Num(right)) => left.partial_cmp(right),
        (Literal::Str(left), Literal::Str(right)) => Some(left.as_bytes().cmp(right.as_bytes())),
        (Literal::Str(left), Literal::Num(right)) => left
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())?
            .partial_cmp(right),
        (Literal::Num(left), Literal::Str(right)) => {
            let right = right.parse::<f64>().ok()?;
            left.partial_cmp(&right)
        }
        _ => None,
    }
}

pub fn like_regex(pattern: &str) -> Regex {
    let mut regex = String::from("^");
    for character in pattern.chars() {
        match character {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            other => regex.push_str(&regex::escape(&other.to_string())),
        }
    }
    regex.push('$');
    Regex::new(&regex).expect("wildcard regex is always valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Format;
    use serde_json::{json, Map};

    fn entry() -> Entry {
        let mut fields = Map::new();
        fields.insert("status".to_string(), json!(500));
        fields.insert("level".to_string(), json!("error"));
        fields.insert("message".to_string(), json!("connection refused"));
        fields.insert("cached".to_string(), json!(true));
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

    fn compare(path: &str, op: Op, value: Literal) -> bool {
        Expr::Compare {
            path: Path::parse(path, false, false).unwrap(),
            op,
            value,
        }
        .matches(&entry())
    }

    #[test]
    fn role_and_source_paths_differ() {
        assert!(compare("level", Op::Eq, Literal::Str("ERROR".into())));
        assert!(!compare(".level", Op::Eq, Literal::Str("ERROR".into())));
        assert!(compare(".level", Op::Eq, Literal::Str("error".into())));
    }

    #[test]
    fn numbers_match_numeric_strings() {
        assert!(compare("status", Op::Eq, Literal::Num(500.0)));
        assert!(compare("status", Op::Ge, Literal::Num(500.0)));
        assert!(!compare("status", Op::Lt, Literal::Num(500.0)));
    }

    #[test]
    fn nested_and_array_paths_resolve() {
        assert!(compare(
            "request.method",
            Op::Eq,
            Literal::Str("GET".into())
        ));
        assert!(compare(
            "span.[].name",
            Op::Eq,
            Literal::Str("sp0002".into())
        ));
        assert!(compare(
            "span.[0].name",
            Op::Eq,
            Literal::Str("sp0001".into())
        ));
    }

    #[test]
    fn absent_modifier_includes_missing_fields() {
        assert!(compare("missing?", Op::Eq, Literal::Str("value".into())));
        assert!(compare("missing?", Op::Ne, Literal::Str("value".into())));
        assert!(!compare("missing", Op::Eq, Literal::Str("value".into())));
        assert!(!compare("missing", Op::Ne, Literal::Str("value".into())));
    }

    #[test]
    fn not_of_absent_comparison_stays_unknown() {
        let expr = Expr::Not(Box::new(Expr::Compare {
            path: Path::parse("missing", false, false).unwrap(),
            op: Op::Eq,
            value: Literal::Str("value".into()),
        }));
        assert!(!expr.matches(&entry()));
    }

    #[test]
    fn string_operators() {
        assert!(compare(
            "message",
            Op::Contains,
            Literal::Str("refused".into())
        ));
        assert!(!compare(
            "message",
            Op::NotContains,
            Literal::Str("refused".into())
        ));
        assert!(compare(
            "message",
            Op::Regex(Regex::new("^connection").unwrap()),
            Literal::Str("".into())
        ));
    }

    #[test]
    fn booleans_do_not_coerce_to_strings() {
        assert!(compare("cached", Op::Eq, Literal::Bool(true)));
        assert!(!compare("cached", Op::Eq, Literal::Str("true".into())));
        assert!(compare("cached", Op::Ne, Literal::Str("true".into())));
    }

    #[test]
    fn like_patterns_are_anchored() {
        assert!(like_regex("/api/*/users").is_match("/api/v1/users"));
        assert!(!like_regex("/api/*/users").is_match("/api/v1/users/extra"));
    }
}
