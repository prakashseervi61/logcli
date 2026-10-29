# Parsing Specification — `logcli`

**Status:** Draft v0.1
**Related:** [DESIGN.md](DESIGN.md) · [QUERY_SPEC.md](QUERY_SPEC.md) · [TEST_PLAN.md](TEST_PLAN.md)

This document defines exactly how bytes on disk become a normalized `Entry`. Anything ambiguous here becomes a bug later, so rules are stated explicitly and paired with fixtures in `tests/fixtures/`.

---

## 1. Supported Input Formats (v1)

| Format | Priority | Detection |
|---|---|---|
| JSON / JSONL | P0 | First non-space byte is `{` and the line parses as an object |
| logfmt | P0 | Line matches `key=value` pairs for at least one pair, no `{` prefix |
| Plain text | P0 | Fallback; entire line becomes `message`, no fields |
| Apache/Nginx combined | P1 | Line starts with an IP and contains `"METHOD path HTTP/x"` |
| Syslog RFC5424 | P1 | `<PRI>1 ` prefix |
| Syslog RFC3164 | P2 | `<PRI>MMM dd hh:mm:ss` prefix |

v1 ships JSON, logfmt, and plain text. Apache/Nginx and syslog are additive parsers with identical interfaces.

---

## 2. Entry Delimiting

### 2.1 Delimiters
- `--delimiter auto` sniffs the first 64 KiB:
  - If `\0` appears at least once per ~1 KiB and `\n` is rare → `nul`.
  - Count `\r\n`, lone `\n`, lone `\r`; most frequent wins. Default `lf`.
- Splitter never splits inside a quoted JSON string when `--input-format=json`? **No** — JSON logs are line-delimited by contract. Multi-line JSON is only supported via `--allow-prefix` + brace balancing on a single logical line, not across records.

### 2.2 Size limits
- An entry longer than `--max-entry-size` (default 64 MiB) is skipped.
- The splitter must not allocate more than `max-entry-size + buffer` for a single entry.
- Skipped oversized entries increment `stats.oversized` and emit a warning in `--verbose`.

### 2.3 Encoding
- Input is treated as bytes. Invalid UTF-8 is handled with `String::from_utf8_lossy` at display time; matching operates on bytes where possible.
- BOM at the start of a file is stripped once.

---

## 3. Format Detection

Detection is per line, with a per-source memo (first 20 lines vote; majority wins; per-line override still allowed for mixed files).

```
detect(line):
  skip leading ASCII whitespace
  if first byte == '{':
      if parses as JSON object          -> JSON
      if --allow-prefix and contains '{' that starts a valid object -> JSON (prefixed)
  if line contains regex \b[a-zA-Z_][\w.-]*=  -> LOGFMT
  if --allow-prefix                       -> try prefix scan
  else                                    -> PLAIN
```

Rules:
- A successful JSON array at top level is treated as a **single entry** with a synthetic field `_array` (P2) or as invalid in v1. v1: JSON arrays are invalid log lines → skipped.
- If `--input-format` is set, detection is bypassed entirely.
- Detection failures are never fatal.

---

## 4. JSON Parsing

### 4.1 Requirements
- Preserve nested objects and arrays.
- Preserve key order.
- Preserve number text where possible (avoid float rounding for IDs).
- Handle escaped Unicode, `\n`, `\t`, `\"`, surrogate pairs.
- Reject trailing garbage after the object unless `--allow-prefix`.
- Accept both single-line JSON and JSON with internal newlines only when the whole entry parses (i.e. the delimiter splitter yielded it as one unit — relevant for NUL-delimited input).

### 4.2 Prefix handling (`--allow-prefix`)
Input such as:
```
2026-10-28T10:00:00Z INFO starting {"level":"info","msg":"ready"}
```
Rules:
- Scan forward for the first `{` that begins a balanced JSON object, accounting for strings and escapes.
- Bytes before it are discarded from field extraction but retained for `--raw`.
- If no valid object is found → fall back to logfmt/plain detection.
- Prefix scanning is O(n) and bounded by `--max-entry-size`.

### 4.3 Duplicate keys
- Last value wins for evaluation; both occurrences remain visible in insertion order for display? **Decision:** last wins everywhere; display shows last only. Documented to avoid ambiguity.

### 4.4 Null and empty
- `null` is preserved as `Value::Null`.
- `-e` hides null, empty strings, empty arrays, and empty objects.
- Field existence (`exists()`) treats `null` as **existing** (presence, not truthiness).

---

## 5. logfmt Parsing

### 5.1 Grammar

```
logfmt   := pair (SP pair)* SP?
pair     := key '=' value
key      := bare-key | quoted-string
bare-key := [A-Za-z0-9_.\-/@]+
value    := bare-value | quoted-string
bare-value := [^\s"]+
quoted-string := '"' ( esc | [^"\\] )* '"'
esc      := '\\' ( '"' | '\\' | 'n' | 't' | 'r' )
```

### 5.2 Rules
- Without quotes, values end at whitespace.
- Quoted values may contain spaces and escaped quotes/newlines.
- A token without `=` is ignored but counted as `stats.logfmt_stray_tokens` (verbose).
- Duplicate keys: last wins (same as JSON).
- Empty value (`key=`) → `Value::String("")`.
- Keys are case-sensitive.
- Numeric-looking values remain strings unless a comparator requires numeric coercion (see QUERY_SPEC §7).

### 5.3 Examples

| Input | Result |
|---|---|
| `level=info msg=hello` | level=`info`, msg=`hello` |
| `msg="hello world" level=warn` | msg=`hello world`, level=`warn` |
| `path=/api/v1?x=1` | path=`/api/v1?x=1` (only first `=` splits) |
| `msg="a \"quoted\" word"` | msg=`a "quoted" word` |
| `novalue` | ignored + stray token counter |

---

## 6. Timestamp Extraction

### 6.1 Field keys (in order)
Configurable via `timestamp-keys`, default:
1. `timestamp`
2. `time`
3. `ts`
4. `@timestamp`
5. `date`
6. `datetime`

Nested access is allowed (`event.time`).

### 6.2 Accepted formats
| Shape | Example | Notes |
|---|---|---|
| RFC3339 / ISO8601 | `2026-10-28T10:15:30.123Z`, `+05:30` | Primary |
| ISO without TZ | `2026-10-28T10:15:30` | Assumed UTC unless config says otherwise |
| Space-separated | `2026-10-28 10:15:30.123` | Assumed UTC |
| Apache | `28/Oct/2026:10:15:30 +0000` | For web logs (P1) |
| Syslog | `Oct 28 10:15:30` | Year inferred; host-local assumed UTC |
| Unix seconds | `1761646530` | 10 digits |
| Unix millis | `1761646530123` | 13 digits |
| Unix micros | `1761646530123456` | 16 digits |
| Unix nanos | `1761646530123456789` | 19 digits |

### 6.3 Unit inference (`--unix-timestamp-unit auto`)
- Length 10 → seconds; 13 → millis; 16 → micros; 19 → nanos.
- Values with a decimal point are seconds (float).
- Otherwise: if `value < 10^11` → seconds; `< 10^14` → millis; `< 10^17` → micros; else nanos.
- Explicit `--unix-timestamp-unit` always wins.

### 6.4 Missing timestamps
- Entry has `timestamp = None`.
- Sorting places these at the end (stable by source/offset).
- `--since`/`--until` **exclude** entries without timestamps.

### 6.5 Timezone
- Internally `DateTime<Utc>`.
- Naive timestamps default to UTC; config key `assume-timezone` (P2) can change this.
- `-Z` / `-L` affect **display only**.

---

## 7. Level Extraction

### 7.1 Field keys (in order)
`level`, `lvl`, `severity`, `severity_text`, `log.level`, `@level`.

### 7.2 Normalization
| Input | Normalized |
|---|---|
| `trace`, `trc`, `t` | Trace |
| `debug`, `dbg`, `d` | Debug |
| `info`, `inf`, `i`, `informational`, `notice` | Info |
| `warn`, `warning`, `wrn`, `w` | Warn |
| `error`, `err`, `e`, `eror`? | Error |
| `fatal`, `ftl`, `crit`, `critical`, `alert`, `emerg` | Fatal |
| `panic` | Panic |
| `10`,`20` (syslog) | map per RFC5424: 0–3 Fatal, 4 Error, 5–6 Warn, 7 Info |
| `0..6` (log4j-ish) | 0 Fatal … 6 Trace? **No** — ambiguous; treat as unknown unless config maps |

Comparison is case-insensitive and trimmed. Unknown strings → `Level::Other`, displayed as-is but sorted below Trace.

### 7.3 Threshold semantics (`--level`)
`trace < debug < info < warn < error < fatal < panic`

`-l w` matches warn, error, fatal, panic. Unknown levels match only if the threshold is not set.

---

## 8. Message, Logger, Caller

| Role | Default keys (in order) | Fallback |
|---|---|---|
| message | `message`, `msg`, `log`, `@message` | plain text line |
| logger | `logger`, `source`, `service`, `@logger` | none |
| caller | `caller`, `file`, `source.file` | none |

- If `message` is an object/array, it is formatted compactly as JSON.
- For plain-text entries the whole line is the message and it is **not** styled as a key-value field.
- `logger` and `caller` are hidden from the trailing field list (they have their own display slots) unless explicitly revealed via `-h '!logger'`.

---

## 9. Multi-line Handling

- A single entry containing embedded `\n` (from JSON strings or escaped logfmt values) is controlled by `-x`:
  - `never`: escape as `\n`, single line.
  - `inline`: emit raw newlines, no indentation.
  - `auto`: expand only if the entry has >1 line or a field > 200 chars (threshold configurable, P2).
  - `always`: every field on its own indented line.
- Indentation is 4 spaces under the field key; continuation lines never receive the time/level prefix.

---

## 10. Compression

| Format | Magic | Crate |
|---|---|---|
| gzip | `1f 8b` | `flate2` |
| zstd | `28 b5 2f fd` | `zstd` |
| bzip2 | `42 5a 68` | `bzip2` |
| xz | `fd 37 7a 58 5a 00` | `xz2` |

- Detection is by magic bytes, never extension.
- Decompression errors are fatal for that source (exit 2) and name the file.
- Concatenated gzip members are supported.
- `.zst` files with skippable frames are supported per zstd spec.

---

## 11. Malformed Input Policy

| Situation | Behavior |
|---|---|
| Invalid JSON line | Skip; increment `invalid`; verbose warning |
| Trailing garbage after JSON | Same as invalid unless `--allow-prefix` accepts a prefix |
| logfmt with no pairs | Falls back to plain text |
| Oversized line | Skip; increment `oversized` |
| Invalid UTF-8 | Lossy conversion; not an error |
| Empty line | Skipped silently (not counted as invalid) |
| NUL in JSON string | Escaped by the parser; literal NUL in input is treated as delimiter when `--delimiter=nul` |

---

## 12. Fixtures (required)

| File | Purpose |
|---|---|
| `fixtures/simple.jsonl` | 100 varied JSON entries |
| `fixtures/nested.jsonl` | Objects, arrays, dotted keys |
| `fixtures/logfmt.log` | Quoted values, escapes, stray tokens |
| `fixtures/mixed.log` | JSON + logfmt + plain interleaved |
| `fixtures/prefixed.log` | `timestamp LEVEL {json}` lines |
| `fixtures/multiline.jsonl` | Embedded newlines, stack traces |
| `fixtures/levels.jsonl` | All level spellings and numerics |
| `fixtures/timestamps.jsonl` | Every supported time format |
| `fixtures/malformed.log` | Truncated JSON, empty lines, huge line |
| `fixtures/web.log` | Apache/Nginx combined (P1) |
| `fixtures/gzip/simple.jsonl.gz` | Compression round-trip |
| `fixtures/unicode.log` | RTL, emoji, CJK, combining chars |

Every rule in this document must map to at least one fixture assertion.
