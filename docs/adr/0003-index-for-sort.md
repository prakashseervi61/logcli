# ADR 0003 — On-Disk Index for Chronological Sorting

**Status:** Accepted
**Date:** 2026-10-28
**Related:** [DESIGN.md §5](../DESIGN.md#5-sorting--indexing) · [CLI_SPEC.md §7](../CLI_SPEC.md#7-sorting--follow)

---

## Context

`--sort` must merge many large files chronologically. Sorting entire entries in memory is infeasible, and repeatedly re-parsing files after filtering is slow (hl reports this as a key design problem). The tool also needs `--follow` with timestamp-ordered output, and incremental updates when files grow.

## Decision

Build a **per-file, versioned, append-friendly on-disk index** containing only:

- timestamp (delta-encoded, sorted)
- source id
- byte offset + byte length of the raw entry
- level (for index-only level filtering)

At query time, perform a k-way merge over indexes and read only matching entries' bytes.

## Options Considered

| Option | Re-parse cost | Disk cost | Incremental updates | Complexity |
|---|---|---|---|---|
| In-memory sort | Low | None | N/A (fails on large files) | Low |
| Re-parse per query (hl without index) | High | None | Easy | Low |
| **On-disk index (chosen)** | None | ~10–20 B/entry | Append + block checksums | High |
| SQLite/DuckDB sidecar | None | High | Easy | Medium (heavy dependency) |

## Consequences

**Positive**
- Sort and time/level filters can skip parsing entirely for excluded entries.
- Repeat queries over the same files are fast (index reuse).
- Follow mode can tail from an index offset.
- Disk usage is proportional to line count, not byte size.

**Negative**
- Index staleness must be detected (size, mtime, version, CRC).
- Format changes require version bumps and reindexing.
- Network filesystems with unreliable mtime require conservative invalidation.
- Adds a cache directory to manage and document.

## Index Invariants

1. Entries are sorted by timestamp (non-decreasing).
2. `source_id` is stable for the lifetime of the index.
3. Offsets refer to **uncompressed** bytes for compressed sources; compressed sources are indexed only after decompression to a sidecar or are excluded from incremental mode.
4. CRC32C covers the entries section.
5. Version mismatch → discard and rebuild.

## Follow-ups

- Add `logcli index` maintenance commands (P2): `list`, `clear`, `rebuild`.
- Benchmark at 1M/10M/100M entries and publish results.
- Decide behavior for compressed files in sort mode (documented limitation for v1: index built per invocation in a temp file).
