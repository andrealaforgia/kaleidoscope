# ADR-0046 — query-api regex label matchers (`=~`, `!~`)

- **Status**: Accepted
- **Date**: 2026-05-22
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `query-api-regex-matchers-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Refines**: ADR-0044 (the query-api label-matcher grammar and Prometheus
  semantics this REFINES). ADR-0044 Decision 4 deferred regex matchers with an
  honest 400 ("any operator other than `=`/`!=`, notably regex `=~`, `!~`,
  returns HTTP 400"). This ADR realises that anticipated extension for the `=~`
  and `!~` operators.
- **Related**: ADR-0042 (the query-api contract and minimal PromQL subset that
  ADR-0044 itself refines; cited as the framing contract, NOT modified).

## Context

ADR-0044 shipped the `=` and `!=` label matchers on the bare-name selector and
deliberately rejected the regex operators `=~`/`!~` with an honest 400, since
regex matching was out of scope for that slice. The `=`/`!=` slice therefore
added no regex crate, holding a zero-dependency posture. This feature delivers
the `=~` (matches) and `!~` (does-not-match) regex label matchers so an on-call
operator can filter a noisy metric to a family of series with one pattern,
server-side, during an incident.

Two things in this slice are non-obvious and correctness-and-security-critical,
so this ADR documents them explicitly: the engine choice (the pattern is
exposed user input, so a backtracking engine would be a ReDoS surface), and the
Prometheus full-anchoring rule (`is_match` is unanchored; Prometheus anchors
both ends).

ADRs in this repository are immutable (superseded, never edited). ADR-0044 is
Accepted and referenced. Rather than mutate it, this ADR is a separate,
referenceable record that REFINES it, leaving the original intact. A new ADR was
chosen over an in-place edit for that immutability reason; the back-reference
keeps the subset contract discoverable as a three-document set (0042 the
contract frame, 0044 the `=`/`!=` matcher refinement, 0046 the regex matcher
refinement). ADR-0046 is the next free number (the highest existing was 0045,
verified).

## Decision

### 1. The regex engine is the `regex` crate, promoted to a direct dependency

Regex matching uses the `regex` crate, promoted from a transitive to a DIRECT
dependency of `crates/query-api`. The `regex` crate is RE2-derived: it matches
in linear time and has no catastrophic backtracking. Because the pattern is
exposed USER input, a backtracking engine would be a ReDoS attack surface; the
`regex` crate removes that class of attack by construction. The crate is ALREADY
present in `Cargo.lock` (v1.12.3) as a transitive dependency, so promoting it to
a direct dependency of `crates/query-api` likely adds NO new transitive crates
to the graph. The `deny.toml` / Gate-4 verification of that claim is a DEVOPS
task (see the wave-decisions DEVOPS handoff); this decision records the engine
choice and the security rationale, not the supply-chain verification.

### 2. Anchoring: compile the user pattern wrapped as `^(?:{pattern})$`

`regex::Regex::is_match` is unanchored (it tests for a substring match).
Prometheus anchors both ends, requiring a full-string match. The raw user
pattern is therefore wrapped as `^(?:{pattern})$` before compilation: the
non-capturing group bounds the pattern's own alternation (so `a|b` becomes
`^(?:a|b)$`, not `^a|b$`), and the `^`/`$` require a full-string match. This is
exactly the Prometheus rule.

### 3. Type shape: extend `MatchOp`; the compiled `Regex` lives filter-side

`MatchOp` is extended from `{Equal, NotEqual}` to `{Equal, NotEqual, Matches,
NotMatches}`. `LabelMatcher` is unchanged: it keeps the RAW pattern string in
its existing `value` field and stays a plain, comparable data struct that
derives `Eq`/`Hash` (for parser tests and any future keying). A compiled
`regex::Regex` is NOT `Eq` and NOT `Hash`, so it must NOT live in `MatchOp` or
`LabelMatcher`. The compiled `Regex` lives in the FILTER, built ONCE per matcher
per query at filter-build time (before the row scan), never per row and never
stored in the parsed types. Building once per query keeps the linear-time
guarantee across many rows: the compile is the only super-linear-in-pattern
step and it runs once. A compile failure at filter-build is the single origin of
the invalid-regex HTTP 400 `{status:error, error:"invalid regex matcher"}`,
which never echoes the offending pattern, the raw query, or a forwarded header
(DD6 redaction).

### 4. Absent label treated as empty string before the anchored regex test

The regex test reuses the absent-as-empty rule the `=`/`!=` arms already use
(`labels.get(name).unwrap_or("")`), applied over the same merged label set
(`resource_attributes` then `point.attributes` winning, then authoritative
`__name__`). One rule yields the whole matrix: `=~` keeps iff the anchored regex
matches the absent-as-empty value, and `!~` is its exact negation. Concretely:

- `=~""`: keeps absent or present-and-empty.
- `=~".+"`: keeps present-and-non-empty.
- `!~""`: keeps present-and-non-empty.
- `!~".+"`: keeps absent or present-and-empty.

Regex and `=`/`!=` matchers are ANDed freely: a row is kept iff it satisfies
every matcher.

### 5. The public `query_api::router` signature is unchanged

`query_api::router` is byte-identical; the behaviour rides the existing
`handle_query_range` handler on the same `/api/v1/query_range` route with the
same response envelope. The only insertion is the compile-and-map step between
`selector::parse` and the existing `retain`; the orchestration order is
otherwise unchanged.

## Alternatives considered

### A (rejected): the no-regex status quo (slice-01's honest 400)

Keep ADR-0044's behaviour: reject `=~`/`!~` with the honest 400. For: zero
change, zero dependency. Against: it leaves the capability unbuilt and is the
deferral this feature is chartered to close (ADR-0044 Decision 4). Rejected; it
is the gap being filled.

### B (rejected): a hand-rolled matcher

Implement a small bespoke matcher instead of taking a regex crate. For: zero new
dependency, preserving the `=`/`!=` slice's posture. Against: it reimplements a
regex engine, and a naive hand-rolled engine is a ReDoS risk on exposed user
input; matching Prometheus pattern semantics by hand is error-prone and would
need its own mutation-grade test bed. Rejected; an RE2-derived crate already in
the lock is safer and smaller than reimplementing an engine.

### C (rejected): compile the regex per row

Compile each matcher's pattern inside the per-row scan rather than once at
filter-build. For: marginally simpler control flow. Against: the compile is the
only super-linear-in-pattern step, and running it per row multiplies it by the
row count, breaking the linear-time-per-row guarantee and inflating the latency
budget. Rejected; compile once per query at filter-build.

### D (rejected): store the compiled `Regex` in `MatchOp`/`LabelMatcher`

Carry the compiled `regex::Regex` inside the parsed matcher types. For: one
place for the matcher and its compiled form. Against: a compiled `Regex` is not
`Eq` and not `Hash`, which the matcher types derive (for parser tests and future
keying); embedding it would force dropping those derives or wrapping the field,
muddying the pure, comparable parsed types. Rejected; the compiled regex lives
filter-side and the parsed types stay pure.

## Consequences

### Positive

- **A new direct dependency, `regex`, chosen for safety.** RE2-derived,
  linear-time, ReDoS-free by construction on exposed user input. Already in the
  lock, so the supply-chain delta is expected to be nil (DEVOPS verifies Gate 4).
- **Correct full-anchor matching.** Wrapping the raw pattern as `^(?:re)$` gives
  the Prometheus full-string-match rule, with the pattern's own alternation
  correctly bounded.
- **The absent-as-empty regex matrix.** The four-arm matrix falls out of one
  rule reused from the `=`/`!=` slice, pinned by per-arm tests.
- **Honest, redacted invalid-regex 400.** A compile failure is a single,
  well-located 400 that never echoes the pattern, the raw query, or a forwarded
  header (DD6). A valid-but-never-matching pattern is the calm 200 empty arm.
- **Unblocks richer PromQL later.** The regex operators are the next rib on the
  same parse + filter spine; the envelope and router are unchanged.

### Negative

- **One new direct dependency to hold.** `regex` departs from the `=`/`!=`
  slice's zero-dependency posture. Mitigated: it is already in the lock at
  1.12.3 under licences the existing `deny.toml` allow-list tolerates; DEVOPS
  verifies Gate 4 confirms no new licence, advisory, or yanked crate.
- **The subset contract is now a three-document set.** ADR-0042 + ADR-0044 +
  ADR-0046. Mitigated: the explicit "Refines" back-references make the chain
  discoverable; this is the cost of ADR immutability.
- **A per-query regex compile + match on the latency path.** Mitigated: compile
  runs once per matcher per query, patterns are short, and the inherited p95 <
  500 ms budget comfortably absorbs it; the per-row scan stays linear.

### Trade-off summary

The slice trades the `=`/`!=` slice's zero-dependency posture for one
RE2-derived crate already in the lock, buying ReDoS-safe regex matching with
full Prometheus anchoring and the absent-as-empty matrix, while keeping the
parsed types pure and the router and envelope unchanged. Both the dependency
promotion and the per-query compile cost are recorded so a future reader
understands the cost was chosen, not overlooked.

## Verification

- Per-arm acceptance tests for the absent-as-empty regex matrix (`=~""`,
  `=~".+"`, `!~""`, `!~".+"`) and AND composition with `=`/`!=` matchers.
- A full-anchor test confirming a substring-only match is rejected (so a mutant
  dropping the `^...$` wrapping must die).
- An invalid-regex 400 test plus a redaction test that the 400 never echoes the
  offending pattern, the raw query, or a forwarded header (DD6).
- A valid-but-never-matching test confirming the calm `result:[]` success arm
  (the invalid-vs-never-matching distinction must not degrade a 400 to a 200, or
  the reverse).
- A contract test that success, empty, and the new 400 still pass Prism's
  `isPromSuccess` / `isPromError` (envelope unchanged).
- Mutation testing: `cargo mutants` scoped to `crates/query-api/src/` via
  `--in-diff` at the project 100% kill-rate gate (ADR-0005 Gate 5; CLAUDE.md).
  Covered by the existing `gate-5-mutants-query-api`; no new gate. The full-anchor
  boundary, the `Matches`/`NotMatches` negation, and the invalid-vs-never-matching
  distinction are the primary mutation targets.
- No Earned-Trust probe change: the new logic is pure and in-process with no
  external substrate; the ADR-0042 Decision 8 startup probe and its
  three-orthogonal-layer enforcement are unchanged.

## External-integration handoff

No new external integration. The Prism contract boundary
(`/api/v1/query_range`) recorded in ADR-0042's handoff is unchanged: the
response envelope (success, empty, the parse 400s, the new invalid-regex 400)
still satisfies Prism's `isPromSuccess` / `isPromError`. The regex feature
changes only WHICH series the success arm carries and adds one new 400 arm,
never the envelope, so the existing consumer-driven contract posture (Apex's
choice of Pact-JS or container fixtures) covers it without a new contract. The
`regex` crate is an in-process library, not a network integration.
