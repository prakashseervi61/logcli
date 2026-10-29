# Product Requirements Document (PRD) — `logcli`

**Status:** Draft v0.1
**Owner:** Prakash
**Last updated:** 2026-10-28
**Related docs:** [PLAN.md](../PLAN.md) · [CLI_SPEC.md](CLI_SPEC.md) · [DESIGN.md](DESIGN.md) · [PARSING_SPEC.md](PARSING_SPEC.md) · [QUERY_SPEC.md](QUERY_SPEC.md)

---

## 1. Overview

`logcli` is a high-performance command-line log viewer and processor, inspired by [hl](https://github.com/pamburus/hl). It reads structured logs (JSON and logfmt), applies fast filtering, and renders them in a human-readable, colorized format — while remaining fully pipeable and script-friendly.

The design goal is simple: **make a 2 GB log file readable and analyzable in about a second, without a database, a server, or a browser.**

---

## 2. Problem Statement

Developers and operators deal with structured logs daily, but the tooling is fragmented:

- `cat`/`grep`/`less` do not understand structure; JSON logs are unreadable in raw form.
- Log aggregation platforms (ELK, Loki, Datadog) require infrastructure and network access; they are overkill for a downloaded log file or a local `kubectl logs` stream.
- Existing local viewers are either slow (`humanlog` on multi-GB files), unmaintained, or do not support structured filtering (field-level, nested, boolean queries).
- Piping between `jq`, `grep`, `sed`, `less` is powerful but verbose and slow, and loses timestamp ordering across multiple files.

**Users need a single binary that reads logs fast, filters them precisely, and displays them clearly — locally and offline.**

---

## 3. Target Users

| Persona | Context | Primary needs |
|---|---|---|
| **Backend developer** | Debugging a service locally or in a container | Read JSON logs clearly, filter by `level` and fields, follow live output |
| **SRE / on-call engineer** | Incident response, downloaded logs from production | Merge multiple files chronologically, filter by time window and error level, fast startup |
| **DevOps engineer** | CI pipelines, post-mortem analysis | Pipe-friendly, deterministic exit codes, works over SSH with no dependencies |
| **Data/Support engineer** | Ad-hoc investigation of customer logs | Query language for nested fields, substring/regex filters, export raw matched lines |

**Non-users (for v1):** Users who need dashboards, alerting, long-term retention, or multi-tenant querying. They should use an observability platform.

---

## 4. Goals

1. **Performance:** Parse and render a 2.3 GiB / 6M-line JSON log in under 3 seconds on a modern laptop (target: ≤ 2 s); sorting index build at ≥ 500 MiB/s.
2. **Fidelity:** Correctly parse JSON and logfmt, including nested objects, arrays, non-JSON prefixes, and multi-line values.
3. **Precision:** Filter by level, field value, substring, regex, boolean query, and time range — without piping to other tools.
4. **Ergonomics:** Zero-config startup; readable defaults; pager integration; NO required install beyond a single binary.
5. **Composability:** Read stdin, write stdout, respect `NO_COLOR`, exit codes suitable for scripts.
6. **Stability:** No panics on malformed input; skip and report bad lines without aborting the stream.

---

## 5. Non-Goals (v1)

- Full-text search engine or inverted index over message bodies.
- Statistical dashboards or charts (that is `goaccess`'s domain).
- Remote log shipping, agents, or server components.
- Log format plugins loaded dynamically (WASM etc.).
- Web UI or TUI (v1 is a streaming CLI; TUI is a possible v2).
- Windows-native pager implementation (we rely on external pagers).
- Windows event log, journald, or Kubernetes API integration.

Explicitly out of scope avoids the "second product" trap.

---

## 6. Feature Requirements

Priorities: **P0 = MVP**, **P1 = v1.0**, **P2 = post-1.0**.

### 6.1 Input (P0)
- Read one or many files; read stdin when no file is given or when `-` is passed.
- Auto-detect gzip, zstd, bzip2, and xz compression by magic bytes.
- Auto-detect entry delimiter (LF, CRLF, CR, NUL).
- Support non-JSON prefixes before a JSON object (`--allow-prefix`).
- Graceful handling of invalid lines: skip with a warning count (verbose mode lists them).

### 6.2 Parsing (P0)
- JSON and JSONL parsing with arbitrary nesting.
- logfmt parsing (`key=value`, quoted values, escaped characters).
- Auto-detection between JSON and logfmt per line, with `--input-format` override.
- Timestamp extraction from common field names and formats (RFC3339, Unix s/ms/µs/ns, configurable custom formats).
- Level normalization: `trace/debug/info/warn/warning/error/fatal/panic`, numeric levels, and case-insensitive matching.
- Message extraction from `message`, `msg`, `log`, and configurable keys.
- Caller/logger extraction (`caller`, `logger`, `source`, configurable).

### 6.3 Filtering (P0)
- `--level` threshold filtering (e.g. `-l w` shows warn and above).
- `--filter` on field expressions: `k=v`, `k!=v`, `k~=substring`, `k!~=substring`, `k~~=regex`.
- Hierarchical field paths (`request.method`, `headers.content-type`).
- Array traversal (`span.[].name`) and indexed access (`span.[1].name`).
- Boolean query expressions (`--query`): `and`/`or`/`not`, comparisons, `in (...)`, `exists(...)`.
- `--since` / `--until` with absolute timestamps, keywords (`today`, `yesterday`, weekday names), and relative offsets (`-3h`, `-14d`).
- Missing-field semantics (`?` modifier) as defined in [QUERY_SPEC.md](QUERY_SPEC.md).

### 6.4 Output (P0)
- Human-readable default format: time, level, logger, message, fields.
- ANSI color by default on TTY; `--color auto|always|never`, `--no-color` alias, `NO_COLOR` env support.
- Pager auto-invocation on TTY (`--paging auto|always|never`, `-P` alias).
- Field hiding/revealing (`-h key`, `-h '!key'`, wildcard `*`).
- Empty-field hiding (`-e`), expansion modes (`-x never|inline|auto|always`).
- Time format (`-t`) and timezone (`-Z`, `-L`).
- Raw passthrough (`-r`) for piping matched source lines.
- Output to file (`-o`).

### 6.5 Sorting & Following (P1)
- Chronological merge across multiple files (`-s`) using an on-disk index.
- Incremental reindex of growing files, skipping unchanged blocks.
- Follow mode (`-F`) with `--tail N` preload and `--sync-interval-ms` merge window.

### 6.6 Configuration & Theming (P1)
- Config file auto-load from OS-specific locations; `--config` and `HL_CONFIG`/`LOGCLI_CONFIG` overrides.
- Precedence: defaults < config < env < CLI flags.
- Built-in themes, user themes in config dir, theme overlays.
- Shell completions and man page generation (P2).

### 6.7 Not included, but considered (P2)
- Custom timestamp/level field mapping via config.
- `--output-format json|logfmt` (re-serialize filtered output).
- Basic aggregations (`--count-by field`).
- Watch mode with desktop notifications.

---

## 7. Success Metrics

| Metric | Target | Measurement |
|---|---|---|
| Throughput (parse+format) | ≥ 2 GiB/s on 6M-line JSON | `hyperfine 'logcli -c -o NUL sample.log'` on M1/AMD 5800X class CPU |
| Cold start | < 50 ms | `hyperfine 'logcli --version'` |
| Peak RSS on 2.3 GiB file (no sort) | < 200 MiB | `/usr/bin/time -v` / `Get-Process` |
| Sort index build rate | ≥ 500 MiB/s | benchmark script |
| Invalid-line resilience | 0 panics across fuzz corpus | `cargo fuzz` + CI corpus |
| CLI correctness | 100% of CLI_SPEC examples tested | integration test suite |
| Binary size | < 15 MiB stripped | release build |

**Adoption metrics (post-release, optional):** GitHub stars, crates.io installs, issues triaged < 7 days.

---

## 8. Assumptions & Constraints

- Logs are newline- or NUL-delimited; arbitrary binary blobs are out of scope.
- Structured logs (JSON/logfmt) are the primary target; plain text is passed through with minimal formatting.
- v1 targets 64-bit Linux, macOS, and Windows.
- Sorting requires disk space roughly equal to the index size (~10–20 bytes/entry) in the cache dir.
- No network access is required at runtime.
- Single-user CLI; no authentication, no telemetry, no background service.

---

## 9. Risks & Mitigations

| Risk | Impact | Likelihood | Mitigation |
|---|---|---|---|
| Performance regressions vs hl | Medium | Medium | CI benchmarks; budgeted allocations; criterion benchmarks |
| Query parser scope creep | High | High | Freeze grammar in QUERY_SPEC; explicit non-goals; feature flag P2 |
| Index corruption / stale index | Medium | Medium | Versioned index format; atomic rename; size+mtime validation; `--dump-index` |
| Windows pager / ANSI quirks | Low | High | Detect TTY + Windows Terminal; document alternative pagers; `--paging never` |
| Malformed or huge single line | Medium | Medium | `--max-entry-size` cap (default 64 MiB); streaming parser with error recovery |
| Overlapping functionality with existing tools | Low | Medium | Differentiate on speed + local-first + structured filtering, not dashboards |

---

## 10. Release Criteria (v1.0)

- [ ] All P0 and P1 features implemented and documented.
- [ ] CLI_SPEC examples pass in CI on Linux, macOS, Windows.
- [ ] Performance targets met on the reference machine (documented results).
- [ ] No known panic/crash bugs; fuzz corpus clean for 24h.
- [ ] README, CONFIG, PARSING, QUERY docs complete.
- [ ] Installable via `cargo install`, GitHub release binaries, and (nice-to-have) Homebrew/Scoop.
- [ ] Changelog and semantic version 1.0.0.

---

## 11. Open Questions

1. Final binary/module name: keep `logcli` or adopt a distinctive name (`logscope`, `lens`)? Names must not collide on crates.io.
2. Should `--sort` index be enabled by default for multi-file input, or opt-in only?
3. Do we need a `--json-out` re-serialization mode in v1 for scripting?
4. Is logfmt a P0 or P1 format? Current plan: P0 (cheap to implement, high value).
5. Which license? MIT or Apache-2.0 (or dual)?
