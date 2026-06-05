<!-- markdownlint-disable MD024 -->

# User Stories — beacon-sighup-reload-v0

British English throughout, no em dashes.

## System Constraints

These cross-cutting constraints apply to every story below.

- **The documented contract is the spec (ADR-0034 "Reload semantics").**
  On SIGHUP the loader re-reads `--rules`; the new catalogue must
  validate completely (every file parses + at least one rule loads); if
  validation fails the active catalogue stays as-is and a diagnostic is
  emitted; the swap is atomic so the evaluator never sees a half-loaded
  catalogue. slice-02 and D3 add: "the previous catalogue stays active
  until the new one validates."
- **ADR-0037 is inviolable.** The pure
  `transition(state, outcome, rule, now) -> (next_state, emission)` in
  `crates/beacon/src/state_machine.rs` stays pure, total, and
  side-effect-free. The SIGHUP handler is a composition-root concern
  (ADR-0037 explicitly lists "the SIGHUP signal handler that triggers
  rule-set reload" as orchestrator responsibility), not part of the
  evaluator. No story may add I/O or signal logic inside `transition`.
- **No new CLI surface and no new HTTP surface.** The operator entry
  point is the POSIX signal `kill -HUP <beacon-server pid>`. The
  `--rules DIR` argument already exists (`main.rs:45-46`). The only new
  operator-visible outputs are structured log events on a successful
  reload and on a refused reload.
- **Reuse the existing loader and durable store.** `load_rules`
  (`crates/beacon/src/loader.rs:111`) already returns
  `LoadOutcome { rules, diagnostics }` and already reports-and-skips a
  single malformed file (B01 posture). The durable `RuleStateStore`
  (`crates/beacon/src/state_store.rs`) already holds each rule's state
  keyed by name and already drops states for rules no longer in config
  on recovery (`main.rs:139-144`). Do not reinvent either.
- **RuleState variants** (confirmed `state_machine.rs:47-61`):
  `Inactive`, `Pending { since: SystemTime }`,
  `Firing { since: SystemTime }`. `Resolved` is an `Emission`, not a
  state. The `since` instant is the in-flight payload that must survive
  a swap for a surviving rule.
- **Per-feature mutation testing 100%** on modified files (CLAUDE.md,
  ADR-0005 Gate 5). British English, no em dashes.

---

## US-01: Apply edited rules with SIGHUP, no restart (slice 01)

### Problem

Sofia Okonkwo is the on-call platform operator for a payments cluster.
The `payments-latency` rule's threshold has been too noisy, so she edits
`payments.toml` in the `--rules` directory and adds a new
`checkout-error-rate` rule. The beacon-server runbook, and beacon's own
architecture docs, tell her to apply the change with
`kill -HUP <pid>`. She does. Nothing happens: the new rule never fires,
the edited threshold is never picked up, and there is no error to tell
her why. beacon-server installed only SIGINT and SIGTERM handlers
(`main.rs:177,179`) and loaded the catalogue once at startup
(`main.rs:65`); SIGHUP is not the promised reload. Her only recourse is
a full process restart, which on a durable-state build means a recovery
cycle and a window where the daemon is not evaluating anything. The docs
promised hot-reload; the binary silently ignored her.

### Elevator Pitch

- **Before**: Sofia edits the rules dir, runs `kill -HUP <pid>`, and
  the running beacon-server keeps the stale catalogue with no signal that
  her edit was ignored. To apply rules she must restart the process.
- **After**: Sofia edits the rules dir and runs
  `kill -HUP <beacon-server pid>`; within one evaluation interval the
  newly-added `checkout-error-rate` rule begins firing and the removed
  rule stops, with no restart, and a structured log event
  `rules reloaded: rules_loaded=7 added=1 removed=1` confirms what
  changed.
- **Decision enabled**: Sofia can decide to ship a rule edit to a live
  alerting daemon and trust it took effect, choosing iterative tuning
  over a disruptive restart.

### Who

- On-call platform operator (Sofia Okonkwo) | editing the `--rules`
  directory of a running beacon-server on a production cluster | wants to
  tune and extend alerting rules without a restart or an evaluation gap.

### Solution

beacon-server installs a SIGHUP handler in addition to SIGINT and
SIGTERM. On SIGHUP it re-runs `load_rules` on the `--rules` directory;
if the new catalogue validates (at least one rule loads), it atomically
swaps the live catalogue: it stops evaluating the old rule set and
starts evaluating the new one, with no restart. A newly-added rule
begins its evaluation lifecycle; a removed rule stops being evaluated.
A structured log event reports the outcome (rules_loaded count, and what
changed) so Sofia and a black-box harness can both observe the reload
took effect.

### Domain Examples

#### 1: Happy Path — added rule begins firing after SIGHUP

beacon-server is running with one rule, `service-down`
(`up{job="payments"} == 0`), currently `Firing`. Sofia adds
`checkout-error-rate.toml`
(`rate(http_errors{route="/checkout"}[5m]) > 0.05`) to the rules dir and
runs `kill -HUP 8431`. Within one evaluation interval the
checkout-error-rate query returns a non-empty vector, the rule
transitions Inactive -> Pending -> Firing, and an incident reaches its
sink. No restart occurred. The reload event logged
`rules reloaded: rules_loaded=2 added=1 removed=0`.

#### 2: Edge Case — removed rule stops evaluating

beacon-server is running with `service-down` and `disk-pressure`. Sofia
deletes `disk-pressure.toml` and runs `kill -HUP 8431`. After the swap,
`disk-pressure` is no longer evaluated: its query is never issued again
and it produces no further incidents. The reload event logged
`rules reloaded: rules_loaded=1 added=0 removed=1`. The durable state
for `disk-pressure` is dropped and logged, exactly as startup recovery
drops state for rules no longer in config (`main.rs:139-144`).

#### 3: Boundary — SIGHUP with no changes is a clean no-op swap

beacon-server is running with `service-down` (Firing) and
`disk-pressure` (Inactive). Sofia runs `kill -HUP 8431` without having
edited anything (for example, a config-management tool sends SIGHUP on
every converge). The catalogue re-loads to an identical set; the swap
produces no spurious incidents and no spurious resolutions; the reload
event logged `rules reloaded: rules_loaded=2 added=0 removed=0`. (The
no-re-page guarantee for the still-Firing `service-down` is specified in
US-02.)

### UAT Scenarios (BDD)

#### Scenario: A newly-added rule begins firing after SIGHUP without a restart

```gherkin
Given beacon-server is running with pid 8431 evaluating one rule "service-down"
And the rules directory contains only "service-down.toml"
When Sofia adds "checkout-error-rate.toml" whose query is currently active
And Sofia runs "kill -HUP 8431"
Then beacon-server begins evaluating "checkout-error-rate" within one evaluation interval
And "checkout-error-rate" transitions to Firing and emits an incident to its sink
And the process is the same process (no restart occurred)
```

#### Scenario: A removed rule stops being evaluated after SIGHUP

```gherkin
Given beacon-server is running and evaluating "service-down" and "disk-pressure"
When Sofia deletes "disk-pressure.toml" from the rules directory
And Sofia runs "kill -HUP <pid>"
Then beacon-server stops evaluating "disk-pressure"
And "disk-pressure" issues no further backend queries and emits no further incidents
And the durable state for "disk-pressure" is dropped and logged
```

#### Scenario: SIGHUP with no on-disk change swaps cleanly with no spurious emissions

```gherkin
Given beacon-server is running and evaluating "service-down" and "disk-pressure"
And the rules directory has not changed since startup
When Sofia runs "kill -HUP <pid>"
Then the catalogue reloads to the identical rule set
And no spurious Firing incident is emitted
And no spurious Resolved incident is emitted
```

#### Scenario: A successful reload emits a structured event naming what changed

```gherkin
Given beacon-server is running and evaluating one rule "service-down"
When Sofia adds one rule and removes none, then runs "kill -HUP <pid>"
And the new catalogue validates
Then beacon-server emits a structured reload event at INFO
And the event carries the loaded rule count (rules_loaded=2)
And the event names what changed (added=1, removed=0)
```

### Acceptance Criteria

- [ ] beacon-server installs a SIGHUP handler in addition to SIGINT and
  SIGTERM; SIGHUP triggers a catalogue reload, not process termination
  and not a silent no-op.
- [ ] On a SIGHUP whose re-loaded catalogue validates, the live rule set
  is replaced without a process restart: a newly-added rule begins
  evaluation and a removed rule stops, within one evaluation interval.
- [ ] A removed rule issues no further backend queries and emits no
  further incidents after the swap; its durable state is dropped and
  logged.
- [ ] A SIGHUP with no on-disk change produces no spurious Firing and no
  spurious Resolved emissions.
- [ ] A successful reload emits one structured INFO event carrying
  `rules_loaded` and a description of what changed (added/removed
  counts), observable by both an operator and a black-box harness.

### Outcome KPIs

- **Who**: on-call platform operators editing a running beacon-server's
  rules directory.
- **Does what**: apply a rule edit (add, remove, or change) to the live
  daemon via SIGHUP instead of restarting the process.
- **By how much**: SIGHUP-applied rule changes take effect within one
  evaluation interval in 100% of valid-catalogue reloads; operator
  restarts performed solely to apply a rule edit fall to zero.
- **Measured by**: black-box harness asserting B03 (add rule, SIGHUP,
  assert it fires) plus the structured reload event in beacon's logs.
- **Baseline**: today SIGHUP applies 0% of edits (unhandled); applying
  any edit requires a full restart.

### Technical Notes (Optional)

- Reuse `load_rules` (`crates/beacon/src/loader.rs:111`) verbatim for
  the re-read; it already returns `LoadOutcome { rules, diagnostics }`
  and reports-and-skips malformed files.
- The SIGHUP handler is a composition-root concern (ADR-0037). Install
  it alongside the existing SIGINT/SIGTERM arms in the
  `tokio::select!` (`main.rs:187`). Install it BEFORE spawning the
  per-rule evaluation tasks so an early SIGHUP during startup does not
  hit the OS default disposition (terminate).
- The atomic-swap mechanism (stop old tasks, start new tasks without a
  double-evaluation or missed-evaluation window) and the in-flight
  alert-state preservation are DESIGN decisions; see US-02 and
  wave-decisions.md "FLAGGED for DESIGN".
- Depends on the durable `RuleStateStore` seam
  (beacon-durable-alert-state-v0, delivered) for state continuity across
  the swap.

---

## US-02: Refuse a malformed reload and keep the previous catalogue (slice 01)

### Problem

Sofia edits `payments.toml` under time pressure during an incident and
introduces a typo: she writes `for_duraton = "5m"` instead of
`for_duration`, leaving a parse error in the file. She runs
`kill -HUP <pid>` to apply her change. The danger is twofold. If beacon
crashed on the bad reload, her single fat-fingered edit would take down
the entire alerting daemon mid-incident, blinding the whole on-call team.
If beacon partially applied the catalogue, some rules would silently
vanish while others stayed, leaving an inconsistent half-catalogue with
no clear state. Worst of all, if beacon silently ignored the bad file
the way it silently ignored SIGHUP before this feature, Sofia would
believe her edit applied when it did not. The docs promise the safe
behaviour: "the previous catalogue stays active until the new one
validates" (slice-02), and ADR-0034 requires a diagnostic plus an atomic
swap that "never sees a half-loaded catalogue."

### Elevator Pitch

- **Before**: A malformed edit plus SIGHUP either has no defined safe
  behaviour or risks taking the daemon down; Sofia cannot trust that a
  bad edit leaves her protected.
- **After**: Sofia's malformed edit plus `kill -HUP <pid>` leaves
  beacon-server running on the previous catalogue, still firing the
  alerts it was firing, and logs a refusal event naming the file and the
  parse error: `reload refused: payments.toml: unknown field
  'for_duraton'; did you mean 'for_duration'?; previous catalogue
  retained`.
- **Decision enabled**: Sofia can fix the typo and re-send SIGHUP,
  knowing a bad edit never silently degrades her alerting and never
  takes the daemon down.

### Who

- On-call platform operator (Sofia Okonkwo) | applies a rule edit that
  contains a parse error to a running beacon-server during an incident |
  needs the daemon to stay up, keep its current alerts, and tell her
  precisely what was wrong.

### Solution

On SIGHUP, after re-running `load_rules`, beacon-server validates the
result before any swap. The catalogue is valid only if at least one rule
loads. If the re-loaded catalogue is invalid (a file the operator
clearly intended as a rule failed to parse, or zero rules loaded),
beacon-server refuses the reload: it keeps the previous catalogue active,
does not crash, does not partially apply, and emits a structured refusal
event naming the offending file and the parse error (reusing the loader's
existing `LoaderDiagnostic::display`, which already carries the "did you
mean" suggestion). A rule that was Firing before the refused reload keeps
firing with its original `since`; on-call is not re-paged and no alert is
silently dropped. The whole reload is all-or-nothing.

### Domain Examples

#### 1: Happy Path of the safety property — malformed edit keeps previous catalogue

beacon-server is running with `service-down` (Firing since 09:14:02) and
`disk-pressure` (Inactive). Sofia edits `payments.toml` to add a rule but
mistypes `for_duraton = "5m"`. She runs `kill -HUP 8431`. The re-load
produces a `LoaderDiagnostic` for `payments.toml` and the new catalogue
does not include the intended rule. beacon-server refuses the swap: it
stays on the previous catalogue, `service-down` is still Firing since
09:14:02 (no second incident, no re-page), `disk-pressure` is still being
evaluated, the process did not exit, and the refusal event logged
`reload refused: payments.toml: unknown field 'for_duraton'; did you
mean 'for_duration'?; previous catalogue retained`.

#### 2: Edge Case — every rule file deleted, reload to empty is refused

beacon-server is running with `service-down` (Firing). A botched deploy
empties the rules directory, then a config tool sends `kill -HUP 8431`.
The re-load yields zero rules. Because a valid catalogue requires at
least one rule (mirroring the startup `has_any_rules` refusal,
`main.rs:77-84`), beacon-server refuses the reload, keeps `service-down`
Firing, and logs `reload refused: rules directory yielded no rules;
previous catalogue retained`. The daemon keeps alerting; it does not go
dark.

#### 3: Boundary — partly-broken catalogue applies the good rules, names the bad file

beacon-server is running with `service-down`. Sofia adds two files:
`checkout-error-rate.toml` (valid) and `inventory.toml` (a parse error).
She runs `kill -HUP 8431`. The loader's report-and-skip posture (B01)
means the re-loaded catalogue contains `service-down` and
`checkout-error-rate` (valid, so the catalogue validates: at least one
rule loaded) and a diagnostic for `inventory.toml`. beacon-server applies
the valid catalogue (checkout-error-rate begins evaluating) AND logs the
diagnostic for `inventory.toml` so Sofia knows one file was skipped. The
swap is not refused, because the catalogue as a whole validated; the
boundary is "at least one rule loaded", consistent with the startup
contract. (Whether a per-file diagnostic should escalate to a full
refusal is named for DESIGN in wave-decisions.md; DISCUSS pins the
startup-consistent "at least one rule loaded" rule and requires the
diagnostic be surfaced either way.)

### UAT Scenarios (BDD)

#### Scenario: A malformed reload keeps the previous catalogue active

```gherkin
Given beacon-server is running and "service-down" has been Firing since 09:14:02
When Sofia introduces a parse error into "payments.toml" in the rules directory
And Sofia runs "kill -HUP <pid>"
Then beacon-server keeps evaluating the previous catalogue
And "service-down" is still Firing since 09:14:02
And the process has not exited
```

#### Scenario: A malformed reload does not re-page on-call

```gherkin
Given "service-down" has been Firing since 09:14:02 with one incident already sent
When a malformed reload is refused via SIGHUP
Then no second Firing incident is emitted for "service-down"
And no Resolved incident is emitted for "service-down"
And on-call is not re-paged
```

#### Scenario: A refused reload emits a diagnostic naming the problem

```gherkin
Given beacon-server is running on a valid catalogue
When Sofia introduces "unknown field for_duraton" into "payments.toml"
And Sofia runs "kill -HUP <pid>"
Then beacon-server emits a structured refusal event naming "payments.toml"
And the event carries the parse error and the "did you mean for_duration" suggestion
And the event states the previous catalogue was retained
```

#### Scenario: A reload that yields zero rules is refused, daemon keeps alerting

```gherkin
Given beacon-server is running and evaluating "service-down" which is Firing
When the rules directory is emptied of all rule files
And Sofia runs "kill -HUP <pid>"
Then beacon-server refuses the reload because no rules loaded
And "service-down" continues to be evaluated and stays Firing
And a refusal event states no rules were found and the previous catalogue was retained
```

#### Scenario: A rule unchanged across a valid reload keeps its in-flight state and does not re-page

```gherkin
Given "service-down" has been Firing since 09:14:02
And the rules directory is edited to add an unrelated new rule, leaving "service-down" unchanged
When Sofia runs "kill -HUP <pid>" and the new catalogue validates and is swapped in
Then "service-down" is still Firing with its original since of 09:14:02
And no second Firing incident is emitted for "service-down"
And on-call is not re-paged for "service-down"
```

### Acceptance Criteria

- [ ] On a SIGHUP whose re-loaded catalogue does not validate (zero
  rules loaded), beacon-server keeps the previous catalogue active, does
  not crash, and does not partially apply (all-or-nothing).
- [ ] A refused reload emits one structured refusal event naming the
  offending file and the parse error (with the loader's "did you mean"
  suggestion when applicable) and stating the previous catalogue was
  retained.
- [ ] A rule that was Firing before a refused reload keeps firing with
  its original `since`; no second Firing incident and no Resolved
  incident is emitted; on-call is not re-paged.
- [ ] **(In-flight-state decision, FLAGGED for DESIGN.)** A rule present
  (matched by name) in both the previous and the successfully-swapped
  catalogue preserves its in-flight state (`Pending`/`Firing`) and its
  original `since` across the swap; it does not emit a spurious second
  Firing and does not reset to Inactive. A removed rule stops; a
  newly-added rule starts Inactive. DESIGN owns the mechanism and the
  matching-key sub-decisions (see wave-decisions.md).
- [ ] A reload that yields at least one valid rule alongside a malformed
  file applies the valid catalogue and still surfaces the diagnostic for
  the skipped file (report-and-skip, B01, consistent with the startup
  contract).

### Outcome KPIs

- **Who**: on-call platform operators applying rule edits to a running
  beacon-server, including under incident pressure.
- **Does what**: recover from a malformed edit without the daemon
  crashing, without losing active alerts, and with a diagnostic that
  names the problem.
- **By how much**: 100% of malformed reloads leave the daemon running on
  the previous catalogue with a diagnostic; 0% cause a crash, a partial
  apply, or a re-page of a surviving Firing rule.
- **Measured by**: black-box harness asserting the malformed-reload-
  keeps-previous negative (co-equal with B03) plus the structured refusal
  event; assertion that a surviving Firing rule's `since` is unchanged
  across a valid swap.
- **Baseline**: today there is no SIGHUP reload at all, so there is no
  defined safe failure path; a restart to apply an edit risks a recovery
  window and, on a bad edit, a failed start.

### Technical Notes (Optional)

- Reuse `LoaderDiagnostic::display` (`loader.rs:75-84`) for the refusal
  event text; it already formats `file: message` plus the "did you mean"
  suggestion.
- The "valid catalogue" bar is "at least one rule loaded", mirroring the
  startup `has_any_rules` refusal (`main.rs:77-84`) so SIGHUP and startup
  share one contract. The pure `transition` is untouched.
- The in-flight-state preservation mechanism, the inhibition-resolver
  rebuild, and the task-lifecycle race are DESIGN decisions; see
  wave-decisions.md "FLAGGED for DESIGN" (sub-decisions 1-4).
- Depends on US-01 (the SIGHUP handler and swap path) and on the durable
  `RuleStateStore` seam for state continuity.
