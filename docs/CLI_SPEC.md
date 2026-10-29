# CLI Specification — `logcli`

**Status:** Draft v0.1
**Related:** [PRD.md](PRD.md) · [QUERY_SPEC.md](QUERY_SPEC.md) · [CONFIG.md](CONFIG.md)

This document is the contract for the command-line interface. Any behavior not described here is unspecified. All examples in this file must be covered by integration tests (see [TEST_PLAN.md](TEST_PLAN.md)).

---

## 1. Invocation Model

`logcli` uses a **single command with flags**, not subcommands. This mirrors `hl` and keeps pipes natural.

```
logcli [OPTIONS] [FILE]...
```

- With no `FILE` and a TTY stdin: print help and exit `0`.
- With no `FILE` and non-TTY stdin: read stdin.
- `-` as a file name means stdin.
- Multiple files are processed in the order given unless `--sort` or `--follow` is used.

**Reserved for future use:** subcommands (`stats`, `completion` if moved out of flags). If a first positional argument matches a reserved word, it is still treated as a file path in v1; document this.

---

## 2. Exit Codes

| Code | Meaning |
|---|---|
| `0` | Success, including zero matched entries |
| `1` | Usage error (bad flag, invalid query, missing value) |
| `2` | I/O error (file missing, permission denied, decompression failure) |
| `3` | Broken pipe (SIGPIPE equivalent); silent, no error message |
| `130` | Interrupted by Ctrl-C |

Rules:
- Filtering that yields no results is **not** an error.
- Malformed individual log lines are **not** fatal; they are counted and skipped.
- `--strict` (P2) would turn parse warnings into exit `2`.

---

## 3. Global Options

| Flag | Value | Default | Description |
|---|---|---|---|
| `--config <FILE>` | path | auto | Config file(s), repeatable. `-` disables config loading. |
| `-V, --version` | | | Print version and exit. |
| `--help[=short\|long]` | | `short` | Print help. |
| `--shell-completions <SHELL>` | bash/zsh/fish/powershell/elvish | | Print completion script and exit. (P2) |
| `--man-page` | | | Print man page and exit. (P2) |

### Environment variables

CLI options map to env vars with a `LOGCLI_` prefix (see [CONFIG.md](CONFIG.md)). CLI flags always win.

---

## 4. Input Options

| Flag | Value | Default | Description |
|---|---|---|---|
| `--input-format <F>` | `auto` \| `json` \| `logfmt` | `auto` | Force a parser. `auto` detects per line. |
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

Accepted (case-insensitive): `trace, debug, info, warn, warning, error, fatal, panic`, plus one-letter aliases `t d i w e f`. Numeric levels accepted when the source uses them.

**Semantics:** threshold, not set membership. `-l w` = warn + error + fatal.

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
| `--theme <NAME>` | theme name | `uni` | Color theme. |
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
| `--output-delimiter <D>` | `newline` \| `nul` | `newline` | Entry terminator. |
| `-r, --raw` | | off | Emit raw source lines (matched entries). |
| `--raw-fields` | | off | Do not unescape/prettify field values. |
| `--paging <WHEN>` | `auto` \| `always` \| `never` | `auto` | Pager control. |
| `-P` | | | Alias for `--paging=never`. |

**Color auto rule:** color if stdout is a TTY and `NO_COLOR` is unset/empty and `TERM != "dumb"`. `CLICOLOR=0` respected. `--color=always` still honors `NO_COLOR`? **No** — explicit `always` overrides `NO_COLOR`; document this.

**Pager auto rule:** invoke pager only when stdout is a TTY, output is not a file, and paging != `never`. Pager command from `LOGCLI_PAGER`, then `PAGER`, then `less -R` (Windows: `less -R` if present, else no pager).

---

## 7. Sorting & Follow

| Flag | Value | Default | Description |
|---|---|---|---|
| `-s, --sort` | | off | Chronological merge of all inputs. |
| `-F, --follow` | | off | Follow inputs; implies sorted output. |
| `--tail <N>` | integer | `10` | Preload last N entries per file in follow mode. |
| `--sync-interval-ms <MS>` | integer | `100` | Merge window for follow mode. |
| `--index-dir <PATH>` | path | OS cache dir | Where sort indexes live. |
| `--no-index` | | off | In-memory sort only (bounded by `--max-memory`). |
| `--dump-index` | | off | Print index metadata and exit (debug). |

Rules:
- `--sort` and `--follow` are mutually exclusive; `-F` implies sorting.
- `-F` implies `--paging=never`.
- Entries without a parseable timestamp are placed at the end in sort mode and counted in the summary shown to stderr when `--verbose`.

---

## 8. Advanced Options

| Flag | Value | Default | Description |
|---|---|---|---|
| `-C, --concurrency <N>` | integer | CPU count | Worker threads for parsing. |
| `--interrupt-ignore-count <N>` | integer | `3` | Ctrl-C presses ignored before exit. |
| `--list-themes[=TAGS]` | tags | | List themes; tags: `dark,light,16color,256color,truecolor,base,overlay`. |
| `-v, --verbose` | | off | Diagnostics to stderr (skipped lines, timings). |
| `-q, --quiet` | | off | Suppress non-fatal warnings. (Careful: `-q` is query! Use `--silent` instead.) |

> **Conflict resolved:** `-q` is reserved for `--query` (matching hl). Quiet mode uses `--silent` only. This avoids an ambiguity that exists in hl.

---

## 9. Behavior Details

### 9.1 stdin
- `logcli` with piped stdin reads until EOF.
- In follow mode, stdin input is supported; EOF ends that source only if it is not a file being watched.
- Broken pipe (reader exits early) → exit `3`, no stack trace, no error text.

### 9.2 Multiple files
- Output preserves per-file order unless `--sort`.
- `--input-info` prefixes entries when more than one input is active.
- A missing file is a fatal I/O error (exit `2`) unless `--ignore-missing` (P2).

### 9.3 Malformed lines
- Skipped by default; count printed to stderr only with `--verbose`.
- `--allow-prefix` increases successful parse rate for prefixed logs.
- A line exceeding `--max-entry-size` is skipped with a warning.

### 9.4 Timezone
- Timestamps are normalized to UTC internally.
- Output defaults to UTC; `-L` switches to local; `-Z` accepts IANA names such as `Europe/Berlin`.
- Invalid timezone → exit `1`.

### 9.5 Color and themes
- Themes only affect TTY-formatted output; raw mode is never colorized.
- `--theme` with unknown name → exit `1`, listing nearest matches.

### 9.6 Piping guarantees
- When stdout is not a TTY, defaults change: no pager, no color, `--input-info=none` unless explicitly set.
- Output is line-buffered when TTY, block-buffered otherwise.
- No ANSI codes ever when `--color=never`.

---

## 10. Examples (normative)

```sh
# Read a file (pager auto on TTY)
logcli app.log

# Read compressed logs
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

# Live follow with 200-line preload
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
```

---

## 11. Error Message Format

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
