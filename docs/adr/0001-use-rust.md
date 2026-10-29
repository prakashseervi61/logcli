# ADR 0001 — Use Rust as the Implementation Language

**Status:** Accepted
**Date:** 2026-10-28
**Deciders:** Prakash
**Related:** [PRD.md](../PRD.md) · [DESIGN.md](../DESIGN.md)

---

## Context

`logcli` must parse and render multi-gigabyte log files with minimal startup latency and bounded memory. The reference implementation, `hl`, achieves ~2 GiB/s in Rust. Candidate languages were Rust, Go, and C++.

## Decision

Implement `logcli` in **Rust** (edition 2021, stable toolchain), targeting Linux, macOS, and Windows.

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **Rust** | Predictable performance, zero-cost abstractions, strong safety, single static binary, mature CLI ecosystem (`clap`), easy cross-compilation, borrow checker enables zero-copy parsing | Steeper learning curve, longer compile times |
| **Go** | Simple concurrency, fast builds, easy deployment, large stdlib | GC pauses under allocation-heavy parsing, weaker zero-copy ergonomics, slower for tight byte-level parsing |
| **C++** | Maximum control, reference implementations available | Memory safety risk, dependency/build complexity, slower iteration |
| **Python** | Fastest prototyping, rich libraries | 10–100× slower on multi-GB inputs; distribution/startup overhead |

## Consequences

**Positive**
- Performance budget is achievable without hand-written assembly.
- Memory safety eliminates a class of parser vulnerabilities (important for untrusted log input).
- `cargo` provides reproducible builds, testing, fuzzing, and benchmarking in one tool.
- Single static binary simplifies distribution (GitHub releases, `cargo install`).

**Negative**
- Steeper contributor onboarding; mitigated by CONTRIBUTING and small module boundaries.
- Longer CI build times; mitigated with caching (`Swatinem/rust-cache`).
- Some crates (compression, timezone) add compile time; enabled via feature flags.

## Follow-ups

- Pin MSRV in `Cargo.toml` (`rust-version`) and test it in CI.
- Benchmark JSON parsing paths before committing to `serde_json` vs a custom parser (see ADR 0003).
