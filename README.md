# logcli

Fast, local-first log viewer and processor for **JSON**, **logfmt**, **plain text**, and **OpenTelemetry OTLP** logs — inspired by [hl](https://github.com/pamburus/hl) and [lnav](https://github.com/tstack/lnav).

> **Status:** v0.2.0. Query language, interactive TUI, declarative config, PII redaction, alert rules, multi-format export, OpenTelemetry parsing, pager integration, shell completions, theming, and compressed log support all work and are covered by tests.
>
> Native database index, AI natural-language queries, web UI, and OTLP receiver are **roadmap** items for v0.3.0.

## Why

`grep` doesn't understand structured logs; observability platforms need infrastructure. `logcli` reads logs locally, filters them precisely, renders them clearly — and stays fully pipeable. It runs with **no Docker at runtime** (native `logcli.exe` on Windows), so you can triage production logs straight from your terminal.

## 2027-ready features

| Feature | Flag / Config | What it does |
|---------|---------------|--------------|
| Boolean query language | `-q 'expr'` | `and`/`or`/`not`, parentheses, `in (...)`, `like`, `exists(...)`, `?` absent modifier |
| Declarative config | `logcli.yaml/json/toml` or `--config` | Saved filters, queries, level, time range, redaction, alerts — no flag soup |
| PII redaction | `redact:` in config | Mask emails, phones, IDs across message/logger/caller/fields |
| Alert rules | `alerts:` in config | Exit with code when query matches N entries (great for CI/sidecar) |
| Multi-format export | `-o` + `--output-format` | `formatted`, `raw`, `json`, `csv`, `prometheus` |
| OpenTelemetry OTLP | `--input-format otel` | Parse `resourceLogs` / `scopeLogs` / `logRecords` lines |
| Interactive TUI | `--tui` | Scrollable dashboard, live query filter, follow mode, details view, histogram |
| File follow | `-F` / `follow: true` | Tail new lines in real time |
| Chronological sort | `-s` / `sort: true` | Merge multiple files by timestamp |
| Pager integration | auto / `--pager` | Auto-launch `less` when output is a TTY |
| Shell completions | `--shell-completions` | bash, zsh, fish, powershell, elvish |
| Theming | `--theme` / `theme:` | Built-in color themes (default, monokai, solarized) |
| Compressed logs | auto-detect | `.gz` (gzip) — `.zst`, `.bz2`, `.xz` roadmap |

## Quick start

```powershell
# View a log file
logcli app.log

# Only warnings and above
logcli -l warn app.log

# Filter nested fields
logcli -f request.method=GET app.log

# Boolean query: errors with slow requests
logcli -q 'level = error and duration > 1.0' app.log

# Value set membership
logcli -q 'status in (200, 201, 204)' app.log

# OpenTelemetry OTLP logs
logcli --input-format otel otel.log

# Export to Prometheus for scraping
logcli --output-format prometheus -o metrics.prom app.log

# Export to CSV for spreadsheets
logcli --output-format csv -o report.csv app.log

# Redact PII automatically
# (see config example below)
logcli --config logcli.yaml app.log

# Interactive TUI dashboard
logcli --tui app.log

# Follow a live log file
logcli -F -l error app.log

# Read from stdin
kubectl logs deploy/api | logcli -l error

# Auto-pager when output is a TTY
logcli app.log        # pipes to less automatically
logcli -P app.log     # disable pager (streaming)

# Shell completions
logcli --shell-completions bash > /etc/bash_completion.d/logcli
```

## Declarative config (`logcli.yaml`)

Save flags you reuse into `logcli.yaml` in your project root (or pass `--config path`). CLI flags always override config values. Config files are also discovered at platform-standard locations:

| OS | System-wide | User profile |
|----|-------------|--------------|
| macOS/Linux | `/etc/logcli/config.{yaml,json,toml}` | `~/.config/logcli/config.{yaml,json,toml}` |
| Windows | `%PROGRAMDATA%\logcli\config.{yaml,json,toml}` | `%USERPROFILE%\AppData\Roaming\logcli\config.{yaml,json,toml}` |

Discovery order: current-directory `logcli.{yaml,yml,json}` → user profile → system-wide → `--config`.

```yaml
# Default input files (used when none passed on CLI)
files:
  - "logs/api.log"
  - "logs/worker.log"

# Default filtering
level: warn
since: "-1h"
until: now
filters:
  - "component=api"
queries:
  - "status >= 500"

# Output
output_format: formatted   # formatted | raw | json | csv | prometheus
output: "report.txt"
time_zone: "Europe/Berlin"
hide:
  - "request.headers.authorization"
hide_empty: true
expansion: auto
sort: true
pager: auto                # auto | always | never
theme: default             # default | monokai | solarized

# PII redaction (applied to message/logger/caller/fields)
redact:
  - field: "message"
    pattern: "[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}"
    replace: "[EMAIL]"
  - field: "*"
    pattern: "\\b\\d{4}[- ]?\\d{4}[- ]?\\d{4}[- ]?\\d{4}\\b"
    replace: "[CARD]"

# Alert rules (great for CI / sidecar containers)
alerts:
  - name: "high_error_rate"
    query: "level = error and status >= 500"
    threshold: 10
    exit_code: 2
  - name: "fatal_occurred"
    query: "level = fatal"
    threshold: 1
    exit_code: 2

# TUI defaults
tui:
  sort: time       # none | time | level
  follow: false
  theme: default
```

## Interactive TUI dashboard

```powershell
logcli --tui app.log
logcli --tui --follow app.log
logcli --tui -q 'level = error' app.log
```

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

The dashboard mirrors the reference layout: a boxed header with the log source and
per-level counts, a timestamp/level/message table on the left, a live JSON details
pane on the right, a filter/status box underneath the table, and a key-hint bar
with the current match count.

The TUI opens automatically when you run `logcli` with no files and stdin is a TTY.

## Output formats

| Format | Flag | Best for |
|--------|------|----------|
| `formatted` (default) | `-o out.txt` | Human reading in terminal |
| `raw` | `--output-format raw -o out.log` | Exact source passthrough |
| `json` | `--output-format json -o out.jsonl` | Downstream JSON tools |
| `csv` | `--output-format csv -o report.csv` | Spreadsheets / Excel |
| `prometheus` | `--output-format prometheus -o metrics.prom` | Prometheus scrape targets |

## Installation

From source:

```sh
cargo build --release
./target/release/logcli --help
```

### Without a local Rust toolchain (Windows/macOS/Linux)

```powershell
# PowerShell
.\scripts\docker-cargo.ps1 build
.\scripts\docker-cargo.ps1 test
```

```sh
# Bash
./scripts/docker-cargo.sh build
./scripts/docker-cargo.sh test
```

### Native Windows executable

```powershell
# Build only: writes dist\logcli.exe
.\scripts\build-windows.ps1

# Build and install: copies to %USERPROFILE%\bin and adds it to the user PATH
.\scripts\build-windows.ps1 -Install
```

Open a **new** terminal after `-Install`, then:

```powershell
logcli --version
logcli tests\fixtures\simple.jsonl
```

## Configuration precedence (lowest → highest)

1. System-wide config file (`/etc/logcli/config.*` or `%PROGRAMDATA%\logcli\config.*`)
2. User profile config file (`~/.config/logcli/config.*` or `%APPDATA%\logcli\config.*`)
3. Current-directory config file (`logcli.yaml` / `logcli.json`)
4. Environment variables (`LOGCLI_*`)
5. Command-line arguments

## Documentation

| Document | Contents |
|----------|----------|
| [docs/PRD.md](docs/PRD.md) | Requirements, scope, success metrics |
| [docs/CLI_SPEC.md](docs/CLI_SPEC.md) | **Normative** CLI contract: flags, exit codes, examples |
| [docs/PARSING_SPEC.md](docs/PARSING_SPEC.md) | **Normative** parsing rules, formats, timestamps, levels |
| [docs/QUERY_SPEC.md](docs/QUERY_SPEC.md) | **Normative** query grammar, operators, semantics |
| [docs/CONFIG.md](docs/CONFIG.md) | Config files, env vars, themes, alerts |
| [docs/TEST_PLAN.md](docs/TEST_PLAN.md) | Test strategy, fixtures, benchmarks, CI |
| [docs/DESIGN.md](docs/DESIGN.md) | Architecture and performance design |
| [docs/adr/](docs/adr/) | Architecture Decision Records 0001–0004 |
| [PLAN.md](PLAN.md) | Implementation roadmap |

## Project layout

```
logcli/
├── Cargo.toml
├── README.md  CONTRIBUTING.md  CHANGELOG.md  PLAN.md
├── docs/
│   ├── README.md  PRD.md  CLI_SPEC.md  DESIGN.md
│   ├── PARSING_SPEC.md  QUERY_SPEC.md  CONFIG.md  TEST_PLAN.md
│   └── adr/0001..0004
├── src/
│   ├── main.rs       entry point and exit-code mapping
│   ├── cli.rs        clap flag surface
│   ├── app.rs        pipeline wiring, config, time/size parsing, export, pager
│   ├── config.rs     declarative config, PII redaction, alert rules
│   ├── error.rs      error types and exit codes
│   ├── filter.rs     level/time/field filters and path resolution
│   ├── input.rs      file/stdin sources, gzip, follow tailing
│   ├── model.rs      Entry, Level, Format, ExportFormat, RedactRules
│   ├── output.rs     formatting, colors, hiding, expansion, writers, pager
│   ├── parser.rs     JSON/logfmt/plain/OTLP detection and normalization
│   ├── query/        lexer, parser, AST evaluator (-q engine)
│   ├── theme.rs      color theme definitions
│   └── tui/          interactive TUI dashboard (ratatui + crossterm)
├── tests/            integration tests and fixtures
└── scripts/          Docker-based cargo wrappers, Windows cross-compile
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Specifications are normative — behavior changes require a doc update in the same PR.

## License

TBD (MIT or Apache-2.0). Do not depend on this project until a license is added.