# Query Language Specification — `logcli`

**Status:** Draft v0.1
**Related:** [CLI_SPEC.md](CLI_SPEC.md) · [PARSING_SPEC.md](PARSING_SPEC.md) · [DESIGN.md](DESIGN.md)

This document defines the filter (`-f`) and query (`-q`) languages. The grammar is frozen for v1; extensions require an ADR.

---

## 1. Two Surfaces, One Engine

- `-f key=value` is **sugar** for a simple query term.
- `-q 'expr'` accepts a full boolean expression.
- Both compile to the same AST and are evaluated identically.
- Multiple `-f` and `-q` flags are combined with **AND**.
- Shell quoting: single quotes are recommended; values containing spaces or special characters must be quoted or JSON-string-escaped.

Equivalences:

| `-f` form | Equivalent `-q` |
|---|---|
| `-f level=error` | `-q 'level = error'` |
| `-f 'status!=200'` | `-q 'status != 200'` |
| `-f 'provider~=string'` | `-q 'provider ~= string'` |
| `-f 'provider!~=string'` | `-q 'not (provider ~= string)'` |
| `-f 'request.method?!=GET'` | `-q 'request.method? != GET'` |

---

## 2. Grammar (EBNF)

```ebnf
query        := or_expr EOF ;

or_expr      := and_expr ( ("or" | "||") and_expr )* ;

and_expr     := not_expr ( ("and" | "&&") not_expr )* ;

not_expr     := ("not" | "!")* primary ;

primary      := "(" or_expr ")"
              | exists_expr
              | comparison
              | bool_literal ;

exists_expr  := ("exists" | "exist") "(" field_path ")" ;

comparison   := operand operator operand
              | operand "in" "(" literal_list ")"
              | operand "not" "in" "(" literal_list ")"
              | operand "in" "@" (filename | "-")
              | operand "not" "in" "@" (filename | "-") ;

operand      := field_path | literal ;

field_path   := reserved_path | source_path | exact_path ;
reserved_path:= "level" | "message" | "caller" | "logger"      (* predefined roles *)
source_path  := ["."] segment ("." segment)* ["?"] ;
exact_path   := json_string ;                                   (* exact source key, no splitting *)

segment      := name | "[" [integer] "]" ;
name         := [A-Za-z_@][A-Za-z0-9_.\-/@]* ;                  (* dots split paths unless quoted *)

operator     := "=" | "==" | "eq"
              | "!=" | "ne"
              | ">"  | "gt"
              | ">=" | "ge"
              | "<"  | "lt"
              | "<=" | "le"
              | "~=" | "contain"
              | "!~=" | "not contain"
              | "~~=" | "match"
              | "!~~=" | "not match"
              | "like" | "not like" ;

literal      := json_string | number | "true" | "false" | "null" | bare_word ;

literal_list := literal ("," literal)* ;

bool_literal := "true" | "false" ;

filename     := bare_word | json_string ;
```

### 2.1 Reserved words

`and, or, not, in, exists, exist, like, contain, match, eq, ne, gt, ge, lt, le, true, false, null`

Use a JSON string to reference a field whose name collides with a reserved word: `-q '"level" = info'` addresses the **source** field `level`, while `-q 'level = info'` addresses the normalized level role.

### 2.2 Precedence (highest to lowest)

1. `exists(...)`
2. comparisons / `in` / `like` / `match`
3. `not`
4. `and`
5. `or`

Parentheses override. Note: `not` binds looser than comparisons but tighter than `and`, matching SQL intuition.

---

## 3. Field Paths

| Syntax | Meaning |
|---|---|
| `level` | Normalized level role (not the raw field) |
| `message` | Normalized message role |
| `logger`, `caller` | Normalized roles |
| `.level` | The **source** field literally named `level` |
| `request.method` | Nested object traversal |
| `request.method` when source has a literal key `"request.method"` | Exact key wins over traversal; traversal is the fallback |
| `"request.method"` | Exact key only, no traversal |
| `span.[].name` | Any array element with `name` (existential match) |
| `span.[1].name` | Second element (0-based) |
| `tags.[0]` | First array element |
| `request.method?` | Include entries where the field is absent |

Rules:
- Path segments are case-sensitive.
- Empty arrays/objects do not satisfy existence checks for their children.
- Traversal into a non-container yields "no value" (condition false, except with `?`).
- A literal dotted key takes precedence over nested traversal when both exist. Documented, tested.

---

## 4. Operators

### 4.1 Equality and ordering

| Operator | Aliases | Semantics |
|---|---|---|
| `=` | `==`, `eq` | Type-aware equality (see §7) |
| `!=` | `ne` | Negation of `=`, **field must exist** |
| `>` | `gt` | Numeric if both numeric, else lexicographic string |
| `>=` | `ge` | Same |
| `<` | `lt` | Same |
| `<=` | `le` | Same |

### 4.2 String operators

| Operator | Aliases | Semantics |
|---|---|---|
| `~=` | `contain` | Substring containment (case-sensitive) |
| `!~=` | `not contain` | Negation, field must exist |
| `~~=` | `match` | Regex search (unanchored, Rust `regex` syntax) |
| `!~~=` | `not match` | Negation, field must exist |
| `like` | — | Wildcards: `*` = any sequence, `?` = one char |
| `not like` | — | Negation, field must exist |

Notes:
- `~=` and `~~=` apply to string representations of scalars. Objects/arrays do not match.
- Regexes are compiled once at startup; invalid regex → exit 1 with the pattern and error.
- `like` is compiled to an anchored regex internally; case-sensitive.

### 4.3 Existence

```
exists(request.id)      // true if the field exists, even if null
exists(.level)          // source field "level"
not exists(request.id)  // true if absent
```

- `exists` is a function form only; no bare `exists field`.
- `exists()` takes exactly one argument.

### 4.4 Sets

```
status in (200, 201, 204)
level in (warn, error)
user not in @blocked_users.txt
ip in @-
```

- Values may be strings, numbers, booleans, null.
- `in @file` reads one value per line; blank lines skipped; lines JSON-decoded when they start with `"`.
- `in @-` reads from stdin **before** log input; only valid when log input is not stdin (else exit 1).
- Set membership is O(1) for ≤ 64 values via a small hash set.

### 4.5 The absent modifier `?`

| Expression | Field exists & matches | Field exists & doesn't match | Field absent |
|---|---|---|---|
| `k = v` | ✅ | ❌ | ❌ |
| `k? = v` | ✅ | ❌ | ✅ |
| `k != v` | ❌ | ✅ | ❌ |
| `k? != v` | ❌ | ✅ | ✅ |

`?` may be appended to any path: `request.method?`, `span.[].name?`.

This is intentionally different from SQL `NULL` semantics: it is an explicit *include-absent* switch, not three-valued logic. Negation (`not (k = v)`) always excludes absent fields.

---

## 5. Literals

| Form | Example | Notes |
|---|---|---|
| Bare word | `info`, `GET`, `/api/v1` | Trimmed; no spaces |
| Number | `400`, `3.14`, `-0.5` | Parsed as f64 for comparison |
| JSON string | `"hello world"`, `"Error:\nSomething"` | Escapes processed |
| Boolean | `true`, `false` | Case-sensitive lowercase |
| Null | `null` | Matches explicit null values |

Bare words may contain `-`, `_`, `.`, `/`, `:`, `@` but **not** whitespace, quotes, parentheses, commas, or operators.

---

## 6. Case Sensitivity

- Field names: case-sensitive.
- Operators and keywords: case-insensitive (`AND`, `and`).
- Values: case-sensitive by default.
- `--ignore-case` (P2) would make string operators case-insensitive globally; not in v1.

---

## 7. Comparison Semantics (Type Coercion)

Equality/order rules, evaluated in order:

1. **Both numbers** (or numeric literal vs numeric value) → numeric comparison.
2. **Numeric literal vs numeric-looking string** → numeric comparison (e.g. `duration > 0.5` matches `"duration":"0.7"`).
3. **Boolean literal vs boolean value** → boolean comparison.
4. **Null literal vs null value** → true.
5. **Both strings** → byte-wise comparison (`>`/`<`) or equality.
6. **Type mismatch** → `=` is false, `!=` is true (field exists), ordering is false.
7. **Objects/arrays** → `=`/`!=` compare structurally (JSON equality); ordering and substring are false.

Special cases:
- `level = info` compares against the **normalized** level, not the raw string (`INFO`, `Info`, `5` all match).
- `level > info` uses level ordering, not string ordering.
- `status >= 400` works whether `status` is `404` or `"404"`.

### 7.1 Truthiness for bare boolean expressions

A comparison must be a comparison; a bare field path is **not** a valid boolean expression (unlike jq). `-q 'error'` is a parse error. Use `exists(error) = true` or `error = true`.

---

## 8. Evaluation Order & Short-Circuiting

- `and` / `or` short-circuit.
- The engine reorders **top-level** AND terms by cost: level → time → equality → existence → substring → regex.
- Reordering never changes semantics because terms are side-effect free.
- Regex is always the last resort on a matching path.

---

## 9. Errors

| Error | Column | Message |
|---|---|---|
| Unterminated string | at quote | `unterminated string literal` |
| Unknown operator | at token | `unexpected token '=>'`, with nearest valid operator suggestion |
| Missing operand | at EOF | `expected a value after '='` |
| Unbalanced parens | at EOF | `expected ')' to close '(' at column N` |
| Invalid regex | at pattern | `invalid regex: ...` |
| Bare field path | at path | `expected a comparison, found field path 'error'` |
| `in @-` with stdin logs | at `@-` | `cannot read value set from stdin while logs are read from stdin` |

All query errors exit `1` and print a caret-positioned snippet (see CLI_SPEC §11).

---

## 10. Examples (normative; each becomes an integration test)

```sh
# Level
-q 'level = error'
-q 'level >= warn'
-q 'level in (warn, error, fatal)'

# Message
-q 'message ~= "connection refused"'
-q 'message ~~= "timeout|deadline"'

# Nested fields
-q 'request.method = GET'
-q 'request.headers.content-type ~= json'
-q '"request.method" = GET'          # exact key, no traversal

# Numeric
-q 'status >= 500'
-q 'duration > 0.5'
-q 'bytes < 1024'

# Booleans
-q 'cached = true'
-q 'retry != false'

# Absent modifier
-q 'request.id? = null'
-q 'internal? != true'

# Existence
-q 'exists(request.id) and not exists(request.internal)'

# Sets
-q 'status in (200, 201, 204)'
-q 'user not in @blocked.txt'
-q 'ip in @-'

# Wildcards
-q 'path like /api/*/users'
-q 'path not like /health*'

# Regex
-q 'msg ~~= "^panic:"'
-q 'msg !~~= "(?i)deprecat"'

# Arrays
-q 'span.[].name = sp0001'
-q 'span.[1].name = sp0001'
-q 'tags.[0] = production'

# Composition
-q '(level > info or status >= 400) and not (path ~= health)'
-q 'level in (error, fatal) or duration > 2.0'
```

---

## 11. Test Matrix Requirements

For every operator, cover:

1. Matching scalar (string, number, bool, null)
2. Non-matching scalar
3. Missing field
4. Nested field
5. Array field (`.[]` and `.[n]`)
6. Type mismatch
7. `?` modifier variants
8. Quoted key with dots
9. Reserved-word key via `.` / quoted form
10. Invalid syntax → error message and exit code

Snapshot tests should assert the **filtered output**, not only exit status.
