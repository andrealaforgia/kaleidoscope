# Application Architecture — `cinder-to-otlp-json-bridge-v0` (propose-mode walkthrough)

> **Author**: `nw-solution-architect` (Morgan), DESIGN wave, 2026-05-18.
> **Mode**: PROPOSE — two-to-three options per load-bearing decision,
> one recommendation per decision with traceable rationale.
> **Companion artefacts**: `wave-decisions.md` (this directory) for the
> compact decision log; ADR-0039 in `docs/product/architecture/` for
> the formal public-surface record; `## Application Architecture —
> cinder-to-otlp-json-bridge-v0` section in
> `docs/product/architecture/brief.md` for the architect-of-record
> summary plus C4 diagrams.

This document expands the DESIGN-wave options analysis behind the
compact decisions in `wave-decisions.md`. It is the artefact a
reviewer reads to audit *why* the recommendation was chosen over the
alternatives; it is not the artefact the crafter reads to implement
(the crafter reads ADR-0039 plus `wave-decisions.md`).

## Feature recap

`cinder-to-otlp-json-bridge-v0` adds one new public type to the
`self-observe` crate: `CinderToOtlpJsonWriter<W: Write + Send + Sync>`.

- **In-edge**: `cinder::MetricsRecorder` trait
  (`crates/cinder/src/metrics.rs:25-29`).
- **Out-edge**: generic `W: Write + Send + Sync` writing NDJSON lines
  in the OTLP-JSON envelope shape.
- **Behaviour**: one OTLP-JSON `ResourceMetrics` line per Cinder event
  (`record_place`, `record_migrate`, `record_evaluate`), with the
  per-event metric/attribute contract pinned in DISCUSS D1–D4 and
  cross-bridge-locked to `CinderToPulseRecorder` (ADR-0038 §2).
- **License**: AGPL-3.0-or-later (matches the `self-observe` crate).
- **CLI wiring**: explicitly out of scope (DISCUSS D9). v0 ships the
  library; the CLI follow-up wires the writer into `--observe-otlp`.

## How this writer relates to its three siblings

The `self-observe` crate already houses three writer files. After this
feature ships, it will house four:

| Crate's input port | In-process Pulse sink | Cross-process NDJSON sink |
|--------------------|------------------------|----------------------------|
| `lumen::MetricsRecorder` | `LumenToPulseRecorder` (shipped) | `LumenToOtlpJsonWriter` (shipped) |
| `cinder::MetricsRecorder` | `CinderToPulseRecorder` (shipped, ADR-0038) | **`CinderToOtlpJsonWriter` (this feature)** |

This is the long-anticipated fourth quadrant (lib.rs:44-47). The
naming convention is `{Source}To{Sink}Writer` for cross-process sinks
and `{Source}To{Sink}Recorder` for in-process sinks; this feature
preserves it.

## Load-bearing decisions — propose-mode walkthrough

### DD1 — Module file location

**Question**: Where does the new source file live inside the
`self-observe` crate?

| # | Option | Detail |
|---|--------|--------|
| 1 | `crates/self-observe/src/cinder_otlp_json.rs` | File-flat sibling of `lumen_otlp_json.rs`, `lumen_bridge.rs`, `cinder_bridge.rs`. |
| 2 | `crates/self-observe/src/bridges/cinder_otlp_json.rs` | New `bridges/` subdirectory; would force a retrospective move of the three existing files (or accept layout inconsistency). |
| 3 | Embed in `cinder_bridge.rs` as `pub mod otlp_json` | Co-locates both Cinder sinks but breaks the parallelism with the Lumen pair, which lives in two separate files. |

**Quality-attribute trade-off**:

- **Maintainability — Modularity**: Option 1 keeps file count manageable at N=4. Option 2 anticipates N=8–10 but forces a churn cost now. Option 3 introduces an outlier layout.
- **Maintainability — Modifiability**: All three options support additive evolution; Option 3 the worst (any future split between in-process and cross-process Cinder sink semantics requires a file move).
- **Compatibility — Interoperability** (operator's mental model): Option 1 preserves "one file per `{Source}{Sink}` writer", which is the convention every shipped writer file already follows.

**Recommendation**: Option 1. Same rule-of-three logic ADR-0038 §4
applied to the Pulse-sink sibling; identical justification here.

### DD2 — Attribute-array shape

**Question**: How are the per-point attribute slots typed when Cinder's
events have different cardinality (1, 2, 3) than Lumen's uniform `[T; 1]`?

| # | Option | Detail |
|---|--------|--------|
| 1 | Per-method fixed-size `[OtlpAttr; N]` (N=2 for place, N=3 for migrate, N=1 for evaluate); envelope structs become generic over a payload type. | Zero heap allocation. Three near-duplicated struct definitions. Per-method monomorphisation. Type-system enforces the cardinality at compile time. |
| 2 | One `OtlpNumberPoint` struct with `attributes: Vec<OtlpAttr<'a>>`; envelope-level fixed-size arrays preserved unchanged. | One small `Vec` allocation per event (≤3 entries, fits the smallest allocator size class). Trivial to extend to a fourth attribute on any event. Cost basis matches the existing `CinderToPulseRecorder`'s `BTreeMap<String, String>` per emission. |
| 3 | `enum OtlpCinderPoint<'a> { Place(...), Migrate(...), Evaluate(...) }` with derived `Serialize`. | Type-system encoded; needs `#[serde(untagged)]` to keep the wire shape, which is easy to get wrong. |

**Quality-attribute trade-off**:

- **Performance Efficiency**: Option 1 < Option 2 < Option 3 in theory; in practice all three are negligible compared to the `serde_json::to_string` call that follows. The writer is on a best-effort observability path, not a hot path. The `BTreeMap<String, String>` cost basis at the sibling `CinderToPulseRecorder` (cinder_bridge.rs:119, 125, 132) is the empirical reference point the operator's intuition is already calibrated against.
- **Maintainability — Modularity**: Option 2 ships one struct; Option 1 ships three near-clones; Option 3 ships one enum plus serde subtleties.
- **Maintainability — Modifiability**: Option 2 is most extensible (add a `cause: &str` to migrate without touching any other event); Option 1 is the most rigid; Option 3 is in between.
- **Functional Suitability — Correctness**: Option 1 provides the strongest compile-time guard against forgetting an attribute; Options 2 and 3 rely on the acceptance tests to catch a missing entry. The acceptance tests already catch this (Slice 01 asserts `tier`; Slice 02 asserts `from` and `to`); the compile-time guard is duplicative.

**Recommendation**: Option 2. The performance argument for Option 1
does not apply at this seam (NDJSON serialisation dominates), the
correctness argument does not apply (acceptance tests cover the
attribute cardinality), and the maintainability argument cuts strongly
for Option 2. The envelope-level `[T; 1]` arrays remain unchanged from
the Lumen writer (one resource, one scope, one metric, one data point
per line per DISCUSS D8); only the point-`attributes` slot becomes
`Vec<OtlpAttr<'a>>`.

The structural difference between the Lumen and Cinder writers'
envelope shapes is now **one line of code**: `attributes: [OtlpAttr<'a>; 1]`
becomes `attributes: Vec<OtlpAttr<'a>>`. Everything else is identical.

### DD3 — Acceptance-test seam

**Question**: How are the acceptance tests wired? Three components are
in play (Cinder driver, writer, sink); the question is which the test
exercises directly and which become substrate.

| # | Option | Detail |
|---|--------|--------|
| 1 | Drive `cinder::InMemoryTieringStore`; sink into `SharedBuf(Arc<Mutex<Vec<u8>>>)`; parse captured bytes as `serde_json::Value`; assert on the JSON tree. Mirrors `tests/lumen_to_otlp_json.rs:54-73` verbatim. | Cinder is the driver; the writer is the unit under test; the sink is the captured byte stream; `Value` assertions are robust to whitespace/field-order. Natural fit for the dual-emission contract (DISCUSS D8). |
| 2 | Drive the writer directly (`writer.record_place(&acme, Tier::Hot)`); sink into `SharedBuf`. | Smallest test surface; no Cinder entanglement; cannot express dual-emission. |
| 3 | Drive Cinder; sink into a real `tempfile::NamedTempFile`; parse the file. | Closest to the post-v0 CLI integration shape; requires `tempfile` dev-dep; the real-file semantics belong to the CLI follow-up's tests (DISCUSS D9 + shared-artifacts-registry MEDIUM-risk `file_handle` note). |

**Quality-attribute trade-off**:

- **Maintainability — Testability**: All three options support isolated testing. Option 1 entangles with Cinder behaviour; Option 2 isolates fully but loses the dual-emission contract; Option 3 introduces a substrate dependency v0 does not need.
- **Functional Suitability — Correctness**: Option 1 exercises the cascade end-to-end (one `evaluate_at` produces N migrate + 1 evaluate lines); Option 2 simulates the cascade by hand (brittle); Option 3 same as Option 1 but with extra substrate.
- **Reliability — Maturity**: Option 3 catches `File` semantics issues the v0 library does not own; out-of-scope for this feature.

**Recommendation**: Option 1. Identical to ADR-0038 §3 and to the
Lumen OTLP-JSON tests already shipped. Consistency across the four
writer test files (`lumen_to_pulse.rs`, `lumen_to_otlp_json.rs`,
`cinder_to_pulse.rs`, `cinder_to_otlp_json.rs`) makes the entanglement
trade-off acceptable; the cross-bridge invariants (NDJSON validity,
metric-name parity) require Option 1 to be expressible naturally.

The `SharedBuf` test substrate and the `collect_lines` helper are
copied byte-for-byte from `tests/lumen_to_otlp_json.rs:54-73`; the
duplication is acknowledged and accepted (third-writer extraction
triggers the rule of three at that future point).

A compile-time `assert_send_sync::<CinderToOtlpJsonWriter<Vec<u8>>>()`
probe lives in Slice 01's test file (covers all slices). This is the
subtype-check layer of the Earned Trust contract (Principle 12c).

### DD4 — Stub posture for the two un-implemented methods in Slice 01

**Question**: When Slice 01 ships `record_place` implemented and
`record_migrate` + `record_evaluate` as stubs, do the stubs panic
(`todo!()`) or silently do nothing (`{}`)?

| # | Option | Detail |
|---|--------|--------|
| 1 | Empty no-op `{}`. | Compiles immediately; matches `NoopRecorder`'s exact behaviour. |
| 2 | `todo!()`. | Loudly panics if accidentally invoked. |
| 3 | `unimplemented!()`. | Semantically equivalent to `todo!()`. |

**Quality-attribute trade-off**:

- **Maintainability — Testability**: Option 1 lets Slice 01 ship with stable, non-flaky tests. Options 2/3 add panic surface that Slice 01 does not cover.
- **Reliability — Maturity**: Options 2/3 fail loudly if a future test refactor causes a stray call. Slice 02 and Slice 03 are the very next slices; their RED tests are the loudness mechanism (a missing implementation produces an empty sink, which the asserts catch).

**Recommendation**: Option 1. Slice 02 and 03 are the immediate
loudness mechanism; the panic-on-stub diagnostic is theoretical in the
Slice-01-only window. Matches the Pulse-sink sibling's behaviour.

### DD5 — ADR scope

**Question**: How many ADRs does this DESIGN wave produce?

| # | Option | Detail |
|---|--------|--------|
| 1 | One ADR (ADR-0039): public surface + crate layout + per-event emission contract + test seam + file location. | Matches the per-crate-public-API convention chain (ADR-0011, ADR-0018, ADR-0022, ADR-0026, ADR-0033, ADR-0038). |
| 2 | Two ADRs (public surface + cross-bridge serde-struct duplication convention). | Premature formalisation; serde-struct duplication has only two exemplars (Lumen, Cinder). |
| 3 | Zero ADRs. | Leaves NDJSON-validity invariant (DISCUSS D6) and cross-bridge metric-name contract (D1) without a referenceable artefact. |

**Recommendation**: Option 1. Identical justification to ADR-0038 §
"Considered Alternatives — Alternative 6".

## Quality-attribute coverage (ISO 25010 — narrative form)

The summary table is in the brief's
`## Application Architecture — cinder-to-otlp-json-bridge-v0` section.
The narrative behind each row:

- **Functional Suitability — Correctness**: the per-event contract (in
  ADR-0039 §2) is exhaustive: three Cinder methods × one locked
  metric-name + attribute schema each. Every BDD scenario in
  `discuss/journey-observe-cinder-via-otlp-json.feature` resolves to a
  single per-event contract check. The cross-bridge metric-name
  parity (DISCUSS D1) is enforced by string-equality asserts in the
  tests on BOTH sides (Pulse sink + OTLP-JSON sink), so a code review
  diffing `cinder_bridge.rs` against `cinder_otlp_json.rs` would
  surface any drift.

- **Performance Efficiency**: one `Vec<OtlpAttr>` allocation per event
  (≤3 entries), one `serde_json::to_string` call (linear in line
  size), one `Mutex<W>` acquisition. No async, no I/O beyond `W`'s
  semantics, no network. The cost basis matches the established
  `CinderToPulseRecorder` per-event cost (`BTreeMap<String, String>`
  allocation).

- **Compatibility — Interoperability**: consumes `cinder::MetricsRecorder`
  (upstream port, unchanged) and produces OTLP-JSON `ResourceMetrics`
  lines (downstream protocol, defined by the OpenTelemetry
  specification). The generic `W: Write + Send + Sync` is the
  technology-neutral seam at the sink side; the operator's sidecar
  chooses any `W`-compatible substrate (`File`, `BufWriter<File>`, a
  memory buffer for tests, a `socket2::Socket` writer, etc.).

- **Reliability — Maturity**: best-effort emission posture (DISCUSS D5)
  prevents serialisation or write failures from propagating to Cinder
  (whose trait methods return `()`). The bridge cannot crash Cinder.
  `Mutex<W>` poisoning is handled with `if let Ok(mut writer) =
  self.inner.lock()` — a poisoned mutex causes silent emission loss,
  which is the documented contract; the same posture as the Lumen
  writer (lumen_otlp_json.rs:183).

- **Security — Integrity**: `tenant_id` is forwarded unchanged from
  Cinder's call to the OTLP-JSON output (DISCUSS D3 / shared-artifacts
  `tenant_id` HIGH-risk row). Two-tenant isolation is asserted in
  every slice's tests, defending against silent transforms (trim,
  case-fold, intern). Tier serialisation is locked to lowercase ASCII
  by the `tier_lowercase` helper from one source location.

- **Maintainability — Modularity, Testability**: one new file, three
  trait method bodies + one `emit` helper + one `tier_lowercase`
  helper. Acceptance tests are per-slice with explicit per-tenant
  isolation, NDJSON validity, and dual-emission tests. Mutation-
  testing scope is one file at 100% kill rate per ADR-0005 Gate 5
  (per CLAUDE.md's per-feature mutation testing strategy).

- **Maintainability — Modifiability**: public surface locked by
  `cargo public-api -p self-observe` (Gate 2) and `cargo semver-checks`
  (Gate 3); breaking changes require a major-version bump on the
  `self-observe` crate. The `attributes: Vec<OtlpAttr<'a>>` choice
  (DD2 Option 2) makes adding a fourth attribute to any event a
  one-line change in the `emit` helper.

- **Portability**: pure Rust, `#![forbid(unsafe_code)]` (inherited
  from `lib.rs:49`), no platform-specific code.

**ATAM sensitivity points**:

1. The `migrated.to_string()` rendering on `record_evaluate`. Exact
   for any `usize` (OTLP-JSON encodes `uint64` as a string, no
   precision loss). Defended by Slice 03's tests asserting the exact
   string ("5" for acme, "2" for globex). The architecturally
   meaningful upper bound on `migrated` is `usize::MAX`, which renders
   correctly because `to_string` on `usize` is the canonical
   representation.

2. The lowercase serialisation of `Tier` (DISCUSS D3). One helper
   function (`tier_lowercase`) drives both the `place` event's `tier`
   attribute and the `migrate` event's `from`/`to` attributes. Slice
   01's three-tier test pins the convention with string-equality
   asserts; Slice 02 reuses the same helper.

**ATAM trade-off points**:

1. Best-effort emission (DISCUSS D5) sacrifices error visibility to
   Cinder for forward compatibility with future non-empty error
   conditions. Same trade-off the Pulse-sink sibling already accepted
   (ADR-0038 trade-offs).

2. Test seam choice (DD3 Option 1) entangles the bridge's tests with
   Cinder's `InMemoryTieringStore` behaviour. Chosen because the
   dual-emission contract (DISCUSS D8) requires it; consistency
   across all four writer test files outweighs the entanglement risk
   (which is the same one ADR-0038 §3 already accepted).

3. Cross-bridge metric-name duplication (DISCUSS D7) sacrifices DRY
   for the rule of three. The three metric-name strings are
   duplicated between `cinder_bridge.rs` and `cinder_otlp_json.rs`;
   the extraction trigger is the third bridge sibling
   (Sluice/Augur/Ray/Strata Pulse-sink or OTLP-JSON-sink).

## Earned Trust (Principle 12) — adapter posture for this writer

The writer's dependencies on the world are:

1. The runtime-supplied `W: Write + Send + Sync` (whose contract is
   `std::io::Write` — well-tested upstream).
2. `SystemTime::now()` (whose nanos-since-epoch value is rendered into
   `timeUnixNano` but is NOT asserted by acceptance tests beyond
   "parses as `u64`"; mirrors the Lumen writer's posture at
   `lumen_to_otlp_json.rs:123-127`).
3. `serde_json::to_string` (whose failure mode for these hand-rolled
   structs is "impossible in practice", because the structs derive
   `Serialize` and contain only strings and integers).
4. `Mutex<W>::lock` (whose failure mode is poisoning under a previous
   panic; handled silently to keep the best-effort contract).

The three Earned-Trust layers (Principle 12c):

1. **Subtype-check layer**: `cargo public-api -p self-observe` (CI
   Gate 2 per ADR-0005) catches public-surface drift at CI time. The
   compile-time `assert_send_sync::<CinderToOtlpJsonWriter<Vec<u8>>>()`
   test in `tests/cinder_to_otlp_json.rs` catches loss of the `Send +
   Sync` trait bound at compile time. The `impl
   cinder::MetricsRecorder for CinderToOtlpJsonWriter<W>` block is
   subtype-checked by the compiler against `cinder::MetricsRecorder`'s
   trait definition.

2. **Behavioural-check layer**: the acceptance-test suite at
   `crates/self-observe/tests/cinder_to_otlp_json.rs` exercises the
   per-event emission contract against a `SharedBuf` byte sink that
   is then parsed and re-asserted as `serde_json::Value`. The Slice
   03 dual-emission test exercises the cross-method contract end-to-
   end (one `evaluate_at` call → N migrate lines + 1 evaluate line in
   the same sink). The "buffer ends with `\n`" assertion (Slice 01)
   defends the NDJSON-validity invariant (DISCUSS D6, OK5).

3. **Structural-check layer**: degenerate for a no-substrate adapter
   (no on-disk source-of-truth schema beyond the public surface; the
   subtype layer covers the surface). This is the same minimum the
   principle permits for a no-substrate adapter (ADR-0001's
   `otlp-conformance-harness`, ADR-0038's `CinderToPulseRecorder`).

**Environments-known-to-lie**:

The writer's only substrate-adjacent dependency is `Mutex<W>::lock`,
whose Rust standard-library implementation is well-tested. The
generic `W` is exercised against an in-memory `Vec<u8>` in tests; the
real `File` (with its `O_APPEND` atomicity guarantees on POSIX) is
the CLI follow-up feature's concern (see DISCUSS D6 paragraph 3 for
the inherited atomicity rationale). The acceptance tests do not need
to exercise `File` lies because:

- The Lumen writer's existing acceptance tests already exercise the
  identical `Mutex<W>` pattern against `SharedBuf` and against the
  CLI follow-up's real `File` (see commits `c6b336c` and `3af7e82`).
  Both passed; the substrate has not lied.
- A future `File`-specific failure (ENOSPC, EAGAIN, partial writes
  smaller than the line) would degrade emission in the same way for
  both writers, and the documented best-effort posture (DISCUSS D5)
  accepts this.

The probe contract for THIS writer is the acceptance-test suite. The
Slice 01 NDJSON-line-termination test (`output_is_ndjson_one_line_per_event_with_trailing_newline`)
is the substrate-lie probe: it asserts that even when the writer is
invoked three times in succession, the byte sequence in the sink is
exactly three lines each terminated by `\n`, with no interleaving,
truncation, or missing terminators. This is the "demonstrate
empirically that it can honor its contract in the real environment
where it will run" requirement of Principle 12.

## External integrations

**None at runtime.** No external network surface, no third-party API,
no webhooks, no OAuth, no subprocess. Dependencies are all
in-workspace path dependencies (`aegis`, `cinder`, `serde`,
`serde_json`). No contract-test recommendation applies for the DEVOPS
handoff.

The downstream OTLP/HTTP collector that Priya's sidecar forwards to
IS an external integration, but it is at the operator's deployment
boundary, not at this library's boundary. Contract testing for the
collector belongs to the operator's deployment topology, not to the
library — and the existing Lumen OTLP-JSON writer has already
validated that the wire shape is collector-acceptable (commit
`c6b336c` shipped that proof).

## Conway's Law check

Single-author file addition built by a single AI agent (the DELIVER
wave's `nw-software-crafter`). The bridge lives inside the
`self-observe` crate, owned by Andrea. File-flat layout is for
*readability and audit*, not for parallel team development.
Satisfied trivially. Same posture as ADR-0038.

## Handoff

See `wave-decisions.md > Handoff` for the artefact list and the next
wave (DISTILL).
