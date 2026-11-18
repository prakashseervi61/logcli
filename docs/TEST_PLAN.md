# Test Plan — `logcli`

**Status:** Draft v0.2
**Related:** [PRD.md](PRD.md) · [CLI_SPEC.md](CLI_SPEC.md) · [PARSING_SPEC.md](PARSING_SPEC.md) · [QUERY_SPEC.md](QUERY_SPEC.md) · [CONFIG.md](CONFIG.md) · [DESIGN.md](DESIGN.md)

---

## 1. Purpose & Scope

Verify that `logcli` meets the requirements in the PRD, behaves exactly as specified in CLI_SPEC, parses per PARSING_SPEC, evaluates queries per QUERY_SPEC, and stays within performance budgets. Covers PII redaction, alert rules, multi-format export, OpenTelemetry parsing, TUI, and follow mode.

**Quality gates for merge:**
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --all-features`
- Integration suite passes on Linux, macOS, Windows
- No performance regression > 10% vs `main` baseline

---

## 2. Test Levels

| Level | Framework | Location | Runs in CI |
|---|---|---|---|
| Unit | `cargo test` | `src/**/tests` or `#[cfg(test)]` | Every push |
| Integration | `assert_cmd` + `predicates` | `tests/*.rs` | Every push |
| Performance | `criterion` + `hyperfine` | `benches/` | PR label `perf` / nightly |
| Manual smoke | script | `scripts/smoke.*` | Pre-release |

---

## 3. Unit Test Coverage Targets

| Module | Must test |
|---|---|
| `parser::json` | valid objects, nested, arrays, escapes, unicode, duplicate keys, trailing garbage, prefix mode |
| `parser::logfmt` | quoted values, escapes, stray tokens, `=` in values, empty values |
| `parser::otel` | resourceLogs/scopeLogs/logRecords, timeUnixNano, severityNumber/Text, body, batching (first record only) |
| `parser::detect` | JSON vs logfmt vs plain vs OTLP, memoization, forced format |
| `parser::normalize` | timestamp keys/units, level spellings/numerics, message/logger/caller fallbacks, OTLP mapping |
| `input::splitter` | LF/CRLF/CR/NUL, no trailing newline, blank lines, max-entry-size |
| `input::decompress` | gzip magic detection, corrupt stream → error |
| `input::FollowSource` | tail new lines, poll timeout, incomplete line handling |
| `filter::level` | threshold semantics, unknown levels, numeric mapping, OTel SeverityNumber |
| `filter::time` | all time formats, keywords, relative offsets, missing timestamps |
| `filter::field` | equality, inequality, substring, regex, numeric coercion, absent modifier |
| `filter::query` | precedence, parentheses, short-circuit, `in`, `exists`, parse errors with columns |
| `config` | precedence (default < file < env < flag), unknown keys, JSON loading, YAML rejection, redaction matchers, alert queries |
| `model::RedactRules` | redaction of message/logger/caller/fields, `*` wildcard, double-apply guard |
| `model::Entry` | flatten (CSV), prometheus_lines, Serialize timestamp as string |
| `output` | layout, expansion modes, flattening, hiding, width under ANSI, CSV escaping, Prometheus format |
| `app` | time/size parsing, alert evaluation, CSV/Prometheus writing |
| `cli` | flag conflicts (`-s` + `-F`), aliases, invalid values, exit codes |
| `tui` | keybindings, filter, sort cycle, follow toggle, details view |

Aim for **≥ 85% line coverage** in `src/` (measured with `cargo-llvm-cov`), with 100% of branch logic in parsers and query evaluation.

---

## 4. Integration Tests (CLI Contract)

Every example in [CLI_SPEC.md §12](CLI_SPEC.md#12-examples-normative) is a test case. Additional required cases:

### 4.1 Input & I/O
- [x] Single file, multiple files, stdin, `-`
- [x] Missing file → exit 2, message includes filename
- [x] Unreadable file (permissions) → exit 2
- [x] Empty file → exit 0, no output
- [x] File without trailing newline
- [x] `.gz` read correctly
- [x] Corrupt compressed stream → exit 2
- [x] Broken pipe: `logcli big.log \| head -1` → exit 3, no error text
- [x] OTLP input format (`--input-format otel`)
- [ ] `.zst`, `.bz2`, `.xz` read correctly (roadmap)

### 4.2 Filtering
- [x] `-l` at every threshold
- [x] `-f` all six operator forms + `?` variants
- [x] `-q` all examples in QUERY_SPEC §10
- [x] `--since/--until` absolute, keyword, relative
- [x] Combination of `-l`, `-f`, `-q`, time → AND semantics
- [x] No match → exit 0, empty output
- [x] Invalid query → exit 1, caret message, no partial output
- [x] Value sets from file (`in @file`) and stdin (`in @-`)

### 4.3 Output
- [x] `--color=never` contains zero ANSI (`\x1b[`)
- [x] `--color=always` contains ANSI even when piped
- [x] `NO_COLOR=1` disables color in auto mode
- [x] `-h key`, `-h '!key'`, `-h '*'`, `-h '!*'`
- [x] `-x` all four modes
- [x] `-t` custom format, `-Z` timezone, `-L`
- [x] `-o file` writes file; stdout remains empty
- [x] `--output-delimiter=nul` produces NUL-separated records
- [x] `-r` reproduces original bytes exactly (round-trip test)
- [x] `--flatten=never` preserves nesting visually
- [x] `--output-format json` produces valid JSON Lines
- [x] `--output-format csv` produces valid CSV with header
- [x] `--output-format prometheus` produces valid Prometheus text exposition
- [x] `--output-format raw` writes source lines verbatim

### 4.4 PII Redaction
- [x] `redact:` rule on `message` field masks email addresses
- [x] `redact:` rule on `*` field masks all string field values
- [x] Redaction applied to `logger` and `caller` fields
- [x] Redaction applied only once per entry (`redacted` flag)
- [x] Invalid regex pattern → config error (exit 1)

### 4.5 Alert Rules
- [x] Alert triggered when count >= threshold → exit with `exit_code`
- [x] Alert not triggered when count < threshold → exit 0
- [x] Multiple alerts: max exit code used
- [x] Invalid alert query → skipped (not fatal)
- [x] Alert summary printed to stderr

### 4.6 Sorting & Follow
- [x] `-s` on shuffled multi-file input yields globally sorted output
- [x] Entries without timestamps appear last, stable
- [x] `-F` follows new lines in files
- [x] `-s -F` together → exit 1 with usage error
- [ ] Index file created; second run faster (on-disk index roadmap)
- [ ] Modified file invalidates index (roadmap)
- [ ] `--dump-index` prints metadata (roadmap)

### 4.7 Config
- [x] `--config file` overrides defaults
- [x] Env var overrides config; flag overrides env
- [x] `--config -` disables auto-loading
- [x] Invalid config → exit 1 with file + key
- [x] Auto-discovery: `logcli.json` in CWD loaded automatically
- [x] YAML config rejected (requires serde_yaml)

### 4.8 TUI
- [x] `--tui` launches and displays entries
- [x] `j`/`k` scroll, `g`/`G` top/bottom
- [x] `/` opens filter prompt
- [x] `Esc` clears the active filter
- [x] `d`/`Enter` focuses the details pane
- [x] `s` cycles sort mode
- [x] `f`/`F` toggles follow mode
- [x] `h` toggles histogram view
- [x] `?` shows help
- [x] `q` quits
- [x] Header shows source and ERRORS/WARNs/INFO counts
- [x] Entries table renders TIMESTAMP/LEVEL/MESSAGE columns
- [x] Details pane renders syntax-colored JSON for the selection
- [x] Bottom bar shows key hints and `Matches: filtered/total`
- [x] Auto-launch when no files + stdin is TTY

### 4.9 Platform
- [ ] Windows: paths with backslashes and spaces
- [ ] Windows: ANSI disabled when not a terminal
- [ ] macOS/Linux: SIGINT exits 130
- [ ] Unicode: CJK/emoji/RTL output width does not break layout

---

## 5. Fixtures

All fixtures in `tests/fixtures/`; each is small (< 1 MB) and deterministic.

| Fixture | Used by |
|---|---|
| `simple.jsonl`, `nested.jsonl` | parser, query, output |
| `otel.jsonl` | OTLP parsing |
| `logfmt.log`, `mixed.log` | detection, parser |
| `prefixed.log` | `--allow-prefix` |
| `multiline.jsonl` | `-x` modes |
| `levels.jsonl` | level normalization |
| `timestamps.jsonl` | time parsing/filtering |
| `malformed.log` | resilience |
| `unicode.log` | width/encoding |
| `web.log` | Apache parser (P1) |
| `compressed/simple.jsonl.gz` | decompression |
| `sort/a.log`, `sort/b.log`, `sort/c.log` | merge ordering |
| `queries/*.txt` | data-driven query tests |

**Golden rule:** fixtures are never edited in place to make a test pass; add a new fixture and document why.

---

## 6. Property-Based Tests

1. **No panics:** arbitrary bytes into the splitter/parsers never panic.
2. **Round-trip:** for any JSON value serialized to a line, parse → format → re-parse yields structurally equal values.
3. **Raw fidelity:** `-r` output is a byte-for-byte subsequence of input lines.
4. **Sort stability:** sorting a permutation of entries yields identical output regardless of input order (ties broken deterministically).
5. **Filter algebra:** `not (not A)` ≡ `A`; `A and true` ≡ `A`; `A or false` ≡ `A`.
6. **PII idempotency:** applying redaction twice produces the same result as once.
7. **Alert determinism:** same entries + same alert rules → same triggered set and exit code.

---

## 7. Performance Tests

Reference machine(s) documented in `benches/README.md`; results committed to `benches/results/`.

### 7.1 Benchmarks (criterion)

| Benchmark | Input | Budget |
|---|---|---|
| `parse_json` | 1M JSON lines | ≥ 1.5 GiB/s |
| `parse_logfmt` | 1M logfmt lines | ≥ 1.0 GiB/s |
| `parse_otel` | 1M OTLP lines | ≥ 1.0 GiB/s |
| `filter_simple` | 1M lines, `level >= warn` | ≥ 3 GiB/s |
| `filter_regex` | 1M lines, one regex | ≥ 500 MiB/s |
| `redact_pii` | 1M lines with redaction rules | ≥ 500 MiB/s |
| `format_plain` | 1M lines, no color | ≥ 1.0 GiB/s |
| `format_color` | 1M lines, truecolor | ≥ 700 MiB/s |
| `format_csv` | 1M lines, CSV export | ≥ 500 MiB/s |
| `format_prometheus` | 1M lines, Prometheus export | ≥ 500 MiB/s |
| `startup` | `--version` | < 100 ms |

### 7.2 End-to-end (hyperfine)

- 2.3 GiB JSON: `logcli -c -o /dev/null file.log` → ≤ 2 s
- Same with `-l w -f component=tsdb` → ≤ 2 s
- With PII redaction → ≤ 3 s
- With alert evaluation → ≤ 3 s
- `-s` cold (in-memory sort) → ≤ 8 s; warm → ≤ 3 s
- Peak RSS ≤ 200 MiB (no sort), ≤ 300 MiB (sort)

Result regressions > 10% require justification in the PR or a fix.

---

## 8. CI Matrix

| OS | Rust | Notes |
|---|---|---|
| ubuntu-latest | stable + MSRV | Full suite, benchmarks (not gating), coverage |
| macos-latest | stable | Full suite |
| windows-latest | stable | Full suite, path/ANSI tests |

Caching: `Swatinem/rust-cache`. Artifacts: test reports, coverage (`cargo-llvm-cov`), benchmark baseline.

---

## 9. Release Checklist

- [ ] Version bumped per SemVer; CHANGELOG updated
- [ ] All quality gates green on all platforms
- [ ] Performance report regenerated and committed
- [ ] Docs updated (CLI_SPEC, CONFIG, QUERY_SPEC, PARSING_SPEC) for any behavior change
- [ ] `--help` and man page regenerated
- [ ] Binaries built for linux/macos/windows (x86_64 + arm64 where applicable)
- [ ] Native Windows `logcli.exe` built via `.\scripts\build-windows.ps1 -Install` and verified
- [ ] Checksums published
- [ ] Smoke test install from release artifact
- [ ] Tag pushed; release notes from CHANGELOG