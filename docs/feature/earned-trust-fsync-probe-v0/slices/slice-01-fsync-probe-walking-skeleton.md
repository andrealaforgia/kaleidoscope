# Slice 01: fsync-probe walking skeleton on ONE pillar

British English. No em dashes. No emoji.

## Origin

This slice is the walking skeleton of M-1 in
`docs/product/architecture/residuality-analysis.md` (commit 50e20b5)
and item 1 in `docs/residuality-followups-roadmap.md` (commit
820176d). The Earned-Trust principle recorded in ADR-0042 Decision
8 (and reproduced in ADR-0047 Decision 6 and ADR-0048 Decision 8)
exists in code today as `composition::probe()`, which asserts only
"open and read", not "survive a crash via fsync". A workspace grep
for `fsync`, `sync_data`, `sync_all`, `FsyncProbe` in `crates/`
returns zero hits; the claim is paper, not code.

This slice closes the gap for ONE pillar's binary. Later slices
extend to the remaining pillars.

## Slice goal

The chosen pillar's binary refuses to bind a listener on a substrate
whose fsync is a no-op (or which silently loses bytes on fsync), and
binds cleanly on an honest substrate. The refusal rides on the
existing `event=health.startup.refused` structured event. The
storage trait surface (`LogStore`, `MetricStore`, `TraceStore`,
beacon `RuleStateStore`) is unchanged.

## Walking-skeleton entry point

The chosen pillar's BINARY (the choice of pillar is FLAGGED to
DESIGN; see `discuss/wave-decisions.md` FLAG 2). Acceptance tests
exercise the binary through whichever invocation shape that pillar
already uses (e.g. the tower `oneshot` for the read APIs, the
existing binary spawn for ingest paths). The observable output is:

- the bind-or-refuse outcome on the listener's configured port, AND
- the structured `event=health.startup.refused` event with a
  substrate descriptor on the refusal arm.

## Stories in this slice

- **US-01** (P1): The platform refuses to serve over a substrate
  whose fsync is a no-op.
- **US-02** (P2, atomic with US-01): An honest fsync probe is
  testable through a hostile-filesystem seam without spawning a
  binary.

Both stories live in `discuss/user-stories.md` with full LeanUX
shape, three domain examples each, and 4-5 BDD scenarios each.

## Learning hypothesis

We believe that a behavioural fsync probe (write sentinel + fsync +
drop handle + re-open + read) at startup is enough to catch the
fsync-no-op class of substrate failure for one pillar, without:

- modifying any storage trait,
- forking and SIGKILL-ing a child process (which would be unsafe
  inside a tokio runtime), or
- inspecting filesystem options via `statfs` / `fstatfs` (which is
  not portable across the Linux / macOS / container matrix the
  platform claims).

We will know we are right when:

- the chosen pillar's binary refuses to bind on every
  fsync-lying-substrate acceptance scenario (US-01 Scenarios 2, 3),
- the chosen pillar's binary binds on every honest-substrate
  acceptance scenario (US-01 Scenario 1),
- the existing `composition::probe()` regression test still passes
  (US-01 Scenario 4),
- the storage trait surface is unchanged (US-01 Scenario 5), AND
- the per-crate mutation gate stays at 100 percent kill on the
  changed files (US-02 mutation criterion).

We will know we are wrong if:

- the behavioural probe leaves documented false negatives in the
  field (escalation path: reserve a successor slice for fork+SIGKILL),
- the probe produces false positives on legitimate substrates that
  honour fsync with delayed semantics (escalation path: document
  the substrate matrix and adjust the probe's verification window),
- the per-crate mutation gate cannot hit 100 percent through the
  DESIGN-chosen seam (escalation path: revisit FLAG 3 in DESIGN).

## Carpaccio taste-tests (three independent demonstrations)

1. **Honest substrate passes**. The chosen pillar's binary, opened
   on a real tempdir, runs both the existing `composition::probe()`
   AND the new fsync-honesty probe; both succeed; the listener
   binds. Demonstrable in a single `cargo test` run.
2. **Lying substrate refuses**. The same binary, opened through the
   DESIGN-chosen seam against a no-op-fsync test double, runs both
   probes; the existing probe passes; the new probe fails; the
   binary emits `event=health.startup.refused` naming the substrate
   AND exits non-zero AND does not bind. Demonstrable in a single
   `cargo test` run.
3. **Trait surface is untouched**. The `gate-2-public-api` diff
   against the prior tag is empty for `LogStore`, `MetricStore`,
   `TraceStore`, and `RuleStateStore`. Demonstrable via the existing
   per-crate CI.

Each taste-test is one acceptance scenario; the slice is "done"
when all three pass and the mutation gate is 100 percent on the
changed files.

## Flagged to DESIGN

Four items are FLAGGED to DESIGN, NOT decided by DISCUSS:

1. **Probe mechanism**: (a) write + fsync + drop handle + re-open +
   read (likely walking skeleton), (b) fork + SIGKILL true crash
   (deferred), (c) `statfs` / `fstatfs` inspection (rejected as
   fragmented).
2. **Pillar choice for slice 01**: `pulse` (most recently touched),
   `kaleidoscope-gateway` (the ingest path), or one of the read
   APIs (`log-query-api` / `trace-query-api`, where the existing
   `probe()` lives).
3. **Test seam for injecting a hostile filesystem**: an
   `FsyncProbe` trait, path injection, or a tempdir double.
4. **Likely new ADR (ADR-0049)** refining the Earned-Trust
   discipline of ADR-0042 Decision 8 from "open and read" to
   "honour fsync".

See `discuss/wave-decisions.md` for the rationale on each flag.

## Out of scope (deferred and DECLARED)

- A true crash simulation (fork + SIGKILL + re-open).
- Coverage of pillars other than the chosen one.
- Automatic detection of overlayfs / tmpfs / mount options via
  `statfs` / `fstatfs`.
- Any change to the storage traits.
- UI or telemetry beyond the existing structured event.

## Effort

Estimated 3 days for slice 01: 2 days for US-01 (write the probe in
the chosen pillar's composition root, the acceptance scenarios, and
the regression on the existing `composition::probe()`) and 1 day for
US-02 (the seam shape and the unit tests that pin the mutation
kill rate to 100 percent on the changed files).
