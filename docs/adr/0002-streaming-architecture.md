# ADR 0002 — Streaming Pipeline Architecture

**Status:** Accepted
**Date:** 2026-10-28
**Related:** [DESIGN.md](../DESIGN.md) · [PRD.md](../PRD.md)

---

## Context

Log files can be much larger than RAM (multi-GB is common). The tool must produce output quickly, support pipes (`kubectl logs | logcli`), and never stall on a single malformed record. Two broad designs were considered:

1. **Load-and-process:** read all input into memory, then parse/filter/sort.
2. **Streaming pipeline:** read, parse, filter, and emit incrementally.

## Decision

Adopt a **streaming pipeline** with a separate, index-backed merge phase for `--sort`:

```
read → decompress → split → parse → normalize → filter → format → write
```

Sorting does not buffer entries in memory; it writes a compact index and performs a k-way merge, reading entry bytes lazily.

## Options Considered

| Option | Memory | Latency to first output | Complexity |
|---|---|---|---|
| Load-and-process | O(file size) — fails on > RAM inputs | Poor | Low |
| Streaming (chosen) | O(buffer + per-thread state) | Immediate | Medium |
| External sort (Unix `sort` style) | O(buffer) | Delayed | Medium–high |
| Index + lazy read (chosen for sort) | O(sources) | Delayed for sort only | High |

## Consequences

**Positive**
- Constant memory for non-sorted reads; works on files larger than RAM.
- Immediate output for pipes and `--follow`.
- Parallel parsing of independent chunks without holding results.
- Sorting remains a separate mode with a clear performance contract.

**Negative**
- Filtering order affects performance; requires cost-based term ordering.
- Errors must be non-fatal per record (otherwise a stream dies mid-file).
- `--sort` requires disk space and a cache directory.
- Backpressure handling between stages adds moderate complexity.

## Follow-ups

- Define the index format in [DESIGN.md §5](../DESIGN.md#5-sorting--indexing) and version it.
- Add a `--max-memory` guard for in-memory sort on small inputs (P2).
- Measure per-stage latency with `--verbose` timings.
