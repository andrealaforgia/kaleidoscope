# Parse-helper spec - log-query-pagination-v0

Two new free functions in `crates/log-query-api/src/lib.rs`, alongside
`parse_min_severity`, `parse_body_contains`, and `parse_body_regex`.
Both return `Result<usize, &'static str>` so the parse-time 400 fires
BEFORE any store work (the store is NEVER touched on an `Err` arm). The
reason literal is a static `&'static str`; the raw value is NEVER echoed.
This document pins the test surface for DISTILL.

## `fn parse_limit(raw: &str) -> Result<usize, &'static str>`

Behaviour:

- `"0"` -> `Err("invalid limit")`. Zero is invalid (PIN 6 / DD5): a page
  of zero records carries no information an absent request would not.
- Non-numeric (`"abc"`) -> `Err("invalid limit")`.
- Negative (`"-5"`) -> `Err("invalid limit")`. A leading `-` makes the
  string non-parseable as `usize`, so this is the same rejection arm as
  non-numeric; no separate sign check is needed.
- Value strictly greater than `query_http_common::MAX_RESULT_ROWS`
  (100000) -> `Err("invalid limit")` (US-05c / DD2). The boundary is
  `>`, INCLUSIVE at the cap.
- Otherwise -> `Ok(n)` for `1 <= n <= 100000`.

Check order: parse to `usize` (rejects non-numeric and negative) ->
reject `0` -> reject `> MAX_RESULT_ROWS` -> `Ok(n)`.

### Pinned test cases

| Input | Expected |
|-------|----------|
| `"3"` | `Ok(3)` |
| `"1"` | `Ok(1)` (smallest valid) |
| `"100000"` | `Ok(100000)` (at the cap, inclusive) |
| `"100001"` | `Err("invalid limit")` (strictly over cap; kills `>` -> `>=`) |
| `"0"` | `Err("invalid limit")` (zero rejected; kills the zero arm) |
| `"-5"` | `Err("invalid limit")` |
| `"abc"` | `Err("invalid limit")` |
| `""` | `Err("invalid limit")` (empty is non-numeric) |
| `"5000000"` | `Err("invalid limit")` (far over cap) |

Redaction assertion: for any `Err`, the returned reason equals exactly
`"invalid limit"` and does NOT contain the raw input substring (for
example the reason for `"-5"` does not contain `"-5"`).

## `fn parse_offset(raw: &str) -> Result<usize, &'static str>`

Behaviour:

- `"0"` -> `Ok(0)`. Zero is VALID (the first page; DD5).
- Non-numeric (`"abc"`) -> `Err("invalid offset")`.
- Negative (`"-1"`) -> `Err("invalid offset")`. Same parse-failure arm
  as non-numeric (leading `-` is non-parseable as `usize`).
- NO upper cap on `offset`. A large offset (for example past the result
  set) is `Ok(n)`; the empty page is produced by the slice
  (`skip(n)` yields an empty iterator), NOT by a parse error (PIN 4 /
  US-02 Example 3).
- Otherwise -> `Ok(n)` for any `n >= 0`.

Check order: parse to `usize` (rejects non-numeric and negative) ->
`Ok(n)`. No zero check, no upper-cap check.

### Pinned test cases

| Input | Expected |
|-------|----------|
| `"0"` | `Ok(0)` (first page; valid, NOT rejected) |
| `"2"` | `Ok(2)` |
| `"100"` | `Ok(100)` (large offset is valid; empty page is the slice's job) |
| `"-1"` | `Err("invalid offset")` |
| `"abc"` | `Err("invalid offset")` |
| `""` | `Err("invalid offset")` (empty is non-numeric) |

Redaction assertion: for any `Err`, the returned reason equals exactly
`"invalid offset"` and does NOT contain the raw input substring.

## Slice expression (handler, after the result-cap check)

```text
let limit = limit.unwrap_or(usize::MAX);  // no limit -> take all (cap is the backstop)
let page: Vec<LogRecord> = records.into_iter().skip(offset).take(limit).collect();
```

`offset` defaults to `0` when the parameter is absent. When `limit` is
absent, `take(usize::MAX)` returns the whole post-cap vector (at most
`MAX_RESULT_ROWS` records, so no overflow concern). DISTILL pins the
off-by-one mutation targets on `skip` / `take` via the US-04 partition
test.

## Asymmetry note (limit vs offset)

The two helpers are deliberately NOT symmetric:

- `limit` rejects `0` and rejects over-cap; `offset` accepts `0` and has
  no upper cap.
- `limit=0` is a client bug (an uninitialised page size); `offset=0` is
  the first page. A large `offset` is a calm empty page; a large `limit`
  contradicts the platform's committed cap and is refused.

This asymmetry is the contract, pinned by DD5, and the test cases above
make it explicit so a mutant that conflates the two helpers' rules is
killed.
