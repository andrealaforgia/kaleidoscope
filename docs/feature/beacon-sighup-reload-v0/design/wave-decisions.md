# Wave Decisions — beacon-sighup-reload-v0 (DESIGN)

British English throughout, no em dashes.

> **Author**: Morgan (`nw-solution-architect`), DESIGN wave, 2026-06-05.
> **Mode**: PROPOSE.
> **Governing contract**: ADR-0034 "Reload semantics" (already specifies
> validate-completely / keep-previous-on-failure / atomic swap). This
> DESIGN delivers that contract and resolves the four mechanism
> sub-decisions DISCUSS flagged. The mechanism decisions are recorded
> immutably in **ADR-0063**; this file is the feature-local digest plus
> the mandatory Reuse Analysis.

## Why a new ADR (ADR-0063), not just a digest

ADR-0034 states the reload *contract* but not the *mechanism* that keeps
live alerting state across the swap. The DISCUSS "FLAGGED for DESIGN"
note (`../discuss/wave-decisions.md`) named four mechanism sub-decisions
that are architecturally significant: they fix `beacon-server`'s reload
concurrency model, the identity of a rule across an edit, the
`InhibitionResolver` lifecycle, and the per-rule task lifecycle. These
constrain every future edit to the reload path and survive the feature,
so they earn an immutable record. **ADR-0063** is authored
(`docs/product/architecture/adr-0063-beacon-sighup-reload-atomic-swap-and-state-carryover.md`),
citing ADR-0034 as the governing contract. ADR-0034 is NOT modified: its
contract is unchanged and fully honoured.

## The four sub-decisions, resolved (one line each)

1. **SIGHUP handler concurrency model** — single-orchestrator: add a
   `SignalKind::hangup()` arm to the main `tokio::select!` (turned into a
   `loop`), run the reload inline between SIGINT/SIGTERM; the orchestrator
   is the sole writer of catalogue + resolver + handles, so the handler
   never races the per-rule loops or the resolver mutex; SIGINT/SIGTERM
   still `break` to shutdown unchanged.
2. **Matching key for carried-over state** — NAME only; a rule that keeps
   its name keeps its `Pending`/`Firing` `since` even if
   query/for_duration/severity/sinks/inhibits changed (a changed
   threshold does NOT reset, so a live edit never re-pages; the next tick
   re-evaluates the new definition against the carried clock anchor).
3. **Atomic swap of the InhibitionResolver** — rebuild
   `InhibitionResolver::new(&new_rules)` for the fresh relation graph,
   then carry over the live `firing` flags and the `pending`
   suppressed-incident entries whose inhibited rule still exists; wholesale
   `Arc<Mutex<>>` replacement (not in-place mutation) so old tasks never
   see a torn relation graph.
4. **Task lifecycle** — build the NEW task set (seeded with carried-over
   state) and NEW resolver FIRST, then atomically replace the live
   generation, THEN `abort()` the old handles; new-live-before-old-aborted
   guarantees no missed-evaluation window and no double-fire (overlapping
   ticks are idempotent under the latest-wins durable store, ADR-0040).

## Handler concurrency model (expanded)

The existing single-shot `tokio::select! { SIGINT, SIGTERM }`
(`main.rs:187`) becomes `loop { select! { SIGINT => break, SIGTERM =>
break, SIGHUP => { reload(); continue } } }`. The handler is installed
BEFORE the per-rule tasks are spawned (mirroring the SIGTERM install
order) so an early SIGHUP during startup does not hit the OS default
disposition (terminate). The reload sequence:

1. `load_rules(&args.rules)` (reused verbatim).
2. Validate: `Err(LoaderError)` or `!has_any_rules()` → refuse, emit
   `beacon.reload.refused`, `continue` (old generation untouched).
3. Valid → snapshot old resolver `firing`/`pending`; `store.load_all()`
   for carried `since`; build new resolver + new tasks; replace
   `handles`/`resolver`; abort old handles; emit `beacon.reload.succeeded`.

`run_rule` (the hot path) is UNCHANGED. All reload logic lives in the
orchestrator. See ADR-0063 sub-decisions 1 and 4 for the full ordering
invariant.

## The safety contract (all-or-nothing)

Validity bar = "at least one rule loaded", identical to the startup
`has_any_rules()` refusal (`main.rs:77-84`), so SIGHUP and startup share
one contract. Directory-unreadable or zero-rules → refuse, keep the
previous catalogue fully active, do NOT crash, do NOT partially apply,
emit `beacon.reload.refused`. A partly-broken catalogue (>=1 valid rule
plus one malformed file) → apply the valid rules AND surface each
`LoaderDiagnostic` via the existing `warn!` (`main.rs:74`), exactly as
startup's report-and-skip (B01). A refusal touches neither `handles` nor
`resolver`, so the swap is genuinely all-or-nothing. (ADR-0063 §"The
safety contract".)

## Matching-key rule (the documented operator consequence)

Same name = same rule = keeps its in-flight `since`. If an operator
raises a threshold on a currently-`Firing` rule, it stays `Firing` with
its original `since` until the next tick re-runs the new query; if the
new query no longer matches, the next tick resolves it normally. The edit
does not retroactively un-fire an already-sent page and does not re-page.
Removed rule → stops, durable state dropped-and-logged (as startup,
`main.rs:139-144`). Added rule → starts `Inactive`, earns state from
fresh ticks. (ADR-0063 sub-decision 2.)

## Observables (named, for DISTILL)

- **`beacon.reload.succeeded`** (INFO): `rules_loaded`, `added`,
  `removed`, `diagnostics`. Read by US-01 AC-5 and the verifier's B03.
- **`beacon.reload.refused`** (WARN): `file` (or no-rules reason),
  `error` (`LoaderDiagnostic::display` text incl. "did you mean", or
  `LoaderError` text), `previous_catalogue_retained = true`. Read by
  US-02 negative AC.

Both are the `message` of a `tracing` event on the existing stream. The
per-file report-and-skip diagnostics keep using the existing
`"rule load diagnostic"` `warn!`, unchanged.

## MANDATORY Reuse Analysis (RCA hard gate — extend, do not reinvent)

| Existing machinery | Path | Decision |
|---|---|---|
| `load_rules` → `LoadOutcome { rules, diagnostics }` + report-and-skip | `crates/beacon/src/loader.rs:111` | **REUSE verbatim** for the re-read. Already returns the outcome and skips a malformed file. No loader change. |
| `LoadOutcome::has_any_rules()` validity bar | `crates/beacon/src/loader.rs:53` | **REUSE.** The reload validity gate is the same predicate the startup refusal uses. One contract. |
| `LoaderDiagnostic::display()` (incl. "did you mean") | `crates/beacon/src/loader.rs:75-84` | **REUSE** for the `beacon.reload.refused` event text and the per-file `warn!`. No new formatting. |
| Durable `RuleStateStore` keyed by name, drops absent-rule state | `crates/beacon/src/state_store.rs`; `main.rs:130-144` | **REUSE** as the carry-over seam. `load_all()` supplies a surviving rule's `since`; the existing dropped-rule log is the removed-rule path. The store is the in-flight-state-preservation mechanism — already delivered (beacon-durable-alert-state-v0). |
| Startup recovery + per-rule seeding loop | `main.rs:130-175` | **EXTEND.** The reload re-runs the same seed-from-store logic the startup path already performs; the difference is "re-run on SIGHUP" not "run once". |
| SIGTERM install pattern + `tokio::select!` shutdown | `main.rs:179-199` | **EXTEND.** Add a third `SignalKind::hangup()` arm and turn the single-shot select into a loop. SIGINT/SIGTERM arms unchanged. |
| `JoinHandle::abort()` task teardown | `main.rs:197-199` | **REUSE.** The same abort used at shutdown is the reload's old-task teardown, sequenced after the new set is live. |
| `InhibitionResolver` (`new`, `observe`, `pending`, `firing`) | `crates/beacon/src/inhibition.rs:48-161` | **EXTEND (small additive surface).** Add a `rebuild_from(&new_rules, carried_firing, carried_pending)`-style inherent constructor beside `new` (mirroring `FileBackedRuleStateStore::open_with_fsync_backend` beside `open`). `observe` and the public surface are unchanged. This is the ONLY new library surface the feature needs. |
| Pure `transition` / `evaluate_once` | `crates/beacon/src/state_machine.rs`; `beacon_server::evaluate_once` | **UNTOUCHED.** No I/O or signal logic enters the pure evaluator (ADR-0037 inviolable). |

**Is there existing reload/swap machinery to extend?** No reload/swap
machinery exists today (SIGHUP is unhandled; the catalogue loads once).
But the two hard parts — re-reading + report-and-skip, and preserving
in-flight `since` across a respawn — are ALREADY solved by the loader and
the durable store respectively. The feature is mostly *wiring* those two
existing seams into a SIGHUP-driven orchestrator loop, plus ONE small
additive `InhibitionResolver` constructor for the resolver carry-over.
Net new library surface: one constructor. Net new binary surface: one
select arm + one reload function + two event names.

## Risk disposition (from DISCUSS, now owned)

| DISCUSS risk | Disposition in this DESIGN |
|---|---|
| Swap races the evaluation loop (double/dropped emissions) | Resolved by sub-decision 4: new-live-before-old-aborted + latest-wins durable store; single-orchestrator writer removes the resolver-mutex race. |
| In-flight state reset re-pages on-call | Resolved by sub-decision 2: name-keyed carry-over of `since`; no reset on definition change. |
| Malformed reload partially applies | Resolved by the all-or-nothing safety contract: new generation built completely before replace; refusal touches nothing. |
| Operator cannot observe whether reload took effect | Resolved by the two named structured events. |
| SIGHUP default disposition fires during startup | Resolved by installing the handler before spawning per-rule tasks. |

## Quality gates

- [x] Requirements (US-01, US-02 AC) traced to mechanism decisions.
- [x] Component boundaries respected: pure evaluator untouched (ADR-0037);
      orchestrator owns reload; loader + store reused.
- [x] Decision recorded in an ADR with >=2 alternatives each
      (ADR-0063, four alternatives documented).
- [x] Reliability/observability addressed: all-or-nothing safety, two
      structured events, no-crash, no-re-page.
- [x] Dependency-inversion preserved: reload uses the `RuleStateStore`
      and `InhibitionResolver` seams, not concretions in the hot path.
- [x] No external integration; no contract-test recommendation
      (signal + local filesystem only).
- [x] Enforcement: per-feature 100% mutation on modified `main.rs` +
      `inhibition.rs` (CLAUDE.md, ADR-0005 Gate 5) + the acceptance
      negatives. No import-graph rule applies (sequencing invariant).
- [x] Simplest solution: build-new-then-swap chosen over `ArcSwap`
      (rejected for the 35-rule scaling target) and per-task self-reload
      (rejected: breaks the atomic-catalogue contract).

## Self-review (reviewer not invocable from subagent context)

A rigorous structured self-review against the
`nw-sa-critique-dimensions` dimensions was performed in lieu of a
dispatched `nw-solution-architect-reviewer` (not invocable from this
subagent context). Result: see the "Self-review" section returned to the
parent. A top-level `@nw-solution-architect-reviewer` run is FLAGGED for
the parent to dispatch before DEVOPS.

## Changelog

- 2026-06-05: DESIGN wave authored. Resolved the four flagged
  sub-decisions; chose the single-orchestrator build-new-then-swap model;
  authored ADR-0063 (citing ADR-0034 as governing contract, ADR-0034
  unmodified); extended brief.md; pinned the two observables.
