# Wave Decisions: earned-trust-fsync-probe-v0 (DISCUSS)

British English. No em dashes. No emoji.

## Origin and frame

This is M-1 in the residuality analysis
(`docs/product/architecture/residuality-analysis.md`, commit 50e20b5)
and the first item in the residuality follow-up roadmap
(`docs/residuality-followups-roadmap.md`, commit 820176d). The roadmap
puts it first deliberately: it closes a CLAIM the code does not keep,
before anything else is added on top.

The platform claims Earned-Trust at startup. ADR-0042 Decision 8 says
"verify your substrate before serving"; ADR-0047 (Decision 6) and
ADR-0048 (Decision 8) reproduce the same posture for the log and trace
read APIs. The existing `probe()` functions in
`crates/log-query-api/src/composition.rs:73` and
`crates/trace-query-api/src/composition.rs:77` verify open-and-read
(call `store.query(...)` and assert `Ok`), NOT survive-via-fsync. Five
storage pillars (pulse, lumen, ray, cinder, strata, sluice) and the
beacon rule-state store all rely on fsync that the WAL recovery
discipline (ADR-0040, WAL + snapshot + replay) implicitly assumes;
none of them probe it.

### Reads checklist

- [x] `docs/product/architecture/residuality-analysis.md` (M-1
  framing, the S02 row of the incidence matrix, A-U1 "silent data
  loss" attractor, "Honest gaps in the platform's claimed invariants"
  section). M-1 is the named resilience modification this feature
  realises.
- [x] `docs/residuality-followups-roadmap.md` (this feature's position
  as item 1 of 3; ground rules; full nWave per feature; no 1.0.0
  bump).
- [x] `docs/product/architecture/adr-0042-query-api-contract-and-promql-subset.md`
  (Decision 8 "Earned-Trust probe (wire-then-probe-then-use)"; the
  `health.startup.refused` event name).
- [x] `docs/product/architecture/adr-0047-lumen-log-query-api-contract-and-crate-layout.md`
  (Decision 6 reproducing the same probe posture for logs).
- [x] `docs/product/architecture/adr-0048-ray-trace-query-api-contract-and-crate-layout.md`
  (Decision 8 reproducing the same probe posture for traces, with the
  `ServiceName` sentinel).
- [x] `crates/log-query-api/src/composition.rs` (the current `probe()`
  pattern, lines 73-84; it asserts the store answers a trivial query,
  not that a write survived a crash).
- [x] `crates/trace-query-api/src/composition.rs` (the same posture,
  lines 77-89, mirrored for traces).
- [x] `crates/pulse/src/file_backed.rs` (where the WAL appends live;
  `append_wal` at line 354 calls `wal.flush()` on a `BufWriter<File>`,
  which empties the user-space buffer to the kernel but NEVER calls
  `sync_data` or `sync_all` on the underlying file). A workspace grep
  for `fsync`, `sync_data`, `sync_all`, `FsyncProbe` returns zero hits
  in `crates/`, exactly as the residuality analysis claims.

The residuality analysis's framing is consistent with what the code
does today; no contradiction was discovered during DISCUSS. The
"Earned-Trust" claim in the three ADRs and the absence of any fsync
honesty check in code is exactly the gap this feature exists to
close.

## DIVERGE status

No DIVERGE artefacts at `docs/feature/earned-trust-fsync-probe-v0/diverge/`.
The job statement is taken from the residuality analysis and the
roadmap: "demonstrate that the substrate honours fsync at startup, or
refuse to bind". JTBD was explicitly NOT requested by the invoking
prompt; the journey is grounded in the residuality analysis and the
existing ADR vocabulary instead.

Risk noted: without DIVERGE there is no separate ODI scoring; the
opportunity priority is taken from the roadmap rather than derived.
This is proportionate: this is one item in a numbered three-item
roadmap, not a competition between candidate features.

## Scope: SLICE 01 THIN (walking skeleton)

An honest fsync probe runs at startup of ONE pillar's binary (DESIGN
picks which one). The probe writes a sentinel into the pillar root on
the REAL disk, fsyncs, drops the file handle, re-opens the file in a
fresh handle, and verifies the bytes survived. This is NOT a true
crash test, but it catches the fsync no-op class of failure
(overlayfs in a container, an accidental tmpfs mount, a misconfigured
mount option, a no-op fsync for "performance"). If the probe fails,
the binary refuses to bind the listener, mirroring the `refuse`
pattern of ADR-0042 Decision 8 (the existing `event=health.startup.refused`
event).

Later slices extend to the remaining pillars (lumen, ray, cinder,
strata, sluice) and to beacon-server (which has keyed-latest-wins
discipline but the same fsync dependency).

### OUT of scope (deferred and DECLARED)

- A true crash simulation (fork + SIGKILL + re-open). Reason: fork is
  unsafe inside a tokio runtime; the cost outweighs the marginal
  honesty over the simpler "drop handle + re-open" approach for the
  fsync-no-op class of failure. Re-evaluated only if slice 01 leaves
  documented false negatives in the field.
- Coverage of ALL pillars in slice 01. Slice 01 is ONE only; the
  others land in subsequent slices on the same feature wave or in a
  successor feature.
- Automatic detection of overlayfs / tmpfs / mount options via
  syscalls (`statfs` / `fstatfs`). Reason: portable behaviour-tests
  (write + fsync + re-open + read) catch the same class of failure
  without the OS-specific fragmentation. The residuality analysis
  M-1 mentions `fstatfs` as an option; DISCUSS prefers the
  behaviour-test approach as a portable walking-skeleton seam.
- Any change to the storage traits (`LogStore`, `MetricStore`,
  `TraceStore`). The probe rides outside the trait, in the
  composition root, identical in shape to the existing `probe()`.
- UI or telemetry beyond the existing `event=health.startup.refused`
  event. No new metric, no new dashboard, no new alert. The refusal
  is the signal.

## Flagged to DESIGN (DISCUSS does NOT decide these)

1. **PROBE MECHANISM**. Three candidates surfaced:
   - (a) write + fsync + drop-handle + re-open + read. Simple,
     portable, NOT a true crash test but catches the fsync no-op
     class. Likely recommendation for the walking skeleton.
   - (b) write + fsync + fork + SIGKILL the child + re-open. A true
     crash test, complex, fork is unsafe inside a tokio runtime.
   - (c) `statfs` / `fstatfs` syscall to inspect filesystem options.
     Fragmented, not portable across the Linux / macOS / container
     matrix the platform claims to run on.
   Escalation to (b) only if (a) leaves documented false negatives.
   Luna flags; DESIGN decides and records in an ADR.

2. **PILLAR CHOICE for slice 01**. Three candidates:
   - `pulse` (the most recently touched storage pillar; the WAL append
     path lives at `crates/pulse/src/file_backed.rs:354`).
   - `kaleidoscope-gateway` (the ingest binary; the place a substrate
     lie hurts most because data is being WRITTEN there).
   - `log-query-api` or `trace-query-api` (the read APIs where the
     existing `probe()` lives; the smallest blast radius for the
     first probe).
   DESIGN picks one and records the rationale.

3. **SEAM for injecting a hostile filesystem in tests**. Three
   candidates:
   - An `FsyncProbe` trait abstracting the write-fsync-reopen
     behaviour, with a real implementation and a lying test double.
   - Path injection: a configurable probe directory the test can
     point at a tmpfs (which honours O_DSYNC differently) or an
     overlayfs simulation.
   - A `tempdir` double in tests: the test owns a tempdir and a
     wrapper that overrides the fsync call to no-op.
   DESIGN picks the seam that holds the project's "data + free
   functions + traits where polymorphism is genuinely needed" line
   (CLAUDE.md) and the existing composition-root testability shape.

4. **NEW ADR likely: ADR-0049**. The Earned-Trust discipline as
   recorded in ADR-0042 Decision 8 means "open and read" in code today;
   this feature refines it to mean "honour fsync". A new ADR (next
   free number 0049, verified) recording the refinement is likely
   load-bearing per the roadmap's "DESIGN via Morgan, with an ADR if
   the change is load-bearing" ground rule. DESIGN confirms ADR
   number and writes the ADR.

These four items are FLAGGED, NOT DECIDED, by DISCUSS.

## Walking-skeleton entry point

The walking skeleton is the chosen pillar's binary, exercised via
acceptance tests that simulate a hostile filesystem through the
DESIGN-chosen seam:

- The binary opened on an HONEST `pillar_root` (a real tempdir on the
  test runner's filesystem) passes the probe and binds the listener.
- The binary opened on a FAUX `pillar_root` whose fsync is a no-op
  refuses with `event=health.startup.refused`, names the substrate, and
  exits non-zero.

The After line of each story names this entry point: the chosen
pillar's binary, observed via the probe outcome (bind versus refuse)
and the structured startup event.

## Learning hypothesis

We believe that an fsync-honesty probe at startup of one pillar's
binary, written as the cheapest portable behavioural test (write +
fsync + drop handle + re-open + verify) rather than the syscall-
inspection or true-crash routes, will let the platform refuse to
serve over a lying substrate, restoring the meaning of the
Earned-Trust principle (ADR-0042 Decision 8, ADR-0047 Decision 6,
ADR-0048 Decision 8) without changing any storage trait.

We will know we are right when:

- A binary opened on a fsync-lying pillar root refuses with
  `event=health.startup.refused` in an acceptance test.
- A binary opened on an honest pillar root passes the probe and binds
  the listener, with no regression on the existing
  `composition::probe()` tests.
- The cost of the probe at startup is bounded (one small file write,
  one fsync, one re-open, one read) and unobservable in operational
  latency.

We will know we are wrong if:

- The probe needs the heavier fork+SIGKILL route to catch real
  substrate lies in the field (escalation path documented).
- The probe produces false positives on legitimate substrates
  (network filesystems with delayed fsync semantics that are still
  honest), in which case the cap and the substrate matrix get
  documented.

## Risks

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| Probe leaves false negatives on substrates that honour fsync only after a much later flush. | Medium | High (Earned-Trust would still be paper). | Document the matrix; reserve escalation to fork+SIGKILL behind the same seam in a successor slice. |
| Probe directory pollutes the pillar root with sentinel files across restarts. | Low | Low. | Sentinel file is at a fixed sub-path (e.g. `pillar_root/.fsync-probe`); the probe overwrites it on every run; no accumulating state. |
| The chosen pillar binary fails the probe legitimately during CI because the runner's filesystem is unexpectedly hostile (rare CI sandbox tmpfs). | Low | Medium (CI flake). | DESIGN selects the slice-01 pillar with this in mind; the probe runs against a real tempdir on the runner, not the CI image's overlay. |
| DESIGN escalates to a new trait change instead of riding the composition-root seam. | Low | Medium (broader blast radius than slice 01 warrants). | DISCUSS pins "no storage trait change" in the System Constraints; DESIGN may refine, not reverse. |

## Carpaccio taste-tests

Three things slice 01 must prove, each independently:

1. **The honest substrate passes**. Open the chosen binary on a real
   tempdir; the probe succeeds; the listener binds; the existing
   `composition::probe()` test still passes.
2. **The lying substrate refuses**. Open the chosen binary against a
   substrate seam whose fsync no-ops; the probe fails; the binary
   does NOT bind a listener; the structured `event=health.startup.refused`
   appears with a substrate descriptor.
3. **The trait surface is untouched**. The `LogStore`, `MetricStore`,
   `TraceStore`, and beacon `RuleStateStore` trait signatures and
   their other callers are unchanged on the diff.

Each taste-test is one acceptance scenario; the slice is "done" when
all three pass and the per-crate mutation gate is 100% on the changed
files (ADR-0005 Gate 5, CLAUDE.md).

## Honest contradiction check

The residuality analysis's framing of this gap was checked against
the codebase. The framing is consistent:

- The three ADRs (0042 / 0047 / 0048) name the Earned-Trust probe and
  the `event=health.startup.refused` event.
- The two `probe()` implementations call `store.query(...)` and
  return `Ok(())` on success, which is "open and read", not "survive
  a crash via fsync".
- `crates/pulse/src/file_backed.rs:354` (`append_wal`) calls
  `BufWriter::flush()` (user-space buffer to kernel) and does NOT call
  `sync_data` or `sync_all`. A workspace grep for `fsync`,
  `sync_data`, `sync_all`, `FsyncProbe` in `crates/` confirms zero
  occurrences.

No contradiction surfaced that DISCUSS could not resolve.

## Changelog

- 2026-05-25: feature `earned-trust-fsync-probe-v0` DISCUSS wave
  artefacts written by Luna. Four items flagged to DESIGN; one
  walking-skeleton slice declared (`slice-01-fsync-probe-walking-skeleton.md`).
