# Wave Decisions: cinder-wal-error-surfacing-v0 (DESIGN)

Author: Morgan (nw-solution-architect). Wave: DESIGN. Mode: PROPOSE (autonomous). Date: 2026-06-05.

Primary artefact: **ADR-0065** (`docs/product/architecture/adr-0065-cinder-wal-error-surfacing-trait-signature.md`).
Brief section: `docs/product/architecture/brief.md` → `## Application Architecture — cinder-wal-error-surfacing-v0`.

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | − (gap / flag) |
|---|---|---|
| `discuss/user-stories.md` | 4 stories, verified code findings, caller tables, BDD scenarios, C1-C7 constraints — all consumed verbatim into D1-D4 | − No DIVERGE wave (recorded low-impact risk; defect verified in code, does not block) |
| `discuss/story-map.md` | WS = US-01+US-02; R1/R2/R3 slicing; sluice = carpaccio cut-line — adopted as-is | − none |
| `discuss/wave-decisions.md` | D1-D4 flagged to DESIGN; Luna's non-binding D2 lean (fail-the-ingest); C1-C7 | − D2/D3/D4 left open for DESIGN — RESOLVED below |
| `discuss/outcome-kpis.md` | KPI-1..4 (swallow sites → 0; falsifiable per site); guardrails (healthy path, no torn memory, write-ahead, 100% mutation) | − none |
| `brief.md` cinder + storage Earned-Trust posture | ADR-0049/0059/0060/0064 lineage; `open_with_fsync_backend` + `FsyncBackend` seam already wired; ADR-0064 all-or-nothing posture | − ADR-0060 C1 preserved `TieringStore` byte-identity — this feature DEPARTS; ADR-0065 amends C1 |
| ADR-0060 | two-mechanism proving; `open_with_fsync_backend`; atomic snapshot already in cinder + sluice | − its C1 byte-identity for `TieringStore` is the constraint this feature deliberately amends |
| ADR-0064 | all-or-nothing ingest posture; §"Out of scope" left mid-commit write-failure to the 0059/0060 line | − this feature IS that line for the cinder write path (D2 extends it) |
| ADR-0049 / ADR-0059 | per-record `sync_all`; shared `replay_wal_tolerating_torn_tail` (cinder already on it) | − none |
| Ground-in-code (store.rs, file_backed.rs ×2, cli/lib.rs) | swallow sites, `migrate` model, `PersistenceFailed` exists, caller list (20 files), sluice unwired confirmed | − none — summary matched code |

## The four flagged decisions — RESOLVED

### D1 — `TieringStore` trait signature change + full caller ripple — RESOLVED

New signatures (the `migrate` shape, generalised; reuse existing `MigrateError`, no new type):

```text
fn place(&self, tenant: &TenantId, item: &ItemId, tier: Tier, placed_at: SystemTime)
    -> Result<(), MigrateError>;        // was -> ()
fn evaluate_at(&self, now: SystemTime, policy: &TierPolicy)
    -> Result<usize, MigrateError>;     // was -> usize
```

- Adapter: `FileBackedTieringStore` follows write-ahead ordering — `append_wal(...)?` BEFORE
  `apply_to_entries` + `record_place`/`record_migrate` (the `migrate` discipline at `file_backed.rs:316`).
- `InMemoryTieringStore`: returns `Ok(())` / `Ok(count)` (never persists).
- Public-API + semver: Gate 2 + Gate 3 WILL flag — **expected, correct**. Deliberate semver-MINOR for
  `cinder`, pre-1.0. **NEVER 1.0.0** (Andrea's call).
- CLI `Error`: add `CinderPlace(MigrateError)` + `CinderEvaluate(MigrateError)` for stderr clarity
  (reusing `CinderMigrate` is behaviourally sufficient; crafter's choice).

### D2 — live ingest behaviour on a tier-persist failure — RESOLVED: **fail-the-ingest**

`flush()` propagates `cinder.place(...).map_err(Error::CinderPlace)?`; ingest stops; stderr
`error: cinder place: persistence failed: io: <reason>`; non-zero exit; the failed batch is never
acked durable. Rationale: Earned-Trust + ADR-0064 all-or-nothing consistency on the write-failure
axis ADR-0064 left to this line; `flush` already returns `Result` and propagates `lumen.ingest` with
`?` one line above (smaller, uniform change); a failing disk is a substrate fault to surface
immediately, not ride through. Locked AC for DISTILL. (Cross-store rollback of earlier batches stays
out of scope, consistent with ADR-0064.)

### D3 — `evaluate_at` sweep on a mid-loop WAL failure — RESOLVED: **fail-whole on first error**

Returns `Err(PersistenceFailed)` on the first `append_wal` failure (the `?`); no partial count.
Post-failure invariant: migrations before the failure are durable AND applied (memory == disk); the
failing migration is neither on disk nor applied (no torn item); migrations after are untouched (prior
durable tier). On success the returned `usize` == durable count exactly. Rationale: simplest honest
`Result<usize, MigrateError>` contract (no new partial-count type); the count's meaning stays crisp;
the durable prefix survives and re-run is idempotent. Locked AC for DISTILL.

### D4 — sluice's surfacing channel — RESOLVED: **change the `Queue` trait's three ops to fallible** (thinner, R3, zero live blast radius)

```text
fn dequeue(&self, tenant: &TenantId) -> Result<Option<Message>, EnqueueError>;  // was -> Option
fn ack(&self, id: MessageId)  -> Result<(), EnqueueError>;                       // was -> ()
fn nack(&self, id: MessageId) -> Result<(), EnqueueError>;                       // was -> ()
```

Reuse existing `EnqueueError::PersistenceFailed` (no new type, no cross-pillar coupling). `dequeue`
nests `Result<Option<_>, _>` (empty-vs-present is orthogonal to persisted-vs-failed). Write-ahead
ordering: append FIRST, mutate `pending`/`in_flight`/`total` only on `Ok`. sluice is UNWIRED (grep of
`crates/**/src/**`: only its own crate, `sluice_crash_target`, integration-suite reference
`FileBackedQueue` — **zero live blast radius**), so the trait change ripples only to sluice's tests,
the crash-target bin, and the integration-suite — the lower-risk R3 slice / carpaccio cut. Splittable
into a separate follow-up feature with no loss of cinder value if the cinder ripple proves large.

## Reuse Analysis (MANDATORY — RCA hard gate)

**Verdict: the fix EXTENDS existing components; net-new components = NONE; net-new types = NONE.**

| Component / type | Path | Decision | Justification |
|---|---|---|---|
| `MigrateError::PersistenceFailed { reason }` | `cinder/src/store.rs:49` | **REUSE verbatim** | Already exists; already what `append_wal` produces. No new error type. |
| `EnqueueError::PersistenceFailed { reason }` | `sluice/src/queue.rs:65` | **REUSE verbatim** | Already exists. sluice's error vocabulary, pillar-local. |
| `TieringStore` trait | `cinder/src/store.rs:77` | **EXTEND (2 sig changes)** | `place`/`evaluate_at` gain `Result`; `migrate` already `Result` — uniform. |
| `Queue` trait | `sluice/src/queue.rs` | **EXTEND (3 sig changes)** | `dequeue`/`ack`/`nack` gain `Result`; `enqueue` already `Result` — uniform. |
| `FileBackedTieringStore::{place,evaluate_at}` | `cinder/src/file_backed.rs:262,333` | **EXTEND** | Re-order append-before-apply + propagate; `migrate` (`:316`) is the in-crate model. |
| `FileBackedQueue::{dequeue,ack,nack}` | `sluice/src/file_backed.rs:334,352,361` | **EXTEND** | Re-order + propagate; `enqueue` (`:318`) is the model. |
| `append_wal` (both crates) | `cinder/...:405`, `sluice/...:416` | **REUSE UNCHANGED** | Already returns `Result<(), …PersistenceFailed>`; the fix stops discarding it. |
| `open_with_fsync_backend` + `FsyncBackend` seam | `wal-recovery` (shared leaf) | **REUSE (extend with failing mode)** | ADR-0060 seam; DISTILL injects a failing/lying backend to make failure ACs falsifiable. Possibly add a `failing` mode to `LyingFsyncBackend` (DELIVER detail). |
| CLI `Error` enum | `kaleidoscope-cli/src/lib.rs:73` | **EXTEND (2 thin variants)** | `CinderPlace`/`CinderEvaluate` for stderr clarity; reusing `CinderMigrate` is acceptable. |
| `InMemoryTieringStore` | `cinder/src/store.rs:139` | **EXTEND (sig only)** | Returns `Ok(...)`; impl, not caller. |

**No new crate, no new trait, no new error type, no new dashboard, no new event.** The only additive
public surface is the trait-signature changes (intended, semver-MINOR) and two thin CLI `Error`
variants.

## Caller-ripple map (verified: grep `.place(` / `.evaluate_at(` across `crates/**/*.rs` = 20 files)

| Caller | Location | Live? | Handling |
|---|---|---|---|
| `flush()` (gateway ingest) | `kaleidoscope-cli/src/lib.rs:265` | **YES** | D2: `.map_err(Error::CinderPlace)?` (fail-the-ingest) |
| `place()` CLI lib fn | `kaleidoscope-cli/src/lib.rs:543` | YES | `.map_err(Error::CinderPlace)?` before stdout line |
| `evaluate_policy()` CLI lib fn | `kaleidoscope-cli/src/lib.rs:590` | YES | D3: `.map_err(Error::CinderEvaluate)?` |
| `cinder_crash_target` bin | `cinder/src/bin/cinder_crash_target.rs:84` | No | `.expect(...)` / propagate |
| integration-suite restart test | `integration-suite/tests/v1_three_adapters_compose_under_restart.rs:140,141,234` | No | `.unwrap()` healthy path |
| cinder slice/lifecycle/durability tests | `cinder/tests/{slice_01,slice_02,v1_slice_01..04}_*` | No | `.unwrap()` healthy; new failing-substrate tests `assert!(matches!(_, Err(PersistenceFailed)))` |
| CLI subcommand tests | `kaleidoscope-cli/tests/{place,evaluate_policy,get_tier,list_items,migrate,migrate_observe_otlp,stats_*,observe_otlp_*}_*` | No | `.unwrap()` healthy path |
| self-observe bridge tests | `self-observe/tests/{cinder_to_pulse,cinder_to_otlp_json}.rs` | No | drive via `InMemoryTieringStore`; `.unwrap()` |

**NOT callers**: self-observe `CinderToPulseRecorder` / `CinderToOtlpJsonWriter` bridges consume the
`MetricsRecorder` port, NOT `TieringStore::place`/`evaluate_at` — ripple does not reach them
(confirmed).

sluice ripple (R3): sluice's own tests, `sluice_crash_target` bin, integration-suite — no live caller.

## Technology Stack

**Rust** (idiomatic per CLAUDE.md — data + free functions + traits where polymorphism is genuine; no
inheritance, composition throughout). No new dependency. Reuses `serde_json`, `wal-recovery`
(`FsyncBackend`), `std::io`. License posture unchanged (AGPL-3.0-or-later workspace).

## Constraints (carried + resolved)

- **C2 — write-ahead ordering, no torn memory**: append FIRST, mutate memory only on `Ok`; failed
  overwrite preserves prior durable value (US-01 #3). Enforced by AST structural hook + failing-
  substrate gold-test asserting memory == disk on failure.
- **C3 — public trait change expected**: Gate 2/Gate 3 flag; semver-MINOR, pre-1.0, NEVER 1.0.0.
- **C5 — failing substrate in-suite**: inject failing `FsyncBackend` via `open_with_fsync_backend`;
  no host disk-fill. Falsifiable: test fails on the swallow bug, passes only on surfaced-and-consistent.
- **C6 — mutation 100%** on modified files (Gate 5): the `?` on `append_wal` and the append-before-
  apply ordering are the primary mutants.
- **C7 — trunk-based, no CI gates** (CI is feedback).

## Upstream Changes to DISCUSS

None blocking. The DISCUSS D2/D3/D4 branches were left open BY DESIGN; this wave resolves them and
they become locked ACs for DISTILL (fail-the-ingest; fail-whole sweep; sluice `Queue` fallible). The
two thin CLI `Error` variants (`CinderPlace`/`CinderEvaluate`) are a DESIGN refinement, not a DISCUSS
change. No story re-scoping; the carpaccio cut-line (US-04/sluice in R3) is preserved.

## Quality gates (DESIGN self-check)

- [x] Requirements traced to components (US-01→place; US-02→flush/D2; US-03→evaluate_at/D3; US-04→sluice/D4)
- [x] Component boundaries + responsibilities (cinder trait + adapter; sluice trait + adapter; CLI callers)
- [x] Tech choices in ADR with 5 alternatives + rejection rationale (A log-only, B callback, C panic, D3-alt partial, D4-alt verbatim-mirror)
- [x] Quality attributes: reliability (fault tolerance / recoverability — the headline), maintainability (uniform `migrate` shape), testability (failing-substrate seam)
- [x] Dependency-inversion: failure surfaced through the port (trait `Result`), substrate injected through `FsyncBackend` port — ports-and-adapters preserved
- [x] C4 (Component, Mermaid) — see brief section
- [x] Integration patterns: in-process `Result` propagation; no external integration
- [x] OSS-only, no proprietary
- [x] AC behavioural (surfaces error / memory == disk / count == durable) — not implementation-coupled
- [x] External integrations annotated: NONE (in-process filesystem)
- [x] Enforcement tooling: AST structural hook + `#[must_use]` Result + failing-substrate gold-test + `cargo mutants`
- [x] Peer review — APPROVED (iteration 1, 0 critical / 0 high)

## Peer review (nw-solution-architect-reviewer dimensions) — iteration 1

Reviewed against `nw-sa-critique-dimensions` (bias, ADR quality, completeness, feasibility, priority).
nWave-order honoured: no production code/tests/CI exist at DESIGN — that absence is EXPECTED and was
NOT treated as a finding.

- **Architectural bias**: none. No new tech; design actively resists a callback framework (alt B),
  a new partial-count type (D3-alt), and a new crate. No resume-driven/latest-tech bias.
- **ADR quality**: context (the swallow + acked-but-not-durable lie) present; 5 alternatives with
  rejection rationale (exceeds 2-min); consequences positive/negative/trade-off present; the
  ADR-0060 C1 amendment is explicit, scoped, and immutability-respecting.
- **Completeness**: reliability (headline), maintainability, testability addressed; observability
  correctly scoped out (a future runtime counter is a separate feature); no performance gap (no fsync
  added — reordering only).
- **Feasibility**: testability STRONG — failing substrate injectable via the EXISTING
  `open_with_fsync_backend`; ports-and-adapters preserved. No capability gap (the seam shipped in
  ADR-0060).
- **Priority**: Q1 YES (live ingest lie addressed first; sluice last), Q2 ADEQUATE, Q3 CORRECT,
  Q4 JUSTIFIED.

Findings: 0 critical, 0 high; 3 LOW (D2 partial-progress disambiguation — already mitigated;
observability scope-out — acceptable; shared-crate `failing` mode caution — FOLDED into the brief
DEVOPS handoff as an additive/behaviour-preserving requirement). **approval_status: approved.**
