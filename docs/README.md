# Documentation Index

`logcli` — a fast, local-first log viewer and processor for **JSON, logfmt, plain text, and OpenTelemetry OTLP** logs, with PII redaction, alert rules, multi-format export, and an interactive TUI. Inspired by [hl](https://github.com/pamburus/hl) and [lnav](https://github.com/tstack/lnav).

## Start here

| Document | What it answers |
|---|---|
| [PRD.md](PRD.md) | Why this project exists, who it is for, what is in/out of scope, success metrics |
| [PLAN.md](../PLAN.md) | Phased implementation roadmap |
| [CLI_SPEC.md](CLI_SPEC.md) | **Normative** command-line contract: flags, exit codes, examples, error formats |
| [DESIGN.md](DESIGN.md) | Architecture, data model, pipeline, indexing, concurrency, performance budgets |
| [PARSING_SPEC.md](PARSING_SPEC.md) | **Normative** how bytes become entries: JSON, logfmt, plain, OTLP, timestamps, levels, malformed-input policy |
| [QUERY_SPEC.md](QUERY_SPEC.md) | **Normative** filter/query grammar, operators, field paths, coercion, error rules |

## Operations & quality

| Document | What it answers |
|---|---|
| [CONFIG.md](CONFIG.md) | Config file locations/schema, env vars, PII redaction, alert rules, themes |
| [TEST_PLAN.md](TEST_PLAN.md) | Test levels, fixtures, benchmarks, CI matrix, release checklist |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | Dev setup, code style, commit/PR process |
| [CHANGELOG.md](../CHANGELOG.md) | Version history |

## Architecture Decision Records

| ADR | Decision |
|---|---|
| [0001](adr/0001-use-rust.md) | Use Rust as the implementation language |
| [0002](adr/0002-streaming-architecture.md) | Streaming pipeline architecture |
| [0003](adr/0003-index-for-sort.md) | On-disk index for chronological sorting |
| [0004](adr/0004-theme-system.md) | Role-based theme and color pipeline |

## Reading order for new contributors

1. PRD (scope) → 2. CLI_SPEC (behavior) → 3. DESIGN (architecture) → 4. PARSING_SPEC + QUERY_SPEC (details) → 5. TEST_PLAN (how we verify).
2. CONFIG (operational details) → README (user-facing quick start).

## Document conventions

- **Normative** documents: CLI_SPEC, PARSING_SPEC, QUERY_SPEC. If code disagrees, code is wrong.
- **Descriptive** documents: PRD, DESIGN, PLAN. Update them when decisions change.
- Every behavior change requires a doc update in the same PR.
- New significant decisions require an ADR (see [CONTRIBUTING.md](../CONTRIBUTING.md#architecture-decisions)).