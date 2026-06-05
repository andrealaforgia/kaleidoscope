# Wave Decisions: cinder-wal-error-surfacing-v0 (DISCUSS)

Author: Luna (nw-product-owner). Wave: DISCUSS. Date: 2026-06-05.

## Feature framing decisions (DISCUSS, decided)

| ID | Decision | Rationale |
|---|---|---|
| F-Type | **Backend** (storage correctness + a trait API change) | No UI surface; the operator touchpoint is the CLI/ingest path and the trait contract. |
| F-Skeleton | **Walking Skeleton = No** (brownfield) | cinder, the `TieringStore` trait, the `FileBackedTieringStore` adapter, and `MigrateError::PersistenceFailed` all exist. The walking skeleton is US-01+US-02 (the trait change + live caller), not a greenfield bootstrap. |
| F-UX | **UX research = Lightweight** | Single persona (Priya, an operator with a failing disk). The emotional arc is Problem Relief (anxious about silent data loss -> relieved by a loud, honest failure). No journey-visual / journey-yaml artifacts produced — backend feature with no screen flow; the CLI error-message shape is the only UX surface and is captured in the AC and TUI-error guidance below. |
| F-JTBD | **The fail-loud-stay-consistent operator job** (see `user-stories.md` > The Operator Job) | Grounded in the four-quadrants Q2-MEDIUM assessment + verifier triage + Earned-Trust posture. |
| F-Slicing | **Single coherent cinder slice (US-01..US-03) + a separable sluice uniformity slice (US-04)** | The trait change and the live-gateway caller handling are load-bearing and ship together (walking skeleton). sluice is isolated to R3 as the carpaccio cut-line. |

## Decisions FLAGGED for DESIGN (the heart of the feature)

These are the load-bearing decisions DESIGN (`nw-solution-architect`) owns. DISCUSS encodes the
REQUIREMENT (fail loud, stay consistent, write-ahead ordering, count honesty); DESIGN decides the exact
signatures, the caller-by-caller handling, and the sweep semantics.

### D1 — The `TieringStore` trait signature change + the full caller ripple (load-bearing)

- **What**: `place(...) -> ()` becomes `place(...) -> Result<(), MigrateError>`; `evaluate_at(...) ->
  usize` becomes `evaluate_at(...) -> Result<usize, MigrateError>` (or a result carrying the durable
  count). `migrate` already returns `Result` and is the model.
- **Public-API impact (expected, correct)**: Gate 2 (`cargo public-api`) and Gate 3 (semver) WILL flag
  this. It is a deliberate semver-**MINOR** at most (pre-1.0). **Do NOT touch 1.0.0** (CLAUDE.md /
  MEMORY: 1.0.0 is Andrea's call). Note: ADR-0060 explicitly PRESERVED `TieringStore` byte-identity
  (C1); this feature is the deliberate, justified departure from that constraint — an operation that
  persists must be able to fail.
- **Caller ripple DESIGN must map and decide handling for** (verified caller list in `user-stories.md`):
  - `flush()` (LIVE gateway ingest, `kaleidoscope-cli/src/lib.rs:265`) — see D2.
  - `place()` CLI library fn (`kaleidoscope-cli/src/lib.rs:543`) — surface to CLI exit/stderr.
  - `evaluate_policy()` CLI library fn (`kaleidoscope-cli/src/lib.rs:590`) — surface + see D3.
  - `InMemoryTieringStore::place` / `evaluate_at` (`cinder/src/store.rs:140,200`) — return `Ok(...)`.
  - `cinder_crash_target` bin; integration-suite restart test; ~12 cinder + CLI + self-observe test files.
  - **NOT** the self-observe `CinderToPulseRecorder` / `CinderToOtlpJsonWriter` bridges — they consume
    the `MetricsRecorder` port, not `TieringStore::place`/`evaluate_at`; the ripple does not reach them.
- **DESIGN owns**: exact signature (`MigrateError` vs a new narrower error), whether to introduce a
  type alias, and the mechanical test-call-site updates.

### D2 — Live-gateway ingest behaviour on a tier-persist failure (operator-visible)

- **What**: when `cinder.place(...)` returns `PersistenceFailed` inside `flush()`, does the ingest
  **fail loudly** (propagate, the batch is reported as not durably tiered, non-zero exit) or
  **log-and-continue** (structured WARN + a non-silent ingest summary counting un-persisted batches)?
- **Why it is flagged, not decided here**: it is an operator-visible behavioural policy with real
  trade-offs (fail-fast durability strictness vs ingest availability under a degrading disk). DISCUSS
  requires only that the behaviour be **deliberate and never a silent green success** (US-02 AC). DESIGN
  picks the branch and documents it; the chosen branch becomes a locked AC for DISTILL.
- **Luna's lean** (non-binding input for DESIGN): fail-the-ingest is the more Earned-Trust-consistent
  default (a tier placement that cannot be persisted is a durability failure, and the ingest already
  returns a `Result`), but log-and-continue may be justified if tier metadata is reconstructible. DESIGN
  decides with the architecture in view.

### D3 — `evaluate_at` partial-vs-fail-whole on a multi-item sweep

- **What**: `evaluate_at` migrates many items in a loop. When one migration's WAL append fails partway
  through, does the sweep **fail the whole sweep** (return `Err` on the first WAL error) or **report a
  partial durable count** (migrate-and-persist what it can, return the durable count plus a failure
  signal)?
- **Why it is flagged**: a real semantic decision affecting what the returned count MEANS and what state
  the store is left in. DISCUSS requires only that the reported count **never overstate durability** and
  that a failure be **surfaced** (US-03 AC). DESIGN picks the semantics; the choice becomes a locked AC.
- **Constraint either way (C2)**: each migration follows write-ahead ordering — append before the
  per-item memory mutation; a failed append does not torn-mutate that item.

### D4 — sluice's surfacing channel (its trait returns differ from cinder's)

- **What**: sluice's `Queue::dequeue` returns `Option<Message>`, `ack`/`nack` return `()`. They have no
  `Result` channel today. DESIGN decides the exact surfacing shape (mirror cinder's `Result`, change the
  `Queue` trait, or another channel) given these return types and that sluice is UNWIRED.
- **Why it is flagged**: the cinder fix shape does not transfer 1:1 because of the different signatures.
  DISCUSS requires only that the three swallow sites stop swallowing and stay consistent (US-04 AC).
- **Note**: this is the carpaccio cut-line. If the cinder ripple (D1) proves large in DESIGN, US-04 can
  be split into a separate follow-up feature with no loss of cinder value (it is already isolated in R3).

## Risks

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| **No DIVERGE artifacts** — JTBD not validated through a DIVERGE wave | Medium | Low | The job is grounded in the four-quadrants Q2-MEDIUM assessment + verifier triage + the Earned-Trust posture (ADR-0049/0059/0060) and verified directly in code. The defect and fix direction are unambiguous. Recorded here; does not block. |
| **Caller ripple larger than expected** (D1) | Low | Medium | Verified caller list is enumerated and bounded (~15 files, mostly tests; one live caller). If the live-handling work balloons, US-04 (sluice) is the pre-defined carpaccio cut. |
| **Gate 2 / Gate 3 flag the public trait change** | High (expected) | Low | This is correct and deliberate (D1). Semver-MINOR, pre-1.0. NEVER 1.0.0. Annotate the expected public-api diff in DESIGN/DELIVER. |
| **A failing-disk test that passes on the bug** (the ADR-0060 §1 false-confidence trap) | Low | High | Reuse the `FsyncBackend` / `open_with_fsync_backend` seam (C5): the failing substrate must make the un-surfaced path OBSERVABLY fail (the un-persisted placement absent after reopen), so the test is falsifiable — it passes only when the error is surfaced AND memory is consistent. DESIGN/DISTILL must not inherit a test that cannot fail on the swallow. |
| **Torn in-memory state on failure** (C2) | Low | High | Write-ahead ordering: append before memory mutation; on failure leave memory untouched. Asserted by the "failed overwrite preserves prior durable value" scenario (US-01 #3) and the guardrail. |

## Notes for downstream waves

- **DESIGN** (`nw-solution-architect`): own D1-D4. Map the caller ripple, pick the D2 ingest policy and
  D3 sweep semantics, decide sluice's D4 channel. Produce the ADR (the trait change is ADR-worthy: it is
  a deliberate departure from ADR-0060 C1). Confirm the failing-substrate mechanism is falsifiable.
- **DISTILL** (`nw-acceptance-designer`): the BDD scenarios in `user-stories.md` are the source; the D2
  and D3 branches DESIGN picks become locked ACs. Do NOT inherit a failing-disk test that passes on the
  swallow (ADR-0060 §1 lesson).
- **DELIVER** (`nw-software-crafter`): only the crafter writes `crates/*/src/`. Write-ahead ordering +
  surfacing + the mechanical caller updates. 100% mutation kill on modified files (Gate 5).
