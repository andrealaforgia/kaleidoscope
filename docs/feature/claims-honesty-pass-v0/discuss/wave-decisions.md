# Wave Decisions — claims-honesty-pass-v0 (DISCUSS)

- **Wave**: DISCUSS (nWave)
- **Analyst**: Luna (nw-product-owner)
- **Date**: 2026-06-05
- **Feature type**: Cross-cutting (documentation honesty across many crates,
  with two possible small code touches)
- **Mode**: Autonomous overnight run. All interactive decisions made by Luna
  and recorded here. No questions returned to the operator.

## Origin and verification posture

The per-module four-quadrants assessment named the "stale prose" family the
cheapest fix and, for a project whose entire thesis is **structural honesty
against vendor overstatement**, the one with the sharpest edge: *the code is
honest, the docs are not*. This is backlog item #3 (after
`store-fsync-durability-v0` and `tls-config-reject-v0`, both shipped).

The defect surface is documentation that claims a capability the code does not
have. The assessment is a guide; **the code is truth**. Luna re-grounded every
named overstatement against the live code on 2026-06-05 by direct archaeology.
Several claims had already shifted (the per-crate `lib.rs` doc comments for
loom, spark, strata, cinder were already corrected by prior waves); the
overstatements have **migrated to the README "Components at a glance" table**
and to a handful of stale-over-green test/Cargo headers. The verified inventory
is in the table below.

## Verified-against-code overstatement inventory

| # | Claim locus | The false claim | Code truth (verified) | Verdict |
|---|-------------|-----------------|-----------------------|---------|
| 1 | `README.md:185` Components table | **Loom**: "Dashboards-as-code, alert-rules-as-code" | `loom/src/lib.rs:17-38` already says "change-control surface", "reads `.toml`", "No I/O beyond filesystem reads". Cargo.toml already says "Git-backed change-control surface for operator-authored TOML catalogues; v1 extends to … Prism dashboards". Loom handles ZERO dashboards today. | OVERSTATED (README row only) |
| 2 | `README.md:171` Components table | **Spark**: "Auto-instrumentation SDKs" | `spark/src/lib.rs:1-17` already says "thin wrapper around the upstream opentelemetry … crates". Spark does ZERO auto-instrumentation; it is a manual-init OTel wrapper. `docs/feature/spark/wave-decisions.md:177,191` already defers auto-instrumentation to v0.2/v1. | OVERSTATED (README row only) |
| 3 | `README.md:179,213` Components table + cost table | **Strata**: "Continuous profiling" / "Continuous profiling as a top-tier add-on … Strata is included." | `strata/src/lib.rs:17-46` says "first-party profile storage engine … Library only at v0. No daemon, no network." It is a passive profile sink with no scheduler/scraper. | OVERSTATED (README only) |
| 4 | `README.md:180` Components table | **Cinder**: "cold-tier coordinator (S3/OpenDAL/Iceberg)" (the row reads "Tier-metadata governor / cold-tier coordinator") | `cinder/src/lib.rs:17-48` says "stores **tier metadata**, not payloads … In-memory only at v0". v1 is file-backed local metadata; the S3 + OpenDAL + Iceberg adapter is v2 (Cargo.toml line 7). No object-storage code exists. | OVERSTATED (README only; roadmap C.6 is correctly future-tense) |
| 5 | `otlp-conformance-harness/src/lib.rs:1-7` + `README.md:3-4` + `Cargo.toml:11` | "validates byte sequences against the OpenTelemetry OTLP **wire specification**" | `validate.rs` + `decode.rs`: validation is structural/decode-level only — non-empty (`validate.rs:19`), first wire tag references resource field #1 (`decode.rs:126-135`), decodes as the asserted prost type (`decode.rs:117-120`), plus a signal-mismatch fallback. NO semantic checks (no trace_id/span_id length, no timestamp, no attribute, no semantic-convention validation). | OVERSTATED (depth of validation) |
| 6 | `otlp-conformance-harness/README.md:8-16` | "Status: … Implementation is intentionally absent at this point — every `validate_*` function returns `unimplemented!()`." | `lib.rs:17-22` says "implemented and green"; `validate.rs`/`decode.rs` are fully live code. | STALE-OVER-GREEN (status block) |
| 7 | `otlp-conformance-harness/src/framing.rs:16-18` + `lib.rs` | `Framing::GrpcProtobuf` is accepted but never acted on (the gRPC length prefix is "the caller's responsibility"; framing is only echoed into violations) | Confirmed: `validate.rs`/`decode.rs` never branch on `framing`; it is passed through into `OtlpViolation` only. The enum doc admits it; `lib.rs`/`README` do not flag it. | DOCUMENT-vs-IMPLEMENT (DESIGN flag #2) |
| 8 | `codex/Cargo.toml:17-24` + 5 test headers + `tests/common/mod.rs:14-16` | "DISTILL-state stub … Every acceptance test under `tests/` panics with `unimplemented!()`" / "Tests panic on `unimplemented!()` until DELIVER lands …" | `codex/src/lib.rs:43-48` says "Fully implemented and green"; `slice_04` test asserts `result.is_err()` against the live `validate` Err path. All five slices real and green. | STALE-OVER-GREEN (Cargo.toml + headers) |
| 9 | `query-http-common/src/lib.rs:30-42` (module doc) | "DISTILL scaffold — DELIVER fills the bodies … All free functions are `unimplemented!("__SCAFFOLD__ query-http-common-v0 RED")` at DISTILL close." | The bodies are FULLY IMPLEMENTED live code (`parse_time_range:178-188`, `resolve_tenant_or_refuse:239-251`, `error_response:268-274`, `init_tracing:317-345`). Each fn's own doc already says "DELIVER state: implemented." Only the module-level summary is stale. | STALE-OVER-GREEN (module doc) |
| 10 | `trace-query-api/src/lib.rs:207-209,228-232` | "Scaffold for DISTILL Mandate 7 RED-not-BROKEN: the handler is `unimplemented!` … DELIVER implements the body" | `handle_traces_by_id:233-292` and `parse_trace_id:304-320` are fully live, real implementations. | STALE-OVER-GREEN (doc comments) |
| 11 | `query-api/src/lib.rs:136-146` + `README.md:104-108` framing | `step` is accepted (deserialised) then silently ignored; raw native-timestamp points returned, not a Prometheus stepped grid. The endpoint is branded "Prometheus-compatible `/api/v1/query_range`". | `lib.rs:138-146` is ALREADY honest in the field doc ("`step` is accepted and ignored at v0 (DD5: raw points, no re-stepping)"). The residual overstatement is the README's "Prometheus-compatible" branding implying stepped-grid semantics, which the verifier's black-box (two step values → identical output) will expose. | DOCUMENT-vs-IMPLEMENT (DESIGN flag #1) |

### Items from the brief that turned out ALREADY-CORRECT (do NOT re-correct)

- **README "durable / survives restart"** — RESOLVED by `store-fsync-durability-v0`.
  All seven stores now fsync + atomic-snapshot; the README `Status` section
  (lines 89-95) already describes this truthfully ("a firing alert survives a
  restart instead of re-paging", the seven `FileBacked*` adapters named). No
  hedge or mis-description remains. **No correction.**
- **loom `lib.rs` / `Cargo.toml`** — already honest (TOML, no CUE, no dashboards).
  Only the README row is stale.
- **spark `lib.rs` / `Cargo.toml`** — already honest (manual-init OTel wrapper).
  Only the README row is stale.
- **strata / cinder `lib.rs` / `Cargo.toml`** — already honest (passive sink /
  tier-metadata, in-memory-at-v0). Only the README rows are stale.
- **query-api `step` field doc** — already honest in-code; the residual is the
  README "Prometheus-compatible" framing only.
- **Genuinely in-flight `__SCAFFOLD__` / RED markers** — the `v1_slice_0{3,4}_crash_durability`
  tests (lumen, ray, strata, cinder, sluice, beacon, pulse), the
  `log-query-api` pagination/body-regex/body-contains/severity scaffolds, the
  aperture `slice_09_tls_config_reject` RED markers, and the gateway/log-query
  tracing-subscriber scaffolds are **legitimately RED or `#[ignore]`d in-flight
  work**, NOT stale-over-green. They describe a true current state. **Do NOT
  touch them.** The honesty pass corrects only markers that lie about GREEN code
  (items 6, 8, 9, 10).

## The job (JTBD)

> "When I read what Kaleidoscope claims — in a README, a crate doc, a codename,
> a Cargo.toml description — the claim matches what the code actually does, or
> is clearly marked as future/roadmap."

This is the project thesis applied to the project's own prose. Each correction
moves an overstated claim to the truth or to explicit future tense. The
corrections change **documentation and codenames/descriptions, not behaviour**,
except for the two flagged document-vs-implement items (query-api `step` and the
harness framing/semantic-validation), where DESIGN decides document-vs-implement
per item.

## Decisions (autonomous, per the overnight brief)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Feature type | Cross-cutting (doc honesty) | Touches many crates' prose; two possible small code touches. |
| Walking skeleton | **No** | Brownfield docs; there is no end-to-end flow to thread. Slices are independent. |
| UX research | Lightweight | The "user" is a reader/evaluator/operator reading the claims. One persona (Devin, below). |
| JTBD | The honesty job above | Recorded as the single job all stories trace to (N:1). |
| DIVERGE | Absent (confirmed) | No `docs/feature/claims-honesty-pass-v0/diverge/`. Job grounded directly in the four-quadrants assessment + the structural-honesty thesis + verified code, mirroring the sibling `store-fsync-durability-v0` posture. Recorded as a risk below. |
| Slicing | Elephant carpaccio, grouped by claim cluster, cheapest/sharpest first | Each slice leaves the docs strictly more honest and is independently shippable. The codenames (README table) and the stale-over-green markers are the cheapest. |
| Pure-prose vs code-touch | 7 pure-prose slices + 2 flagged document-vs-implement | The document-vs-implement decision (query-api `step`, harness framing/semantic-validation) is DEFERRED to DESIGN; DISCUSS only flags and frames them. |
| Mutation testing | Pure-prose slices have little/nothing to mutate | Per CLAUDE.md, mutation is per-feature on modified files. The doc-only slices carry no production-code change; the two code-touch slices (if DESIGN chooses "implement") carry the mutation obligation. Recorded as a guardrail. |
| Acceptance shape | Guard-style: false string ABSENT + corrected string PRESENT; for code-touch items, a test asserts the real behaviour the doc now describes | A prose-honesty correction is not conventionally acceptance-testable; the testable form is a doc-lint / grep guard plus (for code touches) a behaviour test. Recorded for DISTILL. |
| README scope | The README "Components at a glance" table is the single locus of items 1-4 | The per-crate docs are already honest; the roadmap is already future-tense. Correct the README rows in place. |

## The two flagged document-vs-implement decisions (for DESIGN)

These two items carry a real DESIGN choice; DISCUSS does NOT pre-decide them.

### DESIGN flag #1 — query-api `step` (item 11)

The `/api/v1/query_range` endpoint accepts `step`, deserialises it, then ignores
it (`query-api/src/lib.rs:143-146`), returning raw native-timestamp points
rather than Prometheus' stepped grid. The in-code field doc is already honest;
the README brands the endpoint "Prometheus-compatible", which implies stepped-grid
semantics. The verifier is independently building a black-box (two `step` values
→ identical output) that proves the overstatement.

- **Option A (document, lighter)** — Drop/qualify the "Prometheus-compatible"
  stepped-grid implication; state plainly that `step` is accepted-and-ignored at
  v0 and raw points are returned. AC: the black-box test asserts identical output
  for two `step` values AND the doc now states `step` is not honoured. The
  honesty fix makes the claim match reality.
- **Option B (implement, heavier)** — Implement the Prometheus stepped grid so
  `step` is honoured. AC: two `step` values produce DIFFERENT, correctly-stepped
  output. Larger, carries the mutation obligation, arguably out of a
  *honesty-pass* feature's scope.

DISCUSS recommendation (non-binding): **Option A** keeps the feature a true
honesty pass; Option B is a real feature that deserves its own slice/feature. The
black-box the verifier is building is satisfied by either — but only A is
proportionate to "make the prose honest."

### DESIGN flag #2 — harness `Framing::GrpcProtobuf` (item 7)

`Framing::GrpcProtobuf` is accepted by every `validate_*` entry point but never
acted on — the gRPC length prefix is the caller's responsibility, and `framing`
is only echoed into `OtlpViolation`. The enum doc (`framing.rs:16-18`) admits
this; `lib.rs` and the README do not.

- **Option A (document, lighter)** — Document at the `lib.rs`/README level that
  `GrpcProtobuf` framing is a label echoed into violations, NOT a behavioural
  branch; the caller strips the length prefix. AC: the doc names the limitation;
  a test asserts identical decode behaviour for both framings on prefix-stripped
  bytes.
- **Option B (honour, heavier)** — Make the harness strip the gRPC length prefix
  when `GrpcProtobuf` is asserted. AC: a length-prefixed body validates under
  `GrpcProtobuf` and the same body without the prefix validates under
  `HttpProtobuf`. Larger; carries the mutation obligation.

Also bundled with item 5 (the harness "wire specification" overclaim): correct
the harness `lib.rs`/`README`/`Cargo.toml` prose to "structural decode-level
validation" regardless of which framing option DESIGN picks. The validation-depth
correction (item 5) is pure prose; the framing decision (item 7) is the
document-vs-implement choice.

DISCUSS recommendation (non-binding): **Option A** for both 5 and 7 — this is a
prose-honesty feature; "honour the framing" is a real capability that belongs in
its own feature, not smuggled into a documentation sweep.

## Risk register

| Risk | Prob | Impact | Mitigation |
|------|------|--------|------------|
| No DIVERGE artifacts present (`docs/feature/claims-honesty-pass-v0/diverge/` absent) | High (confirmed absent) | Low | Job grounded directly in the four-quadrants assessment #3 ranking + the structural-honesty thesis + verified code residue. JTBD recorded; no ODI re-run for a prose-correctness feature. Mirrors the sibling `store-fsync-durability-v0` posture. |
| Touching a GENUINELY in-flight `__SCAFFOLD__`/RED marker by mistake (would make an honest in-flight marker lie) | Medium | High | The inventory explicitly separates stale-over-green (items 6,8,9,10) from genuinely-RED in-flight markers (the crash-durability / pagination / body-regex / tls-config / tracing-subscriber scaffolds). Each correction slice names the EXACT file+line and asserts the code it covers is GREEN before the marker is touched. Pinned as a guardrail. |
| A "corrected" claim is itself subtly wrong (e.g. over-correcting Strata to "no profiling at all" when it IS a profile sink) | Medium | Medium | Every correction uses the per-crate `lib.rs` already-honest wording as the canonical truth (loom→"change-control surface / TOML", spark→"manual-init OTel SDK wrapper", strata→"passive profile sink", cinder→"local tier-metadata coordinator", harness→"structural decode-level validation"). The README is aligned TO the lib.rs, not invented fresh. |
| query-api / harness document-vs-implement decision pre-empted in DISCUSS | Low | Medium | DISCUSS only flags and frames; the decision is DESIGN's. Recommendations are explicitly non-binding. |
| README "durable / survives restart" re-corrected when it is now TRUE | Low (flagged in brief) | Low | Explicitly verified as RESOLVED; recorded in the already-correct list. No story touches it. |
| A pure-prose slice carries no mutation target and a naive gate flags it | Low | Low | Per CLAUDE.md mutation is per-feature on modified files; doc-only changes have nothing to mutate. Only the two code-touch items (if DESIGN picks "implement") carry the obligation. Recorded as a guardrail; trunk-based, CI-is-feedback (project memory). |

## What this feature does NOT do

- Does not change any store/handler/validator BEHAVIOUR, except possibly
  query-api `step` and the harness framing — and only if DESIGN explicitly picks
  the "implement" option for those two flagged items.
- Does not re-correct the README durability claim (now true after
  `store-fsync-durability-v0`).
- Does not touch genuinely-RED / `#[ignore]`d in-flight scaffolds.
- Does not re-run ODI / opportunity scoring (prose-correctness feature with a
  job pre-validated by the four-quadrants assessment).
- Does not make the document-vs-implement decision for the two flagged items
  (that is DESIGN's call).

## Peer review

Peer review (nw-product-owner-reviewer) run at end of DISCUSS; result recorded in
`dor-validation.md`. Handoff to DESIGN is NOT performed by this wave (brief: "Do
NOT proceed into DESIGN").
