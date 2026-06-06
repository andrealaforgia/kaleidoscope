# Wave Decisions — beacon-slo-operator-path-v0 (DISCUSS)

British English throughout, no em dashes.

## Origin

The four-quadrants assessment for beacon
(`/Users/andrealaforgia/dev/kaleidoscope-4-quadrants-theory/reports/beacon.md`,
Q3) records the gap verbatim:

> An operator cannot declare an SLO in a rule file today. The MWMBR
> engine is correct but unreachable from the product surface.

This is the UNWIRED set (Tested But Unwired). The black-box verifier
wants it because it makes her B06 contract buildable. Beacon's SLO
multi-window multi-burn-rate (MWMBR) engine is correct and well-tested
but library-only: it ships dead. This feature wires the correct-but-dead
headline feature to the operator surface. This is genuinely valuable
wiring, NOT a low-severity nicety.

## Code verification (the report's read, confirmed against source)

Every claim below cites a file:line actually opened in this wave.

- **The `Slo` struct shape (slo.rs:36-57).** Fields are: `service:
  String` (the service identifier, used in synthesised rule names and the
  `slo_service` label), `sli_good_events: String` (PromQL "good events"
  numerator), `sli_total_events: String` (PromQL "total events"
  denominator), `target_availability: f64` (doc says `(0.0, 1.0)`, error
  budget is `1 - target_availability`), `error_budget_period: Duration`
  (doc says 30-day only at slice 05), `sinks: Vec<SinkConfig>`,
  `source_path: Option<String>`.
- **`synthesise_slo` (slo.rs:106-156)** maps a `&Slo` to a `Vec<Rule>`
  deterministically (no clock, no RNG). It always produces exactly FOUR
  rules, one per `MWMBR_TABLE` row (slo.rs:64-93): page `14.4 / 1h / 5m`
  (critical), page `6.0 / 6h / 30m` (critical), ticket `3.0 / 1d / 2h`
  (warning), ticket `1.0 / 3d / 6h` (warning). The firing predicate is
  `error_rate > budget * threshold` (slo.rs:181-184). `slice_05_slo_burn_rate.rs`
  is 20/20 green.
  - NOTE: ADR-0036 says "five-rule" in places and "four rows" in others;
    the shipped code synthesises FOUR rules. ADR-0036 also shows an
    `annotations` field on the synthesised `Rule` and CUE-schema
    validation; neither exists in the shipped `Rule` type (types.rs:62-90)
    or the shipped loader. The shipped code is the authority; flagged for
    DESIGN to reconcile the ADR text.
- **The loader has NO SLO path (loader.rs:260-265).** `FileShape` is
  `#[serde(deny_unknown_fields)]` and contains ONLY `rules: Vec<RawRule>`.
  There is no `[[slo]]` / `[[slos]]` table and no SLO deserialisation.
  `load_rules` (loader.rs:111-132) never calls `synthesise_slo`.
- **What `deny_unknown_fields` does with an `[[slo]]` block TODAY
  (confirmed).** Because `FileShape` denies unknown fields, a file
  containing an `[[slo]]` table makes `toml::from_str` fail with "unknown
  field `slo`". `parse_file` (loader.rs:157-168) returns `Err(diag)`, so
  the ENTIRE file is skipped: every `[[rules]]` table in that same file is
  lost too, surfaced as one `LoaderDiagnostic`. An `[[slo]]` block today is
  therefore NOT silently ignored and NOT parsed; it poisons its file. This
  is the precise "before" state the elevator pitch contrasts against.
- **beacon-server has ZERO SLO references.** The only callers of
  `synthesise_slo` in the whole repo are in
  `crates/beacon/tests/slice_05_slo_burn_rate.rs`.
- **Two doc-lies in the same area (confirmed).**
  - slo.rs:49-51 claims non-30d `error_budget_period` values "are rejected
    by the loader". The loader has no SLO handling, so nothing rejects
    anything. Wiring loader validation of `error_budget_period == 30d`
    makes this claim TRUE (US-03).
  - slo.rs:24-26 and slice_05:27-30 claim a 24-hour-trace cross-validation
    test that does NOT exist (deferred to a "slice 05b"). This is adjacent,
    not core; flagged for DESIGN/DISTILL (FLAG-5 below).
  - Also [LOW]: `budget = 0` (`target_availability = 1.0`) yields a
    degenerate always-fire threshold (slo.rs:114, `limit = 0`, predicate
    becomes `error_rate > 0`), unguarded. Both this and the non-30d case
    become loader-validation concerns once the operator path exists.

## Contract being delivered (the operator job, from the report)

> When I declare an SLO in my beacon rule file (a name, a target
> availability, and the good-events and total-events queries), beacon
> synthesises the four SRE-workbook multi-window multi-burn-rate alert
> rules from it and evaluates them alongside my hand-authored rules, so I
> get correct burn-rate alerting (page fast on a 14.4x burn, ticket on a
> slow burn) WITHOUT hand-writing the four threshold/window rules myself;
> and a malformed SLO (target availability not strictly between 0 and 1,
> or a non-30-day budget) is REJECTED at load with a clear message rather
> than silently producing a degenerate always-fire rule.

Grounded in: ADR-0036 (beacon SLO MWMBR synthesis), ADR-0063 (beacon
SIGHUP reload atomic swap, the all-or-nothing reload contract), the
loader/rules architecture, the brief's beacon/SLO posture.

## Decisions taken in this wave (autonomous, per the overnight brief)

- **[DEC-1] Wire, do not retract.** The correct-but-dead engine is wired
  to the operator surface so the documented headline SLO feature actually
  ships reachable. Earned-Trust theme: the engine exists and is tested;
  this feature makes it reachable and makes two false doc claims true.
- **[DEC-2] Feature Type = Backend** (alerting config + loader wiring).
  The operator-invocable surface is the existing beacon rule file plus the
  running `beacon-server` (started, or reloaded via the existing
  `kill -HUP <pid>` path from ADR-0063). No new CLI surface, no new HTTP
  surface. Walking Skeleton = No (brownfield: the engine, the loader, the
  server, and the reload path all exist; this is wiring).
- **[DEC-3] UX research = Lightweight.** The persona is a single SRE
  operator authoring an SLO in a TOML rule file; the medium is a config
  file plus structured logs, not an interactive UI. The relevant UX skills
  are the TUI/CLI error-message patterns (a clear rejection diagnostic) and
  material honesty (a config file behaves like a config file).
- **[DEC-4] Reuse, do not re-engine.** Reuse `synthesise_slo` +
  `MWMBR_TABLE` + the loader + the rule catalogue + the ADR-0063 reload
  path verbatim. The only new code is: the `[[slo]]` wire shape on
  `FileShape`, its deserialisation into a `Slo`, the validation, and the
  merge of synthesised rules into the loaded catalogue. No new engine
  logic.
- **[DEC-5] Slicing (carpaccio).** Three thin end-to-end slices, each
  independently demonstrable:
  - **Slice 1 (Walking slice, US-01):** one `[[slo]]` parses + validates +
    synthesises four rules + loads at startup, reachable end-to-end. This
    is the headline made real.
  - **Slice 2 (Safety, US-02 + US-03):** the validation refusals (target
    not in `(0,1)`; non-30d budget), each refused at load with a clear
    message, keeping the previous catalogue under the all-or-nothing
    contract. US-03 also makes the slo.rs:49-51 doc claim true.
  - **Slice 3 (Coexistence + reload, US-04 + US-05):** synthesised SLO
    rules coexist with hand-authored `[[rules]]` in one catalogue; an SLO
    edit hot-reloads under SIGHUP exactly as a rule edit does, honouring
    ADR-0063.
  Carpaccio taste tests: each slice delivers a behaviour the operator can
  verify; none is infrastructure-only; the headline (Slice 1) is
  user-visible on its own; the safety negatives (Slice 2) are exactly what
  make the headline trustworthy.

## FLAGGED for DESIGN (five decisions; named here, not answered)

DISCUSS pins the OBSERVABLE requirement (the AC). DESIGN picks the
mechanism. These five are the brief's flagged decisions, confirmed against
source.

**[FLAG-1] The `[[slo]]` TOML schema and the `FileShape` extension.**
What are the wire field names for the SLO table, and how are the good /
total event queries expressed? The shipped `Slo` (slo.rs:37-57) has
`service`, `sli_good_events`, `sli_total_events`, `target_availability`,
`error_budget_period`, `sinks`, `source_path`. DESIGN owns the exact TOML
key names (e.g. `service` vs `name`; `sli_good_events` vs `good`;
`error_budget_period` as a humantime string `"30d"` like the existing
`for_duration`/`interval` fields), whether the table is `[[slo]]` or
`[[slos]]`, how `sinks` are expressed (reuse the existing `RawSink` wire
shape), and how `source_path` is populated (the loader knows the file
path). The `FileShape` gains a second `#[serde(default)]` vector beside
`rules`; `deny_unknown_fields` must continue to hold so an unknown SLO
sub-field still earns a "did you mean" diagnostic.

**[FLAG-2] Merge semantics: synthesised SLO rules coexisting with
hand-authored `[[rules]]` in one catalogue.** `synthesise_slo` names each
rule `{service}_{page|ticket}_{long}_{short}` (slo.rs:124-127). DESIGN must
decide: how a name collision between a synthesised rule name and a
hand-authored rule name is handled (refuse the load with a clear message?
last-wins? namespace the synthesised names?); the ordering of synthesised
vs hand-authored rules in the merged catalogue (the loader sorts files but
not rules within the merged `Vec`); and how a single file holding both
`[[slo]]` and `[[rules]]` is loaded. The catalogue is the existing
`LoadOutcome.rules: Vec<Rule>` (loader.rs:46-49); the merge appends
synthesised rules to it.

**[FLAG-3] The validation rules and their exact rejection messages.** The
two validations are `target_availability` strictly in `(0.0, 1.0)` (closing
the LOW degenerate-always-fire gap at slo.rs:114) and
`error_budget_period == 30d` (making the slo.rs:49-51 doc claim true).
DESIGN owns the exact diagnostic text for each (it must answer what
happened, why, and what to do, per the CLI error-message pattern), whether
the rejection is a per-file `LoaderDiagnostic` (report-and-skip, like a
malformed rule) or a harder failure, and how a malformed SLO interacts with
the all-or-nothing reload contract (ADR-0063): a malformed SLO must be
REFUSED and the previous catalogue KEPT, never partially applied, never a
degenerate always-fire rule reaching evaluation.

**[FLAG-4] Interaction with SIGHUP reload (ADR-0063).** An SLO edit must
reload like a rule edit: the existing `reload` sequence
(main.rs:292-440) re-reads the dir via `load_rules` verbatim, so once the
loader synthesises SLO rules, a reload picks them up automatically. DESIGN
must confirm the refuse-vs-apply rules carry over unchanged: a malformed
SLO in a reload is refused (previous catalogue retained, `beacon.reload.refused`
emitted) exactly as a malformed rule is; a valid SLO edit succeeds and the
`beacon.reload.succeeded` event's `rules_loaded` / `added` counts reflect the
four-rule expansion (an added SLO shows `added = 4`, not `added = 1`).
DESIGN must decide whether that expansion-aware count is acceptable or
whether the event needs an SLO-aware field.

**[FLAG-5] The missing 24-hour cross-validation test (slo.rs:24-26 /
slice_05:27-30).** This is a separate test-claim, adjacent to the core. The
doc claims a cross-validation test that does not exist. DESIGN/DISTILL must
decide: either deliver the missing 24-hour synthetic-trace cross-validation
test (the slice 05b that was deferred), or correct the doc to remove the
claim. This is a DESIGN/DISTILL call, not the core of this feature; it is
captured so it is not lost.

## Constraints to honour (from the brief)

- beacon-server is the alerting engine and is operator-critical. A
  degenerate always-fire SLO would be its own dishonesty, so validation
  (FLAG-3) is load-bearing.
- The merge (FLAG-2) must NOT break the existing hand-authored rule path:
  `slice_05_slo_burn_rate.rs` and the beacon-server rule tests stay green.
- Must honour the ADR-0063 all-or-nothing reload contract: a malformed SLO
  is refused and the previous catalogue kept; no partial apply.
- Inherits ADR-0005's five gates; per-feature mutation 100% on the modified
  loader / slo / server lines. Rust idiomatic per CLAUDE.md.
- NEVER bump any crate to 1.0.0.
- The beacon durability caveat (the store is flush-only, not fsync'd) is a
  SEPARATE known issue from the four-quadrants report Q2 [HIGH]. It is
  explicitly OUT OF SCOPE for this feature and must not be pulled in.

## Risks surfaced (managed downstream)

| Risk | Prob | Impact | Mitigation owner |
|------|------|--------|------------------|
| A name collision between a synthesised SLO rule and a hand-authored rule silently shadows one | Med | High | DESIGN (FLAG-2: collision policy + clear message) |
| A malformed SLO partially applies, leaving a degenerate always-fire rule in the live catalogue | Low | High | Hard AC: validate-before-merge; ADR-0063 all-or-nothing (FLAG-3, FLAG-4) |
| The merge breaks the existing hand-authored rule load path | Low | High | AC: existing rule + slice_05 tests stay green; mutation gate on modified lines |
| `deny_unknown_fields` change drops the "did you mean" diagnostic for SLO sub-fields | Low | Med | DESIGN keeps `deny_unknown_fields`; extends `BLESSED_FIELDS` (FLAG-1) |
| The reload `added` count is confusing because one SLO expands to four rules | Med | Low | DESIGN decides expansion-aware counting (FLAG-4) |
| The missing cross-validation test claim is forgotten | Low | Med | Captured as FLAG-5 for DESIGN/DISTILL |

## Missing DIVERGE note

No DIVERGE artifacts exist for this feature
(`docs/feature/beacon-slo-operator-path-v0/diverge/` absent). The JTBD was
supplied directly in the overnight brief and is grounded in the existing
beacon architecture docs (ADR-0036 SLO synthesis, ADR-0063 reload,
loader/rules ADRs) and the shipped code, which serve as the validated
contract. Risk of skipping DIVERGE is LOW here: the job is to make an
existing, correct, tested engine reachable from the product surface, with
the fix direction already decided (wire it). There is no open
design-direction question, only the five mechanism decisions flagged above
for DESIGN.

## Changelog

- 2026-06-06: DISCUSS wave authored. Verified the four-quadrants Q3 read in
  source (the `Slo` struct shape, the loader's absent SLO path, the
  `deny_unknown_fields` poisons-the-file behaviour, the two doc-lies, the
  ADR-0063 reload contract). Took DEC-1..DEC-5; flagged FLAG-1..FLAG-5 for
  DESIGN.
