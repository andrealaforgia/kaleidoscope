<!-- markdownlint-disable MD024 -->

# User Stories — wal-torn-tail-recovery-v0

British English. No em dashes in body.

Single-story brownfield slice. Hardens the WAL replay path of three (conditionally four) file-backed storage pillars so that a torn final WAL line, the expected post-crash shape, no longer bricks recovery of the intact acked prefix that precedes it. Also corrects a false cinder module doc that already claims this robustness while the code lacks it.

## System Constraints

- **Brownfield, not greenfield**. The stores, their WAL append paths, their snapshot-plus-replay recovery discipline (ADR-0040), and their fsync honesty (ADR-0049) already exist and are unchanged in shape. This feature changes only the parse-failure arm of the WAL replay loop on `open`, plus one module doc.
- **Fail-closed stays the default**. The ONLY newly-tolerated shape is a parse failure on the LAST line of the WAL when that line has no trailing newline. Every other parse failure (mid-file, or a malformed line that DOES end in a newline) MUST still return the existing `PersistenceFailed` error and refuse to start. This is a narrowing of fail-closed, not an abandonment of it.
- **No data is ever fabricated or partially served**. The torn tail is DROPPED, never repaired, never partially decoded. Recovery serves the intact acked prefix exactly as it would have before the torn record was appended.
- **Recovery contract change requires an ADR**. This feature changes the documented recovery contract of the file-backed stores, so it ships with a new ADR. The next free number is ADR-0059 (highest existing is ADR-0058, verified by `ls docs/product/architecture/adr-*.md`). The ADR is authored in the DESIGN wave by the solution-architect, not in DISCUSS.
- **Earned-Trust lineage**. This feature extends the Earned-Trust principle line: ADR-0040 (WAL plus snapshot plus replay recovery discipline), ADR-0049 (the write path honours fsync), ADR-0050 (read-side honest caps). ADR-0049 made writes crash-honest; this feature makes the matching READ-back of a crash-honest write recover the durable prefix instead of refusing it. The torn tail is precisely the residue a fsync-honest, append-only WAL leaves after `kill -9` or power loss.
- **Scope: lumen, ray, cinder confirmed; pulse conditional**. The parse-or-die replay shape is identical and verified in `crates/lumen/src/file_backed.rs:107-121`, `crates/ray/src/file_backed.rs:120-135`, and `crates/cinder/src/file_backed.rs:135-163`. The same shape is also present in `crates/pulse/src/file_backed.rs:164-173`; pulse is IN SCOPE conditionally and the DESIGN wave confirms whether its replay path is close enough to extend in the same slice. See FLAG 1 in `wave-decisions.md`.
- **cinder doc correction is in scope and mandatory**. The cinder module doc at `crates/cinder/src/file_backed.rs:36-38` (and the `open` doc comment at lines 104-106) already CLAIMS a truncated last WAL line is "detected and ignored". The code does the opposite (it returns `PersistenceFailed`). This feature makes the code true AND corrects the doc to match the actual behaviour. A project whose thesis is structural honesty against vendor overstatement must not ship a doc claiming a robustness the code lacks.
- **No trait change**. `LogStore`, `TraceStore`, `TieringStore`, and (if in scope) `MetricStore` trait signatures stay byte-identical. Gate 2 (`cargo public-api`) catches any regression. The change is internal to each pillar's `open` replay loop.
- **Observable warning required**. When a torn tail is dropped, recovery MUST emit a structured `tracing` WARN event. The read tier and storage composition roots already install `tracing` subscribers (`crates/log-query-api/src/main.rs:76`, `crates/kaleidoscope-gateway/src/main.rs`), so the warning is visible to operators. The event follows the existing structured `event = "..."` field convention (e.g. `event = "log_query_api_starting"`, `event = "health.startup.refused"`).
- **Per-feature mutation testing at 100% kill rate** (ADR-0005 Gate 5, CLAUDE.md), scoped to modified files via each pillar's existing `gate-5-mutants-*` job and `--in-diff`. The newly-tolerated arm and the three guard conditions (is-last-line, no-trailing-newline, parse-failed) are the primary mutation targets.

---

## US-01: Crashed-then-restarted store recovers its intact acked prefix and warns about the dropped torn tail

Tag: `@user-visible`

### Elevator Pitch

- **Before**: an operator runs the Kaleidoscope collector binary that opens a file-backed store rooted at `pillar_root` (for lumen, the log-query read API binary `crates/log-query-api/src/main.rs` opens `FileBackedLogStore::open(pillar_root, ...)` before binding its listener; the storage gateway `crates/kaleidoscope-gateway/src/main.rs` does the equivalent for the tiering store). The process is killed mid-write by `kill -9`, an OOM kill, a power loss, or an unclean container stop. The append-only WAL now ends in a torn record: a partial JSON line with no trailing newline, because the write was interrupted between `write_all(bytes)` and `write_all(b"\n")` or part-way through the bytes. On the next start, the open path replays the WAL line by line; the torn final line fails `serde_json::from_str`; the very first parse failure returns `PersistenceFailed` and the store refuses to start. Every durably acked record BEFORE the torn tail (which may be the entire production history) is unreachable. The read API's Earned-Trust probe then fails, the binary logs `event=health.startup.refused`, and exits non-zero. The operator sees a collector that will not come back up after a crash that the WAL discipline was specifically designed to survive.
- **After**: the operator restarts the same binary against the same crashed `pillar_root`. The open path replays the WAL; the torn final line fails to parse; the recovery loop recognises it as the EXPECTED post-crash tear (it is the last line AND it has no trailing newline), drops just that one torn record, and recovers every acked record before it. The binary starts successfully and binds its listener. The operator queries the read API (for lumen, `GET /api/v1/logs?...` on the bound port) and sees every log record that was durably acked before the crash returned in the response body, in the same order as before the crash. The recovery emits a structured WARN line on stderr the operator can read in `journalctl` or the container log, naming the event (`event="wal.recovery.torn_tail_dropped"`), the pillar, the line number, and the byte length of the dropped tail.
- **Decision enabled**: the operator decides the collector has recovered cleanly and resumes serving traffic, instead of escalating a "store will not start after crash" incident, hand-editing the WAL to strip the torn bytes, or restoring from a backup. The structured warning lets the operator confirm exactly one torn tail was dropped (not a silent mid-file gap) before trusting the recovered store.

### Problem

Kaleidoscope's file-backed stores recover their state on `open` by loading an optional snapshot and then replaying an append-only write-ahead log: one `serde_json`-serialised record per line, newline-terminated. The replay loop is parse-or-die. The FIRST line that fails to parse returns `PersistenceFailed` and aborts the whole open. Verified, identical shape in:

- `crates/lumen/src/file_backed.rs:107-121` (`FileBackedLogStore::open`)
- `crates/ray/src/file_backed.rs:120-135` (`FileBackedTraceStore::open`)
- `crates/cinder/src/file_backed.rs:135-163` (`FileBackedTieringStore::open`)
- `crates/pulse/src/file_backed.rs:164-173` (`FileBackedMetricStore::open`) — conditional, see FLAG 1

After an abrupt process death (`kill -9`, OOM kill, power loss, unclean container stop), the LAST WAL line is commonly torn: a partial write with no trailing newline, because the append path writes the record bytes, then the newline, then fsyncs (ADR-0049), and an interruption between any of those steps leaves a tail that does not parse. That torn tail is the EXPECTED post-crash shape that the v1 "survives a restart" durability promise is supposed to handle. Today a single torn trailing line blocks recovery of the entire intact, acked, durable prefix that precedes it.

This is fail-closed and therefore SAFE (the store never serves partial or corrupt data, so this is not a correctness bug). But it contradicts the durability promise: an operator whose collector was killed cannot bring it back up, even though every acked record up to the crash is intact on disk. It is an Earned-Trust gap in the same family ADR-0040 / 0049 / 0050 address: the platform claims durable recovery, and the code refuses the very shape a crash produces.

Worse, cinder's module doc at `crates/cinder/src/file_backed.rs:36-38` and its `open` doc at lines 104-106 already CLAIM that a truncated last WAL line "is detected and ignored" and that all other parse errors are surfaced. The code does NOT do this; it surfaces the truncated last line as `PersistenceFailed` just like every other pillar. The doc is a false robustness claim shipped in a project whose whole thesis is structural honesty against vendor overstatement.

### Who

- **User type**: an operator running a Kaleidoscope collector or read-API binary that opens a file-backed store. Concretely: the on-call SRE who restarts `log-query-api` (lumen-backed), `trace-query-api` (ray-backed), or the `kaleidoscope-gateway` (cinder tiering store, and, if in scope, pulse metric store) after the process was killed.
- **Context**: the operator has just experienced an abrupt process death of a collector that had been durably acking writes. They run the same binary against the same `pillar_root` to bring the service back. They read the process stderr (via `journalctl`, `docker logs`, or `kubectl logs`) and the read API's HTTP responses to judge whether recovery succeeded. They trust that an append-only, fsync-honest WAL (ADR-0049) means everything acked before the crash is recoverable.
- **Motivation**: get the collector serving again, fast, without losing the acked history and without hand-surgery on the WAL file. Confirm that exactly one torn tail was dropped (the benign post-crash residue) and not that a mid-file gap was silently skipped (which would be data loss the operator must NOT tolerate silently).

### Solution

In each in-scope pillar's WAL replay loop on `open`, change ONLY the parse-failure arm. When `serde_json::from_str` fails on a line, the loop checks three conditions: (a) it is the LAST line of the WAL, (b) that line has NO trailing newline (it was torn, not a complete record that happens to be malformed), and (c) it is the parse failure under consideration. If all three hold, the loop DROPS that one torn record, finishes recovery with the intact prefix already accumulated, emits a structured `tracing` WARN event naming the pillar, the line number, and the dropped byte length, and returns the recovered store. If the parse failure is NOT the torn final line (it is followed by further lines, or the failing line ends in a newline), the loop returns `PersistenceFailed` exactly as today.

Additionally, correct the cinder module doc at `crates/cinder/src/file_backed.rs:36-38` and the `open` doc at lines 104-106 so the prose matches the actual (newly correct) behaviour: a torn final line with no trailing newline is dropped with a warning; every other parse failure is surfaced as `PersistenceFailed`.

The DESIGN wave (solution-architect) decides the exact detection mechanism (read-ahead vs trailing-byte inspection vs buffering the last line), the warning event name and payload fields, whether the recovery logic is shared across pillars or replicated per pillar, and authors ADR-0059. The implementation is the crafter's, in DELIVER.

### Domain Examples

#### 1. Happy path: lumen recovers 10,000 acked log records after a kill -9 tore the 10,001st

An operator runs the lumen-backed `log-query-api` binary against `pillar_root=/var/lib/kaleidoscope`. Tenant `acme-corp` has durably acked 10,000 log records; the WAL contains 10,000 newline-terminated `{"op":"ingest",...}` lines. While appending the 10,001st record (a batch for tenant `acme-corp`), the process is killed by `kill -9`. The append had written `{"op":"ingest","tenant":"acme-corp","records":[{"body":"order 4471 shi` and no more: a partial line, no trailing newline. The operator restarts `log-query-api` against the same `pillar_root`. The replay loop reads 10,000 valid lines, then hits the torn line 10,001, recognises it as the last line with no trailing newline, drops it, and recovers the 10,000-record prefix. The binary starts and binds its listener on `127.0.0.1:8080`. The operator runs `curl 'http://127.0.0.1:8080/api/v1/logs?tenant=acme-corp&start=0&end=9999999999'` and the response body contains all 10,000 acked records for `acme-corp`, in `observed_time` order. The process stderr contains one WARN line: `event="wal.recovery.torn_tail_dropped" pillar="lumen" line=10001 dropped_bytes=58`.

#### 2. Edge case: ray recovers when ONLY the torn tail exists (snapshot present, single torn WAL line, no intact WAL prefix)

An operator runs the ray-backed `trace-query-api` binary. A prior `snapshot()` persisted all trace state for tenant `globex` and truncated the WAL. One trace-ingest record was then appended and the process was killed by an OOM kill while writing it, leaving a WAL of exactly ONE torn line: `{"op":"ingest","tenant":"globex","spans":[{"trace_id":"a1b2` with no trailing newline. On restart, the snapshot loads fully (all of `globex`'s pre-snapshot spans recover), the WAL replay reads the single torn line, recognises it as the last line with no trailing newline, drops it, and recovers cleanly with exactly the snapshot state. The binary starts. A `GET /api/v1/traces/{trace_id}` for any span that was in the snapshot returns it. The torn span (the one that was being appended when the crash hit, never acked to the durable WAL) is absent, which is correct: it was never durably acked. Stderr shows `event="wal.recovery.torn_tail_dropped" pillar="ray" line=1 dropped_bytes=42`.

#### 3. Error / boundary: cinder REFUSES a mid-file corruption (a bad line followed by valid lines), preserving fail-closed

An operator runs the `kaleidoscope-gateway` binary, which opens the cinder `FileBackedTieringStore` against `pillar_root=/var/lib/kaleidoscope`. The WAL for tenant `initech` has been corrupted in the MIDDLE: line 3 of 5 is a malformed `{"op":"place","tenant":"initech","item":"img-9` followed by a newline, and lines 4 and 5 are valid newline-terminated records. This is NOT a torn tail (line 3 is not the last line, and it ends in a newline). The replay loop hits the parse failure at line 3, sees that it is NOT the last line, and returns `MigrateError::PersistenceFailed { reason: "WAL parse error at line 3: ..." }`. The gateway refuses to start and exits non-zero, exactly as today. The operator sees no torn-tail warning; they see a hard persistence failure that correctly signals real mid-file corruption requiring investigation, not the benign post-crash tear. The cinder module doc now correctly states this behaviour.

### UAT Scenarios (BDD)

Five scenarios. Scenario titles describe WHAT the operator observes, never HOW the replay loop is implemented. Scenarios are written pillar-generically where the behaviour is identical; lumen is the concrete worked pillar for the end-to-end query assertion because its read API (`GET /api/v1/logs`) is the path verifier expectation D04 will black-box assert.

```gherkin
Scenario: Store recovers the intact acked prefix after a kill -9 tore the final WAL record
  Given a lumen-backed store at pillar_root with 10000 durably acked log records for tenant "acme-corp" in its WAL
  And the WAL ends in a torn 11th-batch line "{\"op\":\"ingest\",\"tenant\":\"acme-corp\",\"records\":[{\"body\":\"order 4471 shi" with no trailing newline
  When the operator restarts the binary that opens the store at the same pillar_root
  Then the store opens successfully and the binary binds its listener
  And a query for tenant "acme-corp" over the full time range returns all 10000 acked records in observed_time order
  And none of the 10000 returned records is partial or corrupt
```

```gherkin
Scenario: A structured warning names the dropped torn tail
  Given a store whose WAL ends in a single torn final line with no trailing newline
  When the operator restarts the binary that opens the store
  Then the process stderr contains exactly one WARN event with field event="wal.recovery.torn_tail_dropped"
  And that event names the pillar, the line number of the dropped tail, and the byte length of the dropped tail
  And no other torn-tail warning is emitted
```

```gherkin
Scenario: Recovery succeeds when the only WAL content is the torn tail on top of a snapshot
  Given a ray-backed store whose snapshot holds all pre-snapshot spans for tenant "globex"
  And whose WAL consists of exactly one torn line with no trailing newline and no intact prefix line
  When the operator restarts the binary that opens the store
  Then the store opens successfully
  And every span present in the snapshot is queryable
  And the torn span that was never durably acked is absent
```

```gherkin
Scenario: A mid-file corruption stays fail-closed and refuses to start
  Given a cinder-backed store whose WAL has a malformed line in the middle followed by one or more valid newline-terminated lines
  When the operator restarts the binary that opens the store
  Then the store refuses to open and returns PersistenceFailed naming the offending line number
  And the binary exits non-zero without binding its listener
  And no torn-tail warning is emitted
```

```gherkin
Scenario: A malformed final line that DOES end in a newline stays fail-closed
  Given a store whose WAL ends in a malformed line that is terminated by a trailing newline
  When the operator restarts the binary that opens the store
  Then the store refuses to open and returns PersistenceFailed naming the offending line number
  And no torn-tail warning is emitted
```

### Acceptance Criteria

- [ ] AC-1 (intact-prefix recovery, end-to-end; verifier D04): for a lumen-backed store whose WAL holds N durably acked records followed by one torn final line with no trailing newline, after restart the store opens, the binary binds its listener, and a query over the full time range returns exactly the N acked records (none partial, none corrupt, original order preserved). N is exercised with N >= 1.
- [ ] AC-2 (torn-tail dropped, not repaired): the torn final record is absent from the recovered state. The recovered state is byte-equivalent to what would have been recovered had the torn record never been appended.
- [ ] AC-3 (structured warning): dropping a torn tail emits exactly one `tracing` WARN event whose structured fields include `event="wal.recovery.torn_tail_dropped"`, the pillar identity, the dropped line number, and the dropped byte length. The exact field names are pinned in DESIGN (ADR-0059); the four facts above are mandatory.
- [ ] AC-4 (snapshot-only-plus-torn-tail): a store whose WAL is a single torn line on top of a loaded snapshot opens successfully and recovers exactly the snapshot state, with the torn (never-acked) record absent.
- [ ] AC-5 (NEGATIVE: mid-file corruption stays fail-closed): a WAL with a parse failure on a line that is NOT the last line returns the existing `PersistenceFailed` error naming the offending line number, the binary exits non-zero without binding its listener, and NO torn-tail warning is emitted. This is the explicit guard that the tolerance is narrow.
- [ ] AC-6 (NEGATIVE: newline-terminated malformed final line stays fail-closed): a WAL whose final line is malformed but DOES end in a trailing newline returns `PersistenceFailed` and emits no torn-tail warning. A complete-but-malformed record is not a torn tear and must not be tolerated.
- [ ] AC-7 (cinder doc correction): the cinder module doc at `crates/cinder/src/file_backed.rs:36-38` and the `open` doc comment no longer claim a behaviour the code lacked; they describe the actual behaviour (torn final line with no trailing newline is dropped with a warning; every other parse failure is surfaced as `PersistenceFailed`). Verified by reading the doc against AC-1 through AC-6.
- [ ] AC-8 (no trait change): `LogStore`, `TraceStore`, `TieringStore`, and (if pulse is confirmed in scope) `MetricStore` trait signatures are byte-identical to the prior tag, verified by Gate 2 (`cargo public-api`).
- [ ] AC-9 (scope coverage): the new behaviour holds for lumen, ray, and cinder. Whether it also covers pulse in this slice is resolved by DESIGN (FLAG 1); if pulse is included, AC-1 through AC-6 hold for pulse too; if deferred, the deferral is recorded with its own follow-up item.
- [ ] AC-10 (mutation kill rate): `cargo mutants` scoped to the modified files in each in-scope pillar passes at 100% kill rate (ADR-0005 Gate 5), with the three guard conditions (is-last-line, no-trailing-newline, parse-failed) and the warning emission as primary targets. Covered by each pillar's existing `gate-5-mutants-*` job via `--in-diff`; no new CI job.

### Outcome KPIs

See `outcome-kpis.md` for the full table.

- **Who**: operators restarting a file-backed Kaleidoscope collector or read-API binary after an abrupt process death (`kill -9`, OOM, power loss, unclean container stop).
- **Does what**: bring the store back up serving every record durably acked before the crash, instead of facing a store that refuses to start because of the benign torn final WAL line a crash leaves behind.
- **By how much**: target = 100% of crashed-with-torn-tail stores recover their intact acked prefix and start successfully across all in-scope pillars; baseline = 0% (today any torn tail blocks the entire open).
- **Measured by**: acceptance tests asserting open-succeeds-and-prefix-queryable across the in-scope pillars (the AC-1 path, which is verifier expectation D04); plus the count of WAL-recovery-refused incidents attributable to a torn tail, which should fall to zero.
- **Baseline**: zero crashed-with-torn-tail stores recover today; a single torn trailing line returns `PersistenceFailed` and blocks the whole open (verified in the four `file_backed.rs` replay loops).

### Technical Notes

- The change is confined to the parse-failure arm of the WAL replay loop on `open` in each in-scope pillar, plus the cinder module and `open` docs. No write path, no snapshot path, no trait signature changes.
- Detection of "torn tail" requires distinguishing the last line from earlier lines AND detecting the absence of a trailing newline. `BufRead::lines()` strips the newline and does not reveal whether the final line had one, so the DESIGN wave must pick a mechanism (read-ahead one line, inspect the final byte of the file, or read the raw tail). This is a DESIGN decision; the requirement only pins the OBSERVABLE behaviour. See FLAG 2 in `wave-decisions.md`.
- The warning rides the existing structured `tracing` `event = "..."` convention (`crates/log-query-api/src/main.rs:76`, `:86`, `:93`). The proposed name `wal.recovery.torn_tail_dropped` mirrors the dotted style of `health.startup.refused`. Final name pinned in ADR-0059. See FLAG 3 in `wave-decisions.md`.
- Whether the torn-tail recovery logic is factored into a shared helper or replicated per pillar is a DESIGN decision. The four pillars have near-identical loops but distinct error types (`LogStoreError`, `TraceStoreError`, `MigrateError`, `MetricStoreError`) and distinct `WalRecord` enums, so a generic helper would need to be parameterised. See FLAG 4 in `wave-decisions.md`.
- The empty-line skip (`if line.is_empty() { continue; }`) already present in all loops is orthogonal and unchanged.
- ADR-0059 (new, recovery-contract change) is authored in DESIGN. It cites ADR-0040 (recovery discipline), ADR-0049 (write-side fsync honesty), ADR-0050 (read-side caps) as the Earned-Trust lineage it extends. ADRs are immutable; ADR-0040 is referenced, not edited.

### Dependencies

- **Resolved**: ADR-0040 (WAL plus snapshot plus replay recovery discipline, the contract this feature refines); ADR-0049 (the write path honours fsync, which makes the torn tail the EXACT residue this feature recovers from); the four existing `file_backed.rs` replay loops (the modification sites); the existing `tracing` subscribers at the read tier and gateway (the warning is observable); each pillar's existing `gate-5-mutants-*` job (mutation coverage via `--in-diff`).
- **Tracked, not blocking**: pulse scope confirmation (FLAG 1) is a DESIGN decision; if pulse is deferred, a follow-up item records it. No external-integration dependency: the change reads the in-process filesystem under `pillar_root`, not a network service.
