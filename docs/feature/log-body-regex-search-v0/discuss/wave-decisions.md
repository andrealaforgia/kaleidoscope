# DISCUSS Decisions — log-body-regex-search-v0

Author: `nw-product-owner` (Luna), DISCUSS wave, 2026-05-29.
Feature type: Backend. Walking skeleton: No. Research depth:
Lightweight. JTBD: No (the operator job is the natural extension
of the `body_contains` job pinned in
`log-body-text-search-v0`).

## Wave context

Brownfield carpaccio slice on `crates/log-query-api`, immediate
sibling of `log-body-text-search-v0` (ADR-0055, shipped at commit
1bfa609 with the gate-5 follow-up at d96a807). ONE optional
query-string parameter `body_regex=<pattern>` on `GET /api/v1/logs`
filters returned `LogRecord`s to those whose `body` field is
matched by a regular expression compiled via the workspace's
existing `regex` crate (already proven for metric-label matchers
in `query-api` per ADR-0046).

The walking skeleton (read endpoint, durable store, tenant seam,
caps, severity floor, byte-substring filter) is already live; this
slice rebuilds none of it. The slice exercises three previously
shipped scaffolds in combination for the first time:

1. The `query-http-common` extraction (ADR-0054) — fourth
   real-world parse-and-wire consumer of the shared scaffold.
2. The `regex` crate (ADR-0046 Decision 1) — first cross-pillar
   reuse outside `query-api`'s metric-label matchers.
3. The `gate-5-mutants-lumen` workflow (d96a807) — first
   `lumen::Predicate` arm born AFTER the gate landed.

## Key Decisions

- **[D1] Feature type: Backend.** The slice has no UI surface;
  every user observation is over HTTP. See `user-stories.md` §
  System Constraints.
- **[D2] Walking skeleton: No.** The endpoint, the durable store,
  the tenant seam, the caps, the severity floor, and the
  byte-substring filter all already ship at HEAD. The slice
  carpaccios on top of the existing skeleton. See `story-map.md`
  § Walking Skeleton.
- **[D3] Research depth: Lightweight.** The user job is identical
  in shape to the `body_contains` job pinned in
  `log-body-text-search-v0`; only the matching grammar changes.
  No new persona discovery, no new emotional arc work. See
  `user-stories.md` § US-01 Who (Maria / Marcus / Priya mirror
  the `body_contains` cohort).
- **[D4] JTBD: No.** The operator job ("isolate records carrying
  a known error signature in this window for this tenant") is
  the same job ADR-0055 ships against; this slice grows the
  signature grammar from byte-substring to regex. The DIVERGE
  artefacts that would document a new job are not required.
- **[D5] Story count: 7.** Within the 3-7 carpaccio sweet spot.
  Scenario count is 9 (US-04 split into 4a, 4b, 4c); split
  rationale in `dor-validation.md` § Item 4.
- **[D6] Estimated effort: 2 days.** Mirrors the
  `log-body-text-search-v0` effort envelope. Right-sized as one
  slice.

## Requirements Summary

- **Primary user need**: Maria (on-call SRE) needs to isolate
  every shape of a known failure family in one log read,
  without running three separate `body_contains` queries or
  downloading the whole window and grepping client-side.
- **Walking skeleton scope**: Not applicable (no walking
  skeleton).
- **Feature type**: Backend (HTTP read endpoint extension).

## Constraints Established

- The existing `MAX_WINDOW_SECONDS = 86_400` and
  `MAX_RESULT_ROWS = 100_000` caps (ADR-0050) are PRESERVED
  unchanged. Both consumed from `query_http_common::`.
- The error envelope on every rejected input is the existing
  `{"status":"error","error":"<reason>"}` shape, emitted via
  `query_http_common::error_response`. No new envelope. No new
  status code.
- The fail-closed tenancy seam goes through
  `query_http_common::resolve_tenant_or_refuse`. No
  re-implementation.
- The redaction posture (the error text NEVER echoes the raw
  parameter value) is preserved on all three new 400 arms
  (empty, over-cap, invalid syntax) and on the mutual-exclusion
  400 arm.
- The bare JSON array success shape (ADR-0047 Decision 1) is
  preserved. Empty result is `[]` with HTTP 200, NEVER 404.
- The `LogRecord.body` field is the ONLY matched field. Other
  fields (`severity_text`, `attributes`, `resource_attributes`,
  trace context) are explicitly out of scope.
- The half-open `[start, end)` window from ADR-0047 § 3 is
  preserved unchanged.
- The `min_severity` parameter from
  `log-query-severity-filter-v0` is PRESERVED unchanged. When
  both `min_severity` AND `body_regex` are present, both filters
  apply (conjunctive AND).
- The `body_contains` parameter from `log-body-text-search-v0`
  is PRESERVED unchanged. When BOTH `body_contains` AND
  `body_regex` are present in the same request, the response is
  HTTP 400 with the literal envelope
  `{"status":"error","error":"specify body_regex or body_contains, not both"}`
  (the mutual-exclusion PIN; see PIN 7 in `user-stories.md`).
- The slice composes with the existing
  `query_with(&tenant, range, &predicate)` seam on
  `lumen::LogStore`. The trait signature stays byte-identical to
  the prior tag.
- The regex grammar is the `regex` crate's default syntax (PIN 1
  in `user-stories.md`).
- Matching is case-sensitive by default; operators opt in to
  case-insensitive matching via the inline `(?i)` flag (PIN 2).
- Length cap is 1024 bytes, INCLUSIVE, reusing the
  `body_contains` value (PIN 3).
- Empty value, over-cap value, and compile-failure value ALL
  return 400 with the SAME literal envelope
  `{"status":"error","error":"invalid body_regex"}` (PIN 4 +
  PIN 5).
- Matching is unanchored by default (`Regex::is_match` matches
  anywhere); operators use `^` / `$` for explicit anchoring
  (PIN 6).
- The `gate-5-mutants-lumen` workflow (shipped at d96a807) will
  exercise the new `Predicate::body_regex` arm at the 100%
  kill-rate gate (KPI K5).

## Flags to DESIGN

Six flags are surfaced for DESIGN to pin verbatim. Recommendations
are based on direct read of the source tree at HEAD; rationale is
in `user-stories.md` § US-01 Technical Notes and the PINs
section.

### FLAG 1 — Regex compile location

| | |
|---|---|
| **Question** | Where does the regex get compiled? In the HTTP handler at parse time, or inside `Predicate::matches` per record? |
| **Options** | (a) Handler-side compile (one `Regex::new` per request, at parse time, fail-fast 400 on invalid syntax, store NEVER touched on compile failure). (b) Predicate-side compile per record (predicate carries `Option<String>`; `matches` calls `Regex::new` per record). |
| **DISCUSS recommendation** | (a) Handler-side compile, symmetric with `query-api`'s ADR-0046 Decision 3 ("Compile the regex matchers ONCE, before the row scan", verified at `crates/query-api/src/lib.rs:190-195`). |
| **Rationale** | (a) Fail-fast: invalid syntax never costs a store scan. (b) Performance: per-record compile dominates the per-record match cost on any non-trivial pattern. (c) Symmetry: `query-api` already pins this shape for label matchers; consistency across pillars. |

### FLAG 2 — Predicate field type

| | |
|---|---|
| **Question** | Does `lumen::Predicate` carry `body_regex: Option<Regex>` (compiled) or `body_regex: Option<String>` (raw)? |
| **Options** | (a) Compiled `Option<Regex>`. (b) Raw `Option<String>` with per-call compile. |
| **DISCUSS recommendation** | (a) Compiled `Option<Regex>`. The handler compiles once at parse; lumen stores and matches the compiled regex. |
| **Rationale** | Consistent with FLAG 1. Avoids re-compile per record. NOTE: `Regex` does NOT implement `PartialEq` / `Eq`; the existing `#[derive(PartialEq, Eq)]` on `Predicate` (verified at `crates/lumen/src/predicate.rs:24`) MUST be dropped. The trait is not used in production paths; lumen tests compare predicates by behaviour. DESIGN to confirm by `grep` of the lumen acceptance suite. |

### FLAG 3 — Length cap value

| | |
|---|---|
| **Question** | What is the maximum byte length of a `body_regex` value? |
| **Options** | (a) 1024 bytes (reuse `MAX_BODY_CONTAINS_LEN`). (b) 512 bytes (tighter). (c) 4096 bytes (looser). (d) No cap. |
| **DISCUSS recommendation** | (a) 1024 bytes, INCLUSIVE, in a new local constant `MAX_BODY_REGEX_LEN` next to `MAX_BODY_CONTAINS_LEN` in `crates/log-query-api/src/lib.rs`. |
| **Rationale** | Operator-facing consistency: the same rule applies to every body-related parameter. The 1024 cap is large enough for any honest runbook-pasted regex (which would typically be 10s to 100s of bytes) and small enough to refuse abuse. ADR-0055 § Decision 5 / Length cap C documents the rationale verbatim. |

### FLAG 4 — Mutual exclusion vs body_contains

| | |
|---|---|
| **Question** | What does the platform do when BOTH `body_contains` AND `body_regex` are present in the same request? |
| **Options** | (a) Mutually exclusive: 400 with literal envelope. (b) AND-compose: both filters apply (intersection). (c) Last-one-wins: silently use `body_regex` (or `body_contains`) and discard the other. (d) Concatenate: treat the substring as a literal regex and AND. |
| **DISCUSS recommendation** | (a) Mutually exclusive at slice 01. Both present is 400 with literal envelope `{"status":"error","error":"specify body_regex or body_contains, not both"}`. Store is NEVER touched on this path. |
| **Rationale** | (i) The semantic question deserves a deliberate answer, not a quiet default. (ii) Dispatch surface stays at 6 reachable arms (the cross product of `min_severity` x exactly-one-of `{none, body_contains, body_regex}`) instead of growing to 8 (with both); the saved arms become testing surface a future slice MAY earn. (iii) The literal envelope text is NEW (it differs from `"invalid body_regex"` because neither value is syntactically invalid); DESIGN may rename or relocate the literal. The user-visible posture is "explicit error, never ambiguity". Future slices MAY relax. Verified by direct read of the current 4-arm dispatch at `crates/log-query-api/src/lib.rs:195-212`. |

### FLAG 5 — ADR-0056

| | |
|---|---|
| **Question** | Does this slice land a new ADR? |
| **Options** | (a) YES (new ADR-0056). (b) NO (slice covered by wave-decisions). |
| **DISCUSS recommendation** | (a) YES. Three triggers, any one of which independently warrants a new ADR. |
| **Rationale** | (i) `lumen::Predicate` grows by ONE new pub builder method whose signature mentions a `Regex` type. The `cargo public-api` diff is non-empty and the surface is now coupled to the `regex` crate's types. (ii) `lumen/Cargo.toml` grows by ONE new direct dependency (`regex = "1"`). The dependency surface of the storage crate is visible to downstream consumers. (iii) The HTTP read contract grows by one optional parameter on the same route, parallel to ADR-0055; the contract change deserves the same durable record. ADR-0001 established immutability for ADRs; ADR-0055 is the latest, so ADR-0056 is the next free slot (DESIGN to confirm via `ls docs/product/architecture/adr-0056*`). The ADR cites ADR-0047 (originating contract), ADR-0046 (the regex grammar precedent), ADR-0050 (caps), ADR-0052 (sibling parameter), ADR-0054 (shared scaffolding), and ADR-0055 (immediate predecessor, the byte-substring filter). |

### FLAG 6 — Anchoring / multiline defaults

| | |
|---|---|
| **Question** | What does `body_regex` match by default with respect to anchoring and multiline semantics? |
| **Options** | (a) `Regex::is_match` semantics: unanchored, single-line by default; operators use `^`, `$`, `(?m)` inline flags for explicit control. (b) Implicitly wrap the user pattern in `^...$` (whole-body match). (c) Implicitly wrap in `(?m)` (multiline). |
| **DISCUSS recommendation** | (a) `Regex::is_match` semantics, no implicit wrapping. |
| **Rationale** | Symmetric with `query-api`'s label matchers (ADR-0046, verified at `crates/query-api/src/lib.rs:190-195`); operator muscle-memory honesty (grep is unanchored, single-line; vim and sed agree); least-surprise default. Implicit wrapping would change the meaning of an operator-pasted pattern without notice. The acceptance suite SHALL include a scenario where the regex matches a substring in the middle of the body (e.g. `body_regex=timeout` against a body of `kafka timeout connecting to broker-3`) so the unanchored posture is observable. |

## Upstream Changes

**None.** Zero DISCOVER assumptions changed. The slice composes
additively on top of ADR-0047, ADR-0050, ADR-0052, ADR-0054,
ADR-0055, and ADR-0046 without altering any of them.

No DIVERGE artefacts were generated for this slice (the operator
job is the immediate extension of the `body_contains` job
pinned in `log-body-text-search-v0`); DIVERGE risk is noted as
acceptable for a parallel-shape carpaccio slice on a mature
endpoint.

## Source-grounded verification (DISCUSS read every file before pinning)

| File | What was verified |
|---|---|
| `crates/log-query-api/src/lib.rs` | Current `LogsParams` shape is `{ start, end, min_severity, body_contains }` (lines 104-114); current dispatch is the 4-arm cross product `min_severity` x `body_contains` (lines 195-212); current parse helpers are `parse_min_severity` (lines 257-276) and `parse_body_contains` (lines 288-296). The slice grows: one new field on `LogsParams`, one new parse helper, one new dispatch growth (6 reachable arms after mutual-exclusion check), one new mutual-exclusion check. |
| `crates/lumen/src/predicate.rs` | Current `Predicate` carries `service`, `min_severity`, `body_contains` (lines 26-33); current `matches` is conjunctive AND with three early-return-false arms (lines 66-84); current `is_empty` checks the three `is_none()`s (lines 88-90). The struct derives `#[derive(Debug, Clone, Default, PartialEq, Eq)]` (line 24); `Regex` lacks `PartialEq` / `Eq`, so the derive must be relaxed. |
| `crates/lumen/Cargo.toml` | Current direct dependencies are `aegis`, `serde`, `serde_json`. **`regex` is NOT a direct dependency.** The slice adds it. |
| `crates/query-api/src/lib.rs` | Verifies the compile-once-per-request pattern at lines 190-195 ("Compile the regex matchers ONCE, before the row scan"). The new `parse_body_regex` mirrors this shape exactly. |
| `crates/query-api/Cargo.toml` | Verifies `regex = "1"` is the workspace-pinned version (line 62), promoted from transitive in ADR-0046. The new direct dep on `lumen` will resolve to the same `Cargo.lock` pin (1.12.3) with zero lockfile change. |
| `docs/product/architecture/adr-0055-log-body-text-search.md` | Sibling pattern. ADR-0055 § "Forward-looking scope" already names the regex slice ("Add regex matching on `body` via a separate parameter (e.g. `body_regex=<pattern>`); the design wave for that slice MUST weigh the ReDoS posture and pick a backend"). This slice answers: backend = the `regex` crate (ADR-0046 grammar). |
| `docs/feature/log-body-text-search-v0/discuss/user-stories.md` | Elevator Pitch + Domain Example + UAT scenario shape verified; this slice mirrors the structure exactly. |
| `docs/feature/log-body-text-search-v0/design/wave-decisions.md` | Read-first verification pattern + Reuse Analysis table + Constraints Established + DEVOPS handoff section verified; this slice mirrors the structure. |
| `docs/product/architecture/adr-0046-query-api-regex-label-matchers.md` | Confirms the `regex` crate's grammar is the workspace standard for user-supplied regex patterns; ReDoS posture is the linear-time engine; compile-once-per-request pattern is the established shape. |

The `regex` crate is **NOT** today a direct dependency of `lumen`. DESIGN
must add `regex = "1"` to `crates/lumen/Cargo.toml` as part of the
DD3 / FLAG 2 / FLAG 5 trio, and snapshot the new public-api baseline
as part of the DELIVER wave.
