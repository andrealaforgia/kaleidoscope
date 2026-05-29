# Parse Helper Spec — `parse_body_regex`

Author: `nw-solution-architect` (Morgan), DESIGN wave, 2026-05-29.

## Signature

```rust
const MAX_BODY_REGEX_LEN: usize = 1024;

fn parse_body_regex(raw: &str) -> Result<Regex, &'static str>
```

Location: `crates/log-query-api/src/lib.rs`, immediately after
`parse_body_contains` (which lives at lines 288-296 today).
Visibility: private (the public surface of `log-query-api` is the
router; the helper is exercised via `#[cfg(test)] mod tests` and
via the slice-01 acceptance suite indirectly through the HTTP
boundary). The return type `Regex` is `regex::Regex`; the new
`use regex::Regex;` import sits next to the existing
`use lumen::{LogStore, Predicate, SeverityNumber, TimeRange};`.

## Error cases (all share the literal reason `"invalid body_regex"`)

| Case | Detection | Return |
|---|---|---|
| Empty string (`raw.is_empty()`) | `?body_regex=` arrives as `Some("")` from serde | `Err("invalid body_regex")` |
| Over-cap (`raw.len() > MAX_BODY_REGEX_LEN`) | strict `>` boundary; 1024 bytes inclusively accepted; 1025 strictly refused | `Err("invalid body_regex")` |
| Regex compile failure | `Regex::new(raw)` returns `Err(regex::Error)` | `Err("invalid body_regex")` |

The three failure modes ALL return the SAME literal
`"invalid body_regex"`. The reason text is a `&'static str`
constant; the raw `raw` value is NEVER interpolated. The
`regex::Error::Display` impl is NEVER called (it would leak
parts of the pattern, e.g. the offending position and a snippet
of the input). The literal is the single source of the redaction
guarantee.

## Success case

`Ok(Regex)` carrying the compiled `regex::Regex`. No
normalisation is applied to the input before `Regex::new`: no
trim, no case folding, no Unicode flag override. The `regex`
crate's default behaviour governs the grammar.

## Pins

- **No normalisation, no case folding, no Unicode flag
  override.** The `regex` crate's default mode is byte-wise
  case-sensitive matching with Unicode-aware character classes
  per the crate's defaults; the helper relies on that default and
  does NOT call `RegexBuilder::case_insensitive(...)`,
  `unicode(false)`, or any builder API. An operator who wants
  case-insensitive matching uses the standard inline `(?i)` flag
  (`body_regex=(?i)kafka`); an operator who wants multiline mode
  uses `(?m)`; an operator who wants whole-body anchoring writes
  `^pattern$` explicitly. Symmetric with PIN 2, PIN 6 in
  `user-stories.md` and with the `query-api` label matcher
  posture at `crates/query-api/src/lib.rs:188-195`.
- **Order of checks: empty -> over-cap -> compile.** The
  empty-string check runs FIRST because `Regex::new("")` returns
  `Ok` (the empty pattern matches every position; rejecting it
  by name keeps the operator-facing semantics honest). The
  over-cap check runs SECOND because `Regex::new` on a 1025-byte
  pattern would pay the parse cost before the cap rejection; the
  cap is a budget on parse work, not a budget on match work, and
  the cap rejection must precede the compile call. The
  compile-failure check runs THIRD because (after the empty and
  over-cap checks pass) `Regex::new` is the only remaining
  failure source.
- **The mutual-exclusion check lives in the HANDLER, NOT in
  `parse_body_regex`.** Pin: `if params.body_regex.is_some() &&
  params.body_contains.is_some() { return error_response(400,
  "specify body_regex or body_contains, not both") }`. Location:
  BEFORE any `parse_body_regex` call, immediately AFTER
  `parse_body_contains` returns. This placement guarantees: (a)
  the mutual-exclusion 400 is honest (it reports the cross-check
  failure, not a downstream compile failure), (b) the
  `body_contains` empty / over-cap 400 still surfaces first
  because `parse_body_contains` runs BEFORE the cross-check, (c)
  the store is NEVER touched on the mutual-exclusion path, and
  (d) `parse_body_regex` itself stays a one-parameter pure
  function over the raw string — it does not need to know about
  other parameters.

## Test surface (inline `#[cfg(test)] mod tests`)

The inline tests pin the boundary one byte at a time so a
`>` -> `>=` mutant on the cap is killed by the unit suite (the
acceptance suite kills it as well, but the inline test is the
fastest signal):

1. `parse_body_regex_accepts_exactly_1024_bytes` — pin a 1024-byte
   pattern that compiles cleanly (e.g. `"a".repeat(1024)`); assert
   `Ok`.
2. `parse_body_regex_rejects_1025_bytes_with_literal_reason` —
   `"a".repeat(1025)`; assert `Err("invalid body_regex")`.
3. `parse_body_regex_rejects_the_empty_string_with_literal_reason`
   — `""`; assert `Err("invalid body_regex")`.
4. `parse_body_regex_rejects_unbalanced_paren_with_literal_reason`
   — `"foo(bar"`; assert `Err("invalid body_regex")`.
5. `parse_body_regex_rejects_unknown_class_with_literal_reason`
   — `"[a-"`; assert `Err("invalid body_regex")`.
6. `parse_body_regex_error_reason_never_echoes_raw_value` —
   feed `"SECRET-foo(bar"`; assert the err reason is byte-equal
   to `"invalid body_regex"` and does NOT contain `"SECRET-"`.
7. `parse_body_regex_compiled_regex_matches_unanchored` —
   compile `"timeout"` and assert
   `Regex::is_match("kafka timeout connecting to broker-3") == true`;
   pins PIN 6 (unanchored) at the helper level.
8. `parse_body_regex_compiled_regex_is_case_sensitive_by_default`
   — compile `"kafka"` and assert `is_match("KAFKA timeout") == false`;
   pins PIN 2 (case-sensitive) at the helper level.
9. `parse_body_regex_inline_case_insensitive_flag_works` —
   compile `"(?i)kafka"` and assert `is_match("KAFKA timeout") == true`;
   pins PIN 2's escape hatch.

The acceptance suite (`tests/slice_01_body_regex.rs`, DISTILL
output) exercises the helper through the HTTP boundary; the
inline suite pins the per-helper behaviour.

## Out of scope (for this helper)

- The mutual-exclusion check (handler responsibility; pinned above).
- Logging or tracing on the failure path (the slice emits no new
  tracing event; the existing `tracing::error!` only fires on
  store failures, and the compile-failure path NEVER touches the
  store; symmetric with ADR-0055 § Decision 13).
- A pre-compiled regex cache across requests (each request
  compiles its own `Regex`; deferred per OUT-of-scope in
  `user-stories.md`).
- A per-pattern compile timeout (the `regex` crate's
  linear-time guarantee plus the 1024-byte cap is the budget).
