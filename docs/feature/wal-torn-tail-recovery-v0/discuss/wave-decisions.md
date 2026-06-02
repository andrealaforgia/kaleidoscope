# Wave Decisions — wal-torn-tail-recovery-v0 (DISCUSS)

British English. No em dashes in body.

## Wave: DISCUSS (Luna, nw-product-owner)

## Autonomous run note

This was an autonomous run. All interactive decisions were made by the agent per the standing instruction; no decision was deferred to the user. The decisions below were made within authorised scope.

## Decisions made in DISCUSS

| # | Decision | Rationale |
|---|---|---|
| D1 | Feature Type: Backend (storage/library; no UI). | The change is to the WAL replay path of file-backed stores. The only operator-facing surface is a process start, a stderr WARN line, and the existing read-API query. Loaded TUI/CLI UX skills for the stderr-and-query touchpoints; no web/desktop patterns apply. |
| D2 | Walking Skeleton: No. | Brownfield. The stores, WAL append, snapshot-plus-replay recovery, and fsync honesty already exist. This hardens one arm of an existing path. There is no greenfield end-to-end skeleton to stand up. Recorded in `story-map.md`. |
| D3 | UX Research Depth: Lightweight. | The user is an operator restarting a crashed collector. The journey is short (crash, restart, recover, confirm). No deep emotional-arc or multi-step journey artifacts warranted; the emotional arc (frustration at a store that will not start, relief at a clean recovery, trust from the explicit warning) is captured inline in the Elevator Pitch and KPIs. |
| D4 | JTBD analysis: No (skipped). | The job is clear and singular (recover the durable acked prefix after an abrupt crash; do not silently tolerate anything but the expected torn tail). The solution is already triaged and agreed as "option 1". Encoded as the requirement, not re-opened. Recorded in `story-map.md` traceability note. |
| D5 | Single thin slice; no padding. | Fundamentally a robustness fix with one operator-visible behaviour. Splitting per pillar would fragment one coherent recovery-contract change into near-identical copies with no independent operator value. Scope assessed PASS on every Elephant Carpaccio dimension. Recorded in `story-map.md`. |
| D6 | Solution is the accepted "option 1"; decision NOT re-opened. | The verifier (issue 006) and the four-quadrants assessment already agreed: tolerate a torn FINAL line (last line, no trailing newline), drop it, recover the prefix, warn; keep every other parse failure fail-closed. DISCUSS encodes this as the requirement. |
| D7 | cinder doc correction is in scope and mandatory. | `crates/cinder/src/file_backed.rs:36-38` and the `open` doc already claim a truncated last line "is detected and ignored" while the code returns `PersistenceFailed`. A project whose thesis is structural honesty must not ship a doc claiming a robustness the code lacks. Bundled into the single slice (AC-7). |
| D8 | Mid-file corruption and newline-terminated malformed final lines stay fail-closed, as explicit NEGATIVE acceptance criteria (AC-5, AC-6). | The value of the tolerance depends entirely on it being NARROW. A tolerance that swallowed mid-file corruption would be strictly worse than today's fail-closed behaviour. These are co-equal with the positive path (K4 guardrail). |
| D9 | Elevator Pitch references a real operator-invocable entry point. | The lumen-backed `log-query-api` binary opens `FileBackedLogStore::open(pillar_root, ...)` before binding; the operator restart of that binary plus `GET /api/v1/logs` is the concrete end-to-end path. The "After" line names observable output (store starts, read API returns the prefix, WARN line on stderr). This is the verifier D04 black-box path. |

## Verified facts (from reading the codebase, not assumed)

- Parse-or-die replay shape confirmed IDENTICAL in four pillars:
  - `crates/lumen/src/file_backed.rs:107-121` (`FileBackedLogStore::open`).
  - `crates/ray/src/file_backed.rs:120-135` (`FileBackedTraceStore::open`).
  - `crates/cinder/src/file_backed.rs:135-163` (`FileBackedTieringStore::open`).
  - `crates/pulse/src/file_backed.rs:164-173` (`FileBackedMetricStore::open`).
  All four call `serde_json::from_str(&line)` and map the first parse failure to a `PersistenceFailed` variant, aborting the open.
- The cinder false doc claim is confirmed at `crates/cinder/src/file_backed.rs:36-38` ("A truncated last WAL line ... is detected and ignored") and the `open` doc at lines 104-106 ("other than a single truncated last line, which is silently tolerated"). The code at lines 143-160 does the opposite.
- Structured `tracing` `event = "..."` convention confirmed at `crates/log-query-api/src/main.rs:76` (`event = "log_query_api_starting"`), `:86` (`event = "health.startup.refused"`), `:93` (`event = "listener_bound"`). Subscribers are installed at the read tier and gateway, so a new WARN event is observable.
- Earned-Trust lineage confirmed: ADR-0049 (write path honours fsync; the torn tail is the residue a fsync-honest append leaves after a crash), ADR-0050 (read-side honest caps), both citing ADR-0040 (the WAL plus snapshot plus replay recovery discipline) as the contract. Next free ADR number is 0059 (highest existing 0058).

## Flags for DESIGN (solution-architect / Morgan)

| Flag | Question | Recommended posture |
|---|---|---|
| FLAG 1 | Is pulse in scope for THIS slice? `crates/pulse/src/file_backed.rs:164-173` has the same parse-or-die shape, but pulse also rebuilds `tenant_counts` after replay and has the fsync-honest write path from ADR-0049. Confirm the replay path is close enough to extend in the same slice. | RECOMMENDED: include pulse if its replay loop is structurally identical (it appears to be); the marginal cost is low and coverage is uniform. If a pulse-specific subtlety (the `tenant_counts` rebuild) complicates the torn-tail drop, defer pulse to a fast-follow and record the deferral. Resolved by DESIGN; AC-9 covers both outcomes. |
| FLAG 2 | What detection mechanism distinguishes a torn final line (no trailing newline) from earlier lines and from a newline-terminated malformed final line? `BufRead::lines()` strips newlines and hides whether the last line had one. | RECOMMENDED: pick the cheapest correct mechanism (inspect the final byte of the WAL file for `\n`, or read-ahead one line so "last line" is known before parsing, or buffer the last line and inspect raw bytes). DESIGN pins it in ADR-0059. The requirement pins only the OBSERVABLE behaviour, not the mechanism. |
| FLAG 3 | Final warning event name and payload field names. The proposed `event="wal.recovery.torn_tail_dropped"` mirrors the dotted style of `health.startup.refused`; fields proposed: pillar, line number, dropped byte length. | RECOMMENDED: adopt the proposed dotted name and the three payload facts (AC-3 mandates the four facts; exact field spellings are DESIGN's to pin). No new metric or dashboard; ride the existing structured `tracing` stream. |
| FLAG 4 | Shared helper vs per-pillar replication of the torn-tail recovery logic. The four loops are near-identical but have distinct error types (`LogStoreError`, `TraceStoreError`, `MigrateError`, `MetricStoreError`) and distinct `WalRecord` enums. | RECOMMENDED: DESIGN's call. A parameterised helper reduces drift but adds a generic seam across four crates; per-pillar replication is simpler but risks divergence. Either satisfies the requirement. Decide with the Rust-idiomatic paradigm (CLAUDE.md: free functions and traits where polymorphism is genuinely needed, no `dyn` where monomorphisation suffices) in mind. |
| FLAG 5 | ADR-0059 authorship. The recovery-contract change requires a new ADR. | RECOMMENDED: solution-architect authors ADR-0059 in DESIGN, citing ADR-0040 / 0049 / 0050 as the Earned-Trust lineage; ADRs are immutable, so ADR-0040 is referenced, not edited. |

## Risks surfaced (for downstream waves to manage, not managed here)

| Risk | Probability | Impact | Mitigation approach |
|---|---|---|---|
| Detection mechanism mis-classifies a newline-terminated malformed final line as a torn tail and silently tolerates real corruption. | Low | High | AC-6 negative criterion plus mutation testing on the no-trailing-newline guard at 100% kill (AC-10). DESIGN must pick a mechanism that inspects the actual trailing byte. Mitigate in DESIGN and DELIVER. |
| A WAL with multiple torn lines (more than one parse failure, the last being torn) is mis-handled. A clean crash leaves at most one torn tail, but a pathological file could have an earlier parse failure too. | Low | Medium | AC-5 covers this: any parse failure that is NOT the last line refuses. The torn-tail tolerance applies only when the LAST line is the failing line. DESIGN clarifies ordering in ADR-0059. |
| pulse `tenant_counts` rebuild interacts awkwardly with dropping the torn tail. | Low | Low | FLAG 1: defer pulse if the interaction is non-trivial; record the deferral. |
| No DIVERGE wave ran for this feature (`docs/feature/wal-torn-tail-recovery-v0/diverge/` absent). | n/a (accepted) | Low | Accepted: the solution was triaged and agreed externally (verifier issue 006 option 1) before DISCUSS. The job is singular and clear; JTBD/DIVERGE would add no information. Recorded here as a noted condition, not a blocker. |

## Peer review

- Reviewer: nw-product-owner-reviewer (review mode).
- Outcome: see the review record appended to `dor-validation.md`.
- DISCUSS completes only when the review passes and the 9-item DoR gate passes.

## Handoff

- To DESIGN (solution-architect / Morgan): `user-stories.md`, `story-map.md`, `outcome-kpis.md`, `dor-validation.md`, this file. Author ADR-0059. Resolve FLAG 1 through FLAG 5.
- To DISTILL (acceptance-designer): the five embedded Gherkin scenarios (three positive, two negative) plus AC-1 through AC-10; the verifier D04 intact-prefix path is the headline acceptance.
- To DEVOPS (platform-architect): the `outcome-kpis.md` handoff section (the WAL-recovery WARN event rides the existing structured `tracing` stream; no new metric or dashboard at v0).
- DISCUSS only. Does NOT proceed into DESIGN.
