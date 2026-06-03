<!-- markdownlint-disable MD024 -->

# User Stories — store-fsync-durability-v0

## Persona

**Priya Nair** — on-call Site Reliability Engineer for a mid-size SaaS
that self-hosts a Kaleidoscope collector to keep its observability data
in-house (AGPL, no vendor egress). Priya runs the collector as a single
container on a bare-metal host with local NVMe. She has been paged at
03:00 because the host lost power when a rack PDU tripped. She restarts
the collector and needs to know: did I lose any telemetry that the
collector told my exporters it had safely stored? Her trust in the
platform's "survives a restart" promise is on the line every time the
host crashes ungracefully. She is technical, reads structured logs, and
will `tail` and `jq` a WAL file if she has to — but she should not have
to.

## The Job (JTBD — Earned-Trust durability)

> When my collector restarts after a power loss or an OS crash, I want it
> to still have every write it acknowledged as durable — and to open
> cleanly even if the crash interrupted a snapshot — so I can trust the
> "survives a restart" promise instead of silently losing acked
> telemetry.

Today the honest answer is: "only if the shutdown was graceful, and only
if no crash hit a snapshot." Every story below traces to closing that
gap (N:1 mapping to this single job).

## System Constraints (cross-cutting, apply to ALL stories)

These are solution-neutral constraints binding every slice. They are NOT
implementation prescriptions; they pin observable behaviour and
guardrails that DESIGN must honour.

- **C1 — No trait signature change.** `LogStore`, `TraceStore`,
  `TieringStore`, `MetricStore`, beacon `RuleStateStore` public
  signatures stay byte-identical to the prior tag. Gate 2
  (`cargo public-api`) enforces this.
- **C2 — Durable means on stable storage, not in the page cache.** A
  write is "acknowledged as durable" only after its bytes are on stable
  storage (the medium survives a power cut), not merely after a
  user-space buffer flush. "Survives a power loss / `kill -9` mid-write"
  is the test of record, not "survives a graceful in-process reopen".
- **C3 — Atomic snapshot: whole or absent, never torn.** A snapshot read
  by the next `open()` is either the complete previous snapshot or the
  complete new one — a crash mid-snapshot never leaves a partially
  written file at the canonical path that `open()` would then fail to
  parse.
- **C4 — Reuse the proven seam.** The fsync discipline reuses the
  `FsyncBackend` seam, `RealFsyncBackend`, `LyingFsyncBackend`, and the
  `event=health.startup.refused` / `substrate=<descriptor>` refusal
  vocabulary already proven in pulse under ADR-0049 §6/§7. No new event
  name, no new dashboard.
- **C5 — The proving test is out-of-process, not `fork()`-in-tokio.**
  The crash is simulated by a real child PROCESS that is killed
  (`kill -9` / `SIGKILL`) after an ack, then the parent reopens. It does
  NOT `fork()` inside a tokio runtime (ADR-0049 §3 rejected that as
  unsafe; ADR-0049 §3 RESERVED an out-of-process true crash test as the
  documented escalation — this feature is that escalation).
- **C6 — The proving test asserts a deterministic invariant, not a
  timing threshold.** It asserts "the acked record is present after
  reopen and the store opens cleanly", never a wall-clock p95. It must
  not flake under load.
- **C7 — Pairs with ADR-0059, does not duplicate it.** The torn-tail
  recovery (ADR-0059) is the read-back that tolerates the torn WAL tail
  this feature's fsync discipline produces. These stories add the
  WRITE-side fsync and the atomic SNAPSHOT; they do not re-implement
  torn-tail recovery. Where a slice's store is not yet covered by
  ADR-0059's recovery routine (sluice, strata), DESIGN reconciles the
  ordering; the proving test still asserts the acked-prefix-present
  outcome.
- **C8 — No WAL format change.** No checksums, no length prefixes; the
  WAL stays human-readable NDJSON (ADR-0059 alt B/C rejected format
  changes for the same reasons).

---

## US-01: lumen survives a power loss with every acked log intact (WALKING SKELETON)

### Elevator Pitch

- **Before**: Priya's collector loses power mid-ingest; on restart, log
  records her exporter received a `200 OK` for are silently gone (lumen
  called only `wal.flush()`, so the bytes were in the page cache, not on
  disk) — or worse, the store refuses to open because a half-written
  snapshot is torn at the path `open()` reads.
- **After**: Priya restarts the collector and runs
  `curl http://localhost:8080/api/v1/logs?tenant=acme&from=...&to=...`;
  the response body contains the exact log record her exporter was acked
  for before the power cut, and the collector started cleanly even though
  the crash hit during a snapshot.
- **Decision enabled**: Priya decides she can trust the collector's
  `200 OK` as a durability promise — she does NOT need to re-send the
  last batch from her exporters' dead-letter queue, and she does NOT need
  to manually repair a torn snapshot file before restarting.

### Problem

Priya is an on-call SRE who self-hosts a Kaleidoscope collector on a
bare-metal host. When a PDU trips and the host loses power mid-ingest,
she finds it impossible to know whether acked log telemetry survived,
because lumen acknowledges a write as durable after only
`BufWriter::flush()` (`crates/lumen/src/file_backed.rs:281`), which
leaves the bytes in the kernel page cache — lost on power failure. Her
workaround is to treat every ungraceful restart as a total-data-loss
event and replay from her exporters' buffers, which she cannot always do.

### Who

- On-call SRE | self-hosted collector on bare-metal with local disk |
  motivated to trust the "survives a restart" promise rather than
  defensively replay after every crash.

### Solution

Bring the proven pulse fsync discipline (ADR-0049) to lumen: call
`sync_all` per WAL record on append, and write the snapshot atomically
(write to a temp file, fsync it, rename onto the canonical path, fsync
the parent directory). Then PROVE it with an out-of-process kill-9 crash
test that demonstrates the acked record survives — the test the current
same-process suite structurally cannot express.

### Domain Examples

#### 1: Happy path — acked log survives a power cut

Priya's `acme` tenant exporter sends a log batch containing the line
`"2026-06-03T03:14:07Z payment-svc ERROR connection pool exhausted"`.
The collector returns `200 OK`. 40ms later the host loses power
(simulated: the collector child process is `SIGKILL`ed after the ack
returns). Priya restarts the collector; `open()` succeeds;
`GET /api/v1/logs?tenant=acme` returns the `connection pool exhausted`
line. Zero acked records lost.

#### 2: Mid-snapshot crash — store still opens

lumen is mid-snapshot (it has written part of the `.snapshot` file) when
the host crashes. Today this leaves a torn JSON file at the snapshot path
and the next `open()` fails with `PersistenceFailed { reason: "parse:
..." }` — total loss. With atomic snapshot, the crash leaves either the
old complete `.snapshot` or the new complete one (the temp file's partial
write is never renamed into place), so `open()` succeeds and serves the
last consistent state. Example: tenant `acme` had 10,000 records
snapshotted; the crash hits while snapshotting the 10,001st-batch state;
on reopen the store opens to the 10,000-record snapshot plus any durable
WAL tail.

#### 3: Error/boundary — torn WAL tail from the crash is recovered, not refused

The power cut interrupts lumen mid-WAL-append: the final line is a
partial record with no trailing newline (the exact residue per-record
fsync + append-then-newline produces; ADR-0059 §Context). On reopen,
lumen recovers the intact acked prefix and drops only the torn,
never-acked tail, emitting `event="wal.recovery.torn_tail_dropped"
pillar="lumen"`. The record Priya was acked for (which had its newline
and fsync) is in the prefix and is returned by `GET /api/v1/logs`; the
torn tail (which was never acked) is correctly absent.

### UAT Scenarios (BDD)

#### Scenario: An acked log survives a power loss and is queryable after restart

```gherkin
Given Priya's collector is running with lumen as the log store
And her "acme" exporter sends a log batch containing "connection pool exhausted"
And the collector returns 200 OK for that batch
When the collector process is killed with SIGKILL before any graceful shutdown
And Priya restarts the collector
And she queries GET /api/v1/logs?tenant=acme for the crash window
Then the response body contains the "connection pool exhausted" record
And no acked record is missing from the response
```

#### Scenario: The store opens cleanly after a crash during a snapshot

```gherkin
Given lumen is part-way through writing a snapshot file
When the collector process is killed with SIGKILL mid-snapshot
And Priya restarts the collector
Then the lumen store opens successfully without a parse error
And the store serves the last consistent snapshot state
And no torn snapshot file blocks the open
```

#### Scenario: A never-acked torn WAL tail is dropped, the acked prefix is kept

```gherkin
Given the SIGKILL interrupted lumen mid-WAL-append leaving a torn final line
And the last fully-acked record precedes the torn line
When Priya restarts the collector
Then lumen recovers the intact acked prefix
And lumen drops only the torn final line
And lumen emits event="wal.recovery.torn_tail_dropped" with pillar="lumen"
And GET /api/v1/logs returns every acked record and not the torn tail
```

#### Scenario: The collector refuses to start on a substrate that lies about fsync

```gherkin
Given lumen's composition root runs the fsync-honesty probe at startup
And the underlying substrate silently discards fsync (a lying substrate)
When the collector starts
Then the collector emits event="health.startup.refused" with a substrate descriptor
And the collector exits non-zero without binding its listener
And no write is ever acked against a substrate proven to lie about durability
```

#### Scenario: A graceful restart still recovers everything (regression guard)

```gherkin
Given the collector has acked a batch of log records for tenant "acme"
When the collector is shut down gracefully and restarted
Then GET /api/v1/logs?tenant=acme returns every acked record
And no torn-tail warning is emitted
```

### Acceptance Criteria

- [ ] A log record acked (`200 OK`) before a `SIGKILL` of the collector
      process is present in `GET /api/v1/logs` after restart.
- [ ] No acked log record is lost across a `SIGKILL`-then-reopen cycle.
- [ ] lumen `open()` succeeds after a `SIGKILL` that interrupted a
      snapshot write; no torn snapshot file at the canonical path blocks
      the open.
- [ ] A torn WAL tail produced by the crash is dropped (not repaired,
      not refused) and the acked prefix is recovered, emitting
      `event="wal.recovery.torn_tail_dropped" pillar="lumen"`.
- [ ] On a substrate that lies about fsync, the collector emits
      `event="health.startup.refused"` with a `substrate=<descriptor>`
      field and exits non-zero without binding the listener.
- [ ] **The proving test exists and is out-of-process**: a CI test
      spawns the store/collector as a child process, acks a write, sends
      it `SIGKILL`, reopens in the parent, and asserts the acked write is
      present and the store opens — with a mid-write variant and a
      mid-snapshot variant. It does NOT `fork()` inside a tokio runtime
      (C5) and asserts a deterministic invariant, not a timing threshold
      (C6).
- [ ] `LogStore` trait signature is byte-identical to the prior tag (C1).

### Outcome KPIs

- **Who**: lumen log stores that experienced an ungraceful crash
  (`SIGKILL` / power loss) with at least one acked-but-unsnapshotted
  write.
- **Does what**: recover every acked log record on reopen and open
  cleanly.
- **By how much**: 100% of crashed-with-acked-tail lumen stores recover
  their acked prefix and open (from a baseline of 0% — today an
  ungraceful crash silently loses page-cached writes and a mid-snapshot
  crash refuses to open).
- **Measured by**: the out-of-process kill-9 proving test in CI
  (acked-write-present assertion + store-opens assertion), reported as a
  pass/fail gate per run.
- **Baseline**: 0% provable — no test in the suite simulates an
  out-of-process crash; the 1194 green tests all reopen in-process.

### Technical Notes

- Reuses `FsyncBackend` / `RealFsyncBackend` / `LyingFsyncBackend` and
  the `fsync_probe` surface proven in pulse (ADR-0049 §6). lumen is the
  first successor pillar ADR-0049 §8 named.
- The atomic snapshot (temp+rename+fsync-parent) is the gap ADR-0049
  left open even in pulse; lumen gets it from the start.
- Pairs with ADR-0059 torn-tail recovery, which already covers lumen;
  this slice produces the genuine torn tail that recovery reads back.
- Probe WIRING lives at lumen's composition root (the gateway, which
  opens `FileBackedLogStore`); probe LOGIC reuses the pulse surface
  (DESIGN decides whether to lift it to a shared crate now or per ADR-0049
  §8's "successor slices will decide").
- Dependencies: ADR-0049 (FsyncBackend seam — landed), ADR-0059
  (lumen torn-tail recovery — landed). Both resolved.

---

## US-02: ray survives a power loss with every acked trace span intact

### Elevator Pitch

- **Before**: After a power cut, ray (traces) silently loses spans the
  collector acked, because its WAL append calls only `wal.flush()`
  (`crates/ray/src/file_backed.rs:392`), and a mid-snapshot crash leaves
  a torn `.snapshot` that refuses to open (`:171`).
- **After**: Priya restarts and runs
  `curl http://localhost:8080/api/v1/traces?trace_id=...`; the acked
  span from before the crash is in the response, and ray opened cleanly.
- **Decision enabled**: Priya trusts that a trace she saw acked is
  durable, so she investigates the incident from the collector's own data
  instead of assuming the trace was lost in the crash.

### Problem

Priya cannot trust that acked trace spans survive an ungraceful restart:
ray acknowledges durability after only `BufWriter::flush()`, leaving span
bytes in the page cache, and writes its snapshot with `File::create`
straight onto the canonical path, so a mid-snapshot crash tears the file
and blocks `open()`.

### Who

- On-call SRE | self-hosted collector | needs acked trace spans to
  survive a crash so post-incident analysis uses real collected data.

### Solution

Apply the proven lumen/pulse pattern to ray: `sync_all` per WAL record,
atomic snapshot (tmp+rename+fsync-parent), and an out-of-process kill-9
proving test on ray's read path.

### Domain Examples

#### 1: Happy path — acked span survives a crash

Tenant `acme` sends a span `trace_id=4bf92f, span_id=00f0, name="POST
/checkout"`; collector acks; host loses power 30ms later (child process
`SIGKILL`ed post-ack). On restart,
`GET /api/v1/traces?trace_id=4bf92f` returns the `POST /checkout` span.

#### 2: Mid-snapshot crash — ray opens

ray crashes while writing its `.snapshot`. With atomic snapshot, reopen
finds either the old or the new complete snapshot — never the torn one —
and `open()` succeeds.

#### 3: Boundary — multi-span trace, only acked spans present

A trace with 5 spans is acked; a 6th span's batch is sent but the crash
hits before its ack returns. On reopen, the 5 acked spans are present;
the 6th (never acked, torn tail) is correctly absent.

### UAT Scenarios (BDD)

#### Scenario: An acked trace span survives a power loss

```gherkin
Given Priya's collector is running with ray as the trace store
And her "acme" exporter sends a span for trace_id "4bf92f" named "POST /checkout"
And the collector returns 200 OK for that span
When the collector process is killed with SIGKILL
And Priya restarts the collector
And she queries GET /api/v1/traces?trace_id=4bf92f
Then the response contains the "POST /checkout" span
And no acked span is missing
```

#### Scenario: ray opens cleanly after a crash during a snapshot

```gherkin
Given ray is part-way through writing a snapshot file
When the collector process is killed with SIGKILL mid-snapshot
And Priya restarts the collector
Then the ray store opens successfully without a parse error
And it serves the last consistent snapshot state
```

#### Scenario: Only acked spans are recovered after a torn-tail crash

```gherkin
Given a trace had 5 acked spans and a 6th span whose ack never returned
And the SIGKILL left the 6th span as a torn WAL tail
When Priya restarts the collector
Then GET /api/v1/traces returns the 5 acked spans
And the 6th never-acked span is absent
And ray emits event="wal.recovery.torn_tail_dropped" with pillar="ray"
```

#### Scenario: ray refuses to start on a substrate that lies about fsync

```gherkin
Given ray's composition root runs the fsync-honesty probe at startup
And the substrate silently discards fsync
When the collector starts
Then it emits event="health.startup.refused" with a substrate descriptor
And it exits non-zero without binding its listener
```

### Acceptance Criteria

- [ ] An acked span survives a `SIGKILL`-then-reopen and is returned by
      `GET /api/v1/traces`.
- [ ] ray `open()` succeeds after a mid-snapshot `SIGKILL`; no torn
      snapshot blocks open.
- [ ] A torn WAL tail is dropped, the acked prefix recovered, with
      `event="wal.recovery.torn_tail_dropped" pillar="ray"`.
- [ ] On a lying-fsync substrate, ray emits
      `event="health.startup.refused"` and exits non-zero without
      binding.
- [ ] The out-of-process kill-9 proving test (mid-write + mid-snapshot
      variants) exists for ray and asserts a deterministic invariant
      (C5, C6).
- [ ] `TraceStore` signature byte-identical to prior tag (C1).

### Outcome KPIs

- **Who**: ray trace stores that crashed ungracefully with acked spans.
- **Does what**: recover every acked span and open cleanly on reopen.
- **By how much**: 100% (baseline 0%).
- **Measured by**: ray's out-of-process kill-9 proving test in CI.
- **Baseline**: 0% provable.

### Technical Notes

- Reuses the seam proven in US-01. Depends on US-01 (proves the crash
  mechanism). ADR-0059 covers ray's torn-tail recovery (landed).

---

## US-03: strata profile store survives a power loss with acked profiles intact

### Elevator Pitch

- **Before**: After a crash, strata silently loses acked profile data
  (`wal.flush()` only, `crates/strata/src/file_backed.rs:333`) and a
  mid-snapshot crash tears its `.snapshot` (`:170`), refusing to open.
- **After**: Priya restarts the collector and strata `open()` succeeds,
  with every acked profile present in the recovered state.
- **Decision enabled**: Priya trusts the collector came back whole and
  does not quarantine the strata pillar as suspect after a crash.

### Problem

strata acknowledges acked profile writes as durable after only a buffer
flush and writes its snapshot non-atomically, so an ungraceful crash
silently loses page-cached profiles and a mid-snapshot crash blocks
`open()` entirely.

### Who

- On-call SRE | self-hosted collector | needs the profile pillar to
  recover its acked state after a crash, not refuse to open.

### Solution

Apply the proven pattern to strata: `sync_all` per WAL record, atomic
snapshot, and an out-of-process kill-9 proving test on strata's reopen
path (strata has no HTTP read path; the outcome is observed at `open()`
and via the store's query API in-process after the child is killed).

### Domain Examples

#### 1: Happy path — acked profile survives a crash

A profile record for `tenant=acme, service=payment-svc` is acked; the
host loses power; on reopen strata's state contains that profile.

#### 2: Mid-snapshot crash — strata opens

strata crashes mid-snapshot; atomic snapshot means reopen finds a whole
snapshot (old or new) and `open()` succeeds.

#### 3: Boundary — empty store survives a crash before any write

The collector starts, strata has an empty WAL and no snapshot, the host
crashes before any profile is acked; on reopen strata opens to an empty
store (no spurious parse error from a zero-byte or absent file).

### UAT Scenarios (BDD)

#### Scenario: An acked profile survives a power loss

```gherkin
Given the collector is running with strata as the profile store
And a profile for tenant "acme" service "payment-svc" is acked
When the collector process is killed with SIGKILL
And the strata store is reopened
Then the recovered state contains the acked profile for "payment-svc"
And no acked profile is missing
```

#### Scenario: strata opens cleanly after a crash during a snapshot

```gherkin
Given strata is part-way through writing a snapshot file
When the process is killed with SIGKILL mid-snapshot
And strata is reopened
Then strata opens successfully without a parse error
And it serves the last consistent snapshot state
```

#### Scenario: An empty strata store opens after a crash before any write

```gherkin
Given strata has no snapshot and an empty WAL
When the process is killed with SIGKILL before any profile is acked
And strata is reopened
Then strata opens successfully as an empty store
And no parse error is raised
```

#### Scenario: strata refuses to start on a substrate that lies about fsync

```gherkin
Given strata's composition root runs the fsync-honesty probe at startup
And the substrate silently discards fsync
When the collector starts
Then it emits event="health.startup.refused" with a substrate descriptor
And it exits non-zero without opening the store for writes
```

### Acceptance Criteria

- [ ] An acked profile survives a `SIGKILL`-then-reopen and is present
      in strata's recovered state.
- [ ] strata `open()` succeeds after a mid-snapshot `SIGKILL`; no torn
      snapshot blocks open.
- [ ] An empty strata store opens cleanly after a pre-write crash.
- [ ] On a lying-fsync substrate, strata's composition root emits
      `event="health.startup.refused"` and refuses to proceed.
- [ ] The out-of-process kill-9 proving test (mid-write + mid-snapshot)
      exists for strata, deterministic (C5, C6).
- [ ] `ProfileStore` (strata) signature byte-identical to prior tag (C1).

### Outcome KPIs

- **Who**: strata profile stores that crashed ungracefully with acked
  profiles.
- **Does what**: recover acked profiles and open cleanly on reopen.
- **By how much**: 100% (baseline 0%).
- **Measured by**: strata's out-of-process kill-9 proving test in CI.
- **Baseline**: 0% provable.

### Technical Notes

- strata is NOT covered by ADR-0059's torn-tail recovery routine yet
  (ADR-0059 §5 lists it out of that slice). DESIGN reconciles whether
  this slice also extends torn-tail recovery to strata (one `apply`-closure
  addition per ADR-0059 §5) or asserts the acked-prefix outcome without
  it. Flagged as a dependency to resolve in DESIGN. Depends on US-01.

---

## US-04: cinder tiering store survives a power loss with acked migrations intact

### Elevator Pitch

- **Before**: After a crash, cinder silently loses acked tiering/migration
  state (`wal.flush()` only, `crates/cinder/src/file_backed.rs:383`) and a
  mid-snapshot crash tears its `.snapshot` (`:207`). Worse, cinder's doc
  historically over-claimed robustness it lacked (corrected under
  ADR-0059 §6).
- **After**: Priya restarts and cinder `open()` succeeds with every acked
  migration record recovered.
- **Decision enabled**: Priya trusts the tiering ledger is intact after a
  crash and does not manually reconcile tier placement.

### Problem

cinder acknowledges acked migration writes after only a buffer flush and
snapshots non-atomically, so an ungraceful crash silently loses
page-cached tiering state and a mid-snapshot crash blocks `open()`.

### Who

- On-call SRE | self-hosted collector | needs the tiering ledger to
  recover its acked state after a crash.

### Solution

Apply the proven pattern to cinder: `sync_all` per WAL record, atomic
snapshot, out-of-process kill-9 proving test.

### Domain Examples

#### 1: Happy path — acked migration survives a crash

A migration record moving `tenant=acme` block `blk-7781` from hot to warm
tier is acked; the host crashes; on reopen cinder's ledger contains the
migration.

#### 2: Mid-snapshot crash — cinder opens

cinder crashes mid-snapshot; atomic snapshot means reopen finds a whole
snapshot and `open()` succeeds (and the doc now matches the behaviour,
per ADR-0059 §6).

#### 3: Boundary — torn migration tail dropped

The crash leaves a torn final migration line; cinder recovers the acked
prefix and drops only the torn tail, emitting
`event="wal.recovery.torn_tail_dropped" pillar="cinder"`.

### UAT Scenarios (BDD)

#### Scenario: An acked migration survives a power loss

```gherkin
Given the collector is running with cinder as the tiering store
And a migration of block "blk-7781" from hot to warm tier is acked
When the collector process is killed with SIGKILL
And cinder is reopened
Then the recovered ledger contains the "blk-7781" hot-to-warm migration
And no acked migration is missing
```

#### Scenario: cinder opens cleanly after a crash during a snapshot

```gherkin
Given cinder is part-way through writing a snapshot file
When the process is killed with SIGKILL mid-snapshot
And cinder is reopened
Then cinder opens successfully without a parse error
And it serves the last consistent snapshot state
```

#### Scenario: A torn migration tail is dropped, the acked prefix kept

```gherkin
Given the SIGKILL left a torn final migration line in cinder's WAL
And the last acked migration precedes the torn line
When cinder is reopened
Then cinder recovers the acked prefix
And drops only the torn tail
And emits event="wal.recovery.torn_tail_dropped" with pillar="cinder"
```

#### Scenario: cinder refuses to start on a substrate that lies about fsync

```gherkin
Given cinder's composition root runs the fsync-honesty probe at startup
And the substrate silently discards fsync
When the collector starts
Then it emits event="health.startup.refused" with a substrate descriptor
And it refuses to open the store for writes
```

### Acceptance Criteria

- [ ] An acked migration survives a `SIGKILL`-then-reopen and is present
      in cinder's recovered ledger.
- [ ] cinder `open()` succeeds after a mid-snapshot `SIGKILL`.
- [ ] A torn migration tail is dropped, acked prefix recovered, with
      `event="wal.recovery.torn_tail_dropped" pillar="cinder"`.
- [ ] On a lying-fsync substrate, cinder emits
      `event="health.startup.refused"` and refuses.
- [ ] The out-of-process kill-9 proving test (mid-write + mid-snapshot)
      exists for cinder, deterministic (C5, C6).
- [ ] `TieringStore` signature byte-identical to prior tag (C1).

### Outcome KPIs

- **Who**: cinder tiering stores that crashed ungracefully with acked
  migrations.
- **Does what**: recover acked migrations and open cleanly on reopen.
- **By how much**: 100% (baseline 0%).
- **Measured by**: cinder's out-of-process kill-9 proving test in CI.
- **Baseline**: 0% provable.

### Technical Notes

- cinder IS covered by ADR-0059's torn-tail recovery (landed). Depends
  on US-01.

---

## US-05: sluice queue store survives a power loss with acked enqueues intact

### Elevator Pitch

- **Before**: After a crash, sluice silently loses acked queue enqueues
  (`wal.flush()` only, `crates/sluice/src/file_backed.rs:391`) and a
  mid-snapshot crash tears its `.snapshot` (`:243`).
- **After**: Priya restarts and sluice `open()` succeeds with every acked
  enqueue recovered and dequeuable.
- **Decision enabled**: Priya trusts the queue did not silently drop work
  items across the crash and does not re-drive upstream producers.

### Problem

sluice acknowledges acked enqueues after only a buffer flush and
snapshots non-atomically; an ungraceful crash silently loses page-cached
queue items and a mid-snapshot crash blocks `open()`. sluice's replay
applies records through a FALLIBLE `apply_record`
(`crates/sluice/src/file_backed.rs:176`, `?`-propagated), unlike the
other stores' infallible apply — a nuance DESIGN must honour.

### Who

- On-call SRE | self-hosted collector | needs the durable queue to
  recover its acked items after a crash so no enqueued work is silently
  lost.

### Solution

Apply the proven pattern to sluice: `sync_all` per WAL record, atomic
snapshot, out-of-process kill-9 proving test. The fsync addition is on
`append_wal`, orthogonal to apply fallibility; DESIGN reconciles the
fallible-apply seam if torn-tail recovery is extended here.

### Domain Examples

#### 1: Happy path — acked enqueue survives a crash

An enqueue of work item `job-5521` is acked; the host crashes; on reopen
sluice's queue contains `job-5521`, dequeuable.

#### 2: Mid-snapshot crash — sluice opens

sluice crashes mid-snapshot; atomic snapshot means reopen finds a whole
snapshot and `open()` succeeds.

#### 3: Boundary — in-flight item not lost

`job-5521` was dequeued-but-not-acked-complete (in-flight) when the crash
hit; on reopen the in-flight item is recovered to its pre-crash state, not
silently dropped.

### UAT Scenarios (BDD)

#### Scenario: An acked enqueue survives a power loss

```gherkin
Given the collector is running with sluice as the queue store
And work item "job-5521" is enqueued and acked
When the collector process is killed with SIGKILL
And sluice is reopened
Then the recovered queue contains "job-5521"
And no acked enqueue is missing
```

#### Scenario: sluice opens cleanly after a crash during a snapshot

```gherkin
Given sluice is part-way through writing a snapshot file
When the process is killed with SIGKILL mid-snapshot
And sluice is reopened
Then sluice opens successfully without a parse error
And it serves the last consistent snapshot state
```

#### Scenario: An in-flight item is recovered after a crash

```gherkin
Given "job-5521" was dequeued and in-flight when the crash hit
When the process is killed with SIGKILL
And sluice is reopened
Then "job-5521" is recovered to its pre-crash in-flight state
And it is not silently dropped
```

#### Scenario: sluice refuses to start on a substrate that lies about fsync

```gherkin
Given sluice's composition root runs the fsync-honesty probe at startup
And the substrate silently discards fsync
When the collector starts
Then it emits event="health.startup.refused" with a substrate descriptor
And it refuses to open the store for writes
```

### Acceptance Criteria

- [ ] An acked enqueue survives a `SIGKILL`-then-reopen and is present
      and dequeuable in sluice's recovered queue.
- [ ] sluice `open()` succeeds after a mid-snapshot `SIGKILL`.
- [ ] An in-flight item is recovered to its pre-crash state, not dropped.
- [ ] On a lying-fsync substrate, sluice emits
      `event="health.startup.refused"` and refuses.
- [ ] The out-of-process kill-9 proving test (mid-write + mid-snapshot)
      exists for sluice, deterministic (C5, C6).
- [ ] `sluice` queue store signature byte-identical to prior tag (C1).

### Outcome KPIs

- **Who**: sluice queue stores that crashed ungracefully with acked
  enqueues.
- **Does what**: recover acked enqueues (and in-flight items) and open
  cleanly on reopen.
- **By how much**: 100% (baseline 0%).
- **Measured by**: sluice's out-of-process kill-9 proving test in CI.
- **Baseline**: 0% provable.

### Technical Notes

- sluice's `apply_record` is fallible (ADR-0059 §5). If torn-tail
  recovery is extended to sluice in this slice, DESIGN must provide the
  fallible-`apply` seam variant ADR-0059 §5 describes. The fsync
  addition on `append_wal` is independent of this. Depends on US-01.

---

## US-06: beacon rule-state store survives a power loss with acked rule state intact

### Elevator Pitch

- **Before**: After a crash, beacon's rule-state store silently loses
  acked rule-state transitions (`wal.flush()` only,
  `crates/beacon/src/state_store.rs:334`) and a mid-snapshot crash tears
  its `.snapshot` (`:259`).
- **After**: Priya restarts and the rule-state store `open()` succeeds
  with every acked rule-state transition recovered.
- **Decision enabled**: Priya trusts that alerting rules resume in their
  last acked state after a crash and does not manually re-arm rules.

### Problem

beacon's rule-state store acknowledges acked transitions after only a
buffer flush and snapshots non-atomically; an ungraceful crash silently
loses page-cached rule state and a mid-snapshot crash blocks `open()`.

### Who

- On-call SRE | self-hosted collector | needs alerting rule state to
  resume in its last acked state after a crash, not reset or refuse.

### Solution

Apply the proven pattern to beacon's rule-state store: `sync_all` per WAL
record, atomic snapshot, out-of-process kill-9 proving test.

### Domain Examples

#### 1: Happy path — acked rule transition survives a crash

Rule `r-payment-latency` transitions to `firing` and the transition is
acked; the host crashes; on reopen the store has `r-payment-latency` in
`firing`.

#### 2: Mid-snapshot crash — store opens

The store crashes mid-snapshot; atomic snapshot means reopen finds a
whole snapshot and `open()` succeeds.

#### 3: Boundary — torn transition tail dropped

The crash leaves a torn final transition line; the store recovers the
acked prefix and drops only the torn, never-acked tail.

### UAT Scenarios (BDD)

#### Scenario: An acked rule-state transition survives a power loss

```gherkin
Given the collector is running with beacon's rule-state store
And rule "r-payment-latency" transitions to "firing" and is acked
When the collector process is killed with SIGKILL
And the rule-state store is reopened
Then the recovered state has "r-payment-latency" in "firing"
And no acked transition is missing
```

#### Scenario: The rule-state store opens cleanly after a crash during a snapshot

```gherkin
Given the rule-state store is part-way through writing a snapshot
When the process is killed with SIGKILL mid-snapshot
And the store is reopened
Then it opens successfully without a parse error
And it serves the last consistent snapshot state
```

#### Scenario: A torn transition tail is dropped, the acked prefix kept

```gherkin
Given the SIGKILL left a torn final transition line in the WAL
And the last acked transition precedes the torn line
When the store is reopened
Then the store recovers the acked prefix
And drops only the torn tail
```

#### Scenario: The store refuses to start on a substrate that lies about fsync

```gherkin
Given the rule-state store's composition root runs the fsync-honesty probe
And the substrate silently discards fsync
When the collector starts
Then it emits event="health.startup.refused" with a substrate descriptor
And it refuses to open the store for writes
```

### Acceptance Criteria

- [ ] An acked rule-state transition survives a `SIGKILL`-then-reopen and
      is present in the recovered state.
- [ ] The store `open()` succeeds after a mid-snapshot `SIGKILL`.
- [ ] A torn transition tail is dropped, the acked prefix recovered.
- [ ] On a lying-fsync substrate, the store emits
      `event="health.startup.refused"` and refuses.
- [ ] The out-of-process kill-9 proving test (mid-write + mid-snapshot)
      exists for beacon's rule-state store, deterministic (C5, C6).
- [ ] `RuleStateStore` signature byte-identical to prior tag (C1).

### Outcome KPIs

- **Who**: beacon rule-state stores that crashed ungracefully with acked
  transitions.
- **Does what**: recover acked rule-state transitions and open cleanly.
- **By how much**: 100% (baseline 0%).
- **Measured by**: beacon rule-state store's out-of-process kill-9
  proving test in CI.
- **Baseline**: 0% provable.

### Technical Notes

- beacon's rule-state store is NOT in ADR-0059's torn-tail recovery slice;
  DESIGN reconciles whether to extend it. ADR-0040 governs the rule-state
  store seam. Depends on US-01.

---

## US-07: pulse metric snapshot is atomic — a mid-snapshot crash no longer destroys the store

### Elevator Pitch

- **Before**: pulse's WAL is already crash-durable (ADR-0049,
  `sync_all` per record), but its snapshot still uses `File::create`
  straight onto the canonical path (`crates/pulse/src/file_backed.rs:257`).
  A crash mid-snapshot leaves a torn `.snapshot` file; the next `open()`
  fails to parse it and the whole metric store refuses to open — total
  loss — despite pulse's per-record fsync, because pulse's per-file
  `sync_all` cannot save a file that is itself torn.
- **After**: Priya restarts after a mid-snapshot crash and pulse `open()`
  succeeds; `GET /api/v1/metrics` serves the last consistent metric state.
- **Decision enabled**: Priya trusts the metrics pillar survives a crash
  that lands during its periodic snapshot, and does not lose her entire
  metric history to one badly-timed power cut.

### Problem

pulse closed its WAL fsync gap under ADR-0049 but left the
snapshot-atomicity gap open: the snapshot is written non-atomically, so a
crash midway through a snapshot tears the file at the path `open()` reads,
and the entire metric store refuses to open. This is the one durability
residue ADR-0049 explicitly did not close in its own pillar.

### Who

- On-call SRE | self-hosted collector | needs the metrics pillar to
  survive a crash during its periodic snapshot, not lose all metric
  history.

### Solution

Make pulse's snapshot atomic: write to a temp file, fsync it, rename onto
the canonical path, fsync the parent directory. Reuse pulse's existing
`FsyncBackend.fsync_file` / `fsync_dir` (already present from ADR-0049).
Prove it with an out-of-process kill-9 mid-snapshot test. WAL is
unchanged (already `sync_all`-per-record).

### Domain Examples

#### 1: Happy path — pulse opens after a mid-snapshot crash

pulse has 50,000 metric points snapshotted; it is mid-snapshot when the
host loses power; on reopen pulse `open()` succeeds and serves the last
consistent snapshot plus the durable WAL tail —
`GET /api/v1/metrics?tenant=acme&metric=http_requests_total` returns the
acked series.

#### 2: Boundary — temp snapshot file never becomes canonical

The crash hits after the temp file is partly written but before the
rename; on reopen the canonical `.snapshot` is the previous complete one
(the partial temp file is ignored / cleaned), and `open()` succeeds.

#### 3: Boundary — crash exactly at the rename

The crash hits at the rename boundary; rename is atomic on POSIX, so the
canonical path points at either the old or the new whole file — never a
torn one — and `open()` succeeds either way.

### UAT Scenarios (BDD)

#### Scenario: pulse opens cleanly after a crash during a snapshot

```gherkin
Given pulse has 50,000 acked metric points and is mid-snapshot
When the collector process is killed with SIGKILL mid-snapshot
And Priya restarts the collector
Then the pulse store opens successfully without a parse error
And GET /api/v1/metrics serves the last consistent snapshot state
```

#### Scenario: A partially-written temp snapshot never becomes the canonical file

```gherkin
Given pulse has written part of a temp snapshot file but not yet renamed it
When the process is killed with SIGKILL before the rename
And pulse is reopened
Then the canonical snapshot is the previous complete snapshot
And pulse opens successfully
And no torn file blocks the open
```

#### Scenario: A crash at the rename boundary still opens to a whole snapshot

```gherkin
Given pulse is at the atomic rename boundary of its snapshot
When the process is killed with SIGKILL at that boundary
And pulse is reopened
Then the canonical snapshot is either the old or the new complete file
And pulse opens successfully in both cases
```

#### Scenario: Acked metrics written after the snapshot also survive (regression)

```gherkin
Given pulse completed an atomic snapshot
And then acked a metric point for "http_requests_total"
When the process is killed with SIGKILL
And pulse is reopened
Then GET /api/v1/metrics returns the "http_requests_total" point
And the WAL durability from ADR-0049 is preserved
```

### Acceptance Criteria

- [ ] pulse `open()` succeeds after a mid-snapshot `SIGKILL`; no torn
      snapshot file at the canonical path blocks the open.
- [ ] A partially-written temp snapshot is never renamed into the
      canonical path; reopen finds the previous complete snapshot.
- [ ] A crash at the rename boundary leaves the canonical path pointing
      at the old OR new complete file; `open()` succeeds in both cases.
- [ ] Acked metrics written after the snapshot still survive a
      `SIGKILL`-then-reopen (ADR-0049 WAL durability preserved).
- [ ] The out-of-process kill-9 mid-snapshot proving test exists for
      pulse, deterministic (C5, C6).
- [ ] `MetricStore` signature byte-identical to prior tag (C1).

### Outcome KPIs

- **Who**: pulse metric stores that crashed ungracefully during a
  snapshot write.
- **Does what**: open cleanly on reopen and serve the last consistent
  state.
- **By how much**: 100% of mid-snapshot-crashed pulse stores open
  (baseline 0% — today a mid-snapshot crash leaves a torn file that
  refuses to open, losing the entire metric store).
- **Measured by**: pulse's out-of-process kill-9 mid-snapshot proving
  test in CI.
- **Baseline**: 0% provable — pulse's snapshot is non-atomic
  (`File::create` at `:257`) and no test simulates a mid-snapshot crash.

### Technical Notes

- pulse's WAL needs NO change (already `sync_all`-per-record, ADR-0049).
  This slice is snapshot-only.
- Reuses pulse's existing `FsyncBackend.fsync_file` / `fsync_dir`
  (already wired from ADR-0049). The temp+rename+fsync-parent sequence is
  the atomic-snapshot pattern proven in US-01, applied to pulse.
- Closes the snapshot-atomicity gap ADR-0049 §5 left open even in its own
  pillar. Depends on US-01 (atomic-snapshot pattern proven there).
