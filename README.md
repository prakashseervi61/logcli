# logcli

Fast, local-first log viewer and processor for **JSON** and **logfmt** logs — inspired by [hl](https://github.com/pamburus/hl).

> **Status:** v0.1.0 MVP. JSON/logfmt/plain parsing, field/level/time filtering, formatted and raw output,
> gzip input, and in-memory sorting work and are covered by tests.
> The query language (`-q`), follow mode (`-F`), pager, themes, and index-backed sorting are **not implemented**
> in this build; those flags fail with a clear error instead of being silently ignored.

## Why

`grep` doesn't understand structured logs; observability platforms need infrastructure. `logcli` reads a multi-GB
log file locally, filters it precisely, and renders it clearly — while remaining fully pipeable.

## Features

Implemented in v0.1.0:

- **Formats:** JSON/JSONL, logfmt, and plain text, with auto-detection per line.
- **Compression:** gzip (including multi-member `.gz`), detected by magic bytes.
- **Filters:** `-l/--level` threshold, `-f/--filter` field filters
  (`=`, `!=`, `~=`, `!~=`, `~~=`, `!~~=`, plus the `key?` absent-field modifier),
  and `--since`/`--until` with RFC 3339, `YYYY-MM-DD`, `now`, `today`, `yesterday`, and `-3h`/`-14d` offsets.
- **Field paths:** nested paths (`request.method`), array wildcards (`span.[].name`), and indexes (`span.[0].name`).
- **Output:** aligned time/level/message columns, color when the output is a TTY, `NO_COLOR` support,
  `-h/--hide` (with `!` reveal and `*`), `-e/-E` empty-field control, `-x` expansion modes,
  `-t` time format, `-Z`/`-L` time zones, `-r` raw passthrough, `-o` file output, NUL-delimited output.
- **Input handling:** stdin or `-`, multiple files, LF/CRLF/CR/NUL delimiters, `--allow-prefix` for prefixed JSON,
  `--max-entry-size` guard.
- **Sorting:** `-s` chronological merge across files (in-memory).
- **Errors:** deterministic exit codes (`0` success, `1` usage/query, `2` I/O, `3` broken pipe, `130` interrupted).

Planned but not in this build: `-q` query language, `-F` follow mode, pager integration, themes, zstd/bzip2/xz,
and index-backed external sorting. See [PLAN.md](PLAN.md).

## Quick start

```sh
# View a log file
logcli app.log

# Only warnings and above
logcli -l warn app.log

# Filter nested fields
logcli -f request.method=GET app.log

# Regex on a field, excluding entries where the field is absent
logcli -f 'message~~="timeout|refused"' app.log

# Sort/merge files chronologically (in-memory)
logcli -s api.log worker.log

# Raw matched lines, for further shell processing
logcli -r -f component=tsdb app.log | grep foo

# Read from stdin
kubectl logs deploy/api | logcli -P -l warn
```

## Installation

From source:

```sh
cargo build --release
./target/release/logcli --help
```

### Building without a local Rust toolchain

If Rust is not installed (for example on Windows without Visual Studio Build Tools), build inside the official
Rust container. The helper scripts keep `target/` and the cargo registry in named Docker volumes so rebuilds are fast:

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

Run the CLI inside the container:

```sh
./scripts/docker-cargo.sh run -- tests/fixtures/simple.jsonl
```

### Native Windows executable

If you want a `logcli.exe` that runs directly in Windows Terminal / PowerShell — no Docker at runtime —
cross-compile it from the Linux container (MinGW is installed in the build image):

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

Until then you can run it from the repository with `.\dist\logcli.exe`.

## Documentation

| Document | Contents |
|---|---|
| [docs/PRD.md](docs/PRD.md) | Requirements, scope, success metrics |
| [docs/CLI_SPEC.md](docs/CLI_SPEC.md) | Command-line contract |
| [docs/DESIGN.md](docs/DESIGN.md) | Architecture and performance design |
| [docs/PARSING_SPEC.md](docs/PARSING_SPEC.md) | Format and parsing rules |
| [docs/QUERY_SPEC.md](docs/QUERY_SPEC.md) | Query language |
| [docs/CONFIG.md](docs/CONFIG.md) | Configuration and themes |
| [docs/TEST_PLAN.md](docs/TEST_PLAN.md) | Testing strategy |
| [PLAN.md](PLAN.md) | Implementation roadmap |

## Project layout

```
src/
  main.rs      entry point and exit-code mapping
  cli.rs       clap flag surface
  app.rs       pipeline wiring, time/size parsing, in-memory sort
  input.rs     file/stdin sources, gzip, delimiter detection
  parser.rs    JSON/logfmt/plain detection and normalization
  filter.rs    level/time/field filters and path resolution
  output.rs    formatting, colors, hiding, expansion, writers
  model.rs     Entry, Level, Format
  error.rs     error types and exit codes
tests/         integration tests and fixtures
docs/          design and specifications
docs/adr/      architecture decision records
scripts/       Docker-based cargo wrappers
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Specifications are normative — behavior changes require a doc update in the same PR.

## License

TBD (MIT or Apache-2.0). Do not depend on this project until a license is added.
