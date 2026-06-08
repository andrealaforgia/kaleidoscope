# DoR Validation: aperture-body-size-cap-v0

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-07.
British English. No em dashes in body.

Validates the 9-item Definition of Ready for US-01, US-02, US-03 against
`user-stories.md`, `story-map.md`, `outcome-kpis.md`, and
`wave-decisions.md`, citing the code loci verified on this branch. The DoR
is a hard gate: every item must PASS with evidence before handoff to
DESIGN. Verdict at the end.

## Verified code loci (re-confirmed on this branch, feeding every story)

| Locus | Confirmed | Used by |
|---|---|---|
| `config/mod.rs:474-485` `max_recv_msg_size: Option<u32>` parsed, `#[allow(dead_code)]`, "unused at v0" | yes | US-01, US-03, D1, KPI-1 |
| `config/mod.rs:46-58` `Config` struct -- no `max_recv_msg_size` field | yes | D1, C8 |
| `config/mod.rs:193-194` `max_concurrent_requests()` accessor (the template for the new one) | yes | D1, C8 |
| `config/mod.rs:315-317` `max_concurrent_requests` builder setter (template for the test seam) | yes | C8, US-01 test seam |
| `config/mod.rs:608-617` `into_config` reads concurrency from gRPC only, ignores HTTP at v0 | yes | D1 |
| `observability.rs:46` `BODY_TOO_LARGE` constant, no emitter | yes | US-01, C5, KPI-2 |
| `app.rs:65-82` `ingest_logs(body: &[u8], ...)` calls `validate_logs` immediately | yes | US-01, US-02, D2, C3 |
| `app.rs:94-111` `ingest_traces(body: &[u8], ...)` calls `validate_traces` immediately | yes | US-01, US-03, D2 |
| `app.rs:62-64` ingest fns "do not emit events" | yes | D2 |
| `app.rs:13-19` single-validator-per-signal CI invariant | yes | C12, D2 |
| `transport.rs:473-477,579-583` HTTP `body: Bytes` buffered before the handler | yes | D2, D3 |
| `transport.rs:865-869,949-953` gRPC decode + `req.encode_to_vec()` re-encode | yes | D2, D3 |
| `transport.rs:524-529,871-876` `request_received` carries `body.len()`/`bytes.len()` | yes | D3, KPI-2 |
| `backpressure.rs:89-140` concurrency-cap event + refusal shape (the precedent) | yes | C6, KPI-2 |
| `transport.rs:485-490,791-801` `refuse_http` 503 + `Retry-After`; `:842-850` gRPC `RESOURCE_EXHAUSTED` | yes | C6, D5 |
| `compose.rs:183-186` limiter wired into handler state (the cap-wiring template) | yes | D2 |
| `testing.rs` `stderr_capture` seam | yes | C8, KPI-2 |

### Config surface confirmation (D1)

The raw schema parses `max_recv_msg_size` per arm
(`[aperture.transport.grpc]` and `[aperture.transport.http]`,
`config/mod.rs:481-485`) but `Config` has no field and `into_config` never
reads it. The parallel concurrency knob took a single value from the gRPC
arm and ignored HTTP at v0 (`config/mod.rs:608-617`), which is the
precedent DESIGN reconciles against in D1. Conclusion: a new `Config`
field + accessor + builder setter is required (DELIVER); the single-vs-
per-arm shape is DESIGN's D1 call. Likely INTERNAL only (the accessor is
`pub(crate)` like `max_concurrent_requests`); any public leak is
semver-MINOR, pre-1.0, NEVER 1.0.0.

### Enforcement-site confirmation (D2, the load-bearing question)

The simplest seam (an app.rs early return at `ingest_logs`/`ingest_traces`)
runs AFTER axum has buffered the HTTP body (`transport.rs:473-477`) and
AFTER tonic has decoded the gRPC frame (`transport.rs:865-869`), so it
guards the harness decode/validate but NOT the allocation -- the weaker
protection. The stronger guard (axum `DefaultBodyLimit` / tonic
`max_decoding_message_size`) rejects before the buffer/decode. This is D2,
flagged for DESIGN; DISCUSS requires the reject "before validate/decode"
and "fail-closed and visible" and NOTES that an after-buffer check is
weaker. DESIGN states the strength achieved and words the AC honestly.

---

## US-01: An oversized OTLP body is rejected before decode and named in a structured event

| # | DoR item | Verdict | Evidence |
|---|---|---|---|
| 1 | Problem statement clear, domain language | PASS | US-01 Problem: Priya sets the cap expecting protection and gets none; a 200 MB body is decoded into memory and OOM-kills the gateway; nothing names it. Domain language (operator, OTLP, OOM, multi-tenant); no solution prescription (the enforcement site is left to DESIGN). |
| 2 | User/persona with specific characteristics | PASS | Priya the platform operator; runs a live multi-tenant aperture ingest fleet exposed to partly-untrusted OTLP clients; scrapes structured stderr; wants machine-parseable notice of a too-large rejection with the limit and the size (US-01 Who). |
| 3 | 3+ domain examples with real data | PASS | Three examples: under-limit 12 KB `payments-api` log accepted under a 4 MiB cap (negative control); 200 MB `bulk-importer` body rejected on HTTP `/v1/logs` with `body_too_large transport=http_protobuf signal=logs limit=4194304 size=209715200`; 180 MB `checkout-api` traces rejected on gRPC. Real service names, real byte counts, real event shape. |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | 3 Gherkin scenarios (under-limit accepted, oversized logs rejected + named, oversized traces rejected + named). Within 3-7. |
| 5 | AC derived from UAT | PASS | 4 AC each trace to a scenario (accept-under-limit-no-event, reject-logs+event, reject-traces+event, use-the-existing-constant). |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios; one config accessor + one enforcement site + one `body_too_large` emit, applied to logs + traces; bounded loci. 1-2 days. Carpaccio split (logs then traces) available (story-map). |
| 7 | Technical notes: constraints/dependencies | PASS | US-01 Technical Notes: depends on D1/D2/D3/D5; reuse the constant (C5); match the concurrency-cap shape (C6); early return before `validate_*` (C12); test seam = new builder setter + `stderr_capture` (C8). |
| 8 | Dependencies resolved or tracked | PASS | D1/D2/D3/D5 flagged for DESIGN (`wave-decisions.md`); the enforcement-site loci enumerated and bounded; no unresolved blocker. |
| 9 | Outcome KPIs defined with measurable targets | PASS | US-01 KPI block + `outcome-kpis.md` KPI-1 (enforced sites 0 -> configured) and KPI-2 (one event per rejection, falsifiable against today's accept-and-ignore). |

US-01 DoR: **9/9 PASS.**

## US-02: The cap is exact at the boundary -- at-limit accepted, one byte over rejected

| # | DoR item | Verdict | Evidence |
|---|---|---|---|
| 1 | Problem statement clear, domain language | PASS | US-02 Problem: a fuzzy boundary loses legitimate telemetry (rejects at/under limit) or leaks the guard (accepts over limit); "maximum 4 MiB" must not secretly mean 4 MiB minus one or plus a bit. Domain language, two-sided framing; no solution prescribed (the comparison location is DESIGN's). |
| 2 | User/persona with specific characteristics | PASS | Priya tunes `max_recv_msg_size` against her observed payload-size distribution and needs the boundary exact (US-02 Who). |
| 3 | 3+ domain examples with real data | PASS | Body of exactly 1048576 bytes accepted under a 1 MiB cap; body of 1048577 bytes (one over) rejected with `limit=1048576 size=1048577`; tiny 16-byte cap rejects an ordinary 12 KB body with `limit=16 size=12288`, proving config-drivenness. Real byte counts. |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | 3 Gherkin scenarios (at-limit accepted, at-limit-plus-one rejected, reject-driven-by-configured-limit). |
| 5 | AC derived from UAT | PASS | 4 AC trace to scenarios + the boundary-mutation-survives-Gate-5 guard; the inclusive-limit semantics is stated as observable behaviour, not implementation. |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios; pins the comparison US-01 introduces; no new mechanism. <1 day on top of US-01. |
| 7 | Technical notes: constraints/dependencies | PASS | US-02 Technical Notes: depends on US-01; inclusive-limit (`size <= limit`) is the requirement, the comparison location is D2; C9/Gate 5 boundary kill; the `u32`/`u64` vs `usize` `body.len()` reconciliation is flagged for DESIGN. |
| 8 | Dependencies resolved or tracked | PASS | Depends on US-01 (tracked, sequenced first in Priority Rationale). The comparison-location dependency is D2, flagged for DESIGN. |
| 9 | Outcome KPIs defined with measurable targets | PASS | US-02 KPI block + `outcome-kpis.md` KPI-3 (boundary ambiguity undefined -> exactly 1 byte) and KPI-6 (0 survivors on the comparison). |

US-02 DoR: **9/9 PASS.**

## US-03: When no cap is set, behaviour is unchanged; the guard covers both logs and traces

| # | DoR item | Verdict | Evidence |
|---|---|---|---|
| 1 | Problem statement clear, domain language | PASS | US-03 Problem: the feature touches every instance incl. the many that do not set the cap; a default-cap or one-signal-only implementation would break unset deployments or leave half the OOM exposure open. Domain language, two-sided (unset safety + full coverage); no solution prescribed (metrics coverage left to D4). |
| 2 | User/persona with specific characteristics | PASS | Priya runs a mixed fleet (some cap, most unset) and ingests both logs and traces on every instance; wants safe upgrade + full-signal coverage (US-03 Who). |
| 3 | 3+ domain examples with real data | PASS | Unset gateway accepts a 200 MB log body byte-for-byte as before (negative control); one 4 MiB cap rejects oversized logs (`signal=logs`) AND oversized traces (`signal=traces`); an absent cap is treated as no-cap, never a zero-byte reject-everything limit. Real byte counts, real signal fields. |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | 3 Gherkin scenarios (unset = unchanged, single cap guards both signals, absent cap is not zero-byte). |
| 5 | AC derived from UAT | PASS | 4 AC trace to scenarios + the existing-suites-stay-green regression guard; written as observable behaviour ("no size check, no event, handled as before"). |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios; the unset branch is a `None` early no-op; the second signal is a thin follow-on over US-01's mechanism. <1 day on top of US-01. |
| 7 | Technical notes: constraints/dependencies | PASS | US-03 Technical Notes: depends on US-01; unset = `None` (`config/mod.rs:485`) = today (C2); D4 metrics coverage flagged for DESIGN with the disclose-if-deferred requirement; carpaccio cut-line (logs then traces) is the ripple fallback. |
| 8 | Dependencies resolved or tracked | PASS | Depends on US-01 (tracked). D4 (metrics) flagged for DESIGN. The `None` early-return locus is verified (`config/mod.rs:485`). |
| 9 | Outcome KPIs defined with measurable targets | PASS | US-03 KPI block + `outcome-kpis.md` KPI-4 (zero unset behaviour change) and KPI-5 (coverage 0 -> 2 signals, metrics named). |

US-03 DoR: **9/9 PASS.**

## Feature-level checks

- **Mutation guardrail (C9 / Gate 5)**: KPI-6 targets 100% kill on the
  modified files (`config/mod.rs`, `app.rs` and/or `transport.rs`,
  `observability.rs` if the emit lands there). The boundary comparison
  (`>`/`>=`), the unset-no-cap `None` branch, and the `body_too_large`
  emit must each be pinned. Tracked as a DELIVER closing check.
- **Solution-neutrality**: D1-D5 are stated as requirements ("reject
  before decode", "exact inclusive boundary", "unset = unchanged", "cover
  both signals", "fail-closed and visible"), not mechanisms. DESIGN owns
  the config surface, the enforcement site (and its honest protection
  strength), the reported-size shape, metrics coverage, and the reject
  codes. PASS.
- **Real data**: every example uses Priya, real service names
  (`payments-api`, `bulk-importer`, `checkout-api`), real byte counts
  (`4194304`, `209715200`, `1048576`/`1048577`, `16`/`12288`), and real
  event fields (`body_too_large transport=.. signal=.. limit=.. size=..`).
  No generic `user123`/`test@test.com`. PASS.
- **No technical-AC anti-pattern**: AC are observable operator outcomes
  ("the body is rejected before the sink is touched", "one `body_too_large`
  event names the limit and the size", "an unset gateway behaves exactly as
  before"), not implementation ("use a tonic builder call"). The
  implementation choices are deferred to DESIGN as D1-D5. PASS.
- **Scenario titles are business outcomes**: each scenario names what the
  operator/system achieves ("An oversized logs body is rejected before
  decode and named on stderr", "A body exactly at the limit is accepted"),
  not a class/method. PASS.
- **Error/edge ratio**: of the 9 scenarios, 6 are error/edge/boundary
  (oversized logs, oversized traces, at-limit, at-limit-plus-one, tiny-cap,
  absent-cap-not-zero-byte) and 3 are happy/negative-control (under-limit
  accept, unset unchanged, single-cap-both-signals). 6/9 = 67%, well above
  the >=40% target. PASS.
- **Right-sizing (feature)**: 3 stories, 9 scenarios, 1 crate, 1 persona,
  no UI; bounded loci. PASS (story-map Scope Assessment).
- **No DIVERGE artifacts**: recorded as a Low/Medium risk in
  `wave-decisions.md`; the job is grounded in the four-quadrants Q3
  DISCLOSED-omission finding + the Earned-Trust posture and verified in
  code. Does not block.
- **Live-gateway regression guard (C1)**: US-01 scenario 1 (under-limit
  accept), US-02 (at-limit accept), and US-03 (unset unchanged + existing
  suites green) together guard the live ingest path against false rejects
  and behaviour change. PASS.

## Verdict

**DoR PASS for US-01, US-02, US-03 (9/9 each).** All three stories are
ready for the DESIGN wave, with D1-D5 correctly flagged as DESIGN-owned
mechanism decisions and every KPI falsifiable against today's
parsed-but-ignored `max_recv_msg_size`. The load-bearing OOM question (D2:
enforcement site and its honest protection strength) is surfaced for Morgan
without being resolved here. Public-API impact expected INTERNAL only.
Proceed to the peer-review gate before handoff.
