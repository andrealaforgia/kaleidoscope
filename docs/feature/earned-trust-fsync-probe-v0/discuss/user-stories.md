<!-- markdownlint-disable MD024 -->

# User Stories: earned-trust-fsync-probe-v0

British English. No em dashes. No emoji.

This feature is M-1 from the residuality analysis: a startup-time
fsync-honesty probe that lets the platform refuse to serve over a
substrate that does not honour fsync. The "Earned-Trust" claim made
in ADR-0042 Decision 8, ADR-0047 Decision 6, and ADR-0048 Decision 8
exists in code today as `composition::probe()` (in `log-query-api`
and `trace-query-api`), but it asserts only "open and read", not
"survive a crash via fsync". This feature closes that gap for ONE
pillar's binary as the walking skeleton; later slices extend to the
rest.

The After line of each story references the real entry point: the
chosen pillar's BINARY (the choice of which pillar is FLAGGED to
DESIGN; see wave-decisions.md, FLAG 2), exercised via acceptance
tests that simulate a hostile filesystem through the DESIGN-chosen
seam. The observable output is the bind-or-refuse outcome and the
structured `event=health.startup.refused` event.

## System Constraints

- The probe rides OUTSIDE the storage traits. `LogStore`,
  `MetricStore`, `TraceStore`, and the beacon `RuleStateStore` trait
  signatures are UNCHANGED. The probe lives in the composition root
  (mirroring `crates/log-query-api/src/composition.rs:73` and
  `crates/trace-query-api/src/composition.rs:77`).
- The refusal path reuses the existing
  `event=health.startup.refused` structured event. No new event name,
  no new metric, no new dashboard.
- Slice 01 covers ONE pillar's binary only. The pillar choice is
  FLAGGED to DESIGN (FLAG 2). Later slices extend to the others.
- Slice 01 is the cheapest portable behavioural test: write + fsync
  + drop handle + re-open + verify bytes. This is NOT a true crash
  test (no fork + SIGKILL). The mechanism is FLAGGED to DESIGN
  (FLAG 1).
- FLAGGED to DESIGN, NOT decided here: (1) the probe mechanism;
  (2) the slice-01 pillar; (3) the test seam for injecting a hostile
  substrate; (4) whether a new ADR (likely ADR-0049) refines the
  Earned-Trust discipline of ADR-0042 Decision 8 from "open and read"
  to "honour fsync".
- OUT of scope for slice 01 (deferred and declared): a true crash
  simulation (fork + SIGKILL + re-open); coverage of pillars other
  than the chosen one; automatic detection of overlayfs / tmpfs /
  mount options via `statfs` / `fstatfs`; any change to the storage
  traits; UI or telemetry beyond the existing structured event.
- The exact CLI / env names for any new configuration (probe
  directory override, substrate descriptor in the event payload) are
  DESIGN decisions. Stories phrase the entry point as "the chosen
  pillar's binary, with its existing env posture, plus the fsync
  probe at startup".

## US-01: The platform refuses to serve over a substrate whose fsync is a no-op

### Elevator Pitch

- Before: an operator starts the chosen pillar's binary on a
  container with an accidental overlayfs `pillar_root` (or a tmpfs,
  or a mount whose fsync is configured for "performance" and silently
  no-ops). The existing `composition::probe()` happily calls
  `store.query(...)` and returns `Ok(())` (it only verifies open-
  and-read), the listener binds, and the platform now LIES about
  durability for every write that follows. The very class of
  failure the residuality analysis flagged as A-U1 "silent data
  loss".
- After: the operator starts the same binary against the same lying
  substrate; the new fsync-honesty probe at startup writes a small
  sentinel into `pillar_root`, fsyncs, drops the handle, re-opens the
  file, and the bytes are missing or wrong; the binary refuses to
  bind the listener and emits the existing structured event
  `event=health.startup.refused` naming the substrate; exit code is
  non-zero; observable in the acceptance suite by asserting the
  listener never bound AND the structured event was emitted. The
  Earned-Trust claim is now code, not paper.
- Decision enabled: the operator stops deploying the platform on a
  silently-lying substrate, fixes the mount (host-mounted volume,
  honest filesystem) and restarts; the platform either binds (the
  substrate is honest now) or refuses again (still lying), with no
  in-between of "binds but lies".

### Problem

Ravi Patel is the platform operator for "acme-prod". He runs the
chosen Kaleidoscope storage pillar's binary inside a containerised
deployment. By accident, the `pillar_root` mount is the container's
overlayfs (no persistent volume bound), and overlayfs's fsync is
known to no-op on some configurations. Today the existing
`composition::probe()` (which calls `store.query(...)` on an empty
range and asserts `Ok`) passes cleanly: the store opens, the read
succeeds, the listener binds. The platform serves writes that appear
to fsync but never survive a crash. Ravi has no way to know until
the container restarts and the WAL recovery produces a smaller story
than the one he wrote. The residuality analysis flagged this as the
single biggest residue gap (S02 row, A-U1 undesired attractor); Ravi
needs the platform to refuse to start on a substrate that lies.

### Who

- Platform operator | running a Kaleidoscope storage pillar in a
  container or on a host filesystem | needs the platform to refuse
  to serve if the substrate is silently lying about fsync, so the
  "durability" claim is honest.
- Future self-host deployer | reading the platform's claim of
  Earned-Trust | trusts the README only if the binary refuses on a
  lying substrate.
- Kaleidoscope itself | as the entity making the Earned-Trust claim
  in three ADRs | needs the code to honour the claim, or the
  principle is paper.

### Solution

At startup of the chosen pillar's binary, after the existing
`composition::probe()` (which still runs and still asserts the store
opens and answers a trivial query), run an additional fsync-honesty
probe: write a small sentinel record into a fixed sub-path under
`pillar_root`, call fsync on the file handle, drop the handle, open
a fresh handle on the same path, read the bytes back, and verify
they match what was written. If the bytes do not match, the
substrate's fsync is a no-op; the binary emits the existing
structured `event=health.startup.refused` event naming the substrate
descriptor and exits non-zero, NEVER binding the listener. If the
bytes match, the binary proceeds to bind. The mechanism is the
cheapest portable behavioural test for the fsync-no-op class of
failure; a true crash simulation (fork + SIGKILL) is deferred.

The probe rides in the composition root, outside the storage traits.
The chosen pillar, the exact mechanism, the test seam, and the new
ADR number are FLAGGED to DESIGN.

### Domain Examples

#### 1: Happy Path - honest substrate, binary binds

Ravi opens the chosen pillar's binary against a `pillar_root` on a
real ext4-mounted host volume. Startup runs the existing
`composition::probe()` (which passes), then the new fsync-honesty
probe: a 64-byte sentinel `b"fsync-probe-2026-05-25-acme-prod"` is
written to `pillar_root/.fsync-probe`, fsynced, the handle is
dropped, a fresh handle opens the file, and reads back the exact 64
bytes. The probe succeeds. The binary binds its listener on the
configured port (the existing default for that pillar's read API or
ingest endpoint). The platform serves. The structured event log
contains no `health.startup.refused` line for this start.

#### 2: Lying Substrate - overlayfs accident, binary refuses

Ravi opens the chosen pillar's binary against a `pillar_root` that
points at the container's overlayfs (no persistent volume mount; an
accident in the deployment configuration). The existing
`composition::probe()` passes (it only opens-and-reads). The new
fsync-honesty probe writes the sentinel, fsyncs, drops the handle,
re-opens, and finds the bytes missing (or zeroed). The probe fails.
The binary emits `event=health.startup.refused` with a payload that
NAMES the substrate descriptor (e.g. "fsync no-op detected at
`pillar_root` /var/lib/kaleidoscope; suggested: bind a persistent
volume") and exits with non-zero status. NO listener is bound. The
acceptance test asserts both: the listener never bound, AND the
event was emitted.

#### 3: Boundary - missing sentinel after fsync (the most informative case)

Ravi opens the chosen pillar's binary against a substrate that
honours the write but silently truncates on fsync (a rarer but
documented failure mode of certain network filesystems and some
container storage drivers). The sentinel is written; fsync returns
`Ok`; the handle is dropped; the fresh handle opens the file; the
file exists but is shorter than the sentinel. The probe treats
"bytes differ from what was written" as a fsync lie, fails, emits
`event=health.startup.refused` naming the substrate, exits non-zero.
The boundary covers both "file gone" and "file present but wrong",
because both are honest signals that fsync did not deliver the
semantics it claimed.

### UAT Scenarios (BDD)

#### Scenario: A binary on an honest substrate passes the fsync probe and binds the listener

```gherkin
Given the chosen pillar's binary configured for tenant "acme-prod"
And `pillar_root` points at a real host-mounted ext4 directory the test runner controls
And the existing `composition::probe()` would succeed against this store
When Ravi starts the binary
Then the fsync-honesty probe writes a sentinel, fsyncs, drops the handle, re-opens, reads back, and the bytes match what was written
And the binary binds its configured listener on its default port
And no `event=health.startup.refused` is emitted during startup
```

#### Scenario: A binary on a fsync-lying substrate refuses to bind and emits the existing health.startup.refused event

```gherkin
Given the chosen pillar's binary configured for tenant "acme-prod"
And `pillar_root` points at a substrate whose fsync is a no-op (simulated via the DESIGN-chosen test seam)
And the existing `composition::probe()` would still succeed (the store opens and reads cleanly)
When Ravi starts the binary
Then the fsync-honesty probe writes a sentinel, fsyncs, drops the handle, re-opens, and the bytes do not match what was written
And the binary emits `event=health.startup.refused` with a payload naming the substrate
And the binary exits with a non-zero status
And the binary does NOT bind any listener on its configured port
```

#### Scenario: The boundary case where the substrate honours the write but loses bytes on fsync is treated as a lie

```gherkin
Given the chosen pillar's binary configured for tenant "acme-prod"
And `pillar_root` points at a substrate that writes the sentinel but silently shortens the file on fsync (simulated via the DESIGN-chosen test seam)
When Ravi starts the binary
Then the fsync-honesty probe writes a sentinel, fsyncs, drops the handle, re-opens, and the file is present but shorter than the sentinel
And the binary emits `event=health.startup.refused` with a payload naming the substrate
And the binary exits with a non-zero status
And the binary does NOT bind any listener on its configured port
```

#### Scenario: The existing composition probe still runs and still refuses on an unreadable store

```gherkin
Given the chosen pillar's binary configured for tenant "acme-prod"
And the store opens cleanly but `store.query(...)` returns `PersistenceFailed` (the existing LyingLogStore / LyingTraceStore shape)
When Ravi starts the binary
Then the existing `composition::probe()` fails before the fsync-honesty probe is reached
And the binary emits `event=health.startup.refused` for the same reason it does today
And the binary exits with a non-zero status
And the binary does NOT bind any listener on its configured port
```

#### Scenario: The storage trait surface is unchanged

```gherkin
Given the workspace as of slice 01 of this feature
When the public-api diff is computed against the prior tag
Then `LogStore`, `MetricStore`, `TraceStore`, and the beacon `RuleStateStore` trait signatures are byte-identical to the prior tag
And no method is added, removed, or re-signed on any storage trait
```

### Acceptance Criteria

- [ ] On an honest substrate, the binary's fsync-honesty probe passes and the listener binds (Scenario 1).
- [ ] On a fsync-lying substrate, the binary refuses to bind and emits `event=health.startup.refused` naming the substrate (Scenario 2).
- [ ] On a substrate that loses bytes on fsync, the binary refuses to bind and emits the same event (Scenario 3).
- [ ] The existing `composition::probe()` continues to refuse on an unreadable store (regression preserved; Scenario 4).
- [ ] The storage trait surface (`LogStore`, `MetricStore`, `TraceStore`, beacon `RuleStateStore`) is unchanged (Scenario 5).
- [ ] No new metric, dashboard, or event name is introduced; the refusal rides on the existing `event=health.startup.refused`.

### Outcome KPIs

- **Who**: a Kaleidoscope operator (or any self-host deployer) running the chosen pillar's binary.
- **Does what**: stops binding the listener whenever the substrate is silently lying about fsync, and instead emits a refusal event identifying the substrate.
- **By how much**: 100 percent of fsync-lying-substrate startups in the acceptance suite refuse to bind, and 100 percent of honest-substrate startups bind without false positives.
- **Measured by**: the acceptance-test outcomes for Scenarios 1-5, plus a per-crate mutation kill rate of 100 percent on the changed files (ADR-0005 Gate 5).
- **Baseline**: 0 percent today. The existing `composition::probe()` accepts every fsync-lying substrate because it only opens-and-reads (`crates/log-query-api/src/composition.rs:73`, `crates/trace-query-api/src/composition.rs:77`); a workspace grep confirms zero fsync calls in `crates/`.

### Technical Notes (DESIGN-flagged, NOT decided here)

- The probe mechanism, the slice-01 pillar, the test seam, and the
  likely new ADR (ADR-0049 refining the Earned-Trust discipline to
  mean "honour fsync") are all FLAGGED to DESIGN. See
  `wave-decisions.md` for the four flags.
- The probe lives in the composition root, mirroring the shape of
  the existing `composition::probe()` (a free function over the
  store + tenant) so the unit and mutation tests stay outside the
  binary spawn.
- The sentinel file path is at a fixed sub-path under `pillar_root`
  (e.g. `pillar_root/.fsync-probe`); the probe overwrites it on
  every run; no accumulating state across restarts. The exact path
  is a DESIGN decision.
- The existing `event=health.startup.refused` is reused verbatim; the
  payload gains a substrate descriptor field (e.g. `substrate=overlayfs`
  or `substrate=tmpfs`), whose exact key name is a DESIGN decision.
- Dependencies: this feature depends on the existing `composition`
  module shape in `log-query-api` / `trace-query-api` and the
  existing `event=health.startup.refused` structured event. Both
  are present and Accepted; no upstream block.

---

## US-02: An honest fsync probe is testable through a hostile-filesystem seam without spawning a binary

### Elevator Pitch

- Before: the existing `composition::probe()` (lines 73 and 77 of
  the two `composition.rs` files) is tested via free functions over
  the store + tenant, with a `LyingLogStore` / `LyingTraceStore`
  test double providing the failure path. There is no equivalent
  for fsync: the platform cannot test "the substrate's fsync is a
  no-op" without actually mounting a hostile filesystem, which CI
  cannot do reliably.
- After: a DESIGN-chosen seam (an `FsyncProbe` trait, a path
  injection, or a tempdir double; FLAG 3) lets a developer write a
  unit test that runs the fsync-honesty probe over a substrate whose
  fsync is a deliberate no-op, and asserts the probe returns the
  refusal error AND the structured event was emitted. Observable
  via `cargo test -p <chosen-pillar>` (or its read API crate); the
  test takes the same shape as the existing `LyingLogStore` /
  `LyingTraceStore` tests (`crates/log-query-api/src/composition.rs:184`,
  `crates/trace-query-api/src/composition.rs:205`).
- Decision enabled: the team can mutation-test the fsync-probe
  logic to 100 percent kill on the changed files (ADR-0005 Gate 5),
  independent of which filesystem the CI runner actually has.

### Problem

Imagine the platform's mutation-test discipline (CLAUDE.md, 100
percent kill on modified files) applied to the new fsync probe.
Without a seam, the probe's "refuse on lying substrate" branch can
only be exercised by actually mounting a hostile filesystem, which
the CI runner cannot do portably. A mutation that turns a `!=` into
a `==` (treating "bytes differ" as success) would survive every test
the CI can practically run, leaving a silent gap in the Earned-Trust
claim. The team needs a unit-testable seam for the lying substrate,
identical in shape to the `LyingLogStore` / `LyingTraceStore` test
doubles that already exist.

### Who

- Kaleidoscope developer | adding the fsync probe to the chosen
  pillar's binary | needs the lying-substrate branch unit-testable
  so the per-crate mutation gate stays at 100 percent.
- Reviewer (Bea / peer) | confirming the slice-01 quality | reads
  the test suite and expects to see both branches covered.
- Future contributor | extending the probe to the remaining
  pillars in slice 02+ | needs the seam to be the shape they can
  re-use, not pillar-specific.

### Solution

DESIGN picks ONE seam from the three candidates flagged in
`wave-decisions.md` FLAG 3 (an `FsyncProbe` trait, a path injection,
or a tempdir double). The seam supports a "real" implementation
(write + fsync + drop handle + re-open + read on the actual
filesystem) and a "lying" test double (the no-op fsync, the
truncating fsync, the byte-flipping fsync). Unit tests in the chosen
pillar's crate exercise both branches without spawning a binary,
mirroring the shape of `probe_succeeds_against_a_readable_store_with_a_tenant`
(`crates/log-query-api/src/composition.rs:196`) and
`probe_refuses_when_the_store_cannot_be_read`
(`crates/log-query-api/src/composition.rs:185`).

### Domain Examples

#### 1: Happy Path - the honest seam test

A developer writes a unit test that constructs the real fsync probe
over a tempdir on the test runner's filesystem and asserts the
probe returns `Ok(())`. The test mirrors the existing
`probe_succeeds_against_a_readable_store_with_a_tenant` test.

#### 2: Lying Substrate Test - the no-op fsync double

A developer writes a unit test that constructs the lying-substrate
seam (per DESIGN's choice) whose fsync is a no-op, runs the fsync-
honesty probe, and asserts the probe returns the refusal error AND
the structured event payload identifies the substrate. The test
mirrors the existing `probe_refuses_when_the_store_cannot_be_read`
test in shape.

#### 3: Boundary - the truncating fsync double

A developer writes a unit test for the rarer "bytes lost on fsync"
case, where the test double's fsync silently truncates the file.
The probe must catch this as a lie (bytes differ) and return the
refusal error. This is the test that pins the boundary between
"file gone" and "file present but wrong" as ONE class of failure.

### UAT Scenarios (BDD)

#### Scenario: The honest seam test passes

```gherkin
Given the DESIGN-chosen seam configured with a real tempdir on the test runner's filesystem
When the fsync-honesty probe runs through the seam
Then the probe returns `Ok(())`
And no refusal error is produced
```

#### Scenario: The lying seam test produces the refusal error and event payload

```gherkin
Given the DESIGN-chosen seam configured with a no-op fsync test double
When the fsync-honesty probe runs through the seam
Then the probe returns an error whose message names the substrate
And the structured `event=health.startup.refused` payload, when serialised, contains a substrate descriptor field
```

#### Scenario: The truncating seam test produces the same refusal

```gherkin
Given the DESIGN-chosen seam configured with a fsync that silently truncates the file
When the fsync-honesty probe runs through the seam
Then the probe returns an error whose message names the substrate
And the boundary case "file present but shorter than the sentinel" is treated identically to "file gone"
```

#### Scenario: Mutation tests on the changed files stay at 100 percent kill

```gherkin
Given the chosen pillar's crate after slice 01 lands
When `cargo mutants` runs scoped to the changed files
Then 100 percent of mutants are killed (ADR-0005 Gate 5; CLAUDE.md)
And no mutant in the fsync-probe branch survives
```

### Acceptance Criteria

- [ ] An honest-substrate unit test asserts the probe returns `Ok(())` over a real tempdir.
- [ ] A no-op-fsync unit test asserts the probe returns the refusal error and the event payload contains the substrate descriptor.
- [ ] A truncating-fsync unit test asserts the boundary case is treated as a lie.
- [ ] The per-crate mutation gate on the chosen pillar's crate stays at 100 percent on the changed files.
- [ ] The seam shape mirrors the existing `LyingLogStore` / `LyingTraceStore` test double pattern: free function in the composition module, test doubles in the same module's `#[cfg(test)] mod tests` block.

### Outcome KPIs

- **Who**: a Kaleidoscope developer extending or maintaining the chosen pillar's binary.
- **Does what**: writes unit tests that cover both the honest and the lying fsync paths through the new seam, without spawning a binary or mounting a hostile filesystem.
- **By how much**: 100 percent of the fsync-probe branches are exercised by unit tests, and 100 percent of mutants in the changed files are killed by the per-crate mutation gate.
- **Measured by**: per-crate `cargo mutants` output (ADR-0005 Gate 5) on the changed files; line coverage of the new probe function as a secondary signal.
- **Baseline**: 0 percent today (the new code does not exist yet); the existing `composition::probe()` already meets this bar for its open-and-read branches.

### Technical Notes (DESIGN-flagged, NOT decided here)

- The seam shape is FLAG 3 (`FsyncProbe` trait, path injection, or
  tempdir double). DESIGN picks one and records the rationale.
- The test doubles in the `#[cfg(test)] mod tests` block follow the
  shape of `LyingLogStore` / `LyingTraceStore` in
  `crates/log-query-api/src/composition.rs:97` and
  `crates/trace-query-api/src/composition.rs:106`.
- Dependencies: depends on the seam-choice decision in DESIGN; no
  cross-crate dependency on the storage pillars beyond what already
  exists for the chosen pillar's binary.

---

## Story sizing summary

| Story | Scenarios | Effort | Right-sized? |
|---|---|---|---|
| US-01 | 5 | 2 days | Yes |
| US-02 | 4 | 1 day | Yes |

Both stories sit inside slice 01 (the walking skeleton). Slice 01 is
ONE pillar; later pillars land in later slices on this feature wave
or a successor feature, per the residuality follow-up roadmap.
