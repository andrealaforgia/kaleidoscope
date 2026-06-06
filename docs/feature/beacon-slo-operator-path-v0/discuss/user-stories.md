<!-- markdownlint-disable MD024 -->

# User Stories — beacon-slo-operator-path-v0

British English throughout, no em dashes.

## Persona

**Priya Nadkarni**, SRE on the payments platform team. She runs
`beacon-server` against her Prometheus-compatible PromQL backend with a
`--rules` directory of TOML files under Git. She already hand-authors
`[[rules]]` (threshold alerts) and tunes them live via `kill -HUP`. She
knows the Google SRE workbook multi-window multi-burn-rate methodology and
wants burn-rate alerting on her checkout SLO, but she does NOT want to
hand-write the four threshold/window PromQL rules and keep them in sync by
hand. She is operator-critical: a degenerate always-fire rule reaching her
on-call rotation is a self-inflicted incident.

## System Constraints (cross-cutting, apply to all stories)

- The operator-invocable surface is the existing beacon rule file (TOML
  under `--rules DIR`) plus the running `beacon-server`, started fresh or
  reloaded via the existing `kill -HUP <pid>` path (ADR-0063). No new CLI
  surface, no new HTTP surface.
- One declared SLO synthesises exactly FOUR MWMBR rules
  (`synthesise_slo`, slo.rs:106-156, `MWMBR_TABLE` slo.rs:64-93): page
  `14.4 / 1h / 5m` (critical), page `6.0 / 6h / 30m` (critical), ticket
  `3.0 / 1d / 2h` (warning), ticket `1.0 / 3d / 6h` (warning).
- Synthesis is deterministic: the same SLO declaration always yields
  byte-identical rules. No clock, no RNG.
- A malformed SLO is REFUSED at load with a clear message; the previous
  catalogue is kept (ADR-0063 all-or-nothing). No degenerate always-fire
  rule ever reaches evaluation.
- The merge must not break the existing hand-authored `[[rules]]` path:
  `slice_05_slo_burn_rate.rs` and the beacon-server rule tests stay green.
- The engine (`synthesise_slo`, `MWMBR_TABLE`), the loader, the catalogue,
  and the reload path are REUSED verbatim. The only new code is the
  `[[slo]]` wire shape, its deserialisation, its validation, and the merge.
- This wave pins observable behaviour. The exact TOML key names, the
  name-collision policy, and the exact rejection-message wording are
  flagged for DESIGN (`wave-decisions.md` FLAG-1..FLAG-4). The scenarios
  below use plausible key names as illustration; DESIGN owns the final
  schema.
- OUT OF SCOPE: the beacon flush-only-not-fsync durability caveat (a
  separate four-quadrants Q2 issue); SMTP sinks; 7-day / 90-day budgets.

---

## US-01: An SLO declared in the file synthesises and loads

### Problem

Priya wants burn-rate alerting on her checkout SLO. The MWMBR engine
that produces the four correct rules already exists and is tested, but she
cannot reach it: there is no way to declare an SLO in her rule file. Today
she would have to hand-write four PromQL threshold/window rules and keep
their thresholds in sync with her availability target by hand, which is
exactly the error-prone toil the engine was built to remove. The headline
SLO feature ships dead.

### Who

- Priya Nadkarni | SRE on payments, runs `beacon-server` with a `--rules`
  TOML directory under Git | wants correct burn-rate alerting without
  hand-writing the four rules.

### Solution

Extend the rule-file loader to accept an `[[slo]]` table. Each declared
SLO is deserialised into a `Slo`, validated, and expanded via the existing
`synthesise_slo` into its four MWMBR rules, which are merged into the
catalogue `beacon-server` loads and evaluates. The operator declares the
SLO once and gets four correct burn-rate rules.

### Elevator Pitch

- **Before:** Priya adds an `[[slo]]` block to `checkout.toml` and starts
  `beacon-server`; today the file fails to parse ("unknown field `slo`")
  and the whole file is skipped, so she gets NO checkout alerting at all and
  a confusing diagnostic.
- **After:** Priya adds the `[[slo]]` block, starts `beacon-server` against
  her `--rules` dir, and sees on stderr `beacon-server starting
  rules_loaded=4` plus, when her checkout error rate burns fast, a page-level
  incident emitted to her sink, all without her hand-writing a single
  threshold rule.
- **Decision enabled:** Priya decides her checkout SLO is now covered by
  correct multi-window burn-rate alerting and removes the four hand-authored
  rules she would otherwise have maintained.

### Domain Examples

#### 1: Happy Path

Priya declares one SLO in `checkout.toml`: service `checkout`, good-events
`http_requests_total{job="checkout",code!~"5.."}`, total-events
`http_requests_total{job="checkout"}`, target availability `0.999`, budget
period `30d`, one webhook sink. She starts `beacon-server --rules ./rules
--backend http://localhost:9090/api/v1`. The loader synthesises four rules
named `checkout_page_1h_5m`, `checkout_page_6h_30m`, `checkout_ticket_1d_2h`,
`checkout_ticket_3d_6h`. stderr shows `beacon-server starting rules_loaded=4`.

#### 2: Edge Case (SLO plus hand-authored rules in the same dir)

Priya's `rules/` dir has `checkout.toml` (one `[[slo]]`) and
`disk.toml` (two hand-authored `[[rules]]`). She starts the server. The
loader synthesises four rules from the SLO and loads the two hand-authored
rules, for `rules_loaded=6`. (Full coexistence behaviour is US-04.)

#### 3: Boundary (the tightest realistic target)

Priya declares a `checkout` SLO with target availability `0.9999` (four
nines). The budget is `1 - 0.9999 = 0.0001`; the page `14.4` threshold
emits the limit `0.00144` in the synthesised PromQL (matching
slice_05:273). The four rules load. The synthesis does not choke on the
tighter budget.

### UAT Scenarios (BDD)

#### Scenario: An SLO declared in the file synthesises and loads

```gherkin
Given Priya's rules directory contains "checkout.toml" with one [[slo]]
  table declaring service "checkout", target availability 0.999, a 30d
  budget, and the good-events and total-events PromQL queries
When she starts beacon-server against that rules directory
Then four burn-rate rules are loaded into the live catalogue
And they are named checkout_page_1h_5m, checkout_page_6h_30m,
  checkout_ticket_1d_2h, and checkout_ticket_3d_6h
And the startup log reports rules_loaded reflecting the four-rule expansion
```

#### Scenario: A fast burn pages, a slow burn tickets

```gherkin
Given Priya's "checkout" SLO is loaded with its four synthesised rules
When the checkout error rate sustains a 14.4x budget burn over the page
  windows
Then a critical page-level incident is emitted to her sink
And when instead the error rate sustains only a slow 1x-3x burn
Then a warning ticket-level incident is emitted, not a page
```

#### Scenario: The synthesised rules are byte-identical across restarts

```gherkin
@property
Given Priya's "checkout" SLO declaration is unchanged on disk
When beacon-server loads it on two separate starts
Then the four synthesised rules are byte-identical between the two loads
```

### Acceptance Criteria

- [ ] One `[[slo]]` table in a rule file loads four synthesised MWMBR rules
      into the live catalogue.
- [ ] The four rules carry the canonical workbook thresholds and windows
      (14.4/1h/5m, 6.0/6h/30m, 3.0/1d/2h, 1.0/3d/6h) and the names
      `{service}_{page|ticket}_{long}_{short}`.
- [ ] A fast burn emits a critical page incident; a slow burn emits a
      warning ticket incident, to the SLO's declared sinks.
- [ ] The synthesised rules are deterministic across loads.
- [ ] The startup `rules_loaded` count reflects the four-rule expansion.

### Outcome KPIs

- **Who**: SRE operators running beacon-server with a `--rules` directory.
- **Does what**: declare an SLO and get correct burn-rate alerting without
  hand-writing the four MWMBR rules.
- **By how much**: SLO operator-reachability goes from 0% (impossible) to
  100% (one `[[slo]]` table yields four loaded rules); declared SLOs
  requiring zero hand-authored MWMBR rules goes from 0 to all of them.
- **Measured by**: an acceptance test that loads an `[[slo]]` file and
  asserts four named rules in the live catalogue; the `rules_loaded`
  startup event.
- **Baseline**: 0 operators can declare an SLO; the engine is library-only
  and unreachable.

### Technical Notes

- REUSE `synthesise_slo` (slo.rs:106-156) and `MWMBR_TABLE` (slo.rs:64-93)
  verbatim. No engine changes.
- The `Slo` struct (slo.rs:37-57) is the deserialisation target.
- DESIGN owns the `[[slo]]` TOML key names and the `FileShape` extension
  (FLAG-1).
- Depends on the existing loader (loader.rs) and catalogue
  (`LoadOutcome.rules`).

---

## US-02: A malformed target availability is refused with a clear message

### Problem

If Priya fat-fingers `target_availability = 1.0` (or `0`, or `1.5`), the
engine computes `budget = 1 - target = 0`, and the synthesised predicate
becomes `error_rate > 0`: every non-zero error rate pages. That is a
degenerate always-fire rule, an on-call storm she inflicted on herself by
a typo. Today nothing guards this because there is no operator path at all
(the gap this feature closes). The moment the operator path exists, an
unvalidated `target_availability` is a loaded gun.

### Who

- Priya Nadkarni | SRE who edits SLO targets by hand under time pressure |
  must be protected from a typo that turns into an always-fire pager rule.

### Solution

Validate `target_availability` strictly in `(0.0, 1.0)` when deserialising
an `[[slo]]`. A value at or outside the open interval is refused at load
with a clear message naming the offending value and the allowed range; the
degenerate rule is never synthesised, never loaded, never evaluated.

### Elevator Pitch

- **Before:** Priya writes `target_availability = 1.0` and starts the
  server; today the file fails to parse or (once naively wired) synthesises
  an `error_rate > 0` always-fire rule that pages on the first stray 5xx.
- **After:** Priya writes `target_availability = 1.0`, starts the server,
  and sees on stderr a refusal naming the file, the bad value, and the
  allowed `(0, 1)` range, with no always-fire rule loaded; she fixes the
  typo to `0.999` and the SLO loads.
- **Decision enabled:** Priya trusts that a target typo is caught at load,
  not at 3am by her pager, so she edits SLO targets without fear of a
  self-inflicted storm.

### Domain Examples

#### 1: Happy Path (valid target loads)

Priya declares `target_availability = 0.999`. It is strictly inside
`(0, 1)`, so the SLO synthesises and loads normally.

#### 2: Error Case (target = 1.0, the degenerate boundary)

Priya declares `target_availability = 1.0`. Budget would be `0`, predicate
would be `error_rate > 0` (always-fire). The load refuses with a message
naming `checkout.toml`, the value `1.0`, and the allowed range
`(0.0, 1.0)`. No rule is loaded from that SLO.

#### 3: Boundary (target = 0.0 and target = 1.5)

Priya declares `target_availability = 0.0` in one test and `1.5` in
another. Both are outside `(0, 1)`; both are refused at load with the same
clear range message. `0.0` would make budget `1.0` (page on any burn-rate
above the workbook multiple of a 100%-error budget, nonsensical); `1.5`
would make budget negative (a negative threshold, always-fire).

### UAT Scenarios (BDD)

#### Scenario: A target availability of 1.0 is refused with a clear message

```gherkin
Given Priya's "checkout.toml" declares an [[slo]] with target_availability
  1.0
When she starts beacon-server against that rules directory
Then the SLO is refused at load
And the diagnostic names checkout.toml, the value 1.0, and the allowed
  range of strictly greater than 0 and strictly less than 1
And no always-fire rule is synthesised or loaded from that SLO
```

#### Scenario: A target availability outside the open interval is refused

```gherkin
Given Priya's "checkout.toml" declares an [[slo]] with target_availability
  0.0
When she starts beacon-server against that rules directory
Then the SLO is refused at load with the same clear range diagnostic
And the same happens for a value of 1.5
```

#### Scenario: A valid target loads unaffected

```gherkin
Given Priya's "checkout.toml" declares an [[slo]] with target_availability
  0.999
When she starts beacon-server against that rules directory
Then the SLO synthesises its four rules and loads normally
```

### Acceptance Criteria

- [ ] `target_availability` at or outside `(0.0, 1.0)` is refused at load.
- [ ] The refusal diagnostic names the offending file, the offending value,
      and the allowed open range.
- [ ] No degenerate always-fire rule is synthesised or loaded from a
      refused SLO.
- [ ] A value strictly inside `(0.0, 1.0)` loads normally.

### Outcome KPIs

- **Who**: SRE operators declaring SLOs.
- **Does what**: have a target-availability typo caught at load instead of
  by an always-fire pager rule.
- **By how much**: degenerate always-fire SLO rules reaching evaluation
  goes from "possible once the path exists" to 0; out-of-range targets
  caught at load goes to 100%.
- **Measured by**: an acceptance test that loads an out-of-range target and
  asserts a refusal diagnostic plus zero loaded rules from that SLO.
- **Baseline**: unguarded (slo.rs:114 has no `(0,1)` check; the LOW gap in
  the four-quadrants report).

### Technical Notes

- The validation lives in the SLO deserialisation/conversion path that
  DESIGN adds (FLAG-3), not in `synthesise_slo` (which stays a pure
  expander).
- The exact diagnostic wording is DESIGN's (must answer what/why/what-to-do
  per the CLI error pattern).
- Closes the four-quadrants Q2 [LOW] degenerate-budget gap.

---

## US-03: A non-30-day budget is refused, making the doc claim true

### Problem

The `MWMBR_TABLE` thresholds (14.4, 6, 3, 1) are correct only for a 30-day
error budget (ADR-0036 Knowledge Gap). The `Slo.error_budget_period` doc
(slo.rs:49-51) already CLAIMS that "non-30d values are rejected by the
loader", but the loader has no SLO handling, so nothing rejects anything:
the claim is false. If Priya declares a 7-day budget, naive wiring would
synthesise the 30-day thresholds against a 7-day budget, producing silently
wrong alerting. Wiring the validation both protects her and makes the
existing doc claim honest.

### Who

- Priya Nadkarni | SRE who might reach for a 7-day or 90-day budget out of
  habit from other tooling | must be told clearly that v0 supports 30d only.

### Solution

Validate `error_budget_period == 30d` when deserialising an `[[slo]]`. A
non-30d value is refused at load with a message naming the supported value.
This makes the slo.rs:49-51 doc claim true.

### Elevator Pitch

- **Before:** Priya writes a 7-day budget; today the file fails to parse
  (no SLO path), and the slo.rs:49-51 doc claims a loader rejection that
  does not exist.
- **After:** Priya writes a 7-day budget, starts the server, and sees a
  refusal naming `checkout.toml` and stating that only a 30-day budget is
  supported at v0; she sets `30d` and the SLO loads. The doc claim is now
  backed by real code.
- **Decision enabled:** Priya knows v0 burn-rate alerting is 30-day-budget
  only and sets her budget accordingly, rather than getting silently-wrong
  thresholds.

### Domain Examples

#### 1: Happy Path (30d loads)

Priya declares `error_budget_period = "30d"`. It matches the supported
value, so the SLO synthesises and loads.

#### 2: Error Case (7d refused)

Priya declares `error_budget_period = "7d"`. The load refuses with a
message naming `checkout.toml` and stating only `30d` is supported at v0.
No rules are loaded from that SLO. The thresholds are never applied against
the wrong budget window.

#### 3: Boundary (90d refused)

Priya declares `error_budget_period = "90d"`. Refused with the same clear
message. (90d is a plausible quarterly-budget habit from other SLO tooling;
the thresholds would be wrong for it.)

### UAT Scenarios (BDD)

#### Scenario: A non-30-day budget is refused with a clear message

```gherkin
Given Priya's "checkout.toml" declares an [[slo]] with error_budget_period
  "7d"
When she starts beacon-server against that rules directory
Then the SLO is refused at load
And the diagnostic names checkout.toml and states that only a 30-day budget
  is supported at v0
And no rules are loaded from that SLO
```

#### Scenario: A 30-day budget loads unaffected

```gherkin
Given Priya's "checkout.toml" declares an [[slo]] with error_budget_period
  "30d"
When she starts beacon-server against that rules directory
Then the SLO synthesises its four rules and loads normally
```

### Acceptance Criteria

- [ ] An `error_budget_period` other than 30 days is refused at load.
- [ ] The refusal diagnostic names the offending file and states the
      supported value (30 days).
- [ ] No rules are loaded from a refused SLO.
- [ ] A 30-day budget loads normally.
- [ ] After this story, the slo.rs:49-51 doc claim ("non-30d values are
      rejected by the loader") is true; the doc comment is updated if its
      wording no longer matches the actual rejection path.

### Outcome KPIs

- **Who**: SRE operators declaring SLOs.
- **Does what**: are told at load that only a 30-day budget is supported,
  instead of getting silently-wrong thresholds for a 7d/90d budget.
- **By how much**: false doc claims in the SLO area go from 1 (slo.rs:49-51)
  to 0; non-30d budgets reaching synthesis goes to 0.
- **Measured by**: an acceptance test that loads a 7d budget and asserts a
  refusal; a doc check that slo.rs:49-51 matches the shipped rejection.
- **Baseline**: the loader rejects nothing (no SLO path); the doc claim is
  false (four-quadrants Q2 [MEDIUM]).

### Technical Notes

- Validation lives in the SLO conversion path (FLAG-3) alongside the
  target-availability check (US-02).
- Update the slo.rs:49-51 doc comment if the rejection mechanism/wording
  differs from "rejected by the loader".
- Grounded in ADR-0036 Knowledge Gap (30d-only thresholds).

---

## US-04: Synthesised SLO rules coexist with hand-authored rules

### Problem

Priya already runs a directory of hand-authored `[[rules]]`. Adding an SLO
must not disturb them: her existing threshold alerts must keep loading and
evaluating exactly as before, and the four synthesised SLO rules must join
the same catalogue and evaluate alongside them. If adding an SLO silently
shadowed or dropped a hand-authored rule (a name collision, an ordering
quirk), she would lose alerting coverage she thought she had.

### Who

- Priya Nadkarni | SRE with an existing hand-authored rule catalogue |
  must add SLOs without breaking or shadowing her existing rules.

### Solution

Merge the synthesised SLO rules into the same `LoadOutcome.rules` catalogue
the hand-authored `[[rules]]` populate. Both kinds flow through the same
evaluator and sink path. A name collision between a synthesised rule and a
hand-authored rule is handled by an explicit, clearly-messaged policy
(DESIGN owns the policy, FLAG-2); it is never a silent shadow.

### Elevator Pitch

- **Before:** Priya has no way to mix an SLO with her hand-authored rules;
  an `[[slo]]` block poisons its whole file, taking any `[[rules]]` in that
  file down with it.
- **After:** Priya keeps `disk.toml` (hand-authored `[[rules]]`) and adds
  `checkout.toml` (one `[[slo]]`); she starts the server and sees
  `rules_loaded=6` (four synthesised plus two hand-authored), with both
  kinds evaluating and emitting to their sinks.
- **Decision enabled:** Priya adopts SLOs incrementally, one service at a
  time, confident her existing hand-authored alerting is untouched.

### Domain Examples

#### 1: Happy Path (both load and evaluate)

`rules/checkout.toml` has one `[[slo]]` (service `checkout`); `rules/disk.toml`
has two `[[rules]]` (`disk-pressure`, `disk-inodes`). On start, the catalogue
holds six rules: four synthesised plus the two hand-authored. All six are
evaluated; a firing on any of the six emits to its sink.

#### 2: Edge Case (existing rule path unchanged)

Priya's pre-existing rules-only directory (no `[[slo]]` anywhere) loads
exactly as it did before this feature: same rule count, same behaviour. The
`slice_05` and beacon-server rule tests stay green. Adding the SLO code did
not regress the rules-only path.

#### 3: Error Case (name collision surfaced, not silent)

Priya hand-authors a rule literally named `checkout_page_1h_5m` (colliding
with a synthesised SLO rule name) in `disk.toml`, alongside her `checkout`
SLO. The load surfaces the collision with a clear message (DESIGN's policy:
refuse, or a documented precedence) rather than silently dropping one of the
two rules.

### UAT Scenarios (BDD)

#### Scenario: Synthesised SLO rules coexist with hand-authored rules

```gherkin
Given Priya's rules directory contains "checkout.toml" with one [[slo]] and
  "disk.toml" with two hand-authored [[rules]]
When she starts beacon-server against that rules directory
Then the live catalogue holds six rules: four synthesised plus two
  hand-authored
And all six are evaluated and emit to their sinks on firing
```

#### Scenario: The existing rules-only path is unchanged

```gherkin
Given Priya's rules directory contains only hand-authored [[rules]] and no
  [[slo]]
When she starts beacon-server against that rules directory
Then the catalogue loads exactly as it did before this feature
And the existing rule and slice_05 acceptance tests stay green
```

#### Scenario: A name collision is surfaced, not silently shadowed

```gherkin
Given Priya hand-authors a rule named checkout_page_1h_5m alongside a
  "checkout" SLO that synthesises a rule of the same name
When she starts beacon-server against that rules directory
Then the collision is surfaced with a clear diagnostic
And neither rule is silently dropped
```

### Acceptance Criteria

- [ ] Synthesised SLO rules and hand-authored `[[rules]]` load into one
      catalogue and evaluate through the same path.
- [ ] A directory with both kinds reports a `rules_loaded` count equal to
      (4 x SLOs) + hand-authored rules.
- [ ] A rules-only directory loads exactly as before; existing rule and
      slice_05 tests stay green.
- [ ] A name collision between a synthesised and a hand-authored rule is
      surfaced with a clear diagnostic, never a silent shadow.

### Outcome KPIs

- **Who**: SRE operators with existing hand-authored rule catalogues.
- **Does what**: adopt SLOs incrementally without losing or shadowing
  existing hand-authored alerting.
- **By how much**: hand-authored rules silently dropped or shadowed by SLO
  adoption goes to 0; existing rule/slice_05 acceptance tests staying green
  stays at 100%.
- **Measured by**: an acceptance test loading a mixed directory and
  asserting the combined count and a collision diagnostic; the existing
  rule-test suite.
- **Baseline**: SLOs and rules cannot coexist (an `[[slo]]` poisons its
  file).

### Technical Notes

- The collision policy and merge ordering are DESIGN's (FLAG-2).
- The merge appends synthesised rules to `LoadOutcome.rules` (loader.rs:46).
- Guardrail: the existing hand-authored rule path must not regress.

---

## US-05: An SLO edit hot-reloads under SIGHUP

### Problem

Priya tunes alerting on a live daemon via `kill -HUP` (ADR-0063); she will
not restart `beacon-server` to apply an SLO change. An SLO edit must reload
exactly like a rule edit: a valid edit applies atomically and keeps live
alert state; a malformed edit is refused and the previous catalogue is kept,
never partially applied. If a bad SLO edit could partially apply, a
degenerate always-fire rule could slip into the live catalogue on a reload,
the very failure US-02 guards at startup.

### Who

- Priya Nadkarni | SRE who edits rules and SLOs on a running daemon and
  applies them with SIGHUP | must reload SLO edits without restart and
  without a bad edit going partially live.

### Solution

Because the SIGHUP reload re-reads the rules directory via the same
`load_rules` (ADR-0063, main.rs:301-302), once the loader synthesises and
validates SLOs, a reload picks up SLO edits automatically. A valid SLO edit
applies under the all-or-nothing swap with carried-over alert state; a
malformed SLO edit is refused, the previous catalogue is retained, and a
`beacon.reload.refused` event is emitted, exactly as a malformed rule edit
is.

### Elevator Pitch

- **Before:** Priya edits an SLO target on disk and runs `kill -HUP <pid>`;
  today there is no SLO path, so nothing about the SLO reloads.
- **After:** Priya edits her `checkout` SLO target from `0.999` to `0.9995`,
  runs `kill -HUP <pid>`, and sees on stderr `beacon.reload.succeeded
  rules_loaded=...` with the four re-synthesised rules live; a surviving
  rule that was firing keeps firing with its original `since`, no re-page. If
  instead she fat-fingers `target = 1.0` and reloads, she sees
  `beacon.reload.refused` naming the file and the previous catalogue stays
  live, with no always-fire rule.
- **Decision enabled:** Priya tunes SLOs on a live daemon at any hour,
  trusting a bad edit cannot go partially live and a good edit will not
  re-page her rotation.

### Domain Examples

#### 1: Happy Path (valid SLO edit reloads)

Priya edits `checkout.toml`, raising `target_availability` from `0.999` to
`0.9995`. She runs `kill -HUP <pid>`. The reload re-synthesises the four
rules with the tighter budget and applies them atomically.
`beacon.reload.succeeded` is emitted; the process is the same process.

#### 2: Error Case (malformed SLO edit refused, previous kept)

Priya edits `checkout.toml`, setting `target_availability = 1.0` by mistake.
She runs `kill -HUP <pid>`. The reload is REFUSED:
`beacon.reload.refused` is emitted naming `checkout.toml`, the previous
catalogue stays fully live, and no degenerate always-fire rule reaches
evaluation. The daemon does not exit.

#### 3: Edge Case (a firing rule survives an unrelated SLO edit)

A synthesised rule `checkout_page_1h_5m` is currently `Firing`. Priya adds a
second, unrelated SLO for service `search` and reloads. `checkout_page_1h_5m`
survives by name and keeps `Firing` with its original `since`; it does not
re-page. The four `search` rules are added. `beacon.reload.succeeded`
reflects the additions.

### UAT Scenarios (BDD)

#### Scenario: A valid SLO edit hot-reloads under SIGHUP

```gherkin
Given beacon-server is running with a loaded "checkout" SLO
And Priya edits checkout.toml to tighten the target availability
When she sends SIGHUP to the running process
Then the four rules are re-synthesised with the new target and applied
  atomically
And a beacon.reload.succeeded event is emitted
And the process is the same process (no restart)
```

#### Scenario: A malformed SLO edit is refused and the previous catalogue is kept

```gherkin
Given beacon-server is running with a valid "checkout" SLO loaded
And Priya edits checkout.toml to set target_availability to 1.0
When she sends SIGHUP to the running process
Then the reload is refused
And a beacon.reload.refused event names checkout.toml and states the
  previous catalogue was retained
And no degenerate always-fire rule reaches evaluation
And the daemon does not exit
```

#### Scenario: A firing synthesised rule survives an unrelated SLO edit without re-paging

```gherkin
Given a synthesised rule checkout_page_1h_5m is currently Firing
When Priya adds an unrelated "search" SLO and sends SIGHUP
Then checkout_page_1h_5m stays Firing with its original since
And it does not emit a second Firing incident
And the four search rules are added to the live catalogue
```

### Acceptance Criteria

- [ ] A valid SLO edit applied with SIGHUP re-synthesises and applies the
      four rules atomically, emitting `beacon.reload.succeeded`, in the same
      process.
- [ ] A malformed SLO edit applied with SIGHUP is refused, the previous
      catalogue is kept, `beacon.reload.refused` is emitted, and no
      degenerate always-fire rule reaches evaluation.
- [ ] A surviving synthesised rule keeps its in-flight state and `since`
      across an unrelated SLO reload and does not re-page.
- [ ] The reload success/refusal behaviour for SLOs is consistent with the
      existing rule-reload contract (ADR-0063).

### Outcome KPIs

- **Who**: SRE operators tuning SLOs on a live beacon-server.
- **Does what**: apply SLO edits with SIGHUP without restart, and have a bad
  SLO edit refused rather than partially applied.
- **By how much**: SLO edits requiring a process restart goes to 0; bad SLO
  edits going partially live goes to 0; surviving rules re-paged on an
  unrelated SLO reload goes to 0.
- **Measured by**: an acceptance test that edits an SLO and SIGHUPs,
  asserting `beacon.reload.succeeded` for a valid edit and
  `beacon.reload.refused` plus a retained previous catalogue for a malformed
  edit.
- **Baseline**: no SLO path, so no SLO reload exists at all.

### Technical Notes

- REUSE the ADR-0063 reload path (main.rs:292-440) verbatim; SLO support
  comes "for free" once the loader synthesises SLOs, because reload re-reads
  via the same `load_rules`.
- DESIGN confirms the refuse-vs-apply rules carry over and decides whether
  the `added` count should be expansion-aware (one SLO shows `added=4`)
  (FLAG-4).
- Depends on US-01 (synthesis path), US-02/US-03 (validation), US-04
  (merge).
