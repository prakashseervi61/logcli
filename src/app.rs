use crate::cli::Args;
use crate::config::{AlertResult, Config};
use crate::error::Error;
use crate::filter::Filters;
use crate::input::{self, open_sources, FollowSource};
use crate::model::{Entry, ExportFormat, Level, RedactRules};
use crate::output::{self, ColorMode, Expansion, FormatOptions, Formatter, OutputTz};
use crate::query;
use chrono::{DateTime, Local, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Duration;

pub fn run(args: Args) -> Result<(), Error> {
    // Load declarative config (CLI flags override config values).
    let config = Config::load(args.config.as_deref())?;
    let config = config.as_ref();

    if args.tui {
        return crate::tui::run(args, config);
    }

    // Resolve effective configuration: CLI flag wins, otherwise config value.
    let input_format = Some(args.input_format.as_str())
        .or(config.and_then(|c| c.input_format.as_deref()))
        .unwrap_or("auto")
        .to_string();

    let level = match &args.level {
        Some(text) => Some(
            Level::parse(text).ok_or_else(|| Error::usage(format!("invalid level '{text}'")))?,
        ),
        None => config
            .and_then(|c| c.level.as_deref())
            .and_then(Level::parse),
    };

    let now = Utc::now();
    let since = resolve_time(
        args.since.as_deref(),
        config.and_then(|c| c.since.as_deref()),
        now,
        "since",
    )?;
    let until = resolve_time(
        args.until.as_deref(),
        config.and_then(|c| c.until.as_deref()),
        now,
        "until",
    )?;

    let mut filters_exprs = args.filters.clone();
    if let Some(config_filters) = config.and_then(|c| c.filters.as_deref()) {
        filters_exprs.extend(config_filters.iter().cloned());
    }
    let mut queries_exprs = args.queries.clone();
    if let Some(config_queries) = config.and_then(|c| c.queries.as_deref()) {
        queries_exprs.extend(config_queries.iter().cloned());
    }

    let mut exprs = parse_exprs(&filters_exprs, &queries_exprs)?;

    let logs_from_stdin = args.files.is_empty() || args.files.iter().any(|p| p == Path::new("-"));
    query::resolve_sets(&mut exprs, logs_from_stdin)?;

    let filters = Filters::new(level, since, until, exprs);

    // Compile PII redaction rules.
    let redact_rules = config.map(|c| c.redact_matchers()).transpose()?;
    let redact_rules = RedactRules::new(redact_rules.unwrap_or_default());

    // Compile alert rules.
    let alert_specs = config.map(|c| c.alert_queries()).unwrap_or_default();

    let output_format = Some(args.output_format.as_str())
        .or(config.and_then(|c| c.output_format.as_deref()))
        .unwrap_or("formatted")
        .to_string();
    let export_format = ExportFormat::parse(&output_format)?;

    let timezone = match args.time_zone.as_deref() {
        Some(text) => OutputTz::parse(Some(text), args.local)?,
        None => match config.and_then(|c| c.time_zone.as_deref()) {
            Some(_) => OutputTz::Utc,
            None => OutputTz::parse(args.time_zone.as_deref(), args.local).unwrap_or(OutputTz::Utc),
        },
    };

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
        expansion: Expansion::parse(&args.expansion)?,
        raw: args.raw,
        trailing,
    })?;

    // Follow mode: tail new lines.
    if args.follow || config.and_then(|c| c.follow).unwrap_or(false) {
        return run_follow(
            &args.files,
            &filters,
            &redact_rules,
            &alert_specs,
            &formatter,
            export_format,
            args.output.as_deref(),
            args.max_entry_size.clone(),
            delimiter_for(&args.delimiter),
            config.and_then(|c| c.tui.as_ref()),
        );
    }

    // Batch mode.
    let mut writer = output::open_output(args.output.as_deref())?;

    let mut entries: Vec<Entry> = Vec::new();
    let mut total = 0u64;
    let mut invalid = 0u64;
    let mut oversized = 0u64;

    let sources = open_sources(&args.files)?;
    let max_entry_size = parse_size(&args.max_entry_size).ok_or_else(|| {
        Error::usage(format!(
            "invalid maximum entry size '{}'",
            args.max_entry_size
        ))
    })?;

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

            let Some(mut entry) = crate::parser::parse_entry(
                &raw,
                &source.name,
                line_number,
                &input_format,
                args.allow_prefix,
            ) else {
                invalid += 1;
                continue;
            };

            // Apply PII redaction.
            entry.redact(&redact_rules);

            if !filters.matches(&entry) {
                continue;
            }

            entries.push(entry);
        }
    }

    // Evaluate alert rules against matched entries.
    let alert_result = evaluate_alerts(&alert_specs, &entries);

    // Sort if requested.
    if args.sort || config.and_then(|c| c.sort).unwrap_or(false) {
        entries.sort_by(
            |left, right| match (left.timestamp.as_ref(), right.timestamp.as_ref()) {
                (Some(left), Some(right)) => left.cmp(right),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            },
        );
    }

    // Write output in the requested export format.
    match export_format {
        ExportFormat::Formatted | ExportFormat::Raw => {
            for entry in &entries {
                formatter.write(entry, writer.as_mut())?;
            }
            writer.flush()?;
        }
        ExportFormat::Json => {
            for entry in &entries {
                let json = serde_json::to_string(entry)
                    .map_err(|err| Error::query(format!("failed to serialize entry: {err}")))?;
                writer.write_all(json.as_bytes())?;
                writer.write_all(b"\n")?;
            }
            writer.flush()?;
        }
        ExportFormat::Csv => {
            write_csv(&mut writer, &entries)?;
        }
        ExportFormat::Prometheus => {
            write_prometheus(&mut writer, &entries, "logcli")?;
        }
    }

    if args.verbose && !args.silent {
        let matched = entries.len() as u64;
        eprintln!(
            "logcli: {total} entries read, {matched} matched, {invalid} invalid, {oversized} oversized"
        );
    }

    // Exit with alert code if any alert triggered.
    if let Some(exit_code) = alert_result.exit_code {
        if !alert_result.triggered.is_empty() {
            for (name, count) in &alert_result.triggered {
                eprintln!(
                    "logcli: alert: {} triggered ({} entries matched)",
                    name, count
                );
            }
            return Err(Error::usage(format!(
                "alert threshold exceeded (exit {exit_code})"
            )));
        }
    }

    Ok(())
}

/// Load and filter all entries from the given args, returning them in file order.
pub fn load_entries(args: &Args) -> Result<Vec<Entry>, Error> {
    let input_format = args.input_format.as_str();

    let mut exprs = args
        .filters
        .iter()
        .map(|raw| query::parse_filter(raw))
        .collect::<Result<Vec<_>, _>>()?;

    for raw in &args.queries {
        exprs.push(
            query::parse(raw).map_err(|error| Error::query(query::format_error(raw, &error)))?,
        );
    }

    let logs_from_stdin = args.files.is_empty() || args.files.iter().any(|p| p == Path::new("-"));
    query::resolve_sets(&mut exprs, logs_from_stdin)?;

    let level = match &args.level {
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

    let filters = Filters::new(level, since, until, exprs);

    let sources = open_sources(&args.files)?;
    let max_entry_size = parse_size(&args.max_entry_size).ok_or_else(|| {
        Error::usage(format!(
            "invalid maximum entry size '{}'",
            args.max_entry_size
        ))
    })?;

    let mut entries: Vec<Entry> = Vec::new();

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
                continue;
            }

            let raw = String::from_utf8_lossy(&buffer);

            if let Some(entry) = crate::parser::parse_entry(
                &raw,
                &source.name,
                line_number,
                input_format,
                args.allow_prefix,
            ) {
                if filters.matches(&entry) {
                    entries.push(entry);
                }
            }
        }
    }

    Ok(entries)
}

/// Follow (tail) mode: watch files for new lines and emit them as they appear.
#[allow(clippy::too_many_arguments)]
fn run_follow(
    files: &[std::path::PathBuf],
    filters: &Filters,
    redact_rules: &RedactRules,
    _alert_specs: &[(String, u64, i32, String)],
    formatter: &Formatter,
    export_format: ExportFormat,
    output: Option<&Path>,
    max_entry_size: String,
    delimiter: u8,
    _tui_config: Option<&crate::config::TuiConfig>,
) -> Result<(), Error> {
    let mut sources: Vec<FollowSource> = Vec::new();
    if files.is_empty() {
        return Err(Error::usage(
            "follow mode with stdin is not supported; pipe stdin into batch mode or follow a file",
        ));
    }

    for path in files {
        let source = FollowSource::open(path, delimiter)?;
        sources.push(source);
    }

    let mut writer = output::open_output(output)?;

    loop {
        let mut any_new = false;
        for source in &mut sources {
            if let Some(lines) = source.poll_new_lines(Duration::from_millis(200))? {
                any_new = true;
                for line in lines {
                    if line.len() > max_entry_size.parse::<usize>().unwrap_or(usize::MAX) {
                        continue;
                    }
                    let raw = line.as_str();
                    let source_name = source.name().to_string();
                    if let Some(mut entry) =
                        crate::parser::parse_entry(raw, &source_name, 0, "auto", false)
                    {
                        entry.redact(redact_rules);
                        if filters.matches(&entry) {
                            match export_format {
                                ExportFormat::Formatted | ExportFormat::Raw => {
                                    formatter.write(&entry, writer.as_mut())?;
                                }
                                ExportFormat::Json => {
                                    let json = serde_json::to_string(&entry).map_err(|err| {
                                        Error::query(format!("failed to serialize entry: {err}"))
                                    })?;
                                    writer.write_all(json.as_bytes())?;
                                    writer.write_all(b"\n")?;
                                }
                                ExportFormat::Csv => {
                                    write_csv_row(&mut writer, &entry)?;
                                }
                                ExportFormat::Prometheus => {
                                    write_prometheus_row(&mut writer, &entry, "logcli")?;
                                }
                            }
                            writer.flush()?;
                        }
                    }
                }
            }
        }

        if !any_new {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// Follow stdin until EOF, then exit.
#[allow(dead_code)]
fn follow_stdin(
    _filters: &Filters,
    _redact_rules: &RedactRules,
    _alert_specs: &[(String, u64, i32, String)],
    _formatter: &Formatter,
    _export_format: ExportFormat,
    _output: Option<&Path>,
) -> Result<(), Error> {
    Err(Error::usage(
        "follow mode with stdin is not supported; pipe stdin into batch mode or follow a file",
    ))
}

fn resolve_time(
    cli: Option<&str>,
    config: Option<&str>,
    now: DateTime<Utc>,
    name: &str,
) -> Result<Option<DateTime<Utc>>, Error> {
    let text = cli.or(config);
    match text {
        Some(text) => {
            let parsed = parse_time(text, now)
                .map_err(|_| Error::usage(format!("invalid {name} '{text}'")))?;
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}

fn parse_exprs(filters: &[String], queries: &[String]) -> Result<Vec<crate::query::Expr>, Error> {
    let mut exprs = Vec::new();
    for raw in filters {
        exprs.push(query::parse_filter(raw)?);
    }
    for raw in queries {
        exprs.push(
            query::parse(raw).map_err(|error| Error::query(query::format_error(raw, &error)))?,
        );
    }
    Ok(exprs)
}

fn delimiter_for(delim: &str) -> u8 {
    match delim {
        "lf" | "crlf" => b'\n',
        "cr" => b'\r',
        "nul" => 0,
        _ => b'\n',
    }
}

fn evaluate_alerts(specs: &[(String, u64, i32, String)], entries: &[Entry]) -> AlertResult {
    let mut triggered = Vec::new();
    let mut exit_code = None;

    for (name, threshold, code, query_text) in specs {
        let expr = match query::parse(query_text) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let count = entries.iter().filter(|e| expr.matches(e)).count() as u64;
        if count >= *threshold {
            triggered.push((name.clone(), count));
            exit_code = Some(exit_code.unwrap_or(*code).max(*code));
        }
    }

    AlertResult {
        triggered,
        exit_code,
    }
}

fn write_csv<W: Write>(writer: &mut W, entries: &[Entry]) -> Result<(), Error> {
    // Header: collect all flattened keys.
    let mut keys: BTreeMap<String, bool> = BTreeMap::new();
    for entry in entries {
        for (k, _) in entry.flatten() {
            keys.insert(k, true);
        }
    }
    let key_list: Vec<&str> = keys.keys().map(|s| s.as_str()).collect();
    writer.write_all(key_list.join(",").as_bytes())?;
    writer.write_all(b"\n")?;

    for entry in entries {
        let pairs: BTreeMap<String, String> = entry.flatten().into_iter().collect();
        let mut row: Vec<String> = Vec::new();
        for key in &key_list {
            let val = pairs.get(*key).map(|v| escape_csv(v)).unwrap_or_default();
            row.push(val);
        }
        writer.write_all(row.join(",").as_bytes())?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

fn write_csv_row<W: Write + ?Sized>(writer: &mut W, entry: &Entry) -> Result<(), Error> {
    let pairs: BTreeMap<String, String> = entry.flatten().into_iter().collect();
    let mut row: Vec<String> = Vec::new();
    for val in pairs.values() {
        row.push(escape_csv(val));
    }
    writer.write_all(row.join(",").as_bytes())?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn write_prometheus<W: Write>(
    writer: &mut W,
    entries: &[Entry],
    metric_name: &str,
) -> Result<(), Error> {
    for entry in entries {
        for line in entry.prometheus_lines(metric_name) {
            writer.write_all(line.as_bytes())?;
            writer.write_all(b"\n")?;
        }
    }
    Ok(())
}

fn write_prometheus_row<W: Write + ?Sized>(
    writer: &mut W,
    entry: &Entry,
    metric_name: &str,
) -> Result<(), Error> {
    for line in entry.prometheus_lines(metric_name) {
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

#[allow(dead_code)]
fn validate_input_format(format: &str) -> Result<(), Error> {
    match format {
        "auto" | "json" | "logfmt" | "plain" | "otel" => Ok(()),
        other => Err(Error::usage(format!(
            "invalid input format '{other}' (expected auto, json, logfmt, plain, or otel)"
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
            "s" => Duration::from_secs(amount as u64),
            "m" => Duration::from_secs(amount as u64 * 60),
            "h" => Duration::from_secs(amount as u64 * 3600),
            "d" => Duration::from_secs(amount as u64 * 86400),
            "w" => Duration::from_secs(amount as u64 * 604800),
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
        assert_eq!(
            parse_time("-3h", now).unwrap(),
            now - Duration::from_secs(3 * 3600)
        );
        assert_eq!(
            parse_time("-14d", now).unwrap(),
            now - Duration::from_secs(14 * 86400)
        );
        assert!(parse_time("garbage", now).is_err());
    }

    #[test]
    fn parses_absolute_times() {
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap();
        let parsed = parse_time("2026-10-28T10:00:00Z", now).unwrap();
        assert_eq!(parsed.timestamp(), 1_793_181_600);
        assert!(parse_time("2026-13-40", now).is_err());
    }

    #[test]
    fn escape_csv_handles_special_chars() {
        assert_eq!(escape_csv("hello"), "hello");
        assert_eq!(escape_csv("a,b"), "\"a,b\"");
        assert_eq!(escape_csv("a\"b"), "\"a\"\"b\"");
        assert_eq!(escape_csv("a\nb"), "\"a\nb\"");
    }
}
