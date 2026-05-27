# `parse_body_contains` helper specification

Author: `nw-solution-architect` (Morgan), DESIGN wave, 2026-05-27.

The slice's new parse helper. Lives in
`crates/log-query-api/src/lib.rs` next to the existing
`parse_min_severity` helper. The DELIVER wave (crafter) implements
the body to satisfy the contract pinned here; the DISTILL wave
(acceptance designer) writes the test cases named in
§ "Test cases pinned for DISTILL" below.

## Signature (pinned)

```text
const MAX_BODY_CONTAINS_LEN: usize = 1024;

fn parse_body_contains(raw: &str) -> Result<String, &'static str>
```

- Lives at module scope in `crates/log-query-api/src/lib.rs`.
- `pub(crate)` (or private). Not exposed on the crate's public
  surface; not reachable from outside `log-query-api`.
- Takes a borrowed `&str` (the raw value from
  `params.body_contains.as_deref()`).
- Returns `Ok(String)` on success — a new owned `String` carrying
  the URL-decoded substring (axum's `Query<LogsParams>` extractor
  has already URL-decoded the parameter by the time the helper is
  called; the helper does NOT re-decode and does NOT escape).
- Returns `Err(&'static str)` on failure — a `'static` string
  literal that the handler passes verbatim to
  `query_http_common::error_response(StatusCode::BAD_REQUEST, _)`.

The `Err` arm carries a `&'static str` rather than a `String`
because the entire failure surface is two reason texts and both
are static literal constants (the DD5 anti-echo pin forbids
interpolation). A `&'static str` makes mutation testing easier
(a mutant that swaps the literal value is caught by the
byte-equality assertion on the response body) and makes the
function allocation-free on the failure path.

## Error cases (pinned)

| Input | Output | Reason |
|---|---|---|
| `""` (empty string) | `Err("invalid body_contains")` | DD4: an empty substring is meaningless on `String::contains` (every string contains the empty substring); refuse the ambiguity out loud. |
| `s` where `s.len() > 1024` | `Err("invalid body_contains")` | DD6: the length cap is 1024 bytes; the SAME literal envelope is reused; no second reason class is introduced; the raw oversize value is NEVER interpolated. |
| any non-empty `s` with `s.len() <= 1024` | `Ok(s.to_string())` | The happy path. No normalisation, no trim, no case folding. Byte-for-byte preservation of the operator's input. |

The `Err` arm uses a SINGLE literal `"invalid body_contains"` for
BOTH the empty-string and the over-cap rejections (DD5 pin: the
raw value is NEVER reflected). The handler's `error_response` call
becomes:

```text
match parse_body_contains(raw) {
    Ok(target) => Some(target),
    Err(reason) => return query_http_common::error_response(
        StatusCode::BAD_REQUEST,
        reason,
    ),
}
```

## Behavioural pins

The DELIVER implementation MUST honour the following:

- **NO case folding.** `parse_body_contains("KAFKA")` returns
  `Ok("KAFKA".to_string())`; `parse_body_contains("kafka")` returns
  `Ok("kafka".to_string())`; the two strings are byte-distinct and
  remain so through the helper. The case-sensitivity of the
  downstream match lives in `String::contains`; the helper does
  NOT pre-normalise.
- **NO Unicode normalisation.** `parse_body_contains("café")`
  (NFC) and `parse_body_contains("café")` (NFD) return two
  byte-distinct `Ok` values; the helper does NOT call
  `unicode-normalization`. A consumer that needs normalised
  matching pre-normalises both sides client-side.
- **NO whitespace trim.** `parse_body_contains(" kafka ")`
  returns `Ok(" kafka ".to_string())` (space-kafka-space, four
  bytes); the leading and trailing space are preserved verbatim.
  The operator decides what they search byte-for-byte; the
  platform does NOT helpfully trim.
- **No-op on multi-byte characters.** `parse_body_contains("§")`
  (a 2-byte UTF-8 character) returns `Ok("§".to_string())` and
  the resulting `String` carries the same two bytes; the helper
  does NOT touch code-point semantics. The length cap of 1024
  bytes is measured in bytes, NOT code points (the cap protects
  the wire / the store from oversize input; the byte count is
  what the wire carries).
- **`'static` reason.** The `Err` variant carries a
  `&'static str` literal. Any mutation that returns a different
  literal, an interpolated `String`, or a non-static reference is
  forbidden by the contract and killed by the byte-equality
  acceptance assertion against the literal envelope.

## Test cases pinned for DISTILL

The DISTILL acceptance designer MUST include the following inline
unit tests in the `#[cfg(test)] mod tests` block of
`crates/log-query-api/src/lib.rs` next to the existing
`parse_min_severity_*` tests. These pin the parser's behaviour
one mutation surface at a time; the cross-cutting acceptance
suite in `crates/log-query-api/tests/slice_01_body_contains.rs`
exercises the end-to-end HTTP behaviour around the helper.

| Test name | Input | Expected outcome | Mutation surface |
|---|---|---|---|
| `parse_body_contains_accepts_a_simple_substring` | `"kafka timeout"` | `Ok("kafka timeout".to_string())` | the success arm exists at all; a mutant that always returns `Err` is killed |
| `parse_body_contains_preserves_case_byte_for_byte` | `"KAFKA"` | `Ok("KAFKA".to_string())` | a `to_lowercase` mutant is killed |
| `parse_body_contains_preserves_leading_and_trailing_whitespace` | `" kafka "` | `Ok(" kafka ".to_string())` (four bytes; space-kafka-space-space) | a `.trim()` mutant is killed |
| `parse_body_contains_accepts_multi_byte_utf8` | `"café §"` | `Ok("café §".to_string())` | a code-point-counting cap mutant is killed (the input is < 1024 bytes); a normalisation mutant is killed |
| `parse_body_contains_rejects_the_empty_string` | `""` | `Err("invalid body_contains")` | a mutant that treats `""` as `Ok(String::new())` is killed (this is the "every record matches" silent-success mutant) |
| `parse_body_contains_rejects_over_cap_input` | `"a".repeat(1025)` (1025 bytes) | `Err("invalid body_contains")` | the boundary mutant `>` to `>=` is killed by the next test |
| `parse_body_contains_accepts_input_at_exactly_the_cap` | `"a".repeat(1024)` (1024 bytes) | `Ok("a".repeat(1024))` | the boundary mutant `>` to `>=` is killed (1024 bytes is INCLUSIVELY accepted; 1025 bytes is rejected; mirrors the inclusive cap-boundary posture from ADR-0050 Decision 1) |
| `parse_body_contains_error_reason_is_the_literal_class_label` | `""` and `"a".repeat(2048)` | the `Err` arm carries exactly `"invalid body_contains"`; the reason does NOT contain the substring `"a"` (the oversize raw value is NEVER echoed); the reason does NOT contain a length number | a mutant that interpolates the raw value into the reason text is killed; symmetric with the `parse_min_severity_error_reason_is_the_literal_class_label` test |

The acceptance suite in
`crates/log-query-api/tests/slice_01_body_contains.rs` SHALL also
exercise the parser end-to-end at the HTTP boundary via the six
scenarios named in `discuss/user-stories.md` § "UAT Scenarios"
(walking skeleton, calm empty, default unchanged, empty 400,
case-sensitive pin, cross-tenant isolation). The end-to-end
suite for the over-cap arm SHALL include one scenario that
sends a 2048-byte `body_contains` value and asserts:

1. The status is 400.
2. The body is byte-equal to
   `{"status":"error","error":"invalid body_contains"}`.
3. The body does NOT contain any byte sequence of the raw
   2048-byte value (a substring assertion against a fixed
   recognisable prefix of the value).
4. The store is NEVER touched (a `tracing` capture or a
   `BulkLogStore`-style spy confirms the query method was not
   called).
