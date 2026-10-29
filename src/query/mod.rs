use crate::error::Error;
use std::io::Read;

pub mod ast;
mod filter;
mod lexer;
mod parser;

pub use ast::{Expr, Literal, SetRef};
pub use filter::parse_filter;
pub use parser::{parse, ParseError};

/// Render a parse error in the CLI_SPEC §11 caret format.
pub fn format_error(input: &str, error: &ParseError) -> String {
    let column = input[..error.offset.min(input.len())].chars().count() + 1;
    let padding = " ".repeat(column.saturating_sub(1));
    format!(
        "invalid query at column {column}: {}\n  {input}\n  {padding}^",
        error.message
    )
}

/// Replace `in @file` and `in @-` references with inline values.
///
/// `logs_from_stdin` must be true when the log input itself is stdin; in that
/// case `@-` is rejected because both cannot consume the same stream.
pub fn resolve_sets(exprs: &mut [Expr], logs_from_stdin: bool) -> Result<(), Error> {
    let mut stdin_values: Option<Vec<Literal>> = None;

    for expr in exprs.iter_mut() {
        walk_sets(expr, logs_from_stdin, &mut stdin_values)?;
    }

    Ok(())
}

fn walk_sets(
    expr: &mut Expr,
    logs_from_stdin: bool,
    stdin_values: &mut Option<Vec<Literal>>,
) -> Result<(), Error> {
    match expr {
        Expr::And(left, right) | Expr::Or(left, right) => {
            walk_sets(left, logs_from_stdin, stdin_values)?;
            walk_sets(right, logs_from_stdin, stdin_values)
        }
        Expr::Not(inner) => walk_sets(inner, logs_from_stdin, stdin_values),
        Expr::In { set, .. } => resolve_set(set, logs_from_stdin, stdin_values),
        Expr::Bool(_) | Expr::Compare { .. } | Expr::Exists { .. } => Ok(()),
    }
}

fn resolve_set(
    set: &mut SetRef,
    logs_from_stdin: bool,
    stdin_values: &mut Option<Vec<Literal>>,
) -> Result<(), Error> {
    let replacement = match set {
        SetRef::Inline(_) => return Ok(()),
        SetRef::File { path } => SetRef::Inline(read_set_file(path)?),
        SetRef::Stdin { offset, query } => {
            if logs_from_stdin {
                return Err(Error::query(format_error(
                    query,
                    &ParseError {
                        offset: *offset,
                        message: "cannot read value set from stdin while logs are read from stdin"
                            .to_string(),
                    },
                )));
            }

            if stdin_values.is_none() {
                let mut text = String::new();
                std::io::stdin()
                    .read_to_string(&mut text)
                    .map_err(|error| Error::io(None, error))?;
                *stdin_values = Some(parse_set_text(&text, "<stdin>")?);
            }

            SetRef::Inline(stdin_values.clone().unwrap_or_default())
        }
    };

    *set = replacement;
    Ok(())
}

fn read_set_file(path: &str) -> Result<Vec<Literal>, Error> {
    let text = std::fs::read_to_string(path).map_err(|error| Error::io(Some(path), error))?;
    parse_set_text(&text, path)
}

fn parse_set_text(text: &str, name: &str) -> Result<Vec<Literal>, Error> {
    let mut values = Vec::new();

    for (index, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }

        if line.starts_with('"') {
            let value = serde_json::from_str::<String>(line).map_err(|error| {
                Error::query(format!(
                    "invalid JSON string in value set '{name}' at line {}: {error}",
                    index + 1
                ))
            })?;
            values.push(Literal::Str(value));
        } else {
            values.push(Literal::Str(line.to_string()));
        }
    }

    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_caret_errors() {
        let input = "status =";
        let error = ParseError {
            offset: input.len(),
            message: "expected a value after '='".to_string(),
        };
        let rendered = format_error(input, &error);
        assert!(rendered.starts_with("invalid query at column 9: "));
        assert!(rendered.ends_with("\n  status =\n          ^"));
    }

    #[test]
    fn parses_value_set_lines() {
        let values = parse_set_text("\"a b\"\nplain\n\n", "set.txt").unwrap();
        assert_eq!(
            values,
            vec![
                Literal::Str("a b".to_string()),
                Literal::Str("plain".to_string())
            ]
        );
    }
}
