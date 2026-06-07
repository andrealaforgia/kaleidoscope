# Story Map: speed-up-local-precommit-v0

## User: Devon, the committing maintainer (human or crafter agent)

## Goal: keep `main` socially green without a 10-20 min wait on every commit, while the deep coverage still has eyes

## Backbone

The driving surface is `git commit` (the local hook) plus the CI results
page / `gh run list`. The user activities, left-to-right:

| Commit locally | Push to main | Watch CI | Recover on failure |
|----------------|--------------|----------|--------------------|
| Run fast gate (fmt+clippy+deny+fast tests) | Deep suite runs in CI gate-1 | Check latest main run status | Fix-forward on a deep-only red |
| Fast subset catches unit/fmt/clippy breaks | (no local deep wait) | Run cadence command | Open failing run, see failing test |

---

## Walking Skeleton

Not applicable as a build artifact — both operator surfaces already exist
(`git commit` and the CI Actions page). The thinnest end-to-end slice that
delivers the outcome is: **slim the hook (US-01 + US-02) so a commit gates
in <= 5 min while still catching cheap mistakes, with the deep gate already
living in CI (US-03)**. US-04 (the cadence) is the honesty mitigation that
makes the skeleton safe to ship.

## Carpaccio: two thin end-to-end slices

Andrea authorised "ONE slice (the hook slim-down + the CI-watching
cadence), or carpaccio (slim the hook first — the urgent win; then the
CI-watching cadence)." Applying the carpaccio taste tests below, the
feature splits cleanly into two slices, each independently shippable and
each delivering a verifiable working behavior.

### Slice 1 (urgent win): The fast local gate — US-01, US-02, US-03

- **Output**: slimmed `scripts/hooks/pre-commit` (fast test subset; fmt,
  clippy, deny, toolchain kept), CI gate-1 confirmed unchanged.
- **Outcome**: Devon's commit gates in <= 5 min and still catches
  unit/fmt/clippy breaks; the deep suite is verifiably still in CI.
- **Verifiable behavior**: `git commit` completes in <= 5 min; a broken
  unit test still rejects the commit.
- **Why first**: this is the urgent pain (10-20 min per commit). It is
  shippable on its own under the trunk-based "CI is feedback" posture
  because the deep gate already exists in CI (US-03 is a verification, not
  new work).

### Slice 2 (the eyes): The CI-watching cadence — US-04

- **Output**: a concrete watch command (recommended `scripts/ci-watch.sh`
  over `gh`) + documented cadence + the documented honesty trade.
- **Outcome**: deep-only regressions are caught within one cadence
  interval, not days.
- **Verifiable behavior**: the watch command prints the latest main run
  status and URL; a deep-only red is surfaced by the cadence.
- **Why second**: it is the mitigation for Slice 1's honesty trade. Slice 1
  is safe to ship first under the existing posture (the local hook was
  always a courtesy, never a hard gate), and Slice 2 hardens the watching
  habit that replaces the lost local-wait signal.

> Carpaccio taste tests applied: each slice (a) is end-to-end and
> demonstrable in a single session; (b) delivers a behavior Devon can
> verify; (c) is independently valuable (Slice 1 removes the pain even
> before Slice 2 lands; Slice 2 adds the eyes); (d) is right-sized (1-3
> days, 3 scenarios each). DESIGN may also collapse both into ONE slice if
> it prefers — the stories are written to support either shape.

## Scope Assessment: PASS — 4 stories, 1 module (scripts/hooks + ci-watch script + docs), estimated 1-2 days

Oversized signals checked: 4 user stories (<= 10, PASS) | touches the
pre-commit hook script, one new watch script, and docs (1-2 modules,
<= 3, PASS) | no walking-skeleton integration points (surfaces exist) |
estimated 1-2 days (< 2 weeks, PASS) | two related user outcomes (fast
gate + watching cadence) that can ship separately but are one coherent
feature. Right-sized; no split forced (carpaccio into 2 slices offered as
the recommended delivery shape, not a scope rescue).

## Priority Rationale

Priority by outcome impact and dependency:

1. **US-01 (P1)** — the largest bottleneck (Q1): the 10-20 min wait is the
   stated pain. Highest value, directly moves the north-star (hook
   wall-clock). Walking-skeleton-equivalent for this feature.
2. **US-02 (P1)** — guardrail on US-01: a fast hook that stops catching
   cheap mistakes is a net loss. Must ship with US-01 (same slice).
3. **US-03 (P1)** — verification that makes US-01 safe: the deep gate must
   demonstrably already live in CI before the local block is removed. Low
   effort (a no-change assertion + diff check), but a hard precondition.
4. **US-04 (P2)** — the mitigation: establishes the eyes that replace the
   local-wait signal. Slightly lower urgency than the slim-down (Slice 1
   is safe under the existing posture), but required for honesty and to
   bound deep-only regression detection latency.
