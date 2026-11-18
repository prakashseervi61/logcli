# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added — v0.2.0 next-level features

- **Declarative configuration** (`docs/CONFIG.md`):
  - Auto-discovery of `logcli.yaml`/`logcli.yml`/`logcli.json` in CWD, user profile, and system-wide locations.
  - `--config` flag to override; CLI flags always win over config values.
  - Environment variables with `LOGCLI_*` prefix.
  - JSON-only config format (YAML detection without `serde_yaml` rejected with a clear error).
- **PII redaction** (`redact:` in config):
  - Declarative rules: `field` + `pattern` (regex) + `replace` (default `[REDACTED]`).
  - Applied to `message`, `logger`, `caller`, and all string values in `fields`.
  - `*` field name redacts all string fields recursively.
  - `Entry.redacted` flag ensures redaction is applied at most once per entry.
- **Alert rules** (`alerts:` in config):
  - Declarative rules: `name` + `query` (same syntax as `-q`) + `threshold` + `exit_code`.
  - Evaluated after filtering; exit with configured code on threshold breach.
  - Alert summary printed to stderr before exit.
- **Multi-format export** (`--output-format`):
  - `formatted` (default), `raw`, `json` (JSON Lines), `csv` (flattened dotted keys, RFC 4180), `prometheus` (text exposition format).
  - CSV header collects all keys across matched entries; special characters quoted.
  - Prometheus emits `logcli_total{label="..."} 1 <ts_ms>` per entry with `service`/`namespace`/`environment`/`component` labels.
- **OpenTelemetry OTLP parsing** (`--input-format otel`):
  - Parses `resourceLogs` → `scopeLogs` → `logRecords` structure.
  - Maps `timeUnixNano` → timestamp, `severityText` → level, `severityNumber` → `fields.severity_number`, `body.stringValue` → message, resource/scope/record attributes → fields.
  - SeverityNumber 1-24 mapped to Level per OTel spec.
  - First log record per line emitted (batching noted as known limitation).
- **Interactive TUI enhancements**:
  - Reference-style dashboard: boxed header with source + ERRORS/WARNs/INFO counts,
    entries table (TIMESTAMP/LEVEL/MESSAGE), always-visible syntax-colored JSON
    details pane, filter/status box under the table, and key-hint bar with
    `Matches: filtered/total`.
  - Histogram view (`h` key).
  - Config integration (`tui.sort`, `tui.follow`, `tui.theme`).
  - Follow mode polling via `FollowSource` (200ms interval).
- **Shell completions** (`--shell-completions bash|zsh|fish|powershell|elvish`) — flag accepted.
- **Pager integration** (`--paging auto|always|never`, `-P`) — flag accepted.
- **Theming** (`--theme default|monokai|solarized`) — flag accepted.
- **Native Windows `logcli.exe`** cross-compiled and installed at `%USERPROFILE%\bin`.

### Changed
- Project direction updated to follow the architecture and UX of [hl](https://github.com/pamburus/hl) and [lnav](https://github.com/tstack/lnav): Rust, streaming pipeline, JSON/logfmt/OTLP focus, query language, PII redaction, alert rules.
- README rewritten with feature comparison table, config example, TUI keybindings, output formats table, installation instructions.
- All normative docs (CLI_SPEC, PARSING_SPEC, QUERY_SPEC, CONFIG, DESIGN, PRD, TEST_PLAN, PLAN) updated for v0.2.0 features.

### Known limitations
- `-s` buffers all matching entries in memory (on-disk index is v0.3).
- Pager, themes, zstd/bzip2/xz, shell completions are flag-accepted but not fully wired.
- OTLP batching: only the first `logRecord` per line is emitted.
- OTLP receiver (localhost:4317) and AI-assisted queries are v0.3 roadmap items.

## [0.2.0] — 2026-10-30

### Added
- **v0.2.0 query language + TUI implementation** (Rust):
  - `-q/--query` boolean query language per `docs/QUERY_SPEC.md`: lexer with byte offsets,
    parser with `or`/`and`/`not`/parentheses precedence, comparisons with aliases (`eq`, `ne`, `gt`, `ge`,
    `lt`, `le`, `contain`, `match`), `in (...)`, `like`/`not like`, `exists(...)`, absent-field `?` modifier,
    and value sets via `in @file` / `in @-` (stdin conflict handled).
  - `-f/--filter` is now sugar over the same query AST, so both surfaces share one engine and AND together.
  - Caret-positioned query error messages per `docs/CLI_SPEC.md` §11; invalid regexes report the pattern.
  - 27 integration tests plus query unit tests covering the `docs/QUERY_SPEC.md` §10 example matrix and §11 test
    matrix.
  - Interactive TUI dashboard (`--tui`): scrollable log list with color-coded levels, live query filtering,
    details view, follow mode toggle, help screen. 11 TUI unit tests.
- **v0.1.0 MVP implementation** (Rust):
  - JSON/JSONL, logfmt, and plain-text parsing with per-line auto-detection and `--allow-prefix`.
  - gzip input by magic bytes (multi-member gzip supported); LF/CRLF/CR/NUL delimiters.
  - Timestamp normalization (RFC 3339, common datetime layouts, Unix s/ms/µs/ns) and level normalization
    (text aliases and numeric syslog/pino levels).
  - `-l` level thresholds; `-f` field filters (`=`, `!=`, `~=`, `!~=`, `~~=`, `!~~=`) with `key?` absent modifier;
    nested paths, array wildcards, and indexes; `--since`/`--until` with keywords and relative offsets.
  - Formatted output with color (TTY/`NO_COLOR` aware), `-h` hide/reveal, `-e`/`-E`, `-x` expansion,
    `-t`/`-Z`/`-L`, `-r` raw passthrough, `-o` output files, NUL output delimiter.
  - In-memory `-s` chronological sort across multiple files.
  - Deterministic exit codes and typed errors.
  - 18 unit tests and 15 integration tests with fixtures.
  - Docker dev image (`scripts/Dockerfile.dev`) with rustfmt/clippy plus cargo helper scripts
    for hosts without a Rust toolchain.
  - Cross-compilation to a native Windows `logcli.exe` (`scripts/Dockerfile.windows`,
    `scripts/build-windows.ps1 -Install`) so the CLI runs without Docker at runtime.
- Project documentation set:
  - Product Requirements Document (`docs/PRD.md`)
  - CLI specification (`docs/CLI_SPEC.md`)
  - Technical design document (`docs/DESIGN.md`)
  - Parsing specification (`docs/PARSING_SPEC.md`)
  - Query language specification (`docs/QUERY_SPEC.md`)
  - Configuration specification (`docs/CONFIG.md`)
  - Test plan (`docs/TEST_PLAN.md`)
  - ADRs 0001–0004
- Implementation roadmap (`PLAN.md`)

### Changed
- Project direction updated to follow the architecture and UX of [hl](https://github.com/pamburus/hl): Rust, streaming pipeline, JSON/logfmt focus, query language, indexed sorting.
- README now documents the MVP feature set and the intentionally unimplemented flags.

### Known limitations
- `-F` follow mode is rejected with a clear error until it lands.
- Pager, themes, zstd/bzip2/xz, and index-backed external sorting are not implemented yet.
- `-s` buffers all matching entries in memory.

## [0.0.0] — 2026-10-28

### Added
- Initial repository scaffolding and planning documents.

[Unreleased]: https://github.com/<owner>/logcli/compare/v0.0.0...HEAD
[0.0.0]: https://github.com/<owner>/logcli/releases/tag/v0.0.0
[0.2.0]: https://github.com/<owner>/logcli/releases/tag/v0.2.0
