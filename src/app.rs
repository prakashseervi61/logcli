use crate::cli::Args;
use crate::error::Error;
use crate::filter::Filters;
use crate::input;
use crate::model::{Entry, Level};
use crate::output::{self, ColorMode, Expansion, FormatOptions, Formatter, OutputTz};
use crate::query;
use chrono::{DateTime, Duration, Local, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use std::io::{BufRead, Write};

pub fn run(args: Args) -> Result<(), Error> {
    if args.follow {
        return Err(Error::usage(
            "follow mode (-F) is not implemented in this build",
        ));
    }

    validate_input_format(&args.input_format)?;

    let mut exprs = args
        .filters
        .iter()
        .map(|raw| query::parse_filter(raw))
        .collect::<Result<Vec<_>, _>>()?;

    for raw in &args.queries {
        let expr =
            query::parse(raw).map_err(|error| Error::query(query::format_error(raw, &error)))?;
        exprs.push(expr);
    }

    let logs_from_stdin = args.files.is_empty()
        || args
            .files
            .iter()
            .any(|path| path == std::path::Path::new("-"));
    query::resolve_sets(&mut exprs, logs_from_stdin)?;

    let min_level = match &args.level {
        Some(text) => Some(
            Level::parse(text).ok_or_else(|| Error::usage(format!("invalid level '{text}'")))?,
        ),
        None => None,
    };

    let now = Utc::now();
    let since = args
        .since
        .as_deref()
        .map(|text| parse_time(text, now))
        .transpose()?;
    let until = args
        .until
        .as_deref()
        .map(|text| parse_time(text, now))
        .transpose()?;

    let filters = Filters::new(min_level, since, until, exprs);

    let expansion = Expansion::parse(&args.expansion)?;
    let timezone = OutputTz::parse(args.time_zone.as_deref(), args.local)?;

    let color_mode = if args.no_color {
        ColorMode::Never
    } else if args.color_always {
        ColorMode::Always
    } else {
        ColorMode::parse(&args.color)?
    };

    let tty_output = output::is_tty_output(args.output.as_deref());
    let color = color_mode.enabled(tty_output);

    let trailing = match args.output_delimiter.as_str() {
        "newline" => b'\n',
        "nul" => 0,
        other => {
            return Err(Error::usage(format!(
                "invalid output delimiter '{other}' (expected newline or nul)"
            )))
        }
    };

    let formatter = Formatter::new(FormatOptions {
        color,
        timezone,
        time_format: args.time_format.clone(),
        hide: args.hide.clone(),
        hide_empty: args.hide_empty && !args.show_empty,
        expansion,
        raw: args.raw,
        trailing,
    })?;

    let max_entry_size = parse_size(&args.max_entry_size).ok_or_else(|| {
        Error::usage(format!(
            "invalid maximum entry size '{}'",
            args.max_entry_size
        ))
    })?;

    let sources = input::open_sources(&args.files)?;
    let mut writer = output::open_output(args.output.as_deref())?;

    let mut total = 0u64;
    let mut matched = 0u64;
    let mut invalid = 0u64;
    let mut oversized = 0u64;
    let mut sorted: Vec<Entry> = Vec::new();

    for mut source in sources {
        let delimiter = input::detect_delimiter(source.reader.as_mut(), &args.delimiter)?;
        let mut buffer = Vec::with_capacity(8 * 1024);
        let mut line_number = 0u64;

        loop {
            buffer.clear();
            let read = source
                .reader
                .read_until(delimiter, &mut buffer)
                .map_err(|err| Error::io(Some(&source.name), err))?;

            if read == 0 {
                break;
            }

            line_number += 1;

            if buffer.last() == Some(&delimiter) {
                buffer.pop();
            }
            if delimiter != b'\r' && buffer.last() == Some(&b'\r') {
                buffer.pop();
            }
            if delimiter == b'\r' && buffer == b"\n" {
                continue;
            }

            if buffer.is_empty() {
                continue;
            }

            if buffer.len() > max_entry_size {
                oversized += 1;
                continue;
            }

            total += 1;
            let raw = String::from_utf8_lossy(&buffer);

            let Some(entry) = crate::parser::parse_entry(
                &raw,
                &source.name,
                line_number,
                &args.input_format,
                args.allow_prefix,
            ) else {
                invalid += 1;
                continue;
            };

            if !filters.matches(&entry) {
                continue;
            }

            matched += 1;

            if args.sort {
                sorted.push(entry);
            } else {
                formatter.write(&entry, writer.as_mut())?;
            }
        }
    }

    if args.sort {
        sorted.sort_by(
            |left, right| match (left.timestamp.as_ref(), right.timestamp.as_ref()) {
                (Some(left), Some(right)) => left.cmp(right),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            },
        );

        for entry in &sorted {
            formatter.write(entry, writer.as_mut())?;
        }
    }

    writer.flush()?;

    if args.verbose && !args.silent {
        eprintln!(
            "logcli: {total} entries read, {matched} matched, {invalid} invalid, {oversized} oversized"
        );
    }

    Ok(())
}

fn validate_input_format(format: &str) -> Result<(), Error> {
    match format {
        "auto" | "json" | "logfmt" | "plain" => Ok(()),
        other => Err(Error::usage(format!(
            "invalid input format '{other}' (expected auto, json, logfmt, or plain)"
        ))),
    }
}

fn parse_time(text: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>, Error> {
    let text = text.trim();

    if text.eq_ignore_ascii_case("now") {
        return Ok(now);
    }

    if text.eq_ignore_ascii_case("today") {
        return Ok(local_midnight(Local::now().date_naive()));
    }

    if text.eq_ignore_ascii_case("yesterday") {
        let yesterday = Local::now()
            .date_naive()
            .pred_opt()
            .unwrap_or_else(|| Local::now().date_naive());
        return Ok(local_midnight(yesterday));
    }

    if let Some(rest) = text.strip_prefix('-') {
        let split = rest
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(rest.len());
        let (number, unit) = rest.split_at(split);
        let amount = number.parse::<i64>().map_err(|_| {
            Error::usage(format!(
                "invalid relative time '{text}' (expected e.g. -3h or -14d)"
            ))
        })?;
        let duration = match unit {
            "s" => Duration::seconds(amount),
            "m" => Duration::minutes(amount),
            "h" => Duration::hours(amount),
            "d" => Duration::days(amount),
            "w" => Duration::weeks(amount),
            _ => {
                return Err(Error::usage(format!(
                    "invalid relative time '{text}' (expected unit s, m, h, d, or w)"
                )))
            }
        };
        return Ok(now - duration);
    }

    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Ok(parsed.with_timezone(&Utc));
    }

    const DATETIME_FORMATS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ];
    for format in DATETIME_FORMATS {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(text, format) {
            return Ok(Utc.from_utc_datetime(&parsed));
        }
    }

    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        let midnight = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| Error::usage(format!("invalid time '{text}'")))?;
        return Ok(Utc.from_utc_datetime(&midnight));
    }

    Err(Error::usage(format!(
        "invalid time '{text}' (expected RFC3339, YYYY-MM-DD, or a relative offset like -3h)"
    )))
}

fn local_midnight(date: NaiveDate) -> DateTime<Utc> {
    let Some(midnight) = date.and_hms_opt(0, 0, 0) else {
        return Utc::now();
    };

    match Local.from_local_datetime(&midnight) {
        LocalResult::Single(value) => value.with_timezone(&Utc),
        LocalResult::Ambiguous(first, _) => first.with_timezone(&Utc),
        LocalResult::None => Utc.from_utc_datetime(&midnight),
    }
}

fn parse_size(text: &str) -> Option<usize> {
    let text = text.trim();
    let (number, multiplier) = if let Some(value) = text.strip_suffix("GiB") {
        (value, 1024usize.pow(3))
    } else if let Some(value) = text.strip_suffix("MiB") {
        (value, 1024usize.pow(2))
    } else if let Some(value) = text.strip_suffix("KiB") {
        (value, 1024)
    } else if let Some(value) = text.strip_suffix("GB") {
        (value, 1_000_000_000)
    } else if let Some(value) = text.strip_suffix("MB") {
        (value, 1_000_000)
    } else if let Some(value) = text.strip_suffix("KB") {
        (value, 1_000)
    } else if let Some(value) = text.strip_suffix('B') {
        (value, 1)
    } else {
        (text, 1)
    };

    number
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|value| value.checked_mul(multiplier))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_size("1024"), Some(1024));
        assert_eq!(parse_size("1KiB"), Some(1024));
        assert_eq!(parse_size("1MiB"), Some(1024 * 1024));
        assert_eq!(parse_size("2GB"), Some(2_000_000_000));
        assert_eq!(parse_size("nope"), None);
    }

    #[test]
    fn parses_relative_times() {
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap();
        assert_eq!(parse_time("now", now).unwrap(), now);
        assert_eq!(parse_time("-3h", now).unwrap(), now - Duration::hours(3));
        assert_eq!(parse_time("-14d", now).unwrap(), now - Duration::days(14));
        assert!(parse_time("garbage", now).is_err());
    }

    #[test]
    fn parses_absolute_times() {
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap();
        let parsed = parse_time("2026-10-28T10:00:00Z", now).unwrap();
        assert_eq!(parsed.timestamp(), 1_793_181_600);
        assert!(parse_time("2026-13-40", now).is_err());
    }
}
