# Configuration Specification — `logcli`

**Status:** Draft v0.1
**Related:** [CLI_SPEC.md](CLI_SPEC.md) · [DESIGN.md §9](DESIGN.md#9-configuration-model) · [ADR 0004](adr/0004-theme-system.md)

---

## 1. Precedence

From lowest to highest:

1. Built-in defaults
2. System-wide config
3. User config
4. Files named by `LOGCLI_CONFIG` (env)
5. Files passed via `--config` (repeatable, applied in order)
6. `LOGCLI_*` environment variables
7. Command-line flags

Special case: `--config -` or `LOGCLI_CONFIG=-` discards all previously loaded locations and starts over. An empty value behaves the same.

---

## 2. File Locations

| OS | System-wide | User |
|---|---|---|
| Linux | `/etc/logcli/config.{toml,yaml,yml,json}` | `~/.config/logcli/config.{toml,yaml,yml,json}` |
| macOS | `/etc/logcli/config.{toml,yaml,yml,json}` | `~/.config/logcli/config.{toml,yaml,yml,json}` |
| Windows | `%PROGRAMDATA%\logcli\config.{toml,yaml,yml,json}` | `%APPDATA%\logcli\config.{toml,yaml,yml,json}` |

Cache directory (sort indexes):

| OS | Path |
|---|---|
| Linux | `$XDG_CACHE_HOME/logcli` or `~/.cache/logcli` |
| macOS | `~/Library/Caches/logcli` |
| Windows | `%LOCALAPPDATA%\logcli\cache` |

Override with `--index-dir`.

Only the **first** filename match per directory is loaded, in the order `toml`, `yaml`, `yml`, `json`.

---

## 3. Schema

All keys optional. Unknown keys are an error (typo protection) with a suggestion.

```toml
# ~/.config/logcli/config.toml

# ---------- Input ----------
input-format = "auto"        # auto | json | logfmt | plain
delimiter = "auto"           # auto | lf | crlf | cr | nul
allow-prefix = false
unix-timestamp-unit = "auto" # auto | s | ms | us | ns
max-entry-size = "64 MiB"
buffer-size = "256 KiB"

[fields]
timestamp-keys = ["timestamp", "time", "ts", "@timestamp", "date", "datetime"]
level-keys = ["level", "lvl", "severity", "severity_text", "log.level", "@level"]
message-keys = ["message", "msg", "log", "@message"]
logger-keys = ["logger", "source", "service", "@logger"]
caller-keys = ["caller", "file", "source.file"]

# ---------- Filtering ----------
[filter]
level = "info"               # default minimum level (off = show all)
# since = "-1h"
# until = "now"
# exclude = ["/health", "/metrics"]   # P2 convenience

# ---------- Output ----------
output = "table"             # reserved; v1 always formatted text
color = "auto"               # auto | always | never
theme = "uni"
paging = "auto"              # auto | always | never
flatten = "always"           # always | never
time-format = "%Y-%m-%d %T.%3N"
time-zone = "UTC"
local-time = false
hide-empty-fields = false
expansion = "auto"           # never | inline | auto | always
ascii = "auto"
input-info = "auto"          # auto | none | minimal | compact | full
output-delimiter = "newline" # newline | nul
# output = "/tmp/out.log"

[output.hide]
fields = ["request_id", "trace_id"]

# ---------- Sorting ----------
[sort]
enabled = false
index-dir = ""               # empty = OS cache dir
memory-limit = "256 MiB"     # for --no-index

[follow]
tail = 10
sync-interval-ms = 100

# ---------- Parsing extensions (P1) ----------
[parsers.apache]
enabled = true

[parsers.syslog]
enabled = false

# ---------- Runtime ----------
concurrency = 0              # 0 = CPU count
interrupt-ignore-count = 3

# ---------- Theme overlays ----------
theme-overlays = []
```

### 3.1 JSON equivalent

```json
{
  "color": "never",
  "time-zone": "Europe/Berlin",
  "fields": { "message-keys": ["msg", "message"] },
  "output": { "hide": { "fields": ["request_id"] } }
}
```

YAML follows the same structure with `:`/indentation.

---

## 4. Environment Variables

Prefix: `LOGCLI_`. Names use `SCREAMING_SNAKE_CASE`. Empty value = unset.

| Variable | Maps to | Example |
|---|---|---|
| `LOGCLI_CONFIG` | `--config` | `/etc/logcli/prod.toml` |
| `LOGCLI_INPUT_FORMAT` | `--input-format` | `json` |
| `LOGCLI_DELIMITER` | `--delimiter` | `lf` |
| `LOGCLI_ALLOW_PREFIX` | `--allow-prefix` | `1` |
| `LOGCLI_LEVEL` | `--level` | `warn` |
| `LOGCLI_COLOR` | `--color` | `never` |
| `LOGCLI_THEME` | `--theme` | `classic` |
| `LOGCLI_PAGING` | `--paging` | `never` |
| `LOGCLI_TIME_FORMAT` | `--time-format` | `%H:%M:%S` |
| `LOGCLI_TIME_ZONE` | `--time-zone` | `America/New_York` |
| `LOGCLI_FLATTEN` | `--flatten` | `never` |
| `LOGCLI_HIDE_EMPTY_FIELDS` | `-e` | `1` |
| `LOGCLI_EXPANSION` | `--expansion` | `always` |
| `LOGCLI_ASCII` | `--ascii` | `always` |
| `LOGCLI_INPUT_INFO` | `--input-info` | `compact` |
| `LOGCLI_OUTPUT_DELIMITER` | `--output-delimiter` | `nul` |
| `LOGCLI_OUTPUT` | `--output` | `/tmp/out.log` |
| `LOGCLI_CONCURRENCY` | `--concurrency` | `4` |
| `LOGCLI_INDEX_DIR` | `--index-dir` | `D:\cache\logcli` |
| `LOGCLI_PAGER` | pager command | `most -w` |
| `LOGCLI_NO_COLOR` | force no color | `1` |

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

### 6.1 Locations

| OS | Path |
|---|---|
| Linux/macOS | `~/.config/logcli/themes/*.{toml,yaml,yml,json}` |
| Windows | `%APPDATA%\logcli\themes\*.{toml,yaml,yml,json}` |

Built-in themes are embedded; user themes with the same name override built-ins.

### 6.2 Structure

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

### 6.3 Fields

| Key | Type | Values |
|---|---|---|
| `version` | string | must be `"1.0"` |
| `tags` | array | `dark`, `light`, `16color`, `256color`, `truecolor`, `base`, `overlay` |
| `styles` | table | reusable styles with inheritance via `style` |
| `elements` | table | `input`, `time`, `level`, `level-inner`, `logger`, `caller`, `message`, `key`, `string`, `number`, `boolean`, `null`, `ellipsis`, … |
| `levels` | table | per-level overrides (`trace`…`panic`) |
| `indicators` | table | follow-mode sync indicator |

### 6.4 Colors & modes

- Named: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white` and `bright-*`.
- 256-color: integer `0`–`255`.
- Truecolor: `"#RRGGBB"`.
- `default` for terminal default.
- Modes: `bold`, `faint`, `italic`, `underline`, `slow-blink`, `rapid-blink`, `reverse`, `conceal`, `crossed-out`.
- Prefix `+` adds, `-` removes; later entries win.

### 6.5 Overlays

```toml
theme = "uni"
theme-overlays = ["@accent-italic", "my-tweaks"]
```

Resolution order: `@base` → `theme` → overlays in list order.

### 6.6 Validation

- Unknown element/role names → error naming the key.
- Invalid color/mode → error naming the value.
- Version mismatch → error with expected version.
- Invalid theme → exit 1; no partial rendering.

---

## 7. Config Examples

### 7.1 Daily driving (quiet, local time)

```toml
color = "auto"
paging = "always"
time-zone = "Europe/Berlin"
local-time = true
hide-empty-fields = true

[output.hide]
fields = ["request_id", "trace_id", "span_id"]

[fields]
message-keys = ["msg", "message"]
```

### 7.2 CI usage

```toml
color = "never"
paging = "never"
input-info = "none"
level = "info"
concurrency = 2

[output.hide]
fields = ["*"]
```

### 7.3 Incident response

```toml
theme = "classic"
expansion = "auto"
time-format = "%H:%M:%S.%3N"
local-time = true

[filter]
level = "warn"
since = "-2h"
```

---

## 8. Validation & Errors

| Situation | Result |
|---|---|
| Unknown key | exit 1, `unknown config key 'colour' (did you mean 'color'?)` |
| Wrong type | exit 1 with expected/actual types |
| Invalid enum value | exit 1 with allowed values |
| Unreadable config | exit 2 |
| `--config` path missing | exit 2 |
| Invalid theme | exit 1 with file/key |

Config errors never fall back silently (except auto-discovered config files that do not exist, which are simply skipped).
