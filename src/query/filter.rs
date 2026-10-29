use crate::error::Error;
use crate::query::ast::{Expr, Literal, Op, Path};
use regex::Regex;

#[derive(Clone, Copy)]
enum FilterOp {
    Eq,
    Ne,
    Contains,
    NotContains,
    Regex,
    NotRegex,
}

const OPERATORS: &[(&str, FilterOp)] = &[
    ("!~~=", FilterOp::NotRegex),
    ("~~=", FilterOp::Regex),
    ("!~=", FilterOp::NotContains),
    ("~=", FilterOp::Contains),
    ("!=", FilterOp::Ne),
    ("==", FilterOp::Eq),
    ("=", FilterOp::Eq),
];

/// Compile the legacy `-f key=value` form into the query AST.
pub fn parse_filter(raw: &str) -> Result<Expr, Error> {
    let mut best: Option<(usize, &'static str, FilterOp)> = None;

    for &(symbol, op) in OPERATORS {
        if let Some(index) = raw.find(symbol) {
            let replace = match best {
                None => true,
                Some((best_index, best_symbol, _)) => {
                    index < best_index || (index == best_index && symbol.len() > best_symbol.len())
                }
            };
            if replace {
                best = Some((index, symbol, op));
            }
        }
    }

    let Some((index, symbol, op)) = best else {
        return Err(invalid(
            raw,
            "expected key=value, key!=value, key~=text, or key~~=regex",
        ));
    };

    let path_text = raw[..index].trim();
    if path_text.is_empty() {
        return Err(invalid(raw, "empty field name"));
    }

    let path = Path::from_filter(path_text).map_err(|message| invalid(raw, message))?;

    let value_text = raw[index + symbol.len()..].trim();
    if value_text.is_empty() {
        return Err(invalid(raw, "empty value"));
    }

    let value = parse_literal(value_text).map_err(|message| invalid(raw, message))?;

    let op = match op {
        FilterOp::Eq => Op::Eq,
        FilterOp::Ne => Op::Ne,
        FilterOp::Contains => Op::Contains,
        FilterOp::NotContains => Op::NotContains,
        FilterOp::Regex => Op::Regex(compile_regex(raw, &value)?),
        FilterOp::NotRegex => Op::NotRegex(compile_regex(raw, &value)?),
    };

    Ok(Expr::Compare { path, op, value })
}

fn compile_regex(raw: &str, value: &Literal) -> Result<Regex, Error> {
    Regex::new(&value.text()).map_err(|error| invalid(raw, format!("invalid regex: {error}")))
}

fn parse_literal(text: &str) -> Result<Literal, String> {
    let text = text.trim();

    if text.is_empty() {
        return Err("empty value".to_string());
    }

    if text.starts_with('"') {
        if text.len() < 2 || !text.ends_with('"') {
            return Err("unterminated quoted value".to_string());
        }
        let value: String =
            serde_json::from_str(text).map_err(|error| format!("invalid quoted value: {error}"))?;
        return Ok(Literal::Str(value));
    }

    match text {
        "true" => return Ok(Literal::Bool(true)),
        "false" => return Ok(Literal::Bool(false)),
        "null" => return Ok(Literal::Null),
        _ => {}
    }

    if let Ok(number) = text.parse::<f64>() {
        if number.is_finite() {
            return Ok(Literal::Num(number));
        }
    }

    Ok(Literal::Str(text.to_string()))
}

fn invalid(raw: &str, message: impl std::fmt::Display) -> Error {
    Error::usage(format!("invalid filter '{raw}': {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_operators() {
        assert!(matches!(
            parse_filter("status=500").unwrap(),
            Expr::Compare {
                op: Op::Eq,
                value: Literal::Num(_),
                ..
            }
        ));
        assert!(matches!(
            parse_filter("status?!=500").unwrap(),
            Expr::Compare { op: Op::Ne, .. }
        ));
        assert!(matches!(
            parse_filter("message~=refused").unwrap(),
            Expr::Compare {
                op: Op::Contains,
                ..
            }
        ));
        assert!(matches!(
            parse_filter("message~~=^connection").unwrap(),
            Expr::Compare {
                op: Op::Regex(_),
                ..
            }
        ));
    }

    #[test]
    fn parses_typed_values() {
        assert!(matches!(
            parse_filter("cached=true").unwrap(),
            Expr::Compare {
                value: Literal::Bool(true),
                ..
            }
        ));
        assert!(matches!(
            parse_filter("deleted=null").unwrap(),
            Expr::Compare {
                value: Literal::Null,
                ..
            }
        ));
        assert!(matches!(
            parse_filter("status=\"500\"").unwrap(),
            Expr::Compare {
                value: Literal::Str(_),
                ..
            }
        ));
    }

    #[test]
    fn rejects_invalid_filters() {
        assert!(parse_filter("nope").is_err());
        assert!(parse_filter("message~~=(unclosed").is_err());
        assert!(parse_filter("=value").is_err());
        assert!(parse_filter("key=").is_err());
    }
}
