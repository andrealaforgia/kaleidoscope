# DoR Validation: aperture-serve-loop-error-surfacing-v0

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-05.
British English. No em dashes in body.

Validates the 9-item Definition of Ready for US-01, US-02, US-03 against
`user-stories.md` and `wave-decisions.md`, citing the code loci verified
on this branch. The DoR is a hard gate: every item must PASS with
evidence before handoff to DESIGN. Verdict at the end.

## Verified code loci (re-confirmed on this branch, feeding every story)

| Locus | Confirmed | Used by |
|---|---|---|
| `transport.rs:50` `pub async fn spawn_grpc`, returns `(SocketAddr, JoinHandle<()>)` | yes | D1, US-01 |
| `transport.rs:89-94` gRPC `let _ = server.await;` (swallow, disclosed by comment `:90-92`) | yes | US-01, KPI-1 |
| `transport.rs:85-87` gRPC graceful `Ok` path (`serve_with_incoming_shutdown`) | yes | US-03, D3 |
| `transport.rs:117` `pub async fn spawn_http`, returns `(SocketAddr, JoinHandle<()>)` | yes | D1, US-01 |
| `transport.rs:152-158` HTTP `let _ = axum::serve(...).with_graceful_shutdown(...).await;` (swallow, SILENT, no comment) | yes | US-01, US-03, KPI-1 |
| `transport.rs:167-173` `/healthz` unconditional 200 | yes | US-02, C6 |
| `transport.rs:179-194` `/readyz` reflects phase only | yes | US-02 |
| `readiness.rs:37-41` `ReadinessPhase` = `Starting`/`Ready`/`Draining` only (no `Failed`) | yes | US-02, D2 |
| `readiness.rs:13-17` `Draining` is sticky, never recovers | yes | US-02, D2 |
| `shutdown.rs:99-106` `DrainOutcome::exit_code()` = 0 clean / 1 deadline | yes | US-02, D2 |
| `shutdown.rs:125-134` `ShutdownBundle` owns `grpc_join`/`http_join: JoinHandle<()>` | yes | D1 |
| `shutdown.rs:185-190` SOLE awaiter: `let _ = join_grpc.await; let _ = join_http.await;` | yes | D1, US-01 |
| `lib.rs:47` `mod transport;` PRIVATE module (no pub qualifier) | yes | D1, C3 |
| `lib.rs:205-227` `run` -> `drain_to_exit_code` -> `exit_code()` exit seam | yes | US-02, D2 |
| `lib.rs:161-171` `Handle::drop_signal_listeners` abandons joins on Drop | yes | D1 |
| `lib.rs:351-356`, `:379-430` hand-constructed-bundle tests (injection seam) | yes | C7, US-01/US-02/US-03 test seam |
| `observability.rs:30-51` closed `event` vocabulary, no `serve_loop_failed` yet | yes | US-01, C5, KPI-2 |
| `compose.rs:132,150,158,180-189` composition root stores joins into bundle | yes | D1 |

### Public-API confirmation (D1 / C3)

`spawn_grpc` and `spawn_http` are declared `pub async fn` but live in a
PRIVATE module: `lib.rs:47` declares `mod transport;` with no `pub` or
`pub(crate)` qualifier, so the module is crate-private and its `pub`
functions are reachable only within the `aperture` crate. They are NOT
re-exported from `lib.rs` (the only `pub mod` re-exports are `config`,
`ports`, `testing`). `ShutdownBundle` (`shutdown.rs:125`),
`ReadinessPhase` (`readiness.rs:37`), and `orchestrate_shutdown`
(`shutdown.rs:138`) are all `pub(crate)`. **Conclusion: NOT public API;
the D1 ripple is INTERNAL only.** This matches the substance of C3 and
the existing docs' "NOT public API" claim. Nuance for DESIGN: the
mechanism is "`pub fn` in a private module" rather than literally
`pub(crate) fn` on the helpers; the effect (crate-private, no public-
surface leak) is identical. DESIGN still confirms no public type leaks
via a returned error type that becomes nameable (D1); any leak is
semver-MINOR at most, pre-1.0, NEVER 1.0.0.

## Sole-awaiter / consumer list (the D1 ripple, re-confirmed)

Producers `spawn_grpc`/`spawn_http` -> `compose::spawn`
(`compose.rs:132,150,158,180-189`) -> `ShutdownBundle`
(`shutdown.rs:125-134`) -> `orchestrate_shutdown` drain future
(`shutdown.rs:185-190`, the **sole awaiter**, `let _ = join.await` x2)
-> exit seam `drain_to_exit_code`/`run` (`lib.rs:205-227`). Plus the Drop
abandon-path (`lib.rs:161-171`, low ripple) and two
hand-constructed-bundle tests (`lib.rs:351-356`, `:379-430`). No other
site produces or consumes the joins (the per-request service impls and
HTTP handlers, `wire_sink`/`probe_or_refuse`, and the public re-exports
are NOT consumers). Bounded and internal.

---

## US-01: A serving loop that dies after bind emits a structured stderr event naming the transport and error

| # | DoR item | Verdict | Evidence |
|---|---|---|---|
| 1 | Problem statement clear, domain language | PASS | `user-stories.md` US-01 Problem: Sam scrapes stderr and alerts on it; a dead listener emits zero telemetry about its own death. Domain language (operator, log pipeline, dead listener), no solution prescription. |
| 2 | User/persona with specific characteristics | PASS | Sam the platform operator; runs a live aperture ingest fleet; scrapes structured stderr into a log/alert pipeline; wants machine-parseable notice of which transport died and why (US-01 Who). |
| 3 | 3+ domain examples with real data | PASS | Three examples: graceful-shutdown negative control (full event sequence named); gRPC death on `:4317` (`event=serve_loop_failed transport=grpc error=<reason>`); HTTP death on `:4318` (`transport=http`, the previously silent arm). Real ports, real event shape. |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | 3 Gherkin scenarios (gRPC named, HTTP named, graceful negative control). Within 3-7. |
| 5 | AC derived from UAT | PASS | 4 AC each trace to a scenario (gRPC event, HTTP event, graceful no-event, single closed-vocabulary constant). |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios; replaces two swallow sites + adds one event constant + routes the `Result` (D1); bounded internal ripple. 1 day. |
| 7 | Technical notes: constraints/dependencies | PASS | US-01 Technical Notes: depends on D1; one additive constant (C5); D3 gates the emit; test seam `lib.rs:379-430` + `testing::stderr_capture` (C7). |
| 8 | Dependencies resolved or tracked | PASS | D1 flagged for DESIGN (`wave-decisions.md` D1), consumer ripple enumerated and bounded. No unresolved blocker. |
| 9 | Outcome KPIs defined with measurable targets | PASS | US-01 KPI block + `outcome-kpis.md` KPI-1 (swallow sites 2 -> 0) and KPI-2 (exactly one event per arm, falsifiable against the swallow). |

US-01 DoR: **9/9 PASS.**

## US-02: A dead serving loop stops the process from reporting healthy/ready

| # | DoR item | Verdict | Evidence |
|---|---|---|---|
| 1 | Problem statement clear, domain language | PASS | US-02 Problem: `/readyz` stays ready, exit code unaffected, orchestrator keeps routing to a zombie that serves nothing. Operator/orchestrator language; no solution prescribed (D2 left open). |
| 2 | User/persona with specific characteristics | PASS | Sam relying on `/readyz` and the exit code as the lever his orchestrator (k8s) acts on (US-02 Who). |
| 3 | 3+ domain examples with real data | PASS | Healthy instance answers `/readyz 200 "ready"` + `/healthz 200 "ok"`; gRPC death flips `/readyz` to 503 while `/healthz` stays 200; serve death drives a non-zero exit distinct from clean-drain `0`. Real probe paths and status codes. |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | 3 Gherkin scenarios (healthy negative control, stops reporting ready, exit reaction observable). |
| 5 | AC derived from UAT | PASS | 4 AC trace to scenarios + the zombie-never-ready guarantee; written to "per the D2 decision" so they stay solution-neutral while still testable. |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios; the reaction reuses the existing readiness machine + exit-code seam; D2 picks the combination. 1 day. |
| 7 | Technical notes: constraints/dependencies | PASS | US-02 Technical Notes: depends on US-01; D2 owned by DESIGN; consider `ReadinessPhase::Failed` vs sticky `Draining` (`readiness.rs:13-17`); exit code must not collide with clean-drain `0` (`shutdown.rs:99-106`); `/healthz` stays 200 (C6). |
| 8 | Dependencies resolved or tracked | PASS | Depends on US-01 (tracked, sequenced first in story-map Priority Rationale). D2 flagged for DESIGN. |
| 9 | Outcome KPIs defined with measurable targets | PASS | US-02 KPI block + `outcome-kpis.md` KPI-3: ready-window indefinite -> next-probe-after-death (0 stays-ready), non-zero exit distinct from 0, falsifiable against today. |

US-02 DoR: **9/9 PASS.**

## US-03: Both transports covered, the silent HTTP arm explicitly proven, and a graceful shutdown never false-alarms

| # | DoR item | Verdict | Evidence |
|---|---|---|---|
| 1 | Problem statement clear, domain language | PASS | US-03 Problem: the gRPC swallow is disclosed, the HTTP swallow silent; a one-arm fix re-ships the undisclosed dishonesty, and over-eager surfacing on a normal SIGTERM is itself a lie (false alarm). Domain language, two-sided framing. |
| 2 | User/persona with specific characteristics | PASS | Sam alerts on `serve_loop_failed` and restarts instances routinely with SIGTERM; wants both arms covered and zero false alarms (US-03 Who). |
| 3 | 3+ domain examples with real data | PASS | SIGTERM drains cleanly ending `shutdown_complete exit_code=0`, no `serve_loop_failed`; the HTTP arm proven by its own example (`transport=http`); unexpected-early-`Ok` follows the D3 decision. Real event names + exit code. |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | 3 Gherkin scenarios (graceful negative control, HTTP arm proven in its own right, serving-loop-ends-without-shutdown follows D3). |
| 5 | AC derived from UAT | PASS | 4 AC trace to scenarios incl. the slice-08-suite-stays-green regression guard. |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios; same shared mechanism as US-01/US-02 proven on both arms + the graceful-vs-fatal guard (D3). 1 day. Carpaccio split (R1 HTTP-first, R2 gRPC) available if D1 ripple balloons (story-map). |
| 7 | Technical notes: constraints/dependencies | PASS | US-03 Technical Notes: depends on US-01 + US-02; D3 owned by DESIGN; C2 regression guard `tests/slice_08_graceful_shutdown.rs`; the HTTP-arm scenario is mandatory and explicit. |
| 8 | Dependencies resolved or tracked | PASS | Depends on US-01 + US-02 (tracked). D3 flagged for DESIGN. The graceful `Ok` vs fatal `Err` distinction has a verified locus (`transport.rs:85-87,153-157`). |
| 9 | Outcome KPIs defined with measurable targets | PASS | US-03 KPI block + `outcome-kpis.md` KPI-4 (false-alarm rate 0) and KPI-5 (coverage 1-of-2-disclosed -> 2-of-2-surfaced, HTTP proven). |

US-03 DoR: **9/9 PASS.**

## Feature-level checks

- **Mutation guardrail (C8 / Gate 5)**: KPI-6 targets 100% kill on
  `transport.rs`, `shutdown.rs`, `readiness.rs`. The swallow lines are
  currently uncovered for the error path (a mutant deleting the swallow
  survives), which is exactly what the new tests must fix. Tracked as a
  DELIVER closing check.
- **Solution-neutrality**: D1/D2/D3 are stated as requirements
  ("surface the error", "do not report ready", "never false-alarm"),
  not mechanisms. DESIGN owns the routing shape, the readiness/exit
  combination, and the graceful-vs-fatal seam. PASS.
- **Real data**: every example uses Sam, real ports (`:4317`/`:4318`),
  and real event names (`serve_loop_failed`, `shutdown_complete
  exit_code=0`). No generic `user123`/`test@test.com`. PASS.
- **No technical-AC anti-pattern**: AC are observable operator outcomes
  ("a structured event names the transport", "`/readyz` no longer reports
  ready"), not implementation ("use a channel"). PASS.
- **Right-sizing (feature)**: 3 stories, 9 scenarios, 1 crate, 1
  persona, no UI; bounded internal ripple. PASS (story-map Scope
  Assessment).
- **No DIVERGE artifacts**: recorded as a Low/Medium risk in
  `wave-decisions.md`; the job is grounded in the four-quadrants Q3
  finding + the Earned-Trust posture and verified in code. Does not block.

## Verdict

**DoR PASS for US-01, US-02, US-03 (9/9 each).** All three stories are
ready for the DESIGN wave, with D1/D2/D3 correctly flagged as
DESIGN-owned mechanism decisions and every KPI falsifiable against
today's swallow. Public-API impact confirmed INTERNAL only. Proceed to
the peer-review gate before handoff.
