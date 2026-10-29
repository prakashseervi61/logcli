# ADR 0004 — Theme System and Color Pipeline

**Status:** Accepted
**Date:** 2026-10-28
**Related:** [DESIGN.md §4.5](../DESIGN.md#45-formatting) · [CONFIG.md](../CONFIG.md)

---

## Context

Logs are read for long periods; color improves scanning for errors and structure. Requirements:

- Sensible defaults with zero configuration.
- Full customization without breaking layout (ANSI sequences must not affect column alignment).
- Respect `NO_COLOR`, `CLICOLOR`, `--color=never`, and non-TTY output.
- Multiple built-in looks (dark/light, 16-color/256/truecolor) plus user themes.

## Decision

Adopt a **role-based token styling** system:

1. The formatter emits semantic tokens (`time`, `level`, `message`, `key`, `string`, `number`, …) rather than raw colors.
2. A theme maps roles/elements to `Style` values.
3. A renderer applies styles while tracking visible width (ANSI stripped for measurement).
4. Themes are declarative files (YAML/TOML) with inheritance from `@base` and optional overlays.
5. Style resolution order: `@base` → selected theme → overlays → level-specific overrides.

## Options Considered

| Option | Flexibility | Layout safety | Complexity |
|---|---|---|---|
| Hard-coded colors | None | Good | Trivial |
| Per-field color flags | Low | Medium | Low |
| **Role-based tokens (chosen)** | High | Good | Medium |
| Full template engine | Very high | Risky | High |

## Consequences

**Positive**
- Layout is computed on unstyled text; color cannot shift columns.
- Themes are data, not code; users can add them without recompiling.
- Level-specific styling (`levels.error.message = red`) is natural.
- `--color=never` simply skips the render step.

**Negative**
- Slightly slower formatting (style lookup per token); mitigated by resolving styles once into a flat table.
- Theme schema must be documented and validated; invalid theme → exit 1 with a clear error.
- Truecolor vs 256-color downgrade logic required.

## Rules

- Never emit ANSI when stdout is not a TTY (unless `--color=always`).
- `--color=always` overrides `NO_COLOR`; `--color=auto` honors it.
- `--raw` output is never colorized.
- Theme parsing errors must name the file and key path.
- Built-in themes are embedded in the binary; user themes live in the config dir.

## Follow-ups

- Publish theme schema in [CONFIG.md](../CONFIG.md#6-themes).
- Add `--list-themes` with tag filtering and fzf preview support (P2).
- Snapshot-test each built-in theme's rendered output.
