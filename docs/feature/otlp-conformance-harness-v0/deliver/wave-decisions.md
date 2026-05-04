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

The cycle log below is appended as DELIVER progresses; each entry
records the test driven to green, the production line(s) that made it
green, and any non-trivial refactor decision.

(Cycle entries follow as commits land.)

---
