# Wave Decisions — beacon-sighup-reload-v0 (DISCUSS)

British English throughout, no em dashes.

## Origin

The black-box verifier filed issue 010 (B03, RED): beacon-server's
documentation promises SIGHUP hot-reload of the rule catalogue, but the
running binary installs only SIGINT and SIGTERM handlers, loads the
rules once at startup, and leaves SIGHUP unhandled. An operator who
edits the rules on disk and sends SIGHUP to apply them does NOT get the
new catalogue; the promised reload never happens. We chose to DELIVER
the promised capability rather than retract the docs, because beacon's
architecture designed this mechanism in detail (ADR-0034 "Reload
semantics", ADR-0033, ADR-0037, slice-02, D3).

## Code verification (the verifier's read, confirmed)

- `crates/beacon-server/src/main.rs:65` loads the catalogue once via
  `load_rules(&args.rules)` at startup.
- `crates/beacon-server/src/main.rs:164-175` spawns one `run_rule`
  Tokio task per rule; each task owns its `RuleState` in a local
  `state` variable, seeded from the recovered durable store.
- `crates/beacon-server/src/main.rs:177` installs `ctrl_c` (SIGINT);
  `:179` installs a SIGTERM handler. The `tokio::select!` at `:187`
  waits on exactly those two. There is NO SIGHUP arm anywhere. The
  `--rules` arg doc-comment at `:45-46` literally admits "Loaded once
  at startup; SIGHUP reload arrives at slice 03" — slice 03 shipped
  grouping and inhibition but the SIGHUP reload was never landed. The
  promise is unkept.
- The loader (`crates/beacon/src/loader.rs:111` `load_rules`) returns a
  `LoadOutcome { rules, diagnostics }`. A single malformed `.toml`
  produces a `LoaderDiagnostic` and is skipped; the rest of the
  catalogue is preserved (the report-and-skip posture from B01). The
  composition root already refuses to start when `!has_any_rules()`
  (`main.rs:77-84`), exit code 1.
- The durable store (`crates/beacon/src/state_store.rs`,
  `RuleStateStore` trait + `FileBackedRuleStateStore`) holds each
  rule's `RuleState` keyed by rule name, keyed-latest-wins (ADR-0040).
  This is the seam that already preserves a rule's in-flight `Firing`
  state across a process restart; the swap path can reuse it.

## Contract being delivered (from the docs)

The load-bearing contract is ADR-0034 "Reload semantics":

> On SIGHUP, the loader re-reads the `--rules` directory. The new
> catalogue must validate completely (every file parses + at least one
> rule loads); if validation fails, the active catalogue stays as-is
> and a diagnostic is emitted via Beacon's telemetry. Atomic swap: the
> evaluator never sees a half-loaded catalogue.

Corroborated by:
- slice-02-cue-catalogue.md: "SIGHUP triggers reload; the previous
  catalogue stays active until the new one validates".
- beacon-v0 D3: "CUE files on disk; SIGHUP reload. ... reloads on
  SIGHUP. The catalogue is in-memory only at v0."
- ADR-0033: beacon-server wires "a SIGHUP handler that reloads the
  rule set"; ADR-0037 names "the SIGHUP signal handler that triggers
  rule-set reload" as a composition-root responsibility (not part of
  the pure evaluator).

## Decisions taken in this wave

- **[DEC-1] Deliver, do not retract.** The capability is built so the
  binary keeps the documented promise. Earned-Trust theme: the docs
  made a promise, this feature makes the running binary keep it.
- **[DEC-2] Feature Type = Backend** (server-binary capability: signal
  handling in the async runtime + safe catalogue swap). No new CLI
  surface and no new HTTP surface; the operator entry point is the
  existing POSIX `kill -HUP <pid>`. Walking Skeleton = No (brownfield:
  beacon-server, the loader, and the durable store all exist).
- **[DEC-3] UX research = Lightweight.** The persona is a single
  operator reloading rules on a running daemon; the medium is signals
  and structured logs, not an interactive UI.
- **[DEC-4] Single slice.** The basic reload and the
  malformed-reload-keeps-previous safety property are co-equal halves
  of ONE testable behaviour (you cannot demonstrate "reload" honestly
  without demonstrating "refuse a bad reload"). Carpaccio was
  considered and rejected: splitting "atomic swap preserving state"
  from "basic reload" would create an infrastructure-only slice with no
  independently demonstrable operator value, and the safety negative is
  exactly what makes the headline trustworthy. One slice, two stories
  that ship together (US-01 reload, US-02 the safety + state decision),
  3-7 scenarios each. Do not pad.

## FLAGGED for DESIGN: in-flight alert-state preservation across a swap

This is a genuine design decision and is flagged here, not decided in
DISCUSS. DISCUSS pins the OBSERVABLE requirement (the AC); DESIGN picks
the mechanism.

**The decision.** When SIGHUP swaps the live catalogue, a rule that
exists in BOTH the old and new catalogues (matched by name) may be
mid-flight: it could be `Pending { since }` or `Firing { since }` right
now. Does the swap preserve that rule's current state and its `since`
instant, or reset it to `Inactive`?

**What DISCUSS requires (the AC, solution-neutral).** A rule that
survives the reload unchanged MUST NOT re-page on-call. Concretely: a
rule that is `Firing` before the reload and is still present (same name)
after the reload keeps firing across the swap with its original `since`;
it does not emit a spurious second `Firing` incident and does not
silently drop to `Inactive`. A removed rule stops (no further
evaluation, no resolved-storm unless DESIGN decides resolution is
owed). A newly-added rule starts `Inactive` and earns its state from
fresh evaluations, exactly as a fresh process would.

**Why it is load-bearing.** The whole JTBD is "apply new rules WITHOUT
dropping the alerts I am already evaluating." If the swap reset live
state, every SIGHUP would clear every active incident and then re-page
the entire on-call rotation a `for_duration` later. That would make the
feature actively harmful, turning a routine rule edit into an alert
storm. The durable `RuleStateStore` already keys state by rule name and
already survives a restart without re-paging (beacon-durable-alert-
state-v0); the swap should reuse that keying so a surviving rule's state
carries across, but the precise mechanism (re-seed new tasks from the
store vs. hand-off live state vs. snapshot-then-recover) is DESIGN's.

**Sub-decisions DESIGN must resolve (named, not answered here):**
1. **Matching key.** Rules are matched old-to-new by `name`. If a rule
   keeps its name but changes its `query`/`for_duration`/`severity`,
   does it keep its in-flight `since`, or is a changed definition a new
   rule that resets? (DISCUSS leans: same name keeps state; document the
   call and its consequence for an operator who edits a threshold on a
   currently-firing rule.)
2. **The shared InhibitionResolver.** It is built once from the
   catalogue (`main.rs:162`) and shared across tasks via
   `Arc<Mutex<>>`. A swap that adds/removes inhibitor relations must
   rebuild or mutate it atomically. Its `pending` suppressed-incident
   map is live state that a naive rebuild would drop. DESIGN must decide
   whether suppressed-pending incidents survive a swap.
3. **Task lifecycle race.** The old per-rule tasks are `tokio::spawn`ed
   and only `abort()`ed at shutdown today (`main.rs:197-199`). A swap
   must stop the old set and start the new set without a window where
   two tasks evaluate the same rule (double emission) or zero tasks
   evaluate a surviving rule (missed transition). The SIGHUP handler
   must not race the per-rule evaluation loops or the resolver mutex.
4. **Durable-store consistency.** A surviving `Firing` rule's state is
   already persisted; a removed rule's persisted state should be
   dropped-and-logged on swap exactly as the startup recovery already
   drops states for rules no longer in config (`main.rs:139-144`).

## Risks surfaced (managed downstream)

| Risk | Prob | Impact | Mitigation owner |
|------|------|--------|------------------|
| Swap races the evaluation loop, causing double or dropped emissions | Med | High | DESIGN (atomic-swap mechanism; sub-decision 3) |
| In-flight state reset on swap re-pages on-call | Med | High | DISCUSS AC pins no-re-page; DESIGN mechanism (the flagged decision) |
| Malformed reload partially applies, leaving a half-catalogue | Low | High | Hard AC: validate-completely-then-swap; ADR-0034 "atomic swap" |
| Operator cannot observe whether a reload took effect | Med | Med | AC: structured success + refusal events (rules_loaded, diagnostic) |
| SIGHUP default disposition (terminate) fires before handler install if signal arrives during startup | Low | Med | DESIGN: install handler before spawning evaluation tasks |

## Missing DIVERGE note

No DIVERGE artifacts exist for this feature
(`docs/feature/beacon-sighup-reload-v0/diverge/` absent). The JTBD was
supplied directly and is grounded in the existing beacon architecture
docs (ADR-0034, ADR-0033, ADR-0037, slice-02, D3), which serve as the
validated contract. Risk of skipping DIVERGE is low here because the
job is to make the binary honour a promise the docs already specify in
mechanism-level detail; there is no open design-direction question, only
the in-flight-state mechanism flagged above for DESIGN.

## Changelog

- 2026-06-05: DISCUSS wave authored. Verified verifier's read in code;
  pinned the ADR-0034 contract; took DEC-1..DEC-4; flagged the
  in-flight-state-preservation decision for DESIGN.
