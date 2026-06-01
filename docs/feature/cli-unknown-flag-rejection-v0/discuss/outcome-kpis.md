# Outcome KPIs — cli-unknown-flag-rejection-v0

These KPIs measure the operator-facing behaviour change and the EDD
verifier anchor. They feed the DEVOPS wave (observability) and the DISTILL
wave (acceptance assertions).

## K1: Unknown flag yields a non-zero exit

- **Who**: operators who pass an unknown or mistyped flag (top-level or
  subcommand-level).
- **Does what**: receive a non-zero exit code instead of a silent 0 or a panic.
- **By how much**: 100% of unknown-flag invocations exit non-zero.
- **Measured by**: acceptance tests asserting the exit code for US-01 and
  US-02 scenarios.
- **Baseline**: top-level unknown flags already exit 2; subcommand-level
  unknown flags currently exit 0 (silent acceptance). The gap is the
  subcommand level.

## K2: A usage error is written to stderr

- **Who**: operators who pass unknown input (flag, option, or subcommand).
- **Does what**: see a usage error on stderr naming the unknown token, so
  they know what was wrong.
- **By how much**: 100% of rejection scenarios emit a usage error substring
  on stderr (exact substring pinned by DESIGN flag 3).
- **Measured by**: acceptance tests asserting a stderr substring for US-01,
  US-02, US-03.
- **Baseline**: top-level and unknown-subcommand paths already emit usage;
  subcommand-level unknown flags emit nothing today.

## K3: Known flags and subcommands are not broken (regression)

- **Who**: operators using the documented flags and subcommands correctly.
- **Does what**: observe behaviour identical to before the US-02 fix.
- **By how much**: 0 regressions across the documented flag and subcommand
  set; the existing acceptance suite stays green.
- **Measured by**: existing `kaleidoscope-cli` test suite plus the US-04
  adjacency scenarios.
- **Baseline**: all known invocations work today; the fix must be additive.

## K4: Fresh non-reverted anchor for EDD defect K11

- **Who**: the EDD black-box verifier re-checking defect K11.
- **Does what**: re-verifies K11 against an acceptance test committed on a
  live (non-reverted) commit, moving K11 from `held` to satisfied.
- **By how much**: K11 transitions from `held` to satisfied on the next
  verifier run; the anchor commit is not reverted.
- **Measured by**: EDD verifier re-run after this feature's DELIVER wave
  lands; K11 status field.
- **Baseline**: K11 is `held` because its prior anchor (commit e7fbee0) was
  dropped en bloc by revert e3a8cad.
