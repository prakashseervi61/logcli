# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
