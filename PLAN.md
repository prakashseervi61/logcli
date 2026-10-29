# Log Analysis CLI — Implementation Roadmap

A fast, local-first log viewer/processor for JSON and logfmt logs, inspired by [hl](https://github.com/pamburus/hl).

## Documentation Map

All project documents live in [`docs/`](docs/README.md):

| Document | Purpose |
|---|---|
| [docs/PRD.md](docs/PRD.md) | Requirements, scope, users, success metrics |
| [docs/CLI_SPEC.md](docs/CLI_SPEC.md) | **Normative** CLI contract: flags, exit codes, examples |
| [docs/DESIGN.md](docs/DESIGN.md) | Architecture, pipeline, indexing, concurrency, performance budgets |
| [docs/PARSING_SPEC.md](docs/PARSING_SPEC.md) | **Normative** parsing rules, formats, timestamps, levels |
| [docs/QUERY_SPEC.md](docs/QUERY_SPEC.md) | **Normative** query grammar, operators, semantics |
| [docs/CONFIG.md](docs/CONFIG.md) | Config files, env vars, themes |
| [docs/TEST_PLAN.md](docs/TEST_PLAN.md) | Test strategy, fixtures, benchmarks, CI, release checklist |
| [docs/adr/](docs/adr/) | Architecture Decision Records 0001–0004 |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Dev setup, style, commit/PR process |
| [CHANGELOG.md](CHANGELOG.md) | Version history |

**Rule:** CLI_SPEC, PARSING_SPEC, and QUERY_SPEC are normative. Behavior changes require a spec update in the same PR.

## Status (v0.1.0 MVP)

Implemented and tested:

- JSON/JSONL, logfmt, and plain-text parsing with auto-detection and `--allow-prefix`
- gzip input, LF/CRLF/CR/NUL delimiters, multiple files and stdin
- level/time/field filtering, nested and array paths, relative and absolute time ranges
- formatted and raw output, colors, hiding, expansion, time zones, file output
- in-memory `-s` sorting
- exit codes and error model

Not implemented (rejected with a clear error or accepted as no-ops): `-q`, `-F`, pager, themes,
zstd/bzip2/xz, index-backed external sorting, config files, completions.

18 unit tests + 15 integration tests pass. Build/test via Docker is documented in
[CONTRIBUTING.md](CONTRIBUTING.md).

## Goal

Build a single binary that reads multi-GB logs quickly, filters them precisely (level, nested fields, boolean queries, time ranges), and renders them clearly — while remaining fully pipeable.

## Reference: hl

hl is the design benchmark:

- JSON + logfmt auto-detection, non-JSON prefix support
- Field-level, level, and time-range filtering; boolean query language
- Chronological sorting via indexed scans; live follow mode
- Compression support, pager integration, themes
- ~2 GiB/s on a 2.3 GiB / 6M-line file

Our target: **match its UX and approach 80% of its throughput in v1**, with a clearer specification and a smaller feature surface.

## Tech Stack

| Layer | Choice | Rationale |
|---|---|---|
| Language | **Rust** (stable, MSRV pinned) | Performance, safety, single binary — [ADR 0001](docs/adr/0001-use-rust.md) |
| CLI | `clap` (derive) | Ergonomics, help, completions |
| JSON | `serde_json` (evaluate a borrowed parser later) | Correctness first; benchmark before optimizing |
| Time | `chrono` (evaluate `jiff`) | Timezone correctness |
| Regex | `regex` | Compile-once filters |
| Compression | `flate2`, `zstd`, `bzip2`, `xz2` | Feature-gated |
| Colors | `nu-ansi-term` or custom writer | Role-based themes — [ADR 0004](docs/adr/0004-theme-system.md) |
| Parallelism | `rayon` (index build only) | Ordered output preserved |
| Config | `serde` + `toml`/`serde_yaml`/`serde_json` | One schema, three formats |
| Errors | `thiserror` | Typed errors mapped to exit codes |
| Testing | `assert_cmd`, `predicates`, `insta`, `proptest`, `criterion` | See [TEST_PLAN](docs/TEST_PLAN.md) |

Rejected: Go (GC/allocation costs in hot parsing), C++ (safety/tooling), Python (throughput).

## Architecture (summary)

```
read → decompress → split → parse → normalize → filter → format → write
                                     │
                                     └─ sort mode → index write → k-way merge → lazy read
```

- Streaming, bounded memory; sorting is the only mode requiring disk.
- On-disk index format and merge strategy: [DESIGN §5](docs/DESIGN.md#5-sorting--indexing), [ADR 0003](docs/adr/0003-index-for-sort.md).
- Data model (`Entry`, `Value`, `Level`) and module map: [DESIGN §3–4](docs/DESIGN.md).

## Feature Scope

### P0 — MVP
- Files/stdin/multiple files; gzip/zstd/bzip2/xz by magic bytes; LF/CRLF/CR/NUL
- JSON + logfmt + plain text; `--allow-prefix`
- Timestamp normalization (RFC3339, Unix s/ms/µs/ns); level normalization
- `-l` level threshold; `-f` field filters (6 operator forms + `?`); `--since/--until`
- Formatted colorized output; `-h` hide/reveal; `-e`; `-x`; `-t`/`-Z`/`-L`
- Pager integration; `--color`; `-r` raw; `-o` file
- Clean exit codes and error messages

### P1 — v1.0
- `-q` boolean query language (full QUERY_SPEC)
- `-s` sorted merge with disk index; `-F` follow mode with `--tail`
- Config files + env vars; themes + overlays
- Shell completions, man page

### P2 — Post-1.0
- Apache/Nginx and syslog parsers
- `--output-format json|logfmt`, `--count-by`
- `index` maintenance subcommands, `--ignore-case`, `--strict`
- TUI mode (evaluate separately)

Out of scope: dashboards/charts, remote shipping, web UI, plugins, alerting.

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
│   ├── main.rs  cli.rs  app.rs  error.rs  model.rs
│   ├── input.rs  parser.rs  filter.rs  output.rs
│   └── (planned: query/, index/, theme/, config.rs)
├── tests/cli.rs + fixtures/
└── scripts/docker-cargo.{sh,ps1}
```

Planned modules from later phases (query engine, on-disk index, themes, config) will be added as
submodules rather than growing the existing files past their current responsibilities.

## Implementation Phases

### Phase 0 — Documentation (done)
- [x] PRD, CLI spec, design, parsing spec, query spec, config spec, test plan
- [x] ADRs 0001–0004
- [x] README, CONTRIBUTING, CHANGELOG, docs index
- [ ] Resolve PRD open questions (name, license, logfmt priority, `--sort` default)

### Phase 1 — Foundation (Week 1) — done in MVP
- [x] `cargo init`; pin MSRV
- [x] `cli.rs` with the full flag surface from CLI_SPEC (including `--help` output)
- [x] `model` types; input sources + gzip + delimiter detection
- [x] JSON parser + `normalize` (timestamp/level/message)
- [x] Minimal formatter; pipeline wiring in `app.rs`
- **Exit:** `logcli fixture.jsonl` renders correctly; `logcli -l w` filters.

### Phase 2 — Filtering (Week 2) — done in MVP
- [x] Level threshold and numeric mapping
- [x] Field filters (all forms), hierarchical paths, absent modifier
- [x] Timestamp parsing + `--since/--until` (absolute, keyword, relative)
- [x] logfmt parser and format detection
- [x] Error model and exit codes per CLI_SPEC
- **Exit:** parsing and filter examples covered by tests.
- [ ] Add wildcard operators and the QUERY_SPEC §10 matrix as dedicated tests

### Phase 3 — Query Language (v0.2 — completed)
- [x] Lexer, parser (EBNF in QUERY_SPEC), AST
- [x] Evaluator with coercion rules, arrays, `in`, `exists`, absent modifier `?`, value sets (`@file`, `@-`)
- [x] Caret-positioned error messages
- [x] Cost-based term ordering; regex compilation
- [x] QUERY_SPEC test matrix and unit tests green (27 new integration tests)
- **Exit:** QUERY_SPEC §10 examples and test matrix pass.

### Phase 4 — Output & UX (Week 4)
- [ ] Theme engine (`@base` + overlays), built-in themes
- [ ] Expansion modes, hiding, flattening, empty-field rules
- [ ] Pager integration, TTY detection, `NO_COLOR`, broken-pipe handling
- [ ] Config loading with precedence
- **Exit:** CLI_SPEC output tests and snapshots green on all platforms.

### Phase 5 — Sorting & Follow (Week 5)
- [ ] Index format (write/read/validate), `--dump-index`
- [ ] K-way merge; index-only level/time filtering fast path
- [ ] Incremental reindex; cache dir management
- [ ] `-F`, `--tail`, `--sync-interval-ms`, stdin follow
- **Exit:** sort/follow tests in TEST_PLAN §4.4 green; benchmarks within budget.

### Phase 6 — Hardening & Release (Week 6)
- [ ] Fuzzing (parsers + query) for 24h clean
- [ ] Performance tuning against budgets; publish results
- [ ] Cross-platform CI, coverage ≥ 85%
- [ ] README/docs final pass; completions + man page
- [ ] Release binaries, checksums, v1.0.0 tag
- **Exit:** TEST_PLAN §10 release checklist complete.

## Testing & Quality Gates

See [TEST_PLAN.md](docs/TEST_PLAN.md). Merge requires:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features` on Linux, macOS, Windows
- No benchmark regression > 10%
- Doc updates when behavior changes

## Performance Budgets

| Scenario | Target |
|---|---|
| 2.3 GiB JSON, parse+format to NUL | ≤ 2 s |
| Peak RSS (no sort) | ≤ 200 MiB |
| Index build | ≥ 500 MiB/s |
| Sorted merge (warm index) | ≥ 300 MiB/s |
| Startup (`--version`) | < 50 ms |
| Binary size (stripped) | < 15 MiB |

## Immediate Next Steps

1. Implement the `-q` query language (lexer, parser, evaluator) per QUERY_SPEC.
2. Replace in-memory `-s` with the on-disk index and k-way merge (ADR 0003).
3. Implement `-F` follow mode with `--tail`.
4. Add theme engine and pager integration.
5. Add zstd/bzip2/xz support behind feature flags.
6. Add CI workflow with fmt/clippy/test on three platforms.
7. Resolve remaining PRD open questions (name, license, logfmt priority, `--sort` default).

## Reference Material

- [hl source](https://github.com/pamburus/hl) — architecture and performance techniques
- [hl README](https://github.com/pamburus/hl#readme) — feature/flag inventory
- [Keep a Changelog](https://keepachangelog.com/) — changelog format
- [ADR guidance](https://adr.github.io/) — decision record structure
