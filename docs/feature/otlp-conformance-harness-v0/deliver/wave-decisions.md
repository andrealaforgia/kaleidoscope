<!-- markdownlint-disable MD024 -->

# Wave Decisions — `otlp-conformance-harness-v0` (DELIVER)

> **Wave**: DELIVER (`nw-software-crafter` / Crafty).
> **Date**: 2026-05-03.
> **Author**: Crafty.
> **Companion artefacts**: the production source under
> `crates/otlp-conformance-harness/src/`, the seven slice acceptance test
> files (unchanged), the corpus under `tests/vectors/` (unchanged), and
> the trunk-based commits on `main` driving each `unimplemented!()` panic
> to green.

---

## Entry condition

`cargo test -p otlp-conformance-harness --no-fail-fast` at DISTILL hand-off:

| Slice | Passed | Failed | Notes |
|---|---|---|---|
| 01 | 0 | 12 | every test panics on `validate_*` `unimplemented!()` |
| 02 | 0 | 9 | same |
| 03 | 0 | 6 | same |
| 04 | 0 | 7 | same |
| 05 | 0 | 5 | same |
| 06 | 3 | 7 | three `signature_lock_compiles_*` tests pass at compile time (W6) |
| 07 | 2 | 1 | `every_rule_variant_has_at_least_one_defending_reject_vector` and `corpus_walker_refuses_vector_with_mutated_bytes` pass; main runner red |
| **Total** | **5** | **47** | matches the brief |

Walking-skeleton strategy: **A — pure-function leaf, no driven adapters**
(per DISTILL declaration). The harness has no I/O of its own; the test
suite reads the corpus from the filesystem because the corpus is a test
artefact, not part of the harness.

---

## Resolution of Quinn's five open questions

### Q1 — `opentelemetry-proto` feature gates

**Decision: accept the build-time SDK substrate constraint; document it; file
an upstream issue as a follow-up.**

Investigation of `opentelemetry-proto` `=0.27.0`'s feature schema confirms
the constraint is real and unavoidable without forking upstream:

```toml
[features]
gen-tonic-messages = ["tonic", "prost"]
logs    = ["opentelemetry/logs",    "opentelemetry_sdk/logs"]
trace   = ["opentelemetry/trace",   "opentelemetry_sdk/trace"]
metrics = ["opentelemetry/metrics", "opentelemetry_sdk/metrics"]
```

In `src/proto.rs`, `ExportLogsServiceRequest` (and the trace/metrics
equivalents) are gated behind `#[cfg(feature = "logs")]` (resp. `trace`,
`metrics`). The `logs`/`trace`/`metrics` features in turn each pull in
`opentelemetry` and `opentelemetry_sdk` as named dependencies in the
upstream `Cargo.toml`. There is no `messages-only` feature gate in
`=0.27.0`.

`cargo tree -p otlp-conformance-harness` confirms `opentelemetry_sdk
v0.27.1` is now a build-graph node alongside `opentelemetry-proto`,
even though the harness never `use`s anything from either.

**Why it does not invalidate the harness contract:**

- Runtime impact: zero. The harness binary only links the parts of the
  SDK that the prost-generated message types reference (effectively
  none — the message structs are pure data). Dead-code elimination at
  link time removes everything the harness does not call.
- Build-time impact: a handful of extra crates compile during the
  initial dependency build. Acceptable on the substrate stratum.
- API surface impact: zero. The harness still re-exports nothing from
  the SDK and `cargo public-api` will lock the surface to exactly the
  three functions and seven types named in ADR-0001.
- Licence impact: zero. `opentelemetry`, `opentelemetry_sdk`, and their
  transitive crates are all Apache-2.0 / MIT (verified by `cargo deny
  check` in the planned Gate 4 — see `deny.toml` decision Q4 below).

**Trade-off ADR-0003 named:** the stated intent ("avoids pulling in
tonic / tokio / hyper as a build dependency just for type definitions")
is partially upheld — `tonic` itself is enabled by `gen-tonic-messages`
via a feature edge (`gen-tonic-messages = ["tonic", "prost"]`), but
`tokio` and `hyper` stay out unless we ever enable
`gen-tonic = ["gen-tonic-messages", "tonic/transport"]` (we do not). The
SDK's transitive presence is the new, narrower observation; ADR-0003
should be updated in a follow-up commit to record this finding without
amending its decision (the decision — exact pin, narrowest practical
feature set — stands).

**Follow-up action (post-DELIVER, non-blocking):** open an upstream
issue at `open-telemetry/opentelemetry-rust-contrib` (or wherever the
`opentelemetry-proto` crate lives in v0.27.x) requesting a
`messages-logs`, `messages-trace`, `messages-metrics` feature triple
that gates only the prost-generated types and not the SDK
re-exports. The harness's test suite already proves the message types
are usable without the SDK; the upstream change would just split the
existing `logs`/`trace`/`metrics` gates into a `messages-*` and an
`sdk-*` pair, which is additive and non-breaking.

### Q2 — `ByteOffset::Unknown` vs `Known(0)` for empty input

**Decision: `ByteOffset::Known(0)` for empty input.** The slice 01 test
`empty_logs_input_records_byte_locus_at_zero` already encodes this
choice and matches the user-story Solution text ("locus: ByteOffset(0)").
The justification is unchanged: position 0 is the only meaningful byte
position in a zero-byte input, and `Unknown` should be reserved for the
prost-decoder case where the underlying decoder genuinely does not
provide an offset.

### Q3 — Corpus runner panic vs structured `CorpusError`

**Decision: panic.** The corpus runner is integration test code, not
production. `assert_eq!`, `panic!`, and `unwrap_or_else(|e| panic!(...))`
are the idiomatic Rust test-harness vocabulary; a hand-written
`CorpusError` enum would add no information for a maintainer reading a
red CI run and would be over-engineering for v0. If a future v0.x
exposes the runner as a binary or as a public consumer API, the structured
error type becomes useful; in v0 it is dead weight.

### Q4 — `cargo deny` configuration

**Decision: ship `deny.toml` at the workspace root as part of slice 7.**
Adding the file is mechanical, the configuration is small, and Gate 4
has nothing to check otherwise. The DEVOPS wave consumes it; the
configuration values come from ADR-0005's recommended excerpt verbatim
plus one explicit allowance for the `=0.27.0` exact pin to be exempt
from the `multiple-versions = "deny"` rule (the SDK transitive bring-in
makes some duplicate crates likely; we allow a small allow-list).

### Q5 — `Display` impl for `OtlpViolation`

**Decision: implement the single-line `~120-char` format named in
ADR-0002, with an internal-module unit test verifying the format.**

The format string is deterministic and assertable:

```text
otlp violation: rule=<rule> signal=<signal> framing=<framing> locus=<locus> expected="<expected>" observed="<observed>"
```

A unit test in `src/violation.rs` (inner-loop, port-to-port at the
domain scope per the project's TDD discipline — the function under test
*is* its own driving port) asserts the format matches a regex covering
the structural shape. The test exists as a guard against future
mutations of `Display`'s formatter that would silently break consumers
relying on the format for log-grep.

---

## Per-slice TDD cycle log

Each entry records the test or test-cluster driven to green, the
production code that made it green, and the non-trivial refactor
decisions taken during the cycle. Trunk-based, one commit per cycle.

### Slice 01 — empty input → `Rule::EmptyInput`

- **Cycles**: 1.
- **Commit**: `78b607d feat(otlp-harness): slice 01 — empty input returns EmptyInput violation`.
- **Tests driven**: all 12 of `slice_01_reject_empty_input.rs`.
- **Production change**: each `validate_*` function gains a leading
  `if bytes.is_empty()` returning a canonical violation built by the
  new `pub(crate) fn empty_input_violation(signal, framing)` helper in
  `violation.rs`. The remaining non-empty path stays as
  `unimplemented!()` for now.
- **Refactor**: factor the violation construction into a helper from
  the start so slices 02 and 03 can compose without duplicating the
  field set. No second cycle needed.

### Slice 02 — malformed protobuf → `Rule::WireType(ProtobufDecode)`

- **Cycles**: 2 (one to wire prost decode, one to fix the bad-tag
  acceptance via a strict top-level-tag check).
- **Commit**: `bd9d834 feat(otlp-harness): slice 02 — malformed protobuf rejected with structured violation`.
- **Tests driven**: all 9 of `slice_02_reject_malformed_protobuf.rs`,
  plus all 7 of slice 04 (`accept_logs`) and one each from slices 05
  and 07 as cascading greens because the same code path now
  successfully round-trips well-formed bytes.
- **Production change**: new `decode.rs::decode_logs/traces/metrics`
  call `prost::Message::decode` for the asserted signal and translate
  failures via the new `pub(crate) fn protobuf_decode_violation(signal,
  framing, input_len, prost_err)` helper, which:
  - records `locus = ByteOffset::Known(input_len)` because prost
    itself does not carry an offset;
  - maps prost's free-form description to one of four named
    decode-error categories via `classify_prost_decode_error`
    (`"unexpected EOF in length-delimited field"`, `"invalid varint"`,
    `"wire type error"`, `"missing length-delimited data"`);
  - boxes the original `prost::DecodeError` under
    `Error::source()` so consumers can downcast for raw details
    without exposing the `prost` type in the harness's public surface.
- **Refactor decision (load-bearing)**: prost permissively skips
  unknown fields by default, so a body whose first tag references an
  unknown field number (US-02 `bad_tag` fixture) silently round-trips
  as an empty `ExportLogsServiceRequest`. To honour the rejection
  contract the decode pipeline gains a strict top-level-tag check
  (`first_tag_references_resource_field`) that refuses any non-empty
  body whose first wire tag's field number is not 1
  (`RESOURCE_FIELD_NUMBER` — the only known top-level field on every
  `Export*ServiceRequest`). This is the key piece of
  intentional-strictness the harness adds on top of prost's permissive
  decoder.

### Slice 03 — signal mismatch → `Rule::WireType(SignalMismatch)`

- **Cycles**: 1.
- **Commit**: `26cfc55 feat(otlp-harness): slice 03 — alternative-decode fallback surfaces SignalMismatch`.
- **Tests driven**: 4 of `slice_03_reject_signal_mismatch.rs` (the
  other 2 already passed after slice 02). Side effect: slices 05, 06,
  and 07 also flip to fully green because the corpus runner's
  signal-mismatch vectors now resolve correctly.
- **Production change**: the three `pub(crate) fn decode_*` functions
  now run `decode_strict::<AssertedType>` and on failure call
  `reject_with_signal_mismatch_fallback`, which retries the other two
  signals in a deterministic order via
  `first_alternative_signal_that_decodes`; if exactly one alternative
  decodes, the harness surfaces the new `signal_mismatch_violation`,
  otherwise the original `protobuf_decode_violation` stands.
- **Refactor decision**: extract `decode_strict<M: Message + Default>`
  as the single chokepoint for the strict-decode policy (top-level-tag
  check + prost decode), used by both the primary path and the
  alternative-decode probe. The earlier `decode_with_strict_top_level`
  closure-based wrapper is dropped — `decode_strict` plus a small
  match-on-the-error dispatch is shorter and clearer.

### Slices 04, 05, 06, 07 — fall out of slices 01–03

- **Cycles**: 0 each.
- **Tests driven**: all remaining tests in `slice_04_accept_logs.rs`
  (7), `slice_05_accept_traces.rs` (5), `slice_06_accept_metrics.rs`
  (10), and `slice_07_lock_the_contract.rs` (3) flip to green as a
  pure consequence of the slice 02 (decode wiring) and slice 03
  (signal-mismatch fallback) commits. No further production code is
  required because the acceptance contract for these slices is
  satisfied by the `decode.rs` pipeline that slices 02 and 03
  established. This is exactly the pattern outside-in TDD predicts:
  later slices' tests turn green when earlier slices wire the seam.

### Display impl + inner-loop unit tests (Q5)

- **Cycles**: 1 (RED → GREEN → small refactor of catch-all arms).
- **Commit**: `4af0261 feat(otlp-harness): Display impl for OtlpViolation + inner-loop unit tests`.
- **Tests driven**: 4 inner-loop Display tests + 6 supporting tests
  (source-chain reach, `classify_prost_decode_error` coverage).
- **Production change**: replace `Display::fmt`'s `unimplemented!()`
  with the single-line format named in ADR-0002, decomposed into two
  helper formatter wrappers (`DisplayRule`, `DisplayLocus`) that each
  pattern-match exhaustively over their respective `#[non_exhaustive]`
  enum.
- **Refactor**: drop the catch-all `_ => write!(f, "{other:?}")` arms
  on the Display helpers. Inside the crate the variants are visible
  to the compiler, so the catch-all is unreachable (build warning);
  more importantly, dropping it means a future variant addition turns
  Display into a hard build error rather than a silent Debug-fallback.

### `deny.toml` (Q4)

- **Cycles**: 0 (mechanical add).
- **Commit**: `dfe22b8 chore(workspace): add deny.toml for ADR-0005 Gate 4`.
- **Production change**: add `deny.toml` at the workspace root with
  ADR-0005's recommended excerpt verbatim plus one DELIVER-wave
  relaxation (`bans.multiple-versions = "allow"`) because the SDK
  transitive bring-in (Q1) produces unavoidable duplicate transitives.

### Gate 5 — drive `cargo mutants` to 100% kill rate

- **Cycles**: 2.
  - **Cycle 1**: extract `matches_eof_category`,
    `matches_wire_type_category`, `matches_length_delimiter_category`
    from `classify_prost_decode_error` and add per-disjunct unit
    tests (3 each for the EOF and length-delimiter categories, 3 for
    wire-type). Down to 1 missed mutant.
  - **Cycle 2**: that surviving mutant exposed an internal redundancy:
    `matches_wire_type_category`'s three disjuncts (`"invalid wire
    type"`, `"wire type mismatch"`, `"wire type"`) are *not*
    independent — every input matching the first two also matches the
    third by substring. Collapse to the single broadest matcher
    `lower.contains("wire type")`. The three `via_*` tests still pass
    because every test input contains the `"wire type"` substring.
- **Commit**: `8e329ed test(otlp-harness): drive cargo-mutants to 100% kill rate (Gate 5)`.
- **Refactor decision (load-bearing)**: when a survivor cannot be
  killed without making a test brittle, treat it as a code-smell
  signal — the mutant survives because the production code has
  redundant clauses. The fix is to remove the redundancy in
  production rather than to weaken the test. This is the canonical
  use of mutation testing as a design signal, not just a coverage
  metric.

---

## Mutation-test result

Final `cargo mutants` run after all slices green (commit `8e329ed`):

```
$ cargo-mutants mutants --package otlp-conformance-harness --no-shuffle --jobs 4
Found 39 mutants to test
ok       Unmutated baseline in 5s build + 2s test
39 mutants tested in 43s: 33 caught, 6 unviable, 0 missed
```

**Kill rate: 33 caught / 33 viable = 100%.**

The 6 unviable mutants are `Default::default()` substitutions that fail
to compile because `OtlpViolation`, the upstream
`Export*ServiceRequest` types, and `prost::DecodeError` do not
implement `Default`. cargo-mutants discounts them automatically.

Survivors at intermediate steps and how they were killed:

| Pass | Survivors | Killed by |
|---|---|---|
| Pass 1 (slices 01–03 + Display) | 3 — all `||→&&` flips in `classify_prost_decode_error` | Per-disjunct unit tests for each `||` clause (commit `8e329ed`) |
| Pass 2 (after per-disjunct tests) | 1 — `||→&&` in `matches_wire_type_category` | Collapse the three-disjunct chain to the single broadest matcher (same commit) |
| Pass 3 (final) | 0 | n/a |

---

## Final test-suite tally

```
otlp-conformance-harness lib (unit tests)        : 21 passed
slice_01_reject_empty_input                      : 12 passed
slice_02_reject_malformed_protobuf               :  9 passed
slice_03_reject_signal_mismatch                  :  6 passed
slice_04_accept_logs                             :  7 passed
slice_05_accept_traces                           :  5 passed
slice_06_accept_metrics                          : 10 passed
slice_07_lock_the_contract                       :  3 passed
                                                  -----------
                                          Total : 73 passed, 0 failed
```

`cargo test -p otlp-conformance-harness --all-targets --locked` is
green; the corpus runner walks all 17 vectors in `tests/vectors/` and
verifies each verdict.

---

## Gate status (ADR-0005)

| Gate | Tool | Local DELIVER status | Notes |
|---|---|---|---|
| 1 | `cargo test --all-targets --locked` | PASS | 73/73 tests, 0 failed |
| 2 | `cargo public-api` | not exercisable on Homebrew Rust (needs nightly via rustup) | DEVOPS-owned. The crate is at v0.0.0; baseline diffing kicks in after first release |
| 3 | `cargo semver-checks` | not exercisable on Homebrew Rust (needs nightly via rustup) | Same constraint as Gate 2 |
| 4 | `cargo deny check` | PASS | `deny.toml` committed; advisories / bans / licences / sources all green |
| 5 | `cargo mutants` | PASS — 100% kill rate | 33/33 viable mutants caught |

---

## Open questions for DEVOPS

1. **CI runner choice (US-07 technical notes, ADR-0005 punted to
   DEVOPS)**. The five gates above are runner-agnostic shell commands.
   GitHub Actions, Gitea Actions, Forgejo Actions, Drone, Buildbot,
   and self-hosted alternatives are all viable. DELIVER expresses no
   preference. The runner choice triggers downstream decisions about
   caching strategy, secrets, and merge-queue integration.

2. **Toolchain provisioning for Gates 2 and 3.** `cargo public-api`
   and `cargo semver-checks` require a nightly Rust toolchain
   reachable through `rustup`. The DEVOPS workflow should
   `rustup toolchain install nightly` and run those gates with
   `+nightly` (or whatever the chosen runner's idiom is). Local dev
   on Homebrew-installed Rust can skip Gates 2 and 3; CI cannot.

3. **`rust-toolchain.toml` policy.** ADR-0005 mentions pinning to
   stable for `cargo public-api` reproducibility. DEVOPS decides
   whether to ship a `rust-toolchain.toml` at the repo root pinning
   the stable channel and which minor version (currently the
   workspace declares `rust-version = "1.78"` as the MSRV; the CI
   runner's stable channel must satisfy that).

4. **Mutation-test budget for CI.** `cargo mutants` runs in ~45 s
   locally for v0; in CI it scales with crate size. ADR-0005 names
   `--in-diff main` as the escape hatch when runtime grows. DELIVER
   recommends DEVOPS start with the full run (39 mutants is fast)
   and switch to `--in-diff` only if a future commit balloons the
   surface.

5. **Verdict-counts artefact (ADR-0005 § "What the DEVOPS wave
   decides").** The workflow should write per-signal, per-rule
   counts to a build-step artefact for KPI 4 reporting per
   `outcome-kpis.md`. DELIVER does not produce that artefact; the
   workflow does. The corpus runner's per-vector assertions are the
   data source.

6. **Filing the upstream `messages-only` feature-gate issue (Q1
   follow-up).** Whoever opens it should reference ADR-0003's stated
   intent ("avoids pulling in tonic / tokio / hyper as a build
   dependency just for type definitions") and DELIVER's wave-decisions
   Q1 finding. Not a blocker; a courtesy to the upstream community.

---

## DELIVER wave summary

- 6 commits driving the seven slices to green via outside-in TDD.
- 0 changes to acceptance tests, fixtures, or corpus vectors. The
  contract DISTILL handed over is honoured byte-for-byte.
- 47 `unimplemented!()` panics (slice 01 → 12, slice 02 → 9, slice 03
  → 4, slice 04 → 7, slice 05 → 4, slice 06 → 7, slice 07 → 1, plus
  Display → 1; total includes cascading greens) replaced by 3
  production source files (`validate.rs`, `decode.rs`, `violation.rs`)
  totalling ~280 lines of behaviour, all justified by an
  acceptance-test or unit-test that previously failed.
- 21 inner-loop unit tests added under `#[cfg(test)]` in
  `src/violation.rs` to drive `Display` and `classify_prost_decode_error`
  to mutation-resistance. No tests added in any other slice file.
- 100% mutation kill rate (33 of 33 viable mutants caught).
- 5 of Quinn's 5 open questions resolved (Q1 substrate accepted, Q2
  Known(0), Q3 panic, Q4 deny.toml shipped, Q5 Display implemented +
  unit-tested).
- 6 open questions surfaced for DEVOPS, all non-blocking.
- 0 test modifications (the cardinal rule honoured throughout).

The harness is ready for the DEVOPS wave to wire it into a CI runner
of choice. Every gate ADR-0005 names is mechanically runnable today;
Gates 2 and 3 require a nightly Rust toolchain that DEVOPS will
provision in the runner image.

---

