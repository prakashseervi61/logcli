# Contributing to `logcli`

Thanks for your interest. This project is specification-first: behavior is defined in `docs/` before it is implemented.

## Prerequisites

- Rust stable (MSRV pinned in `Cargo.toml`)
- `cargo-fmt`, `cargo-clippy` (installed via rustup)
- Optional: `cargo-nextest`, `cargo-llvm-cov`, `cargo-fuzz`, `hyperfine`

## Setup

```sh
git clone <repo-url>
cd logcli
cargo build
cargo test
```

### Building without a local Rust toolchain (Docker)

If Rust is not installed (for example on Windows without Visual Studio Build Tools), use the
Rust container. The scripts build a small dev image from `scripts/Dockerfile.dev` (Rust plus
`rustfmt` and `clippy`) and keep `target/` and the cargo registry in named volumes so rebuilds
stay fast:

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

Run the same quality gates CI uses:

```sh
./scripts/docker-cargo.sh fmt --check
./scripts/docker-cargo.sh clippy --all-targets -- -D warnings
```

## Development commands

| Task | Command |
|---|---|
| Format | `cargo fmt --all` |
| Lint | `cargo clippy --all-targets --all-features -- -D warnings` |
| Test | `cargo test --all-features` |
| Coverage | `cargo llvm-cov --all-features --summary-only` |
| Benchmarks | `cargo bench` |
| Fuzz | `cargo +nightly fuzz run parse_json` |
| Docs | `cargo doc --no-deps --open` |

The CI gate is exactly: fmt check, clippy `-D warnings`, all tests, on Linux/macOS/Windows.

## Before writing code

1. Read [docs/PRD.md](docs/PRD.md) for scope and non-goals.
2. Read [docs/CLI_SPEC.md](docs/CLI_SPEC.md) for the behavior contract.
3. Read [docs/DESIGN.md](docs/DESIGN.md) for architecture.
4. For parsing/query changes, read the respective spec first.

If your change adds or alters user-visible behavior, update the normative doc **in the same PR**. A PR that changes behavior without a spec update will be asked to add one.

## Code style

- `rustfmt` defaults; no custom config unless agreed.
- Modules follow the layout in DESIGN §11. Keep modules small and single-purpose.
- Public items need doc comments; internal items need comments only where intent is non-obvious.
- No `unwrap()`/`expect()` on input-derived data; use typed errors.
- No `println!` in library paths; write through the output abstraction.
- Hot-loop code must avoid allocation; document any allocation that remains.
- Prefer `&str`/`&[u8]` borrows over `String` clones in the pipeline.

## Testing expectations

- Every bug fix gets a regression test.
- New query operators/properties require cases from the QUERY_SPEC test matrix.
- New parsing rules require a fixture and an assertion in `tests/parsing.rs`.
- Snapshot changes (`insta`) must be reviewed, not blindly accepted (`cargo insta review`).
- Performance-sensitive changes require a `criterion` result in the PR description.

## Commit messages

Conventional Commits:

```
feat(parser): support JSON arrays at top level
fix(filter): treat absent fields correctly with `?`
docs(cli): clarify -q collision with --silent
perf(index): delta-encode timestamps with varint
test(query): cover nested array paths
chore(ci): cache cargo builds
```

Scope is the module name (`parser`, `filter`, `index`, `output`, `cli`, `config`, `docs`).

## Pull requests

1. Branch from `main`: `feat/short-name`, `fix/short-name`, `docs/short-name`.
2. Keep PRs focused; one logical change.
3. Fill in: what changed, why, how it was tested, and any doc updates.
4. Link related issues.
5. Ensure CI is green.
6. Squash-merge; the PR title becomes the commit subject.

## Architecture decisions

Use an ADR for decisions that:

- affect more than one module,
- are expensive to reverse (file formats, dependencies, public CLI behavior),
- or where alternatives were seriously considered.

Create `docs/adr/NNNN-short-title.md` with: Status, Context, Decision, Options Considered, Consequences, Follow-ups. Number sequentially. Superseded ADRs are never deleted; mark them `Superseded by NNNN`.

## Documentation

- Normative specs: CLI_SPEC, PARSING_SPEC, QUERY_SPEC.
- Update `docs/README.md` when adding a document.
- Keep examples executable: every `logcli` example should work against a fixture or be clearly illustrative.

## Security

Logs are untrusted input. Report parser crashes, path traversal, or memory-safety issues privately to the maintainer before opening a public issue. Do not include production log contents in bug reports; redact secrets.

## License

By contributing, you agree your contributions are licensed under the project license (TBD).
