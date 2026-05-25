# Story Map: earned-trust-fsync-probe-v0

British English. No em dashes. No emoji.

## User

Ravi Patel, the platform operator for tenant "acme-prod", running
the chosen Kaleidoscope storage pillar's binary inside a
containerised deployment.

## Goal

The platform refuses to bind a listener whenever the substrate is
silently lying about fsync, so the Earned-Trust claim made in
ADR-0042 Decision 8, ADR-0047 Decision 6, and ADR-0048 Decision 8 is
honoured by code rather than left as paper.

## Backbone

The user activities run left-to-right across one startup cycle:

| Configure substrate | Start binary | Probe substrate | Bind or refuse |
|---|---|---|---|
| Ravi sets `pillar_root` and the existing env posture for the chosen pillar | Ravi runs the binary the way he runs it today (cargo run / docker run / systemd) | The startup sequence runs the existing `composition::probe()` then the new fsync-honesty probe through the DESIGN-chosen seam | The binary either binds its listener (honest substrate) or emits `event=health.startup.refused` and exits non-zero (lying substrate) |

Each backbone column is one user activity. The walking skeleton is
the minimum slice across all four columns that delivers an honest
bind-or-refuse outcome for ONE pillar.

## Walking skeleton (slice 01)

Exactly the minimum to make the flow work for ONE pillar:

- **Configure substrate**: an acceptance-test fixture provides a real
  tempdir (the honest case) AND a DESIGN-chosen test seam that
  simulates a lying substrate (the refusal case). No real container,
  no real overlayfs in CI.
- **Start binary**: the chosen pillar's binary is invoked by the
  acceptance suite via the existing pattern for that pillar (e.g.
  the tower `oneshot` for read APIs, or the existing binary spawn
  for ingest paths). The exact invocation mirrors the test shape
  already in use for that pillar.
- **Probe substrate**: the existing `composition::probe()` runs
  first (unchanged); then the new fsync-honesty probe runs through
  the seam (write sentinel + fsync + drop handle + re-open + read).
- **Bind or refuse**: on success, the binary binds the listener; on
  failure, the binary emits `event=health.startup.refused` with a
  substrate descriptor and exits non-zero.

This is the THINNEST end-to-end slice that connects all four
backbone activities for ONE pillar. Every later slice extends column
3 (more pillars) without changing the shape of column 4.

## Slice plan

### Slice 01 (this feature wave): walking skeleton on ONE pillar

Stories:

- **US-01** (the bind-or-refuse outcome on the chosen pillar's
  binary, observable via acceptance tests on honest, lying, and
  truncating-fsync substrates; plus the regression on the existing
  `composition::probe()` and the no-storage-trait-change invariant).
- **US-02** (the unit-testable seam that lets the lying-substrate
  branch be mutation-killed without spawning a binary or mounting a
  real hostile filesystem).

Outcome KPI: the chosen pillar's binary refuses to bind on every
fsync-lying-substrate acceptance scenario and binds on every honest-
substrate scenario, with 100 percent mutation kill on the changed
files (see `outcome-kpis.md`).

### Slice 02 (later, scoped by successor feature or this feature's next slice)

Extend the probe to the remaining storage pillars (lumen, ray,
cinder, strata, sluice) and to beacon-server (which has keyed-
latest-wins discipline per ADR-0040 but the same fsync dependency).
Each pillar's binary gains the same composition-root probe shape.

### Slice 03 (deferred, named in residuality analysis)

Optional escalation to a true crash simulation (fork + SIGKILL +
re-open) per the residuality analysis's M-1 mention of "kill-and-
reopen at the process level". Only undertaken if slice 01's
behavioural probe leaves documented false negatives in the field.

## Priority Rationale

Priority order:

1. **US-01** (P1). The bind-or-refuse outcome IS the feature. Without
   it, the Earned-Trust claim remains paper. Highest outcome impact;
   lowest unknown-resolution risk (the existing `composition::probe()`
   shape and the existing `event=health.startup.refused` event are
   precedents).
2. **US-02** (P2). The unit-testable seam is a dependency of the
   mutation-kill-rate-100 invariant (ADR-0005 Gate 5; CLAUDE.md);
   without it, US-01 cannot pass the project's quality gate. US-02
   is therefore P2 (immediately after US-01, atomic with it in
   DELIVER) rather than a separate later release.

Dependencies:

- US-02 must land in the same slice as US-01; the per-crate
  mutation gate evaluates the whole crate after the change, not
  story-by-story.
- Both stories depend on DESIGN resolving FLAG 1 (probe mechanism)
  and FLAG 3 (seam shape) before DISTILL writes the acceptance
  tests; FLAG 2 (pillar choice) drives which crate the work
  actually lands in; FLAG 4 (new ADR) is recorded by DESIGN in
  parallel.

## Scope Assessment

PASS - 2 stories, 1 crate (the DESIGN-chosen pillar; no cross-crate
work in slice 01), estimated 3 days. The residuality follow-up
roadmap explicitly carves this as one of three numbered features,
each going through full nWave; the carpaccio slicing rule is
honoured by deferring all other pillars to slice 02 / a successor
feature.

Carpaccio taste-tests (see `wave-decisions.md`):

1. Honest substrate passes the new probe and the existing probe.
2. Lying substrate refuses with the existing event.
3. Storage trait surface is unchanged.

Three independently demonstrable behaviours; one user outcome; one
crate. Right-sized.
