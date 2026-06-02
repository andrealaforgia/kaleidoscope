# Story Map — wal-torn-tail-recovery-v0

British English. No em dashes in body.

## User: operator restarting a file-backed Kaleidoscope collector after an abrupt process death

## Goal: bring the store back up serving everything durably acked before the crash, instead of a store that refuses to start because of the benign torn final WAL line a crash leaves

## Traceability note

JTBD skipped: the job is already clear and singular (recover the durable acked prefix after an abrupt crash, and do not silently tolerate anything other than the expected post-crash torn tail). No DIVERGE artifacts exist for this feature (`docs/feature/wal-torn-tail-recovery-v0/diverge/` is absent); this is a robustness hardening of an existing path triaged from black-box verifier issue 006 and a third-party four-quadrants assessment, with the solution already agreed as "option 1" (tolerate the torn final line). The decision is encoded as the requirement, not re-opened. Recorded as a noted condition in `wave-decisions.md`.

## Backbone

The operator journey is short (lightweight UX depth: the user is an operator restarting a crashed collector, the interface is a process start plus stderr plus a read-API query).

| Crash happens | Restart the binary | Store recovers | Confirm and resume |
|---|---|---|---|
| Process killed mid-WAL-append, torn final line left | Run the same binary against the same pillar_root | Replay drops the torn tail, recovers the intact prefix | Query the read API; read the WARN line; resume traffic |

---

### Walking Skeleton

Not applicable as a greenfield end-to-end build: this is a brownfield hardening of an existing, shipped recovery path. The stores, their WAL append, their snapshot-plus-replay recovery, and their fsync honesty already exist. The thinnest end-to-end operator-visible behaviour is the single slice below, which is itself the whole feature.

### Single Slice (the whole feature): US-01 Crashed-then-restarted store recovers its intact acked prefix and warns about the dropped torn tail

This is deliberately ONE thin slice. The feature is fundamentally a robustness fix with one operator-visible behaviour: a crashed-then-restarted store recovers its intact acked prefix and warns about the dropped torn tail. It is NOT padded with extra slices.

- **Tasks in the slice**:
  - Change the parse-failure arm of the WAL replay loop in lumen, ray, cinder (and pulse, conditional on DESIGN FLAG 1) so a torn final line (last line, no trailing newline) is dropped and the intact prefix recovers.
  - Emit a structured `tracing` WARN event naming the pillar, line number, and dropped byte length.
  - Keep every other parse failure (mid-file, or newline-terminated malformed final line) fail-closed with the existing `PersistenceFailed`.
  - Correct the false cinder module doc at `crates/cinder/src/file_backed.rs:36-38` and the `open` doc to match the actual behaviour.
- **Target outcome KPI**: K1 (intact-prefix recovery rate to 100%) and K2 (torn-tail-attributable refusal incidents to 0). See `outcome-kpis.md`.
- **Operator-visible end-to-end path**: restart binary -> store opens -> read API returns the acked prefix -> WARN line in stderr. The lumen `GET /api/v1/logs` path is the concrete end-to-end assertion (verifier expectation D04).

## Priority Rationale

There is one slice, so prioritisation is trivial, but the ordering of WORK WITHIN the slice and the rationale for not splitting are recorded here:

1. **Why one slice, not many**: the feature delivers exactly one operator-visible behaviour change. Splitting per pillar (lumen slice, ray slice, cinder slice) would fragment a single coherent recovery-contract change into near-identical copies with no independent operator value: an operator does not care which pillar recovers first; they care that the recovery contract is honest across the stores they run. The four pillars share a verified-identical replay shape, so the marginal cost of doing them together is low and the coherence benefit (one ADR, one contract, one test pattern) is high. Per the Elephant Carpaccio gate, this is right-sized: one user outcome, six to seven UAT-relevant assertions (three positive, two negative, plus doc-correction and scope coverage), one to three days, demonstrable in a single session.
2. **Within the slice, positive path first**: AC-1 (intact-prefix recovery, the verifier D04 path) is the riskiest assumption and the highest-value behaviour. It is built and tested first.
3. **Negative guards next**: AC-5 and AC-6 (mid-file and newline-terminated-malformed stay fail-closed) are built immediately after, because the value of the tolerance depends entirely on it being NARROW. A tolerance that swallows mid-file corruption would be worse than the current fail-closed behaviour. These negative criteria are not optional polish; they are co-equal with the positive path.
4. **cinder doc correction**: bundled into the same slice because the doc is false TODAY and the project's thesis is structural honesty. Correcting the code without correcting the doc, or vice versa, would leave a known dishonesty in the tree.

## Scope Assessment: PASS

One user story, one operator outcome. Bounded contexts touched: the WAL replay path of three confirmed pillars (lumen, ray, cinder) plus one conditional (pulse), all sharing one verified-identical code shape, plus one cinder doc. Integration points: zero external; the change reads the in-process filesystem under `pillar_root`. No walking-skeleton integration sprawl. Estimated effort: one to three days (a confined edit to one arm of a loop replicated across three or four near-identical sites, plus acceptance tests and a doc fix). Single demonstrable session. PASS on every Elephant Carpaccio dimension; no split needed. The only scope question (pulse in or out of this slice) is a DESIGN decision recorded as FLAG 1, not a re-slice.
