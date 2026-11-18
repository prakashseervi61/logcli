# Technical Design Document — `logcli`

**Status:** Draft v0.2
**Related:** [PRD.md](PRD.md) · [CLI_SPEC.md](CLI_SPEC.md) · [PARSING_SPEC.md](PARSING_SPEC.md) · [QUERY_SPEC.md](QUERY_SPEC.md) · [CONFIG.md](CONFIG.md)
**ADRs:** [0001 Rust](adr/0001-use-rust.md) · [0002 Streaming](adr/0002-streaming-architecture.md) · [0003 Indexing](adr/0003-index-for-sort.md) · [0004 Themes](adr/0004-theme-system.md)

---

## 1. Overview

`logcli` is a streaming pipeline: **input → decompress → split → parse → filter → redact → alert-eval → format → output**. Sorting is a separate merge phase. The design prioritizes:

1. **Correctness first** — every behavior is specified in normative docs; behavior changes require doc updates.
2. **Bounded memory** — never hold a full file; streaming through the pipeline.
3. **Deterministic output** — same input and flags produce byte-identical output.
4. **Privacy by default** — PII redaction is declarative and applied before output.
5. **Alerting** — declarative rules exit with configured codes for CI/sidecar use.

---

## 2. Architecture

```mermaid
flowchart LR
    A[Files / stdin] --> B{Decompress (gzip)}
    B --> C[Delimiter Splitter]
    C --> D[Format Detector]
    D --> E[JSON Parser]
    D --> F[logfmt Parser]
    D --> G[OTLP Parser]
    E --> H[Entry Normalizer]
    F --> H
    G --> H
    H --> I[Filter Engine]
    I -->|match| J[PII Redaction]
    J --> K[Alert Evaluation]
    K -->|match| L[Formatter]
    K -->|breach| M[Exit with code]
    I -->|no match| X[Drop]
    L --> N[Pager / stdout / file]
    H -. sort mode .-> O[Index Writer]
    O --> P[Index Reader + K-way Merge]
    P --> L
```

### Module map

| Module | Responsibility | Key types |
|---|---|---|
| `cli` | Flag parsing, help, completions | `Args` |
| `config` | File/env/flag merge, PII rules, alert rules | `Config`, `RedactRule`, `AlertRule` |
| `input` | File discovery, decompression, delimiter splitting, follow tailing | `Source`, `FollowSource` |
| `parser` | Format detection, JSON/logfmt/plain/OTLP extraction | `parse_entry`, `parse_otel` |
| `model` | Normalized entry model | `Entry`, `Level`, `Format`, `ExportFormat`, `RedactRules` |
| `filter` | Level/time/field/query evaluation | `Filter`, `Expr` |
| `output` | Formatting, themes, pager, CSV, Prometheus | `Formatter`, `ExportFormat` |
| `app` | Pipeline wiring, config integration, export, alerts | `run`, `load_entries`, `run_follow` |
| `error` | Error types and exit codes | `Error`, `ExitCode` |
| `tui` | Interactive TUI dashboard (ratatui + crossterm) | `TuiApp` |

---

## 3. Data Model

```rust
pub enum Level { Trace, Debug, Info, Warn, Error, Fatal, Panic }

pub enum Format { Json, Logfmt, Plain, Otel }

pub enum ExportFormat { Formatted, Raw, Json, Csv, Prometheus }

pub struct Entry {
    pub source: String,
    pub line: u64,
    pub timestamp: Option<DateTime<Utc>>,
    pub level: Option<Level>,
    pub level_raw: Option<String>,
    pub message: Option<String>,
    pub logger: Option<String>,
    pub caller: Option<String>,
    pub fields: Map<String, Value>,
    pub consumed: Vec<String>,
    pub raw: String,
    pub format: Format,
    pub redacted: bool,
}

pub struct RedactRules {
    pub rules: Vec<(String, Regex, String)>,
}

pub struct AlertResult {
    pub triggered: Vec<(String, u64)>,
    pub exit_code: Option<i32>,
}
```

Design notes:
- `Entry.redacted` flag ensures PII redaction is applied at most once per entry.
- `Serialize for Entry` renders `timestamp` as RFC3339 string for JSON export.
- `Entry::flatten()` produces dotted key-value pairs for CSV export.
- `Entry::prometheus_lines()` emits Prometheus text exposition format.

---

## 4. Pipeline Stages

### 4.1 Input

- Sources are opened lazily, in order, unless `--sort`.
- Compression detected by magic bytes (`1f 8b` gzip), not extension.
- Reader stack: `File → BufReader(256 KiB) → [MultiGzDecoder] → DelimiterSplitter`.
- `DelimiterSplitter` handles LF/CRLF/CR/NUL and enforces `--max-entry-size`.
- `FollowSource` tails files for new lines (follow mode).
- A line is yielded as `&[u8]`; UTF-8 lossy conversion happens only when fields are displayed.

### 4.2 Parsing

- Detection is per-line with a per-source memo: `{` → JSON/OTLP, `key=` → logfmt, otherwise plain.
- OTLP detected by `resourceLogs` key in JSON object.
- JSON uses `serde_json` into `Map<String, Value>`; key order preserved.
- logfmt is a hand-written single-pass parser (quotes, escapes).
- `--allow-prefix` skips a balanced-brace-aware prefix scan to the first `{`.
- Parser errors produce `None`; the entry is counted as invalid and skipped.

### 4.3 Normalization

- Timestamp: first present of configured keys, then OTel `timeUnixNano`.
- Level: configured keys, then `level`, `lvl`, `severity`, `severity_text`; OTel `SeverityNumber` 1-24 mapped.
- Message: `message`, `msg`, `log`, `@message`, OTLP `body.stringValue`.
- Logger/caller: `logger`, `source`, `caller`, `file`.
- Reserved names (`level`, `message`, `caller`, `logger`) stay addressable via `.` prefix in queries.

### 4.4 Filter Engine

- Filters compile once into an AST and are evaluated per entry.
- Order of evaluation is cost-based: level → time → cheap field equality → substring → regex.
- Short-circuit boolean evaluation.
- Regexes are compiled at startup; invalid regex → exit `1`.
- Query ASTs are `Clone` and shared read-only.

### 4.5 PII Redaction

- `RedactRules::apply()` iterates rules and mutates `Entry` in place.
- Applied to `message`, `logger`, `caller` for all rules.
- For `*` field rules: all string values in `fields` map are redacted.
- For specific field rules: only that field if it's a `Value::String`.
- `Entry::redact()` guards against double-application via the `redacted` flag.

### 4.6 Alert Evaluation

- `evaluate_alerts()` iterates alert specs, parses each query, counts matches.
- If count >= threshold, alert is triggered; exit code is the max of all triggered codes.
- Alert breach prints summary to stderr and exits with the configured code.

### 4.7 Formatting

- Default layout: `TIME LEVEL LOGGER [CALLER] MESSAGE key=value ...`
- Column widths: fixed for time/level; message is last-field-flow layout.
- Expansion modes control multi-line handling.
- `--flatten always` renders nested objects as dotted keys.
- `--hide`/`--flatten` operate on the display tree, never mutate the normalized entry.
- Color application is token-based (theme roles).

### 4.8 Output Formats

| Format | Implementation |
|---|---|
| `Formatted` | `Formatter::write()` with ANSI color, alignment, hiding |
| `Raw` | Original raw bytes written verbatim |
| `Json` | `serde_json::to_string(&entry)` + newline (JSON Lines) |
| `Csv` | `Entry::flatten()` → RFC 4180 CSV with header row |
| `Prometheus` | `Entry::prometheus_lines()` → text exposition format |

### 4.9 Output

- Writer selection: `stdout` (with optional pager child process) or file.
- Broken pipe is detected from `EPIPE`/`ERROR_BROKEN_PIPE` and mapped to exit `3`.
- A single `BufWriter` is shared; flushing policy depends on TTY.

---

## 5. Sorting & Indexing

Goal: chronological merge across many files without loading them into memory.

### 5.1 Current implementation (v0.2)

- In-memory sort: all matched entries collected into `Vec<Entry>`, sorted by timestamp.
- Suitable for files up to ~100M entries (memory-bound).
- On-disk index (ADR 0003) is a roadmap item for larger inputs.

### 5.2 Planned (v0.3+)

```
Header: magic "LGCI" | version u16 | flags u16 | entry_count u64 | source_table_len u32
Source table: path, size, mtime_ns, offset_count
Entries: timestamp_delta, source_id, byte_offset, byte_length, level
Index footer: crc32c of entries section
```

- One index per source file, stored as `<hash>.lgci` in `--index-dir`.
- Validation checks magic, version, file size, mtime, and CRC. On mismatch, reindex.
- K-way merge over index readers (binary heap keyed by timestamp, then source id, then offset).

---

## 6. Concurrency Model

- **Non-sorted mode:** single reader thread per source; files processed sequentially to preserve output order.
- **Sort/index build:** Rayon work-stealing over byte ranges (planned).
- **Follow mode:** one poller task per file; merge happens on the main thread.
- Writer is single-threaded; no locks on the hot path.

Rule: **parallelism must never change output.** Ordering keys are `(source order, offset)` in normal mode and `(timestamp, source order, offset)` in sort mode.

---

## 7. Performance Budget

| Stage | Budget (2.3 GiB, 6M lines) |
|---|---|
| Read + decompress (gz) | ~0.4 s |
| Split + detect + parse | ~0.8 s |
| Normalize + redact | ~0.2 s |
| Filter (typical) | ~0.1 s |
| Format + write | ~0.3 s |
| **Total (uncompressed)** | **≤ 1.8 s** |

Techniques:
- Borrowed strings where possible; avoid `String` allocation for non-displayed fields.
- Reused per-thread buffers.
- Batch writes through a 64 KiB buffer.
- `criterion` benchmarks committed; CI fails on > 10% regression vs baseline.

---

## 8. Error Handling

```rust
pub enum Error {
    Usage { message: String, tip: Option<String> },   // exit 1
    Io { path: Option<PathBuf>, source: io::Error },  // exit 2
    Query { column: usize, message: String },         // exit 1
    Config { path: Option<PathBuf>, message: String },// exit 1
    BrokenPipe,                                       // exit 3
}
```

- `main` maps errors to exit codes; only `--verbose` prints backtraces.
- Alert breach: `Error::usage(format!("alert threshold exceeded (exit {code})"))` → exits with `code`.
- Per-line parse failures are data errors, not `Error`; they surface through stats.
- Panics are caught at the top level, reported tersely, and exit `2` (should never happen; fuzz-tested).

---

## 9. Configuration Model

- Precedence: built-in defaults → system config → user config → CWD config → `LOGCLI_CONFIG` files → `LOGCLI_*` env → CLI flags.
- Config parsed into `Config` struct (serde, JSON-only in v0.2).
- CLI flags override config values field-by-field.
- Theme resolution: built-in defaults → user theme → overrides.
- See [CONFIG.md](CONFIG.md) for schema and locations.

---

## 10. TUI Design

- Built with `ratatui` + `crossterm`.
- Color theme constants (not a Config struct) — see ADR 0004.
- Layout (matching the reference dashboard):
  - Boxed header: `logcli v<version>` (magenta), centered `[ Source: <file> ]`, right-aligned `[ ERRORS ] [ WARNs ] [ INFO ]` counts.
  - Body split ~68/32: entries table on the left, always-visible JSON details pane on the right.
  - Entries table: `TIMESTAMP` (HH:MM:SS.mmm), `LEVEL` (ERR/WRN/INF/DBG/TRC/FTL/PNC), `MESSAGE`; selected row uses a green `>` marker and a dark highlight.
  - Details pane: pretty-printed, syntax-colored JSON with keys bold, strings cyan, numbers yellow, booleans magenta; `level` uses the level color.
  - Filter/status box under the entries pane only; shows `Filter: [query]` plus the latest status message.
  - One-line key-hint bar with `Matches: filtered/total` at the bottom right.
- Views: entries + details, histogram, help popup, filter prompt, file picker popup.
- Follow mode: `FollowSource` polls files for new lines at 200ms intervals.
- Auto-launch: no files + stdin is a TTY → `--tui` is implied.

---

## 11. Testing Strategy (summary)

| Level | Tool | Scope |
|---|---|---|
| Unit | `cargo test` | parsers, filters, query AST, level/time parsing, config, redaction, alerts |
| Integration | `assert_cmd` + `predicates` | every CLI_SPEC example, PII redaction, alert rules, export formats |
| Golden | output comparison | formatted, JSON, CSV, Prometheus output |
| Performance | `criterion` + `hyperfine` | throughput, RSS, startup |

Full details: [TEST_PLAN.md](TEST_PLAN.md).

---

## 12. Project Layout

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
│   ├── app.rs        pipeline wiring, config, time/size parsing, export, alerts, pager
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

## 13. Dependencies

| Crate | Purpose |
|---|---|
| `clap` (derive) | CLI |
| `serde`, `serde_json` | JSON, config serialization |
| `chrono` / `chrono-tz` | Time, timezones |
| `regex` | Regex filters, PII redaction patterns |
| `flate2` | gzip decompression |
| `memchr` | Fast byte scanning |
| `ratatui` + `crossterm` | TUI |
| `assert_cmd`, `predicates` | Integration testing |

---

## 14. Open Technical Questions

1. On-disk index for sorting (>100MB files) — v0.3 or v1.0?
2. zstd/bzip2/xz compression support — behind feature flags?
3. OTLP receiver (localhost:4317 HTTP endpoint) — v0.3 roadmap.
4. AI-assisted natural-language queries — v0.3 roadmap.
5. Windows ANSI/pager detection details (ConPTY, Windows Terminal, legacy console).