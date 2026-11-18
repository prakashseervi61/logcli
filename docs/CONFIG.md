# Configuration Specification — `logcli`

**Status:** Draft v0.2
**Related:** [CLI_SPEC.md](CLI_SPEC.md) · [DESIGN.md §9](DESIGN.md#9-configuration-model) · [ADR 0004](adr/0004-theme-system.md)

---

## 1. Precedence

From lowest to highest:

1. Built-in defaults
2. System-wide config
3. User config
4. Current-directory config (`logcli.yaml`, `logcli.yml`, `logcli.json`)
5. Files named by `LOGCLI_CONFIG` (env)
6. Files passed via `--config` (repeatable, applied in order)
7. `LOGCLI_*` environment variables
8. Command-line flags

Special case: `--config -` or `LOGCLI_CONFIG=-` discards all previously loaded locations and starts over. An empty value behaves the same.

Auto-discovered config files that do not exist are silently skipped. A `--config` path that does not exist is an error (exit 2).

---

## 2. File Locations

| OS | System-wide | User | Current directory |
|---|---|---|---|
| Linux | `/etc/logcli/config.{yaml,yml,json}` | `~/.config/logcli/config.{yaml,yml,json}` | `logcli.{yaml,yml,json}` |
| macOS | `/etc/logcli/config.{yaml,yml,json}` | `~/.config/logcli/config.{yaml,yml,json}` | `logcli.{yaml,yml,json}` |
| Windows | `%PROGRAMDATA%\logcli\config.{yaml,yml,json}` | `%APPDATA%\logcli\config.{yaml,yml,json}` | `logcli.{yaml,yml,json}` |

Cache directory (sort indexes):

| OS | Path |
|---|---|
| Linux | `$XDG_CACHE_HOME/logcli` or `~/.cache/logcli` |
| macOS | `~/Library/Caches/logcli` |
| Windows | `%LOCALAPPDATA%\logcli\cache` |

Override with `--index-dir`.

Only the **first** filename match per directory is loaded, in the order `yaml`, `yml`, `json`. TOML support is a roadmap item (requires `toml` crate).

> **Note:** YAML config files are detected but rejected with an error unless the `serde_yaml` dependency is enabled. JSON (`logcli.json`) is the recommended format.

---

## 3. Schema

All keys optional. Unknown keys are an error (typo protection) with a suggestion.

### JSON example (`logcli.json`)

```json
{
  "files": ["logs/api.log", "logs/worker.log"],
  "input_format": "auto",
  "level": "warn",
  "since": "-1h",
  "until": "now",
  "filters": ["component=api"],
  "queries": ["status >= 500"],
  "output_format": "formatted",
  "output": "report.txt",
  "time_zone": "Europe/Berlin",
  "hide": ["request.headers.authorization"],
  "hide_empty": true,
  "expansion": "auto",
  "sort": true,
  "follow": false,
  "pager": "auto",
  "theme": "default",
  "redact": [
    {
      "field": "message",
      "pattern": "[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}",
      "replace": "[EMAIL]"
    },
    {
      "field": "*",
      "pattern": "\\b\\d{4}[- ]?\\d{4}[- ]?\\d{4}[- ]?\\d{4}\\b",
      "replace": "[CARD]"
    }
  ],
  "alerts": [
    {
      "name": "high_error_rate",
      "query": "level = error and status >= 500",
      "threshold": 10,
      "exit_code": 2
    },
    {
      "name": "fatal_occurred",
      "query": "level = fatal",
      "threshold": 1,
      "exit_code": 2
    }
  ],
  "tui": {
    "sort": "time",
    "follow": false,
    "theme": "default"
  }
}
```

### YAML equivalent (`logcli.yaml`)

```yaml
files:
  - "logs/api.log"
  - "logs/worker.log"
level: warn
since: "-1h"
until: now
filters:
  - "component=api"
queries:
  - "status >= 500"
output_format: formatted
output: "report.txt"
time_zone: "Europe/Berlin"
hide:
  - "request.headers.authorization"
hide_empty: true
expansion: auto
sort: true
follow: false
pager: auto
theme: default
redact:
  - field: "message"
    pattern: "[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}"
    replace: "[EMAIL]"
  - field: "*"
    pattern: "\\b\\d{4}[- ]?\\d{4}[- ]?\\d{4}[- ]?\\d{4}\\b"
    replace: "[CARD]"
alerts:
  - name: "high_error_rate"
    query: "level = error and status >= 500"
    threshold: 10
    exit_code: 2
  - name: "fatal_occurred"
    query: "level = fatal"
    threshold: 1
    exit_code: 2
tui:
  sort: time
  follow: false
  theme: default
```

### 3.1 Top-level keys

| Key | Type | Default | Description |
|---|---|---|---|
| `files` | array of path | none | Default input files (used when none on CLI). |
| `input_format` | string | `"auto"` | `auto`, `json`, `logfmt`, `plain`, `otel`. |
| `level` | string | none | Default minimum level. |
| `since` | string | none | Default time range start. |
| `until` | string | none | Default time range end. |
| `filters` | array of string | none | Default field filters (ANDed with `-f`). |
| `queries` | array of string | none | Default query expressions (ANDed with `-q`). |
| `output_format` | string | `"formatted"` | `formatted`, `raw`, `json`, `csv`, `prometheus`. |
| `output` | path | none | Default output file. |
| `time_zone` | string | `"UTC"` | Output timezone (IANA name). |
| `hide` | array of string | none | Fields to hide by default. |
| `hide_empty` | boolean | false | Hide null/empty fields. |
| `expansion` | string | `"auto"` | `never`, `inline`, `auto`, `always`. |
| `sort` | boolean | false | Chronological sort. |
| `follow` | boolean | false | Follow mode (tail new lines). |
| `pager` | string | `"auto"` | `auto`, `always`, `never`. |
| `theme` | string | `"default"` | `default`, `monokai`, `solarized`. |
| `sets` | map of string to array of string | none | Value sets referenced by `in @name`. |
| `redact` | array of redact rule | none | PII redaction rules (§3.2). |
| `alerts` | array of alert rule | none | Alert rules (§3.3). |
| `tui` | TuiConfig | none | TUI defaults (§3.4). |

### 3.2 Redact rule

| Key | Type | Default | Description |
|---|---|---|---|
| `field` | string | required | Field name (`"message"`, `"logger"`, `"caller"`, `"*"` for all string fields, or a specific field key). |
| `pattern` | string | required | Regex pattern to match within the field value. |
| `replace` | string | `"[REDACTED]"` | Replacement text. |

### 3.3 Alert rule

| Key | Type | Default | Description |
|---|---|---|---|
| `name` | string | required | Human-readable alert name. |
| `query` | string | required | Query expression (same syntax as `-q`). |
| `threshold` | integer | `1` | Minimum matching entries to trigger. |
| `exit_code` | integer | `2` | Exit code when triggered. |

### 3.4 TuiConfig

| Key | Type | Default | Description |
|---|---|---|---|
| `sort` | string | `"time"` | `none`, `time`, `level`. |
| `follow` | boolean | false | Default follow mode. |
| `theme` | string | `"default"` | `default`, `monokai`, `solarized`. |

---

## 4. Environment Variables

Prefix: `LOGCLI_`. Names use `SCREAMING_SNAKE_CASE`. Empty value = unset.

| Variable | Maps to | Example |
|---|---|---|
| `LOGCLI_CONFIG` | `--config` | `/etc/logcli/prod.json` |
| `LOGCLI_INPUT_FORMAT` | `--input-format` | `json` |
| `LOGCLI_DELIMITER` | `--delimiter` | `lf` |
| `LOGCLI_ALLOW_PREFIX` | `--allow-prefix` | `1` |
| `LOGCLI_LEVEL` | `--level` | `warn` |
| `LOGCLI_SINCE` | `--since` | `-1h` |
| `LOGCLI_UNTIL` | `--until` | `now` |
| `LOGCLI_COLOR` | `--color` | `never` |
| `LOGCLI_THEME` | `--theme` | `classic` |
| `LOGCLI_PAGING` | `--paging` | `never` |
| `LOGCLI_TIME_FORMAT` | `--time-format` | `%H:%M:%S` |
| `LOGCLI_TIME_ZONE` | `--time-zone` | `America/New_York` |
| `LOGCLI_HIDE_EMPTY_FIELDS` | `-e` | `1` |
| `LOGCLI_EXPANSION` | `--expansion` | `always` |
| `LOGCLI_ASCII` | `--ascii` | `always` |
| `LOGCLI_INPUT_INFO` | `--input-info` | `compact` |
| `LOGCLI_OUTPUT_DELIMITER` | `--output-delimiter` | `nul` |
| `LOGCLI_OUTPUT` | `--output` | `/tmp/out.log` |
| `LOGCLI_CONCURRENCY` | `--concurrency` | `4` |
| `LOGCLI_INDEX_DIR` | `--index-dir` | `D:\\cache\\logcli` |
| `LOGCLI_PAGER` | pager command | `most -w` |
| `LOGCLI_NO_COLOR` | force no color | `1` |
| `LOGCLI_SILENT` | `--silent` | `1` |

Boolean env vars accept `1/0`, `true/false`, `yes/no`, `on/off`.

**Standard variables honored:** `NO_COLOR` (any non-empty value disables color in `auto` mode), `CLICOLOR=0`, `CLICOLOR_FORCE=1`, `PAGER`, `TERM`.

---

## 5. Pager Resolution

1. `--paging never` or `-P` → no pager.
2. `LOGCLI_PAGER` → use it.
3. `PAGER` → use it.
4. `less -R` if available (Unix/macOS); on Windows, `less -R` if on `PATH`, else no pager.
5. Only when stdout is a TTY and not redirecting to `--output`.

Pager is invoked as a child process; `logcli` exits with the pager's status unless the pager exits 0. Broken pipe → exit 3.

---

## 6. Themes

### 6.1 Built-in themes

| Name | Tags | Description |
|---|---|---|
| `default` | `light`, `16color` | Clean light-background theme |
| `monokai` | `dark`, `256color` | Monokai-inspired dark theme |
| `solarized` | `light`, `16color` | Solarized light theme |

### 6.2 User themes

| OS | Path |
|---|---|
| Linux/macOS | `~/.config/logcli/themes/*.{toml,yaml,yml,json}` |
| Windows | `%APPDATA%\logcli\themes\*.{toml,yaml,yml,json}` |

Built-in themes are embedded; user themes with the same name override built-ins.

### 6.3 Theme structure (TOML)

```toml
version = "1.0"
tags = ["dark", "256color"]

[styles]
primary   = { modes = ["-faint"] }
secondary = { style = "primary", modes = ["faint"] }
warning   = { style = "primary", foreground = "yellow" }
error     = { style = "primary", foreground = "bright-red" }

[elements]
message     = { style = "primary", modes = ["bold"] }
time        = { style = "secondary" }
level-inner = { style = "primary" }

[levels.warning]
level-inner = { style = ["primary", "warning"] }
message     = { style = ["primary", "warning"] }

[levels.error]
level-inner = { style = ["primary", "error"] }
message     = { style = ["primary", "error"] }
```

Theme overlays are applied in order: `@base` → configured theme → overlays.

---

## 7. Validation & Errors

| Situation | Result |
|---|---|
| Unknown key | exit 1, `unknown config key 'colour' (did you mean 'color'?)` |
| Wrong type | exit 1 with expected/actual types |
| Invalid enum value | exit 1 with allowed values |
| Unreadable config | exit 2 |
| `--config` path missing | exit 2 |
| Invalid theme | exit 1 with file/key |
| YAML config without serde_yaml | exit 1 (use JSON instead) |

Config errors never fall back silently (except auto-discovered config files that do not exist, which are simply skipped).