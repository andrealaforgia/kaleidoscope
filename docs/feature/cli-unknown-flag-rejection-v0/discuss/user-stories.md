<!-- markdownlint-disable MD024 -->

# User Stories — cli-unknown-flag-rejection-v0

## System Constraints

- The CLI argument parser is hand-rolled (no clap). The grounding at
  `crates/kaleidoscope-cli/src/main.rs` confirms this is a deliberate,
  documented choice. No clap dependency is introduced by this feature.
- The fix for US-02 must be **additive**: every flag and subcommand that
  works today must keep working byte-for-byte (US-04 is the regression
  guard).
- Exit code and stderr message shape are observable contracts the EDD
  verifier will assert on. They are pinned in DESIGN (see
  `wave-decisions.md`, Flags to DESIGN 2 and 3).
- The verifier needs a fresh, non-reverted anchor for defect K11. The
  acceptance tests these stories drive ARE that anchor.

JTBD analysis was skipped (Decision 4 = No). The job is self-evident: an
operator who mistypes a flag must be told, not silently obeyed.

---

## US-01: Unknown top-level flag is rejected

### Problem

Sofia Marino is a platform operator who drives kaleidoscope-cli from a
shell and from cron wrappers. When she fat-fingers a top-level flag (types
`--observ-otlp` instead of `--observe-otlp`, or `--bogus`), she needs the
binary to fail loudly. If it accepted the typo silently and exited 0, she
would believe her command ran as intended when it did not.

### Who

- Platform operator | running kaleidoscope-cli directly in a shell |
  motivated to trust the exit code in scripts and cron.

### Solution

When the first argument is a `--`-prefixed token that is not `--help` or
`-h`, the CLI exits non-zero and writes a usage error to stderr. (Observed
today: this already happens via the unknown-subcommand arm, exit 2.) This
story pins the contract and adds the missing acceptance test.

### Elevator Pitch

Before: a mistyped top-level flag has no covered, anchored contract proving it is rejected.
After: run `kaleidoscope-cli --bogus` -> sees `kaleidoscope-cli: unknown ...` on stderr and exit code 2.
Decision enabled: Sofia knows she mistyped and that nothing ran, so she re-runs with the correct flag.

### Domain Examples

#### 1: Happy Path (rejection) — Sofia types `kaleidoscope-cli --bogus`; CLI writes a usage error naming the bad token to stderr and exits 2; nothing is read or written

#### 2: Edge Case — Sofia types `kaleidoscope-cli --observ-otlp /tmp/m.ndjson` (single-letter typo of a real flag); CLI rejects it the same way rather than treating the path as data

#### 3: Boundary — Sofia types `kaleidoscope-cli --help`; CLI prints the full usage block and exits 0 (the help path is NOT a rejection and must stay 0)

### UAT Scenarios (BDD)

#### Scenario: Operator is told when a top-level flag is mistyped

```gherkin
Given Sofia has built kaleidoscope-cli
When she runs `kaleidoscope-cli --bogus`
Then the process exits with a non-zero code
And stderr contains a usage error naming the unknown token
```

#### Scenario: Asking for help is not treated as an error

```gherkin
Given Sofia has built kaleidoscope-cli
When she runs `kaleidoscope-cli --help`
Then the process exits 0
And stderr contains the usage block listing the ingest and read subcommands
```

### Acceptance Criteria

- [ ] `kaleidoscope-cli --bogus` exits non-zero (pinned value per DESIGN flag 2).
- [ ] Its stderr contains a usage error that names the unknown token.
- [ ] `kaleidoscope-cli --help` still exits 0 with the usage block.

### Outcome KPIs

- **Who**: operators running kaleidoscope-cli with a mistyped top-level flag.
- **Does what**: receive a non-zero exit plus a usage error instead of silent success.
- **By how much**: 100% of mistyped top-level flags rejected (0 silent acceptances).
- **Measured by**: acceptance test asserting exit code and stderr substring; EDD verifier K11 re-check.
- **Baseline**: behaviour correct today but unanchored (contract lost in revert e3a8cad).

### Technical Notes

- Code path: `main.rs:70` `Some(other)` arm already returns
  `ExitCode::from(2)` with a usage error. This story is re-anchor only
  unless DESIGN changes the message wording (flag 3).

---

## US-02: Unknown flag on a real subcommand is rejected

### Problem

Diego Herrera is a platform operator who runs `kaleidoscope-cli read acme
/data --bogus` after copying a half-remembered command from a runbook.
Today the subcommand flag scanner silently ignores `--bogus` and the read
runs as if the flag were absent, exiting 0. Diego believes the flag took
effect. This is the real defect: silent acceptance of an unknown
subcommand-level flag.

### Who

- Platform operator | running a real subcommand with an extra or mistyped
  flag | motivated to trust that a 0 exit means every flag was honoured.

### Solution

Each subcommand validates its argv against the known-flag set for that
subcommand. A `--`-prefixed token that no scanner consumes causes a
non-zero exit and a usage error on stderr. This is the one additive code
change the feature requires.

### Elevator Pitch

Before: `kaleidoscope-cli read acme /data --bogus` silently ignores `--bogus` and exits 0.
After: run `kaleidoscope-cli read acme /data --bogus` -> sees a usage error naming `--bogus` on stderr and a non-zero exit.
Decision enabled: Diego learns the flag was never honoured, so he fixes the runbook rather than trusting a wrong result.

### Domain Examples

#### 1: Happy Path (rejection) — Diego runs `kaleidoscope-cli read acme /data --bogus`; CLI rejects with a usage error naming `--bogus` and exits non-zero; no records are read

#### 2: Edge Case — Diego runs `kaleidoscope-cli stats acme /data --sicne 2026-01-01T00:00:00Z` (typo of `--since`); CLI rejects `--sicne` rather than silently querying the full range

#### 3: Boundary — Diego runs `kaleidoscope-cli read acme /data --observe-otlp /tmp/m.ndjson --bogus`; the known `--observe-otlp` is consumed but `--bogus` still triggers rejection (a valid flag does not mask an adjacent invalid one)

### UAT Scenarios (BDD)

#### Scenario: Operator is told when a subcommand flag is unknown

```gherkin
Given Diego has built kaleidoscope-cli
And a data directory with at least one ingested record for tenant "acme"
When he runs `kaleidoscope-cli read acme <data_dir> --bogus`
Then the process exits with a non-zero code
And stderr contains a usage error naming the unknown flag
And no records are written to stdout
```

#### Scenario: A mistyped known flag is not silently accepted

```gherkin
Given Diego has built kaleidoscope-cli
And a data directory for tenant "acme"
When he runs `kaleidoscope-cli stats acme <data_dir> --sicne 2026-01-01T00:00:00Z`
Then the process exits with a non-zero code
And stderr contains a usage error naming the unknown flag
```

#### Scenario: A valid flag next to an invalid flag does not mask the invalid one

```gherkin
Given Diego has built kaleidoscope-cli
And a data directory with at least one ingested record for tenant "acme"
When he runs `kaleidoscope-cli read acme <data_dir> --observe-otlp /tmp/m.ndjson --bogus`
Then the process exits with a non-zero code
And stderr contains a usage error naming the unknown flag
```

### Acceptance Criteria

- [ ] `read acme <data_dir> --bogus` exits non-zero with a usage error naming `--bogus`.
- [ ] No stdout records are produced when an unknown subcommand flag is present.
- [ ] A mistyped known flag (`--sicne`) is rejected, not silently ignored.
- [ ] A valid flag adjacent to an invalid one does not suppress rejection of the invalid one.

### Outcome KPIs

- **Who**: operators running a real subcommand with an unknown or mistyped flag.
- **Does what**: receive a non-zero exit plus a usage error instead of a silent 0.
- **By how much**: 100% of unknown subcommand-level flags rejected (0 silent acceptances); current baseline is 0% rejection.
- **Measured by**: acceptance tests per scenario; EDD verifier K11 re-check.
- **Baseline**: today every unknown subcommand-level flag is silently ignored and exits 0 (the gap).

### Technical Notes

- Code path: `parse_observe_otlp`, `parse_flag_iso`, and the positional
  parsers in `main.rs` scan only for known flags and skip the rest. The
  additive fix teaches each subcommand to detect a `--`-prefixed token
  that no scanner consumed. DESIGN pins the fix shape (per-subcommand or
  shared helper) and the exit code (flag 2).
- This is the only story with a genuine code change. Sizing: small,
  bounded to argv validation in the subcommand wrappers.

---

## US-03: Unknown subcommand is rejected

### Problem

Sofia Marino types `kaleidoscope-cli stat acme /data` (meaning `stats`).
She needs the CLI to reject the unknown verb loudly rather than do nothing
silently or panic, so she recognises the typo immediately.

### Who

- Platform operator | invoking a subcommand by name | motivated to catch a
  mistyped verb before it reaches a script.

### Solution

When the first argument is not a known subcommand and not a help flag, the
CLI writes a usage error naming the unknown verb to stderr and exits
non-zero. (Observed today: already implemented, exit 2.) This story pins
the contract and adds the missing acceptance test.

### Elevator Pitch

Before: a mistyped subcommand verb has no anchored contract proving it is rejected.
After: run `kaleidoscope-cli stat acme /data` -> sees `kaleidoscope-cli: unknown subcommand "stat"` on stderr and exit 2.
Decision enabled: Sofia recognises the typo and re-runs with `stats`.

### Domain Examples

#### 1: Happy Path (rejection) — Sofia runs `kaleidoscope-cli stat acme /data`; CLI writes `unknown subcommand "stat"` plus usage to stderr and exits 2

#### 2: Edge Case — Sofia runs `kaleidoscope-cli migrate-tier ...` (plausible but nonexistent verb); CLI rejects it the same way

#### 3: Boundary — Sofia runs `kaleidoscope-cli` with no arguments at all; CLI prints usage and exits 0 (the no-arg help path is NOT a rejection)

### UAT Scenarios (BDD)

#### Scenario: Operator is told when a subcommand verb is unknown

```gherkin
Given Sofia has built kaleidoscope-cli
When she runs `kaleidoscope-cli stat acme /data`
Then the process exits with a non-zero code
And stderr contains a usage error naming the unknown subcommand
```

#### Scenario: No arguments shows help and is not an error

```gherkin
Given Sofia has built kaleidoscope-cli
When she runs `kaleidoscope-cli` with no arguments
Then the process exits 0
And stderr contains the usage block
```

### Acceptance Criteria

- [ ] `kaleidoscope-cli stat acme /data` exits non-zero with a usage error naming `stat`.
- [ ] `kaleidoscope-cli` with no arguments exits 0 with the usage block.

### Outcome KPIs

- **Who**: operators invoking a mistyped subcommand verb.
- **Does what**: receive a non-zero exit plus a usage error naming the verb.
- **By how much**: 100% of unknown subcommands rejected (0 silent or panicking outcomes).
- **Measured by**: acceptance test asserting exit code and stderr substring; EDD verifier K11 re-check.
- **Baseline**: behaviour correct today but unanchored (contract lost in revert e3a8cad).

### Technical Notes

- Code path: `main.rs:70` `Some(other)` arm. Re-anchor only unless DESIGN
  changes wording (flag 3).

---

## US-04: Known flags and subcommands still work (regression guard)

### Problem

The US-02 fix changes how unknown subcommand-level flags are handled. If it
is not strictly additive, it could break a known flag (for example reject
`--observe-otlp` because the new validation is too eager). Marcus Bauer, a
platform operator, relies on the existing flags daily and must see no
behavioural change for correct input.

### Who

- Platform operator | using the existing, documented flags and subcommands
  correctly | motivated to keep their working runbooks working.

### Solution

Pin the behaviour of representative known invocations so the US-02 change
is provably additive. Every known flag and subcommand keeps its current
exit code and output.

### Elevator Pitch

Before: there is no anchored proof that the unknown-flag fix leaves correct input untouched.
After: run `kaleidoscope-cli read acme <data_dir> --observe-otlp /tmp/m.ndjson` -> sees records on stdout and exit 0, exactly as before.
Decision enabled: Marcus trusts that upgrading does not break his existing runbooks.

### Domain Examples

#### 1: Happy Path — Marcus runs `kaleidoscope-cli read acme <data_dir> --observe-otlp /tmp/m.ndjson`; records print to stdout, exit 0, metric line appended (unchanged)

#### 2: Edge Case — Marcus runs `kaleidoscope-cli stats acme <data_dir> --since 2026-01-01T00:00:00Z --until 2026-02-01T00:00:00Z`; the windowed summary prints and exits 0 (both known flags still consumed)

#### 3: Boundary — Marcus runs `kaleidoscope-cli list-items acme <data_dir> hot` (no flags); behaviour is byte-equivalent to before the fix

### UAT Scenarios (BDD)

#### Scenario: A known subcommand with a known flag still succeeds

```gherkin
Given Marcus has built kaleidoscope-cli
And a data directory with one ingested record for tenant "acme"
When he runs `kaleidoscope-cli read acme <data_dir> --observe-otlp <path>`
Then the process exits 0
And stdout contains the ingested record
And the metric file at <path> receives a query metric line
```

#### Scenario: Both time-range flags are still accepted together

```gherkin
Given Marcus has built kaleidoscope-cli
And a data directory for tenant "acme"
When he runs `kaleidoscope-cli stats acme <data_dir> --since 2026-01-01T00:00:00Z --until 2026-02-01T00:00:00Z`
Then the process exits 0
And stdout contains the records summary
```

### Acceptance Criteria

- [ ] `read` with `--observe-otlp` still exits 0 and emits records plus a metric line.
- [ ] `stats` with `--since` and `--until` together still exits 0 with the summary.
- [ ] A no-flag subcommand invocation is byte-equivalent to pre-fix behaviour.

### Outcome KPIs

- **Who**: operators using existing flags and subcommands correctly.
- **Does what**: observe identical behaviour before and after the US-02 fix.
- **By how much**: 0 regressions across the documented flag and subcommand set.
- **Measured by**: existing acceptance suite stays green plus the regression scenarios above.
- **Baseline**: all known invocations work today; the fix must not change that.

### Technical Notes

- The existing `observe_otlp_*`, `stats_*`, `read_time_range`, and per-
  subcommand tests already cover much of this. US-04 adds explicit
  adjacency cases that pin "valid input is not collateral damage of the
  US-02 validation".
