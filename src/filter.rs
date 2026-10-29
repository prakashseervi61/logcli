use crate::model::{Entry, Level};
use crate::query::Expr;
use chrono::{DateTime, Utc};

pub struct Filters {
    min_level: Option<Level>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    fields: Vec<Expr>,
}

impl Filters {
    pub fn new(
        min_level: Option<Level>,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
        mut fields: Vec<Expr>,
    ) -> Self {
        // QUERY_SPEC §8: order top-level AND terms by cost.
        fields.sort_by_key(Expr::cost);

        Self {
            min_level,
            since,
            until,
            fields,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Format;
    use crate::query::parse_filter;
    use serde_json::{json, Map};

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
        let filters = Filters::new(None, None, None, vec![parse_filter(filter).unwrap()]);
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
        assert!(parse_filter("nope").is_err());
        assert!(parse_filter("message~~=(unclosed").is_err());
    }
}
