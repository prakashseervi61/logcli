# Log Analysis CLI — Implementation Roadmap

A fast, local-first log viewer/processor for JSON, logfmt, plain text, and OpenTelemetry OTLP logs, inspired by [hl](https://github.com/pamburus/hl) and [lnav](https://github.com/tstack/lnav).

## Documentation Map

All project documents live in [`docs/`](docs/README.md):

| Document | Purpose |
|---|---|
| [docs/PRD.md](docs/PRD.md) | Requirements, scope, users, success metrics |
| [docs/CLI_SPEC.md](docs/CLI_SPEC.md) | **Normative** CLI contract: flags, exit codes, examples |
| [docs/DESIGN.md](docs/DESIGN.md) | Architecture, pipeline, indexing, concurrency, performance budgets |
| [docs/PARSING_SPEC.md](docs/PARSING_SPEC.md) | **Normative** parsing rules, formats, timestamps, levels |
| [docs/QUERY_SPEC.md](docs/QUERY_SPEC.md) | **Normative** query grammar, operators, semantics |
| [docs/CONFIG.md](docs/CONFIG.md) | Config files, env vars, themes, PII redaction, alert rules |
| [docs/TEST_PLAN.md](docs/TEST_PLAN.md) | Test strategy, fixtures, benchmarks, CI, release checklist |
| [docs/adr/](docs/adr/) | Architecture Decision Records 0001–0004 |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Dev setup, style, commit/PR process |
| [CHANGELOG.md](CHANGELOG.md) | Version history |

**Rule:** CLI_SPEC, PARSING_SPEC, and QUERY_SPEC are normative. Behavior changes require a spec update in the same PR.

---

## Status (v0.2.0)

### Implemented and tested

- JSON/JSONL, logfmt, and plain-text parsing with auto-detection and `--allow-prefix`
- OpenTelemetry OTLP JSON parsing (`--input-format otel`): `resourceLogs` → `scopeLogs` → `logRecords`
- gzip input, LF/CRLF/CR/NUL delimiters, multiple files and stdin
- level/time/field filtering, nested and array paths, relative and absolute time ranges
- boolean `-q` query language (full QUERY_SPEC: `and`/`or`/`not`, parentheses, comparisons, `in (...)`, `like`/`not like`, `exists(...)`, absent-field `?`, value sets `@file`/`@-`)
- formatted and raw output, colors, hiding, expansion, time zones, file output
- multi-format export: `json` (JSON Lines), `csv` (flattened), `prometheus` (text exposition)
- PII redaction via declarative `redact:` rules in config (message/logger/caller/fields, `*` wildcard)
- Alert rules via declarative `alerts:` in config (threshold breach → exit code)
- Config file auto-discovery (`logcli.yaml/json`), `--config` flag, CLI overrides config
- Interactive TUI dashboard (`--tui`): scrollable list, color-coded levels, live query filtering, details view, follow mode toggle, histogram view, help screen
- File follow mode (`-F`) tails new lines in real time
- Chronological sort (`-s`) merges multiple files by timestamp
- 52 unit tests + 15 CLI integration tests + 27 query integration tests pass
- `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test` all green

### Not implemented (rejected with a clear error or accepted as no-ops)

- zstd/bzip2/xz compression (only gzip supported)
- On-disk index for sorting (in-memory sort only)
- Shell completions (`--shell-completions` flag accepted but not implemented)
- Pager integration (`--paging` flag accepted but not implemented)
- Theme engine (`--theme` flag accepted but not implemented)
- OTLP receiver (localhost:4317 HTTP endpoint)
- AI-assisted natural-language queries

Native Windows `logcli.exe` built and installed at `C:\Users\praka\bin\logcli.exe` (PATH set).

All code committed and pushed to GitHub: `https://github.com/prakashseervi61/logcli.git`

---

## Goal

Build a single binary that reads multi-GB logs quickly, filters them precisely (level, nested fields, boolean queries, time ranges), redacts PII automatically, alerts on thresholds, exports to multiple formats, parses OpenTelemetry OTLP, and renders them clearly — while remaining fully pipeable.

---

## Reference: hl

hl is the design benchmark:

- JSON + logfmt auto-detection, non-JSON prefix support
- Field-level, level, and time-range filtering; boolean query language
- Chronological sorting via indexed scans; live follow mode
- Compression support, pager integration, themes
- ~2 GiB/s on a 2.3 GiB / 6M-line file

Our target: **match its UX and approach 80% of its throughput in v1**, with a clearer specification, PII redaction, alert rules, and OTLP support.

---

## Tech Stack

| Layer | Choice | Rationale |
|---|---|---|
| Language | **Rust** (stable, MSRV pinned) | Performance, safety, single binary — [ADR 0001](docs/adr/0001-use-rust.md) |
| CLI | `clap` (derive) | Ergonomics, help, completions |
| JSON | `serde_json` | Correctness first |
| Time | `chrono` / `chrono-tz` | Timezone correctness |
| Regex | `regex` | Compile-once filters, PII redaction patterns |
| Compression | `flate2` | gzip support |
| TUI | `ratatui` + `crossterm` | Interactive dashboard |
| Config | `serde` + `serde_json` | JSON-only config files |
| Errors | Custom `Error` enum | Typed errors mapped to exit codes |
| Testing | `assert_cmd`, `predicates` | Integration testing |

Rejected: Go (GC/allocation costs), C++ (safety/tooling), Python (throughput).

---

## Architecture (summary)

```
read → decompress → split → parse → normalize → filter → redact → alert → format → write
                                                                │
                                                                └─ sort mode → sort → write
```

- Streaming, bounded memory; sorting collects entries in memory.
- Data model (`Entry`, `Level`, `Format`, `ExportFormat`, `RedactRules`) and module map: [DESIGN §2–3](docs/DESIGN.md).

---

## Feature Scope

### v0.2.0 — Current (implemented)

- Files/stdin/multiple files; gzip by magic bytes; LF/CRLF/CR/NUL
- JSON + logfmt + plain text + OTLP; `--allow-prefix`
- Timestamp normalization (RFC3339, Unix s/ms/µs/ns); level normalization (incl. OTel SeverityNumber)
- `-l` level threshold; `-f` field filters (6 operator forms + `?`); `--since/--until`
- Formatted colorized output; `-h` hide/reveal; `-e`; `-x`; `-t`/`-Z`/`-L`
- Multi-format export: formatted, raw, json, csv, prometheus
- PII redaction via config `redact:` rules
- Alert rules via config `alerts:` (threshold → exit code)
- Config file auto-discovery + `--config` + env vars + CLI overrides
- Interactive TUI with histogram, follow, details view
- File follow mode (`-F`)
- Clean exit codes and error messages

### v0.3 — Planned

- zstd/bzip2/xz compression support (feature-gated)
- On-disk index for sorting large inputs
- Shell completions (`--shell-completions`)
- Pager integration (`--paging`)
- Theme engine (`--theme` with built-in themes)
- OTLP receiver (localhost:4317 HTTP endpoint)
- AI-assisted query helper (`--explain`)

### v1.0 — Post-v0.3

- Apache/Nginx and syslog parsers
- `--output-format json|logfmt` re-serialization
- `--count-by` basic aggregations
- `index` maintenance subcommands
- `--ignore-case`, `--strict`

Out of scope: dashboards/charts, remote shipping, web UI, plugins, alerting as a service.

---

## Project Structure

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
│   ├── app.rs        pipeline wiring, config, time/size parsing, export, alerts
│   ├── config.rs     declarative config, PII redaction, alert rules
│   ├── error.rs      error types and exit codes
│   ├── filter.rs     level/time/field filters and path resolution
│   ├── input.rs      file/stdin sources, gzip, follow tailing
│   ├── model.rs      Entry, Level, Format, ExportFormat, RedactRules
│   ├── output.rs     formatting, colors, hiding, expansion, writers, CSV, Prometheus
│   ├── parser.rs     JSON/logfmt/plain/OTLP detection and normalization
│   ├── query/        lexer, parser, AST evaluator (-q engine)
│   ├── tui/          interactive TUI dashboard (ratatui + crossterm)
│   └── theme.rs      color theme constants
├── tests/            integration tests and fixtures
└── scripts/          Docker-based cargo wrappers, Windows cross-compile
```

---

## Implementation Phases

### Phase 0 — Documentation (done)
- [x] PRD, CLI spec, design, parsing spec, query spec, config spec, test plan
- [x] ADRs 0001–0004
- [x] README, CONTRIBUTING, CHANGELOG, docs index

### Phase 1 — Foundation (done in MVP)
- [x] `cargo init`; pin MSRV
- [x] `cli.rs` with the full flag surface from CLI_SPEC
- [x] `model` types; input sources + gzip + delimiter detection
- [x] JSON parser + `normalize` (timestamp/level/message)
- [x] Minimal formatter; pipeline wiring in `app.rs`

### Phase 2 — Filtering (done in MVP)
- [x] Level threshold and numeric mapping
- [x] Field filters (all forms), hierarchical paths, absent modifier
- [x] Timestamp parsing + `--since/--until` (absolute, keyword, relative)
- [x] logfmt parser and format detection
- [x] Error model and exit codes per CLI_SPEC

### Phase 3 — Query Language (done in v0.2)
- [x] Lexer, parser (EBNF in QUERY_SPEC), AST
- [x] Evaluator with coercion rules, arrays, `in`, `exists`, absent modifier `?`, value sets (`@file`, `@-`)
- [x] Caret-positioned error messages
- [x] QUERY_SPEC test matrix and unit tests green

### Phase 3.5 — Interactive TUI (done in v0.2)
- [x] TUI scaffolding (ratatui + crossterm)
- [x] Log list view with color-coded levels
- [x] Scrollable entry list, query filter, details view, follow toggle, help, histogram
- [x] Auto-launch when no files + stdin is TTY

### Phase 4 — Next-Level Features (done in v0.2)
- [x] Declarative config files (JSON-only)
- [x] PII redaction rules (`redact:` in config)
- [x] Alert rules (`alerts:` in config with threshold + exit code)
- [x] Multi-format export (json, csv, prometheus)
- [x] OpenTelemetry OTLP parsing (`--input-format otel`)
- [x] File follow mode (`-F`)
- [x] Native Windows `logcli.exe` cross-compiled and installed

### Phase 5 — v0.3 (planned)
- [ ] zstd/bzip2/xz compression support
- [ ] On-disk index for sorting
- [ ] Shell completions
- [ ] Pager integration
- [ ] Theme engine
- [ ] OTLP receiver (localhost:4317)
- [ ] AI-assisted query helper

### Phase 6 — Hardening & Release (planned)
- [ ] Fuzzing (parsers + query) for 24h clean
- [ ] Performance tuning against budgets; publish results
- [ ] Cross-platform CI, coverage ≥ 85%
- [ ] README/docs final pass; completions + man page
- [ ] Release binaries, checksums, v1.0.0 tag

---

## Testing & Quality Gates

See [TEST_PLAN.md](docs/TEST_PLAN.md). Merge requires:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features` on Linux, macOS, Windows
- No benchmark regression > 10%
- Doc updates when behavior changes

---

## Performance Budgets

| Scenario | Target |
|---|---|
| 2.3 GiB JSON, parse+format to NUL | ≤ 2 s |
| Peak RSS (no sort) | ≤ 200 MiB |
| Startup (`--version`) | < 100 ms |
| Binary size (stripped) | < 15 MiB |

---

## Immediate Next Steps

1. Implement zstd/bzip2/xz compression support.
2. Implement on-disk index for sorting large inputs.
3. Implement shell completions and pager integration.
4. Implement theme engine with built-in themes.
5. Add CI workflow with fmt/clippy/test on three platforms.
6. Resolve remaining PRD open questions (name, license).