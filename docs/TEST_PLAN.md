# Test Plan — `logcli`

**Status:** Draft v0.1
**Related:** [PRD.md](PRD.md) · [CLI_SPEC.md](CLI_SPEC.md) · [PARSING_SPEC.md](PARSING_SPEC.md) · [QUERY_SPEC.md](QUERY_SPEC.md) · [DESIGN.md](DESIGN.md)

---

## 1. Purpose & Scope

Verify that `logcli` meets the requirements in the PRD, behaves exactly as specified in CLI_SPEC, parses per PARSING_SPEC, evaluates queries per QUERY_SPEC, and stays within performance budgets. Out of scope: load testing of external services (there are none), UI testing (CLI only).

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
| Property | `proptest` | `tests/prop/` | Every push |
| Fuzz | `cargo-fuzz` | `fuzz/` | Nightly + on parser changes |
| Integration | `assert_cmd` + `predicates` | `tests/*.rs` | Every push |
| Golden/snapshot | `insta` | `tests/snapshots/` | Every push |
| Performance | `criterion` + `hyperfine` | `benches/` | PR label `perf` / nightly |
| Manual smoke | script | `scripts/smoke.*` | Pre-release |

---

## 3. Unit Test Coverage Targets

| Module | Must test |
|---|---|
| `parser::json` | valid objects, nested, arrays, escapes, unicode, duplicate keys, trailing garbage, prefix mode |
| `parser::logfmt` | quoted values, escapes, stray tokens, `=` in values, empty values |
| `parser::detect` | JSON vs logfmt vs plain, memoization, forced format |
| `parser::normalize` | timestamp keys/units, level spellings/numerics, message/logger/caller fallbacks |
| `input::splitter` | LF/CRLF/CR/NUL, no trailing newline, blank lines, max-entry-size |
| `input::decompress` | gzip/zstd/bzip2/xz magic detection, corrupt stream → error |
| `filter::level` | threshold semantics, unknown levels, numeric mapping |
| `filter::time` | all time formats, keywords, relative offsets, missing timestamps |
| `filter::field` | equality, inequality, substring, regex, numeric coercion, absent modifier |
| `filter::query` | precedence, parentheses, short-circuit, `in`, `exists`, parse errors with columns |
| `index` | header round-trip, delta encoding, CRC, version mismatch, merge ordering |
| `output::format` | layout, expansion modes, flattening, hiding, width under ANSI |
| `output::theme` | inheritance, overlays, invalid theme errors |
| `config` | precedence (default < file < env < flag), unknown keys |
| `cli` | flag conflicts (`-s` + `-F`), aliases, invalid values, exit codes |

Aim for **≥ 85% line coverage** in `src/` (measured with `cargo-llvm-cov`), with 100% of branch logic in parsers and query evaluation.

---

## 4. Integration Tests (CLI Contract)

Every example in [CLI_SPEC.md §10](CLI_SPEC.md#10-examples-normative) is a test case. Additional required cases:

### 4.1 Input & I/O
- [ ] Single file, multiple files, stdin, `-`
- [ ] Missing file → exit 2, message includes filename
- [ ] Unreadable file (permissions) → exit 2
- [ ] Empty file → exit 0, no output
- [ ] File without trailing newline
- [ ] `.gz`, `.zst`, `.bz2`, `.xz` all read correctly
- [ ] Corrupt compressed stream → exit 2
- [ ] Broken pipe: `logcli big.log | head -1` → exit 3, no error text

### 4.2 Filtering
- [ ] `-l` at every threshold
- [ ] `-f` all six operator forms + `?` variants
- [ ] `-q` all examples in QUERY_SPEC §10
- [ ] `--since/--until` absolute, keyword, relative (freeze clock or use generated fixtures relative to now)
- [ ] Combination of `-l`, `-f`, `-q`, time → AND semantics
- [ ] No match → exit 0, empty output
- [ ] Invalid query → exit 1, caret message, no partial output

### 4.3 Output
- [ ] `--color=never` contains zero ANSI (`\x1b[`)
- [ ] `--color=always` contains ANSI even when piped
- [ ] `NO_COLOR=1` disables color in auto mode
- [ ] `-h key`, `-h '!key'`, `-h '*'`, `-h '!*'`
- [ ] `-x` all four modes
- [ ] `-t` custom format, `-Z` timezone, `-L`
- [ ] `-o file` writes file; stdout remains empty
- [ ] `--output-delimiter=nul` produces NUL-separated records
- [ ] `-r` reproduces original bytes exactly (round-trip test)
- [ ] `--flatten=never` preserves nesting visually

### 4.4 Sorting & Follow
- [ ] `-s` on shuffled multi-file input yields globally sorted output
- [ ] Entries without timestamps appear last, stable
- [ ] Re-running `-s` reuses index (assert index file created; second run faster / no rewrite)
- [ ] Modified file invalidates index
- [ ] `--dump-index` prints metadata
- [ ] `-F` emits appended lines within 2× sync interval (timing test, generous bounds)
- [ ] `-F --tail 5` preloads exactly 5
- [ ] `-s -F` together → exit 1 with usage error

### 4.5 Config
- [ ] `--config file` overrides defaults
- [ ] Env var overrides config; flag overrides env
- [ ] `--config -` disables auto-loading
- [ ] Invalid config → exit 1 with file + key

### 4.6 Platform
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
| `logfmt.log`, `mixed.log` | detection, parser |
| `prefixed.log` | `--allow-prefix` |
| `multiline.jsonl` | `-x` modes |
| `levels.jsonl` | level normalization |
| `timestamps.jsonl` | time parsing/filtering |
| `malformed.log` | resilience |
| `unicode.log` | width/encoding |
| `web.log` | Apache parser (P1) |
| `compressed/*` | decompression matrix |
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

---

## 7. Fuzzing

Targets: JSON parser, prefix scanner, logfmt parser, query parser, delimiter splitter.

- `cargo +nightly fuzz run <target>`
- Seed corpus from `tests/fixtures/`.
- Nightly CI job: 10 minutes per target; crash corpus checked in.
- Any crash blocks release.

---

## 8. Performance Tests

Reference machine(s) documented in `benches/README.md`; results committed to `benches/results/`.

### 8.1 Benchmarks (criterion)

| Benchmark | Input | Budget |
|---|---|---|
| `parse_json` | 1M JSON lines | ≥ 1.5 GiB/s |
| `parse_logfmt` | 1M logfmt lines | ≥ 1.0 GiB/s |
| `filter_simple` | 1M lines, `level >= warn` | ≥ 3 GiB/s |
| `filter_regex` | 1M lines, one regex | ≥ 500 MiB/s |
| `format_plain` | 1M lines, no color | ≥ 1.0 GiB/s |
| `format_color` | 1M lines, truecolor | ≥ 700 MiB/s |
| `index_build` | 1M lines | ≥ 500 MiB/s |
| `index_merge` | 3 × 1M lines | ≥ 300 MiB/s |
| `startup` | `--version` | < 50 ms |

### 8.2 End-to-end (hyperfine)

- 2.3 GiB JSON: `logcli -c -o NUL file.log` → ≤ 2 s
- Same with `-l w -f component=tsdb` → ≤ 2 s
- `-s` cold (build index) → ≤ 8 s; warm → ≤ 3 s
- Peak RSS ≤ 200 MiB (non-sort), ≤ 300 MiB (sort)

Result regressions > 10% require justification in the PR or a fix.

---

## 9. CI Matrix

| OS | Rust | Notes |
|---|---|---|
| ubuntu-latest | stable + MSRV | Full suite, benchmarks (not gating), coverage |
| macos-latest | stable | Full suite |
| windows-latest | stable | Full suite, path/ANSI tests |
| ubuntu-latest | nightly | Fuzz smoke (10 min/target) |

Caching: `Swatinem/rust-cache`. Artifacts: test reports, coverage (`cargo-llvm-cov`), benchmark baseline.

---

## 10. Release Checklist

- [ ] Version bumped per SemVer; CHANGELOG updated
- [ ] All quality gates green on all platforms
- [ ] Performance report regenerated and committed
- [ ] Docs updated (CLI_SPEC, CONFIG, QUERY_SPEC) for any behavior change
- [ ] `--help` and man page regenerated
- [ ] Binaries built for linux/macos/windows (x86_64 + arm64 where applicable)
- [ ] Checksums published
- [ ] Smoke test install from release artifact
- [ ] Tag pushed; release notes from CHANGELOG
