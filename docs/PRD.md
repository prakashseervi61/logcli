# Product Requirements Document (PRD) — `logcli`

**Status:** Draft v0.2
**Owner:** Prakash
**Last updated:** 2026-10-30
**Related docs:** [PLAN.md](../PLAN.md) · [CLI_SPEC.md](CLI_SPEC.md) · [DESIGN.md](DESIGN.md) · [PARSING_SPEC.md](PARSING_SPEC.md) · [QUERY_SPEC.md](QUERY_SPEC.md) · [CONFIG.md](CONFIG.md)

---

## 1. Overview

`logcli` is a high-performance command-line log viewer and processor for **JSON**, **logfmt**, **plain text**, and **OpenTelemetry OTLP** logs, inspired by [hl](https://github.com/pamburus/hl) and [lnav](https://github.com/tstack/lnav). It reads structured logs, applies fast filtering, PII redaction, and alert rules, and renders them clearly — while remaining fully pipeable and script-friendly. Native Windows `logcli.exe` is cross-compiled and installed separately.

The design goal: **make multi-GB log files readable and analyzable in seconds, locally and offline, with no infrastructure required.**

---

## 2. Problem Statement

Developers and operators deal with structured logs daily, but tooling is fragmented:

- `cat`/`grep`/`less` do not understand structure; JSON logs are unreadable in raw form.
- Log aggregation platforms (ELK, Loki, Datadog) require infrastructure and network access; overkill for a downloaded log file or a local `kubectl logs` stream.
- Existing local viewers are either slow, unmaintained, or lack structured filtering (field-level, nested, boolean queries).
- PII (emails, credit cards, phones) leaks into terminal output and logs shared with teammates.
- CI pipelines need automated alerting on error thresholds without a separate monitoring stack.

**Users need a single binary that reads logs fast, filters precisely, redacts PII automatically, alerts on thresholds, and exports to multiple formats — locally and offline.**

---

## 3. Target Users

| Persona | Context | Primary needs |
|---|---|---|
| **Backend developer** | Debugging a service locally or in a container | Read JSON logs clearly, filter by `level` and fields, follow live output |
| **SRE / on-call engineer** | Incident response, downloaded logs from production | Merge multiple files chronologically, filter by time window and error level, fast startup |
| **DevOps engineer** | CI pipelines, post-mortem analysis | Pipe-friendly, deterministic exit codes, alert rules, works over SSH with no dependencies |
| **Data/Support engineer** | Ad-hoc investigation of customer logs | Query language for nested fields, substring/regex filters, export to CSV/Prometheus |
| **Security/Compliance engineer** | PII handling, audit trails | Automatic redaction of emails/cards/IDs across all fields |

**Non-users (for v1):** Users who need dashboards, long-term retention, multi-tenant querying, or a web UI. They should use an observability platform.

---

## 4. Goals

1. **Performance:** Parse and render a 2.3 GiB / 6M-line JSON log in under 3 seconds on a modern laptop (target: ≤ 2 s).
2. **Fidelity:** Correctly parse JSON, logfmt, plain text, and OpenTelemetry OTLP, including nested objects, arrays, non-JSON prefixes, and multi-line values.
3. **Precision:** Filter by level, field value, substring, regex, boolean query, time range, and value sets — without piping to other tools.
4. **Privacy:** Declarative PII redaction rules applied to message/logger/caller/fields before output.
5. **Alerting:** Declarative alert rules evaluated against matched entries; exit with configured code on threshold breach (ideal for CI/sidecar).
6. **Interoperability:** Export to formatted, raw, JSON Lines, CSV, and Prometheus text exposition format.
7. **Observability input:** Accept OpenTelemetry OTLP JSON log lines via `--input-format otel`.
8. **Ergonomics:** Zero-config startup; declarative config files; pager integration; shell completions; theming; NO required install beyond a single binary.
9. **Composability:** Read stdin, write stdout, respect `NO_COLOR`, exit codes suitable for scripts.
10. **Stability:** No panics on malformed input; skip and report bad lines without aborting the stream.

---

## 5. Non-Goals (v1)

- Full-text search engine or inverted index over message bodies.
- Statistical dashboards or charts beyond the TUI histogram view.
- Remote log shipping, agents, or server components (OTLP *receiver* is a v0.3 roadmap item).
- Log format plugins loaded dynamically (WASM etc.).
- Web UI (humanlog model) — local-first CLI only.
- Windows event log, journald, or Kubernetes API integration.
- AI/natural-language query (roadmap: `--explain` helper, v0.3).

---

## 6. Feature Requirements

Priorities: **P0 = MVP**, **P1 = v0.2**, **P2 = post-v0.2**.

### 6.1 Input (P0)
- Read one or many files; read stdin when no file is given or when `-` is passed.
- Auto-detect gzip compression by magic bytes (zstd/bzip2/xz roadmap).
- Auto-detect entry delimiter (LF, CRLF, CR, NUL).
- Support non-JSON prefixes before a JSON object (`--allow-prefix`).
- Graceful handling of invalid lines: skip with a warning count (verbose mode lists them).

### 6.2 Parsing (P0)
- JSON and JSONL parsing with arbitrary nesting.
- logfmt parsing (`key=value`, quoted values, escaped characters).
- Auto-detection between JSON and logfmt per line, with `--input-format` override.
- OpenTelemetry OTLP JSON parsing (`--input-format otel`): `resourceLogs` → `scopeLogs` → `logRecords` → normalized Entry.
- Timestamp extraction from common field names and formats (RFC3339, Unix s/ms/µs/ns).
- Level normalization: `trace/debug/info/warn/warning/error/fatal/panic`, numeric levels, case-insensitive.
- Message extraction from `message`, `msg`, `log`, `@message`.
- Caller/logger extraction (`caller`, `logger`, `source`, `@logger`).
- SeverityNumber (OTel 1-24) mapping to Level.

### 6.3 Filtering (P0)
- `--level` threshold filtering (e.g. `-l w` shows warn and above).
- `--filter` on field expressions: `k=v`, `k!=v`, `k~=substring`, `k!~=substring`, `k~~=regex`.
- Hierarchical field paths (`request.method`, `headers.content-type`).
- Array traversal (`span.[].name`) and indexed access (`span.[1].name`).
- Boolean query expressions (`--query`): `and`/`or`/`not`, comparisons, `in (...)`, `exists(...)`.
- `--since` / `--until` with absolute timestamps, keywords (`today`, `yesterday`), and relative offsets (`-3h`, `-14d`).
- Missing-field semantics (`?` modifier).
- Value sets from file (`in @file`) or stdin (`in @-`).

### 6.4 Output (P0 → P1)
- Human-readable default format: time, level, logger, message, fields.
- ANSI color by default on TTY; `--color auto|always|never`, `--no-color`, `NO_COLOR` env.
- Pager auto-invocation on TTY (`--paging auto|always|never`, `-P` alias).
- Field hiding/revealing (`-h key`, `-h '!key'`, wildcard `*`).
- Empty-field hiding (`-e`), expansion modes (`-x never|inline|auto|always`).
- Time format (`-t`) and timezone (`-Z`, `-L`).
- Raw passthrough (`-r`) for piping matched source lines.
- Output to file (`-o`).
- **Multi-format export:** `formatted`, `raw`, `json` (JSON Lines), `csv` (flattened), `prometheus` (text exposition).
- **Prometheus export:** `logcli_total{label="..."} 1 <timestamp_ms>` per matched entry.

### 6.5 PII Redaction (P1)
- Declarative `redact:` rules in config: field name + regex pattern + replacement.
- Applied to `message`, `logger`, `caller`, and all string values in `fields`.
- `*` field name redacts all string fields recursively.
- Redacted entries are flagged so redaction is applied at most once per entry.

### 6.6 Alert Rules (P1)
- Declarative `alerts:` rules in config: name + query expression + threshold + exit code.
- Evaluated after filtering against matched entries.
- Exit with configured code when threshold breached; print alert summary to stderr.
- Ideal for CI pipelines and sidecar containers.

### 6.7 Sorting & Following (P1)
- Chronological merge across multiple files (`-s`) using in-memory sort (on-disk index roadmap).
- Follow mode (`-F`) tails new lines in files in real time.
- TUI follow mode with live file polling.

### 6.8 Interactive TUI (P1)
- `--tui` launches a scrollable dashboard (ratatui + crossterm).
- Keybindings: `j`/`k` scroll, `g`/`G` top/bottom, `/` or `f` filter, `d`/`Enter` details, `o` add file, `s` sort cycle, `F` follow toggle, `h` histogram, `?` help, `q` quit.
- Auto-launch when no files and stdin is a TTY.
- Histogram view of entries over time (lnav-style).
- Color-coded levels (trace/debug/info/warn/error/fatal/panic).

### 6.9 Configuration & Theming (P1)
- Config file auto-discovery: `logcli.yaml`/`logcli.yml`/`logcli.json` in CWD, then user profile, then system-wide.
- `--config` flag overrides; CLI flags always win over config.
- Environment variables with `LOGCLI_*` prefix.
- Built-in themes: `default`, `monokai`, `solarized`.
- Shell completions (`--shell-completions bash|zsh|fish|powershell|elvish`).

---

## 7. Success Metrics

| Metric | Target | Measurement |
|---|---|---|
| Throughput (parse+format) | ≥ 1 GiB/s on 6M-line JSON | `hyperfine 'logcli -c -o /dev/null sample.log'` |
| Cold start | < 100 ms | `hyperfine 'logcli --version'` |
| Peak RSS on 2.3 GiB file (no sort) | < 200 MiB | `/usr/bin/time -v` / `Get-Process` |
| CLI correctness | 100% of CLI_SPEC examples tested | integration test suite |
| Binary size | < 15 MiB stripped | release build |
| Test coverage | ≥ 85% line coverage in `src/` | `cargo-llvm-cov` |
| PII redaction | 100% of config rules applied correctly | unit + integration tests |
| Alert rules | Correct exit code on threshold breach | integration tests |

---

## 8. Assumptions & Constraints

- Logs are newline- or NUL-delimited; arbitrary binary blobs are out of scope.
- Structured logs (JSON/logfmt/OTLP/plain) are the primary target.
- v1 targets 64-bit Linux, macOS, and Windows.
- No network access required at runtime (OTLP receiver is v0.3).
- Single-user CLI; no authentication, no telemetry, no background service.
- Native Windows `logcli.exe` is cross-compiled via Docker and installed to `%USERPROFILE%\bin`.

---

## 9. Risks & Mitigations

| Risk | Impact | Likelihood | Mitigation |
|---|---|---|---|
| Performance regressions vs hl | Medium | Medium | CI benchmarks; budgeted allocations |
| Query parser scope creep | High | High | Freeze grammar in QUERY_SPEC; explicit non-goals |
| PII redaction misses fields | High | Low | Test matrix covers all field types; `*` wildcard |
| Alert rules fire incorrectly | Medium | Low | Query validated at config load; unit tests |
| Windows pager / ANSI quirks | Low | High | Detect TTY + Windows Terminal; `--paging never` |
| Malformed or huge single line | Medium | Medium | `--max-entry-size` cap; streaming parser with recovery |
| Overlapping functionality with existing tools | Low | Medium | Differentiate on PII redaction, alert rules, OTLP support, local-first |

---

## 10. Release Criteria (v0.2.0)

- [ ] All P0 and P1 features implemented and documented.
- [ ] CLI_SPEC examples pass in CI.
- [ ] PII redaction, alert rules, multi-format export, OTLP parsing tested.
- [ ] TUI keybindings and histogram work correctly.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test` all green.
- [ ] Native Windows `logcli.exe` built and verified.
- [ ] All docs updated (README, PRD, CLI_SPEC, PARSING_SPEC, QUERY_SPEC, CONFIG, DESIGN, TEST_PLAN, PLAN).
- [ ] Changelog updated.

---

## 11. Open Questions

1. Final license: MIT or Apache-2.0 (or dual)?
2. Should OTLP receiver (localhost:4317) be added in v0.3?
3. Should AI-assisted natural-language queries (`--explain`) be added in v0.3?
4. On-disk index for sorting (>100MB files) — v0.3 or v1.0?
5. zstd/bzip2/xz compression support — behind feature flags?