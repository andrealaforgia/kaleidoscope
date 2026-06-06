# Evolution archive — beacon-slo-operator-path-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md`,
`store-fsync-durability-v0-evolution.md`,
`tls-config-reject-v0-evolution.md`,
`claims-honesty-pass-v0-evolution.md`,
`beacon-sighup-reload-v0-evolution.md`,
`cli-ingest-atomic-v0-evolution.md`,
`cinder-wal-error-surfacing-v0-evolution.md` and
`aperture-serve-loop-error-surfacing-v0-evolution.md`, which established
the per-file convention: one file per feature, named
`<feature-id>-evolution.md`, with the sections below.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0067
  (`docs/product/architecture/adr-0067-beacon-slo-operator-path.md`),
  which WIRES (does NOT re-engineer) the ADR-0036 MWMBR SLO synthesis
  engine, honours the ADR-0063 all-or-nothing SIGHUP reload contract
  unchanged, and reconciles ADR-0036's own inconsistencies via an
  appended "Corrected by ADR-0067" note rather than rewriting the
  immutable record.
- Closes: the four-quadrants Q3 "Tested But Unwired" gap, carried forward
  on every recent archive as the named follow-up "beacon SLO unreachable
  (B06)". The verifier left the SLO MWMBR synthesis "for later"; this
  feature is the later. It was the next unwired headline item on the
  carried project-wide list.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| deliver | `4bc8d58` | wire the `[[slo]]` operator path into the loader and reload |
| docs | `0855c4e` | narrative + slide closure |

The DISCUSS, DESIGN, DEVOPS and DISTILL artefacts landed on `main` ahead
of DELIVER, each from its own wave agent; the as-built facts below are
read from the DELIVER commit `4bc8d58`.

## The problem, in Earned-Trust framing

beacon's MWMBR SLO engine was CORRECT and DEAD. `synthesise_slo`
(`slo.rs:106-156`) takes a declared `Slo` and synthesises four
multi-window multi-burn-rate alerting rules (`MWMBR_TABLE`, four rows,
`slo.rs:64-93`); twenty engine tests were green. But across the whole
repo `synthesise_slo` had exactly ONE caller, and that caller was a test.
No operator could reach it. The headline engine of the beacon product was
exercised only by the suite that proved it correct, never by the door the
operator walks through.

The loader was the missing door, and it was nailed shut. `FileShape`
(`loader.rs:260-265`) was `#[serde(deny_unknown_fields)]` with a single
`rules` field and no `[[slo]]` table. So an operator who declared an
`[[slo]]` table did not merely fail to get SLO rules: the `[[slo]]` table
POISONED its whole file. `toml::from_str` failed with `unknown field
'slo', expected 'rules'`, and `parse_file` returned the entire file as
one `LoaderDiagnostic`, dropping the hand-authored `[[rules]]` in that
same file along with it. The feature's most-wanted action silently broke
the operator's existing rules.

Two doc-lies sat on top of the dead engine, both false precisely because
the feature was never wired:

- `slo.rs:49-51` claimed the loader REJECTS a non-30d error budget
  period. It did not. There was no loader path that parsed an SLO at all,
  so there was no validation to reject anything. An always-fire degenerate
  rule (a `budget = 0` SLO) had no guard standing between it and
  evaluation, because nothing reached evaluation.
- `slo.rs:24-26` claimed a 24-hour cross-validation test backed the
  engine's firing pattern. No such test existed.

This is the acked-but-actually-broken lie the project's Earned-Trust
posture forbids, in its purest unwired form: a wall of green tests
standing in for a shipped feature, two doc comments asserting behaviour
the absent wiring could not possibly produce, and the headline value of
the product reachable by nobody but the test that proved the dead engine
correct.

## The decision lineage

### ADR-0067 WIRES, it does not re-engineer

ADR-0067's whole posture is restraint. The engine is correct; the
synthesis is correct; the four-row `MWMBR_TABLE` is correct. The ADR adds
NO engine logic, NO new synthesis, NO new rule shape. It adds the loader
path that lets a declared `[[slo]]` table reach the already-correct
`synthesise_slo`, and nothing more. The Reuse Analysis (DESIGN F1-F5)
records `synthesise_slo`, `MWMBR_TABLE`, the `Slo` struct, `RawSink` and
its sink validation, the `RawRule::into_rule` pattern, `parse_duration`,
and the entire beacon-server reload orchestrator as REUSE-verbatim or
EXTEND-only. The net-new surface is one private wire struct, one
conversion, one defaulted field, five blessed-field entries, and one
duplicate-name scan.

### It honours the ADR-0063 all-or-nothing reload UNCHANGED

A malformed SLO is a `LoaderDiagnostic` exactly like a malformed rule. At
startup a poisoned file is skipped, and if nothing valid remains the
existing `has_any_rules()` startup refusal fires. Under SIGHUP the
existing `broken_edit_added_nothing = has_diagnostics() && added_count ==
0` guard (`main.rs:343`) refuses the reload, retains the previous
catalogue, and emits `beacon.reload.refused`. There is NO new reload code:
a refused SLO surfaces exactly as a refused rule, so a degenerate rule
never reaches evaluation by the same mechanism that already protects
against a degenerate hand-authored rule.

### It reconciles ADR-0036's inconsistencies without rewriting it

ADRs are immutable. ADR-0036 carried three claims the as-built wiring
shows were wrong, so DELIVER APPENDED a "Corrected by ADR-0067" note
rather than editing the record: FOUR rules per SLO not five (the
`MWMBR_TABLE` has four rows); NO `annotations` field on the synthesised
`Rule` (correlation is the `slo_source` label, confirmed against
`types.rs`); and validation is the Rust TOML loader, not a CUE schema, so
the catalogue language is TOML and the reference fixtures are
PromQL/expected-firing, not `.cue`.

## The as-built shape

### F1 — the private `RawSlo`, the `FileShape.slo` field, the blessed fields

A private `RawSlo` (`#[serde(deny_unknown_fields)]`) lands in `loader.rs`
beside `RawRule`. Its five wire keys (`service`, `good_events_query`,
`total_events_query`, `target_availability`, `error_budget_period`
defaulting `"30d"`, plus `sinks` reusing `RawSink` verbatim) are the
operator-facing names; `source_path` is filled by the loader from the
file path, not a wire key. `FileShape` gains `#[serde(default)] slo:
Vec<RawSlo>` beside `rules`, so the rules-only path stays byte-identical
(the vector defaults empty). `deny_unknown_fields` is KEPT on `FileShape`,
`RawSlo` and `RawSink`, and `BLESSED_FIELDS` is extended with the five SLO
keys so a near-miss still earns its "did you mean" suggestion.

### F2 — the merge, refuse-on-duplicate-name, the real synthesised names

Per file the loader takes `[[rules]]` to rules first, then each `[[slo]]`
through `into_slo` then `synthesise_slo` to four appended rules, and
`load_rules` extends across files in the existing sorted-path order. The
collision policy is REFUSE the load with a `LoaderDiagnostic` naming the
duplicated `name` and the file(s); the scan runs over the WHOLE merged
catalogue because a collision can span two files. A colliding rule is
DROPPED by refusing the load, NEVER silently shadowed (US-04, "never a
silent shadow"); precedence/last-wins was rejected as a silent drop. The
synthesised names are the engine's real format `{service}_slo_{page|
ticket}_{long}_{short}` (`slo.rs:124-127`), e.g. `checkout_slo_page_1h_5m`,
`checkout_slo_page_6h_30m`, `checkout_slo_ticket_*`, not the DISCUSS
illustrative `checkout_page_1h_5m`; the shipped code is the authority and
DISTILL asserted the real `_slo_`-infixed names.

### F3 — validation in `RawSlo::into_slo`, the exact messages, the killed degenerate

Validation runs in a new `RawSlo::into_slo(...) -> Result<Slo, String>`
(mirroring `RawRule::into_rule`), BEFORE `synthesise_slo`:

1. `target_availability` strictly in `(0.0, 1.0)`, rejecting `<= 0.0` or
   `>= 1.0`:
   `invalid target_availability 1.0 (must be strictly greater than 0 and
   strictly less than 1) in SLO "checkout"`. This kills the degenerate
   always-fire (a `budget = 0` SLO whose page rule fires unconditionally).
2. `error_budget_period == 30d`, rejecting any other:
   `unsupported error_budget_period "7d" (only "30d" is supported at v0)
   in SLO "checkout"`. This makes the `slo.rs:49-51` doc claim TRUE.

Each returns `Err(String)` to a per-file `LoaderDiagnostic`
(report-and-fail-the-file, not a crash; the loader's wrapper prefixes
`{file}:`). No degenerate rule is ever synthesised, merged, or evaluated:
the guard sits before synthesis.

### F4 — the SIGHUP reload, carried for FREE through the reused loader

beacon-server's `main.rs` is UNTOUCHED. The SIGHUP reload carries SLOs for
free because it reuses the same loader and the same diff machinery. Counts
are expansion-aware BY CONSTRUCTION: the existing reload computes
`new_names` from `outcome.rules` (which already holds the four synthesised
rules per SLO) and `added = new_names.difference(live_names).count()`
(`main.rs:338-340,408`), so one new SLO yields `added = 4` and
`rules_loaded` counts the four, with no new code and no new event field (a
dedicated `slos_added` was rejected as redundant; the `slo_service` label
already groups them). Firing state carries over by stable synthesised
name (ADR-0063 sub-decision 2 matches `RuleState` by `name`), so a firing
synthesised rule survives an unrelated SLO edit, keeps its `Firing`
`since`, and earns no re-page.

### F5 — the delivered 24-hour cross-validation test and the corrected docs

The missing 24-hour cross-validation test was DELIVERED (the honest fix),
and the `slo.rs:24-26` doc wording corrected to point at it. The engine is
deterministic (no clock, no RNG), so a synthetic 24-hour trace asserts the
firing pattern against a hand-authored reference, bounded and in-tree with
no new dependency. The test reads the `budget * threshold` limit back out
of each synthesised rule's PromQL (the engine embeds it,
`slo.rs:181-189`), so the cross-validation evaluates the SAME number the
engine emitted, no second source of truth. Arm A (a sustained 5% error
rate against a 0.999 SLO) MUST fire both page rules; Arm B (a sustained
0.05% rate, the negative control) MUST fire nothing; an ordering arm
asserts page limits are tighter than ticket limits. The reference is
hand-authored PromQL/expected-firing, NOT `.cue` (ADR-0036's CUE
references corrected by ADR-0067).

## The proof and its boundary

- 100% mutation kill on the MODIFIED loader surface (ADR-0005 Gate 5;
  CLAUDE.md per-feature 100%), via the existing `gate-5-mutants-beacon
  --in-diff` job: 29 caught + 3 unviable, 0 missed; the five survivors
  that surfaced were killed by ADDING tests, not by weakening any
  assertion. No new CI job was needed.
- `beacon-server/src` is UNCHANGED, so its mutation surface is empty: the
  reload arm carries SLOs for free precisely because no production line
  there moved. There is nothing to mutate where nothing changed.
- The wiring smoke is the real `beacon-server` subprocess firing at the
  webhook with `rules_loaded=4` / `added=4`: a real child process, a real
  POSIX `kill -HUP`, a real `wiremock` PromQL backend and webhook catcher.
  This is what makes the verifier's B06 buildable: the engine is now
  reachable through the operator's door, proven by a black-box test that
  enters through that door rather than calling the engine directly.
- Every subprocess test reaps its child (`shutdown` = kill + wait), so
  there are no leaked beacon-server processes and no port pollution.
- Semver held at `0.1.0`: the change is additive (`FileShape` gains a
  defaulted field; `RawSlo`/`into_slo` are private; public
  `Rule`/`LoadOutcome`/`load_rules`/`synthesise_slo` unchanged). beacon
  and beacon-server are not enrolled in the Gate 2/3 public-API tracking;
  no public-api gate fires. NEVER 1.0.0: that is a public stability
  promise, Andrea's call alone, and premature while these APIs churn.

## The honest finding: the two test-correctness fixes in DELIVER

Recorded in the same spirit as the prior archives' honest-finding
sections. DELIVER made two test-only corrections, neither of which changed
production behaviour or weakened an assertion:

1. The `slo_reload` firing bound was widened from 20s to 90s. The
   synthesised rule's evaluation interval is fixed at 30s and a fresh rule
   fires on its SECOND tick, so the first page lands near 30s; the old 20s
   bound was an incorrect timing assumption that could miss a correct
   first page, not a weakening of what is asserted. The assertion (the
   named synthesised rule fires) is unchanged; only the generous wait that
   lets it land was corrected.
2. The child runs with `NO_COLOR=1`. ANSI escape sequences inserted
   between a tracing field key and its `=value` broke the substring match
   on `rules_loaded=4` / `added=4`. Disabling colour on the child makes
   the un-coloured value assertions match the structured output. The
   asserted values are unchanged; only the rendering that the substring
   reads was de-coloured.

Both are corrections to wrong test assumptions about timing and rendering,
not relaxations of the contract. No production line moved for either.

## The lesson

A wall of green tests is not a shipped feature when every test calls the
engine directly and none enters through the operator's door. beacon's SLO
engine was correct, twenty tests green, and reachable by nobody: the
loader had no `[[slo]]` table, so the operator's most-wanted action
poisoned its own file, and two doc comments asserted validation and a
cross-validation test that the absent wiring could not back. The value was
not in the engine, which already worked. It was in the door: one private
wire struct, one conversion with two validations, one merge, one
duplicate-name scan, and the reload that then carried SLOs for free
through machinery that did not have to change. This also makes the
verifier's B06 buildable, because the engine is finally reachable through
the surface the verifier probes rather than the suite that proved it
correct in private.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted. The "beacon SLO
unreachable (B06)" item that headed prior archives' lists is CLOSED by
this feature and removed below.

1. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap needs its own slice. Open.

2. sluice wiring. sluice remains UNWIRED: no gateway/server `src` path
   constructs or drives `FileBackedQueue`. Its `Queue` surface was made
   fail-loud before it is wired (zero live blast radius); the wiring
   itself is a separate, still-open slice. Open.

3. sluice torn-tail migration. sluice still carries the inline
   parse-or-die recovery loop; its migration to the shared
   `replay_wal_tolerating_torn_tail` routine is the tracked ADR-0059 §5
   follow-up. Open.

4. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
   doubles the store, because lumen has no idempotency key. The designed
   extraction (ADR-0064 DD-3): success-case dedup earns its own slice.
   Open.

5. ingest-bounded-memory. The buffer-all-then-flush design (ADR-0064)
   holds the whole input's records in RAM before commit. A future feature
   lifts it with a temp-WAL staging stage or a max-records streaming cap.
   Open.

6. ADR-0059 Decision 8 layer b, the AST structural check, remains
   UNWIRED. The structural pre-commit check asserting in-scope stores
   delegate to the shared wal-recovery routine and carry no `let _ =`
   swallow; the tool choice was deferred and remains deferred. It is
   feedback, not a gate, consistent with the pure trunk-based,
   no-required-checks posture; when wired it belongs in the local
   pre-commit stage. Open.

7. aegis unwired. No path authenticates; the aegis surface exists but is
   not on any request path. Open.

8. OTLP partial_success never populated. The OTLP `partial_success`
   response field is never populated, so partial-accept signalling is not
   surfaced to clients. Open.

9. The two claims-honesty DOCUMENT items remain future features if
   wanted. The actual Prometheus-stepped grid for `query_range` (a
   query-api feature) and real gRPC-prefix honouring for `harness`
   (`Framing::GrpcProtobuf`) were documented as v0 reality rather than
   built; each would retire its respective pin. Open only if wanted.

10. aperture early-Ok tolerance. The unexpected-early-`Ok`-without-shutdown
    is treated as FATAL at v0 (surfaced, not tolerated), the honest
    default for a listener that stops unbidden. If a future transport
    legitimately self-stops `Ok` without a shutdown request, that
    distinction would earn its own slice. Open only if such a path ever
    appears.

11. beacon non-30d error budget periods. v0 supports ONLY a 30d error
    budget period (F3 rejects any other with the exact `unsupported
    error_budget_period` message). Other windows (7d, 90d) would each
    need their own `MWMBR_TABLE` row set and earn their own slice. Open
    only if wanted.
