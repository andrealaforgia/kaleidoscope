# Wave Decisions — claims-honesty-pass-v0 (DESIGN)

- **Wave**: DESIGN (nWave, LIGHT)
- **Architect**: Morgan (nw-solution-architect)
- **Date**: 2026-06-05
- **Mode**: PROPOSE, autonomous overnight. No questions returned to the operator.
- **Scope**: This is a prose-honesty feature. DESIGN owns ONLY the two flagged
  document-vs-implement decisions (US-05, US-06). The seven pure-prose slices
  (US-01..US-04) need no architecture — they are confirmed below as doc/string
  changes grounded in already-honest code. **Deliberately not over-built.**

## Verification posture

"The code is truth." Before resolving the flags, the two code loci were read
directly (not trusted from the inventory):

- `crates/query-api/src/lib.rs:136-146` — `step` is `#[serde(default)]` +
  `#[allow(dead_code)]`; the field doc says "`step` is accepted and ignored at
  v0 (DD5: raw points, no re-stepping)". **Already honest in-code.** The
  handler (`handle_query_range`) never reads `step`. Confirmed.
- `crates/otlp-conformance-harness/src/framing.rs:14-18` — the `GrpcProtobuf`
  variant doc says "the gRPC length prefix is the caller's responsibility to
  strip before invoking the harness". **Already honest in the enum doc.**
  Confirmed `validate.rs`/`decode.rs` never branch on `framing`.
- `README.md:106` — "a Prometheus-compatible `/api/v1/query_range` HTTP
  endpoint". Confirmed: this is the residual flag-#1 overstatement.

Luna's DISCUSS inventory holds exactly against the live code.

## The two flag resolutions

### FLAG #1 — query-api `step` (US-05) → **DOCUMENT**. ADR-0062 authored.

| | |
|---|---|
| **Decision** | DOCUMENT. Qualify the README so `query_range` is no longer branded as a full Prometheus stepped-grid contract; state `step` is accepted-but-not-honoured at v0, raw in-window points returned. No behaviour change. |
| **One-line rationale** | Implementing the stepped grid is a genuine, not-low-risk feature (re-sampling + staleness + grid alignment) with its own mutation obligation — disproportionate to a prose-honesty pass; the in-code field doc is already honest, only the README overstates. |
| **ADR** | **ADR-0062** — `query_range at v0 returns raw in-window points; step is reserved`. Authored because this is an architectural SCOPE statement a future implementer + Prism will rely on. |
| **Verifier contract** | The verifier's black-box (two `step` values → compare output) asserts **INVARIANCE**: fixed `query`/`start`/`end`, `step=15s` vs `step=60s` vs omitted-`step` → byte-identical output. Under DOCUMENT this passes and pins the documented boundary. (ADR-0062 flags that a future stepped-grid feature will intentionally retire this assertion — a planned break, not a regression.) |
| **README correction** | `README.md:106` must stop implying a Prometheus stepped grid for `query_range` and state plainly what it returns (raw stored points over the window; `step` accepted for request-shape compatibility, not yet honoured at v0 — no grid re-sampling). Aligned TO `lib.rs:136-137`, not invented fresh. |
| **Code touch** | None (DOCUMENT). No mutation target. |

### FLAG #2 — harness `Framing::GrpcProtobuf` (US-06) → **DOCUMENT**. No ADR.

| | |
|---|---|
| **Decision** | DOCUMENT. State at `lib.rs`/README level that `GrpcProtobuf` is an inert label echoed into violations, NOT a behavioural branch; the caller is responsible for stripping the gRPC length prefix before calling the harness. No behaviour change. |
| **One-line rationale** | Honouring means stripping + validating the 5-byte gRPC length prefix (compression flag + big-endian length) with new error semantics — new contract behaviour and a mutation obligation; the enum doc is already honest, so DOCUMENT just propagates that note up to the louder surfaces. |
| **ADR** | **None.** Local doc-honesty propagation, not a cross-cutting scope decision; captured fully here. |
| **Verifier contract** | Prefix-stripped bytes validate **identically** under `HttpProtobuf` and `GrpcProtobuf` (framing inert); a still-length-prefixed body under `GrpcProtobuf` **fails to decode** (matching the "strip first" doc). |
| **Doc correction** | Harness `lib.rs` + README must state `GrpcProtobuf` does not change validation and the caller strips the prefix. Aligned TO `framing.rs:14-18`. |
| **Code touch** | None (DOCUMENT). No mutation target. |

Both follow DISCUSS's non-binding recommendation and the feature thesis: the
heavier "implement/honour" options are real capabilities each deserving their
own feature, not smuggled into a documentation sweep.

## Pure-prose slices — per-slice confirmation (no ADR, no architecture)

Each is confirmed as a doc/string change whose corrected wording is grounded in
the crate's already-honest `lib.rs` (the canonical truth), per the DISCUSS
inventory. The acceptance shape is the grep/doc-lint guard: false string ABSENT
+ corrected string PRESENT.

| Slice | Loci (doc/string only) | Grounded in (already-honest code) |
|---|---|---|
| **US-01** README codenames | `README.md` Spark/Strata/Cinder/Loom rows (171, 179, 180, 185) + cost line (213) | `spark/src/lib.rs:1-17`, `strata/src/lib.rs:17-46`, `cinder/src/lib.rs:17-48`, `loom/src/lib.rs:17-38` (+ their `Cargo.toml`s, already honest) |
| **US-02** codex stub headers | `codex/Cargo.toml:17-24`, 5 `tests/slice_0*.rs` headers, `tests/common/mod.rs:14-16` | `codex/src/lib.rs:43-48` ("Fully implemented and green"); `slice_04` asserts the live `Err` path |
| **US-03** stale `__SCAFFOLD__`-over-green | `query-http-common/src/lib.rs:30-42` (module doc), `trace-query-api/src/lib.rs:207-209,228-232` (handler doc) | each fn's own "DELIVER state: implemented" note + the live bodies (`parse_time_range`, `resolve_tenant_or_refuse`, `error_response`, `init_tracing`; `handle_traces_by_id:233-292`, `parse_trace_id:304-320`) |
| **US-04** harness depth + status | harness `lib.rs:1-7`, `README.md:3-4` + `:8-16`, `Cargo.toml:11` | `validate.rs`/`decode.rs` (structural-only: non-empty, resource-field-first, prost-decodable, signal-mismatch fallback); `lib.rs:17-22` ("implemented and green") |

**Guardrail (US-03, hard).** Touch ONLY markers proven to sit over GREEN code.
The genuinely-RED / `#[ignore]`d in-flight scaffolds — `*_crash_durability`
(lumen, ray, strata, cinder, sluice, beacon, pulse), `log-query-api`
pagination/body-regex scaffolds, aperture `slice_09_tls_config_reject`, the
gateway/log-query tracing-subscriber scaffolds — describe a TRUE current state
and MUST NOT be touched. The US-03 guard asserts BOTH directions: stale phrasing
absent in the two corrected loci AND `__SCAFFOLD__` still present in the named
in-flight loci.

## Constraints (carried from DISCUSS, reaffirmed)

- The per-crate `lib.rs` already-honest wording is the canonical truth; the
  README/headers are aligned TO it, never invented fresh.
- No store/handler/validator behaviour changes. Both flags resolved DOCUMENT, so
  this holds with zero exceptions — the feature changes documentation only.
- Trunk-based, CI-is-feedback; no CI gate blocks a doc-only change. The guard
  tests are the regression net.
- Mutation: nothing to mutate (no production-code change). Recorded as a
  guardrail, not a gap (CLAUDE.md per-feature mutation is on modified production
  files).
- No external integration; no contract-test recommendation.
- No C4 update — topology is identical before and after.

## ADR summary

- **ADR-0062** authored (FLAG #1 scope statement).
  `docs/product/architecture/adr-0062-query-range-v0-raw-points-step-reserved.md`.
- FLAG #2 and the four pure-prose slices: no ADR (captured here).

## Peer review

Self-review against the SA critique dimensions recorded below (sub-agent context;
`nw-solution-architect-reviewer` not separately invocable from here — flagged for
a top-level reviewer run if desired). No DEVOPS/DISTILL hand-off performed by this
wave, per the LIGHT brief.

### Self-review (nw-sa-critique-dimensions)

| Dimension | Assessment |
|---|---|
| Architectural bias (resume-driven / latest-tech) | None. Both flags resolved to the LIGHTER option (DOCUMENT); the feature explicitly refuses to inflate a doc sweep into a behavioural feature. Anti-bias by construction. |
| ADR quality (ADR-0062) | Context (the gap + already-honest field doc + verifier), Decision, 2 alternatives with rejection rationale (implement-the-grid; leave-README-as-is), Consequences incl. the flagged future-break of the invariance assertion. Complete. |
| Completeness (quality attributes) | No quality-attribute strategy needed — zero behaviour change, zero new component. The relevant attribute is **maintainability/honesty of documentation**, addressed directly. Confirmed not a gap. |
| Implementation feasibility / testability | Every slice has a concrete observable (grep guard ± one black-box assertion). Both flag slices are testable as INVARIANCE. Feasible; no team-capability or budget concern (doc edits). |
| Priority validation | The verified inventory IS the data: 11 overstatements, each cited to file+line and to the honest code it contradicts. The two flags address the only loci with a real behaviour question; both resolved proportionately (Q3 not inverted — DOCUMENT is the <effort solution to the honesty problem, not a >50% solution to a <30% problem). |

**Self-review verdict**: no critical/high issues. The single watch-item is the
US-03 over-reach guardrail (touching an in-flight marker) — already pinned as a
both-directions guard and carried as a HIGH-impact risk from DISCUSS. Approved to
hand to DISTILL (by a separate wave; not performed here).
