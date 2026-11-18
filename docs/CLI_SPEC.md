# CLI Specification — `logcli`

**Status:** Draft v0.2
**Related:** [PRD.md](PRD.md) · [QUERY_SPEC.md](QUERY_SPEC.md) · [CONFIG.md](CONFIG.md) · [CONTRIBUTING.md](../CONTRIBUTING.md)

This document is the contract for the command-line interface. Any behavior not described here is unspecified. All examples in this file must be covered by integration tests (see [TEST_PLAN.md](TEST_PLAN.md)).

---

## 1. Invocation Model

`logcli` uses a **single command with flags**, not subcommands. This mirrors `hl` and keeps pipes natural.

```
logcli [OPTIONS] [FILE]...
```

- With no `FILE` and a TTY stdin: auto-launch TUI (`--tui`).
- With no `FILE` and non-TTY stdin: read stdin in batch mode.
- `-` as a file name means stdin.
- Multiple files are processed in order given unless `--sort` or `--follow` is used.
- Config files are auto-discovered (see [CONFIG.md](CONFIG.md)) unless `--config` disables or overrides.

**Reserved for future use:** subcommands (`stats`, `completion` if moved out of flags).

---

## 2. Exit Codes

| Code | Meaning |
|---|---|
| `0` | Success, including zero matched entries |
| `1` | Usage error (bad flag, invalid query, invalid config) |
| `2` | I/O error (file missing, permission denied, decompression failure) |
| `3` | Broken pipe (SIGPIPE equivalent); silent, no error message |
| `130` | Interrupted by Ctrl-C |
| `N` (configured) | Alert threshold breach (exit code from `alerts:` config rule) |

Rules:
- Filtering that yields no results is **not** an error.
- Malformed individual log lines are **not** fatal; they are counted and skipped.
- Alert breach exits with the rule's `exit_code` (default 2).

---

## 3. Global Options

| Flag | Value | Default | Description |
|---|---|---|---|
| `--config <FILE>` | path | auto | Config file(s), repeatable. `-` disables config loading. |
| `--shell-completions <SHELL>` | bash/zsh/fish/powershell/elvish | | Print completion script and exit. |
| `--man-page` | | | Print man page and exit. (P2) |
| `-V, --version` | | | Print version and exit. |
| `--help[=short\|long]` | | `short` | Print help. |

### Environment variables

CLI options map to env vars with a `LOGCLI_` prefix (see [CONFIG.md](CONFIG.md)). CLI flags always win.

---

## 4. Input Options

| Flag | Value | Default | Description |
|---|---|---|---|
| `--input-format <F>` | `auto` \| `json` \| `logfmt` \| `plain` \| `otel` | `auto` | Force a parser. `auto` detects per line. `otel` parses OpenTelemetry OTLP JSON. |
| `--delimiter <D>` | `auto` \| `lf` \| `crlf` \| `cr` \| `nul` | `auto` | Entry delimiter. `auto` sniffs the first 64 KiB. |
| `--allow-prefix` | | off | Allow non-JSON text before a JSON object. |
| `--unix-timestamp-unit <U>` | `auto` \| `s` \| `ms` \| `us` \| `ns` | `auto` | How to interpret numeric timestamps. |
| `--max-entry-size <SIZE>` | size string | `64 MiB` | Reject/skip lines larger than this. |
| `--buffer-size <SIZE>` | size string | `256 KiB` | Read buffer size. |

**Size strings:** `512`, `10KB`, `256KiB`, `2MiB`, `1GiB` (case-insensitive, `KB=1000`, `KiB=1024`).

---

## 5. Filtering Options

| Flag | Value | Default | Description |
|---|---|---|---|
| `-l, --level <LEVEL>` | `t`/`d`/`i`/`w`/`e`/`f` or full name | none | Minimum level (inclusive). |
| `--since <TIME>` | time expression | none | Timestamp >= TIME. |
| `--until <TIME>` | time expression | none | Timestamp <= TIME. |
| `-f, --filter <EXPR>` | field expression | none | Repeatable; multiple filters are ANDed. |
| `-q, --query <EXPR>` | boolean query | none | Repeatable; multiple queries are ANDed. |
| `--strict` | | off | Treat malformed lines as fatal. (P2) |

### 5.1 Level values

Accepted (case-insensitive): `trace, debug, info, warn, warning, error, fatal, panic`, plus one-letter aliases `t d i w e f`. Numeric levels accepted when the source uses them. OTel `SeverityNumber` 1-24 is mapped per OpenTelemetry spec.

**Semantics:** threshold, not set membership. `-l w` = warn + error + fatal + panic.

### 5.2 Time expressions

Accepted forms:
- RFC3339: `2026-10-28T12:00:00Z`, `2026-10-28T12:00:00+05:30`
- Date/time in `--time-format`: `2026-10-28 12:00:00.123`
- Keywords: `now`, `today`, `yesterday`, `monday` … `sunday` (most recent past occurrence, local time)
- Relative: `-3h`, `-14d`, `-30m`, `-10s`, `-2w`
- Bare clock time: `18:00` (today, or yesterday if in the future)

Invalid time strings → exit `1` with a message naming the offending value.

### 5.3 Field filter grammar (summary)

| Form | Meaning |
|---|---|
| `key=value` | equals |
| `key!=value` | not equals (field must exist) |
| `key~=substr` | contains substring |
| `key!~=substr` | does not contain (field must exist) |
| `key~~=regex` | regex match |
| `key!~~=regex` | regex non-match |
| `key?=value` | equals, or field absent |
| `key?!=value` | not equals, or field absent |
| `.key` | address a source field named like a reserved word (`level`, `message`) |
| `"quoted.key"` | exact field name |

Full grammar and semantics: [QUERY_SPEC.md](QUERY_SPEC.md).

---

## 6. Output Options

| Flag | Value | Default | Description |
|---|---|---|---|
| `--color [WHEN]` | `auto` \| `always` \| `never` | `auto` | Color mode. |
| `-c` | | | Alias for `--color=always`. |
| `--theme <NAME>` | theme name | `default` | Color theme (`default`, `monokai`, `solarized`). |
| `-h, --hide <KEY>` | field key, `!key`, `*`, `!*` | none | Repeatable. `!` reveals. |
| `--flatten <WHEN>` | `always` \| `never` | `always` | Flatten nested objects into dotted keys. |
| `-t, --time-format <F>` | strftime-like | `%Y-%m-%d %T.%3N` | Timestamp format. |
| `-Z, --time-zone <TZ>` | IANA name | `UTC` | Output timezone. |
| `-L, --local` | | off | Use local timezone. |
| `-e, --hide-empty-fields` | | off | Hide null/empty values. |
| `-E, --show-empty-fields` | | off | Override `-e`. |
| `-x, --expansion [MODE]` | `never` \| `inline` \| `auto` \| `always` | `auto` | Multi-line handling. |
| `--ascii [WHEN]` | `auto` \| `always` \| `never` | `auto` | ASCII-only punctuation. |
| `--input-info <LAYOUT>` | `auto` \| `none` \| `minimal` \| `compact` \| `full` | `auto` | Show file/line info. |
| `-o, --output <FILE>` | path | stdout | Write here. `-` = stdout. |
| `--output-format <FORMAT>` | `formatted` \| `raw` \| `json` \| `csv` \| `prometheus` | `formatted` | Export format. |
| `--output-delimiter <D>` | `newline` \| `nul` | `newline` | Entry terminator. |
| `-r, --raw` | | off | Emit raw source lines (matched entries). |
| `--raw-fields` | | off | Do not unescape/prettify field values. |
| `--paging <WHEN>` | `auto` \| `always` \| `never` | `auto` | Pager control. |
| `-P` | | | Alias for `--paging=never`. |

**Color auto rule:** color if stdout is a TTY and `NO_COLOR` is unset/empty and `TERM != "dumb"`. `CLICOLOR=0` respected. `--color=always` overrides `NO_COLOR`.

**Pager auto rule:** invoke pager only when stdout is a TTY, output is not a file, and paging != `never`. Pager command from `LOGCLI_PAGER`, then `PAGER`, then `less -R` (Windows: `less -R` if on `PATH`, else no pager).

---

## 7. Output Formats

| Format | Flag | Best for |
|---|---|---|
| `formatted` (default) | `-o out.txt` | Human reading in terminal |
| `raw` | `--output-format raw -o out.log` | Exact source passthrough |
| `json` | `--output-format json -o out.jsonl` | Downstream JSON tools (JSON Lines) |
| `csv` | `--output-format csv -o report.csv` | Spreadsheets / Excel |
| `prometheus` | `--output-format prometheus -o metrics.prom` | Prometheus scrape targets |

### 7.1 CSV format

Each entry is flattened into dotted keys (`fields.request.method`, `timestamp`, `level`, `message`, `logger`). Header row lists all keys found across matched entries. Values containing `,`, `"`, newlines, or CR are quoted per RFC 4180.

### 7.2 Prometheus format

Each matched entry emits:
```
logcli_total{service="api",namespace="prod"} 1 <timestamp_ms>
```
Labels are taken from fields `service`, `namespace`, `environment`, `component`. Timestamp is milliseconds since epoch (or 0 if no timestamp).

---

## 8. TUI Options

| Flag | Value | Default | Description |
|---|---|---|---|
| `--tui` | | off (auto if no files + TTY stdin) | Launch interactive TUI dashboard. |
| `--follow` \| `-F` | | off | Tail new lines in files (implies sorted output). |

### TUI keybindings

| Key | Action |
|-----|--------|
| `j`/`k` | Scroll up/down |
| `g`/`G` | Go to top/bottom |
| `/` | Open filter prompt |
| `Esc` | Clear the active filter |
| `d`/`Enter` | Focus the details pane (scroll with `j`/`k`) |
| `o` | Add a log file (enter path) |
| `s` | Cycle sort (none / time / level) |
| `f`/`F` | Toggle follow mode |
| `h` | Toggle histogram view |
| `?` | Show help |
| `q` | Quit |

Layout: boxed header (`logcli` version, source, ERRORS/WARNs/INFO counts);
entries table (`TIMESTAMP`, `LEVEL`, `MESSAGE`) with an always-visible JSON
details pane to its right; filter/status box beneath the table; key-hint bar
with the current `Matches: filtered/total` count at the bottom right.

Rules:
- `--sort` and `--follow` are mutually exclusive in batch mode; `-F` implies sorting.
- `-F` implies `--paging=never`.
- Entries without a parseable timestamp are placed at the end in sort mode.

---

## 9. PII Redaction & Alert Rules (via config)

PII redaction and alert rules are configured in the config file (see [CONFIG.md](CONFIG.md)), not via CLI flags. Example:

```yaml
redact:
  - field: "message"
    pattern: "[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}"
    replace: "[EMAIL]"
alerts:
  - name: "high_error_rate"
    query: "level = error and status >= 500"
    threshold: 10
    exit_code: 2
```

---

## 10. Advanced Options

| Flag | Value | Default | Description |
|---|---|---|---|
| `-C, --concurrency <N>` | integer | CPU count | Worker threads for parsing. |
| `--interrupt-ignore-count <N>` | integer | `3` | Ctrl-C presses ignored before exit. |
| `--list-themes[=TAGS]` | tags | | List themes; tags: `dark,light,16color,256color,truecolor,base,overlay`. |
| `-v, --verbose` | | off | Diagnostics to stderr (skipped lines, timings). |
| `--silent` | | off | Suppress non-fatal warnings and summary. |

> **Note:** `-q` is reserved for `--query` (matching `hl`). Quiet mode uses `--silent` only.

---

## 11. Behavior Details

### 11.1 stdin
- `logcli` with piped stdin reads until EOF.
- In follow mode, stdin input is supported; EOF ends that source only if it is not a file being watched.
- Broken pipe (reader exits early) → exit `3`, no stack trace, no error text.

### 11.2 Multiple files
- Output preserves per-file order unless `--sort`.
- `--input-info` prefixes entries when more than one input is active.
- A missing file is a fatal I/O error (exit `2`).

### 11.3 Malformed lines
- Skipped by default; count printed to stderr only with `--verbose`.
- `--allow-prefix` increases successful parse rate for prefixed logs.
- A line exceeding `--max-entry-size` is skipped with a warning.

### 11.4 Timezone
- Timestamps are normalized to UTC internally.
- Output defaults to UTC; `-L` switches to local; `-Z` accepts IANA names such as `Europe/Berlin`.
- Invalid timezone → exit `1`.

### 11.5 Color and themes
- Themes only affect TTY-formatted output; raw mode is never colorized.
- `--theme` with unknown name → exit `1`, listing nearest matches.

### 11.6 Piping guarantees
- When stdout is not a TTY, defaults change: no pager, no color, `--input-info=none` unless explicitly set.
- Output is line-buffered when TTY, block-buffered otherwise.
- No ANSI codes ever when `--color=never`.

---

## 12. Examples (normative)

```sh
# Read a file (pager auto on TTY)
logcli app.log

# Read compressed logs (gzip)
logcli app.log.gz

# Only errors and above
logcli -l e app.log

# Filter by a nested field
logcli -f request.method=GET app.log

# Exclude health checks
logcli -f 'path!~=health' access.log

# Complex query
logcli -q 'level > info or status >= 500' app.log

# Time window
logcli --since -3h --until now app.log

# Merge two files chronologically, errors only
logcli -s -l w api.log worker.log

# Live follow with preload
logcli -F --tail 200 app.log

# Pipe from kubectl
kubectl logs -f deploy/api | logcli -P -l w

# Hide noisy fields
logcli -h request_id -h '!trace_id' app.log

# Raw matched lines only (for further processing)
logcli -r -f component=tsdb app.log | grep foo

# JSON array field access
logcli -f 'span.[].name=sp0001' trace.log

# Preserve only provider field
logcli --hide '*' --hide '!provider' app.log

# OpenTelemetry OTLP logs
logcli --input-format otel otel.log

# Export to Prometheus for scraping
logcli --output-format prometheus -o metrics.prom app.log

# Export to CSV for spreadsheets
logcli --output-format csv -o report.csv app.log

# Interactive TUI dashboard
logcli --tui app.log

# Follow a live log file
logcli -F -l error app.log

# Shell completions
logcli --shell-completions bash > /etc/bash_completion.d/logcli
```

---

## 13. Error Message Format

All errors go to stderr, prefixed with `logcli: error: `. Usage errors are followed by a hint:

```
logcli: error: invalid value 'warnn' for '--level <LEVEL>'
  tip: expected one of trace, debug, info, warn, error, fatal
```

I/O errors name the file and OS error:

```
logcli: error: cannot open 'missing.log': No such file or directory (os error 2)
```

Query parse errors include a caret position:

```
logcli: error: invalid query at column 12: expected ')' after expression
  level > info and (status >=
                    ^
```

Alert breach:

```
logcli: alert: high_error_rate triggered (12 entries matched)
logcli: error: alert threshold exceeded (exit 2)
```