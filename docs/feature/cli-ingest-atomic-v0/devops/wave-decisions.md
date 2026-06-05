# Wave Decisions — `cli-ingest-atomic-v0` / DEVOPS (SLIM)

> **Author**: `nw-platform-architect` (Apex), DEVOPS wave, 2026-06-05.
> **Mode**: SLIM. Scope: a single-function re-ordering inside one existing
> file — `kaleidoscope_cli::ingest` (`crates/kaleidoscope-cli/src/lib.rs`)
> becomes parse-all-then-flush-all, making CLI ingest all-or-nothing on a
> parse error. No new crate, no new dependency, no new CLI surface, no
> deploy target.
> **nWAVE-ORDER**: order is DISCUSS -> DESIGN -> DEVOPS -> DISTILL -> DELIVER.
> DEVOPS runs BEFORE DISTILL and DELIVER. The proving test
> (`crates/kaleidoscope-cli/tests/ingest_atomic.rs`) and the production
> change DO NOT EXIST YET — that is the EXPECTED, CORRECT state at this wave.
> Absent code/tests are NOT a defect here.
> **Inputs read**: DESIGN `design/wave-decisions.md` + ADR-0064 (DD-1
> buffer-all-then-flush, DD-6 For-Acceptance-Designer, the 5 AC);
> DISCUSS `discuss/outcome-kpis.md`; `.github/workflows/ci.yml`;
> `crates/kaleidoscope-cli/tests/ingest_and_read_roundtrip.rs`;
> `scripts/hooks/pre-commit`.

## Headline

**Existing gates cover this change; NO new CI job is needed.** The proving
test is a **deterministic typed-error + count-readback in-process test** with
**no flake surface**. The only DEVOPS artefacts are this file and
`environments.yaml` (slim: `clean` + `ci`, no deploy surface).

## A1 — CI delta: NO new crate -> NO new CI job

This feature adds no crate and no dependency, so it adds no CI job. The
existing gates already cover it:

- **gate-5-mutants-kaleidoscope-cli** (`.github/workflows/ci.yml:1725`, name
  at `:1726`) already mutates **every** src file under
  `crates/kaleidoscope-cli/**` via path-filtered `--in-diff` (the `git diff
  "$BASELINE" HEAD -- 'crates/kaleidoscope-cli/**'` at **ci.yml:1782**, with
  the full-suite fallback at `:1791`/`:1798`). The re-ordered `ingest` fn in
  `src/lib.rs` is therefore mutation-tested at the project's per-feature
  **100%** kill-rate target (root `CLAUDE.md` "Mutation Testing Strategy";
  ADR-0005 Gate 5) **without any new job**. Baseline cascade: `origin/main`
  (PR) -> `HEAD~1` (push to main) -> full.
- **gate-1-test** (`ci.yml:136`) runs `cargo test --workspace --all-targets
  --locked` (`ci.yml:184`). The new integration test
  `crates/kaleidoscope-cli/tests/ingest_atomic.rs` is an `--all-targets`
  integration target inside an already-graduated workspace member, so it is
  compiled and run by this gate automatically — only a single `[[test]]`
  manifest entry is added (per ADR-0064 DD-6), no harness wiring.
- **gate-4-deny** (`ci.yml:83`) auto-covers: no new dependency, so the
  dependency-graph delta is empty (no-op).
- **gate-2-public-api** (`ci.yml:248`) and **gate-3-semver** (`ci.yml:356`):
  `kaleidoscope-cli` is not in their package lists, and in any case ADR-0064
  DD-4 confirms **net-new public surface is NONE** — nothing to lock.

**Decision: no new CI job, no ci.yml edit.** Stating it explicitly so a future
maintainer does not reflexively add a per-feature job for a re-ordering the
existing path-filtered gate already covers.

## A2 — Determinism of the proving test

The proving test (`tests/ingest_atomic.rs`, authored in DISTILL/DELIVER —
NOT yet, correctly) mirrors the harness of the existing
`tests/ingest_and_read_roundtrip.rs`: an **in-process library call** to
`ingest(tenant, tmp_data_dir, batch_size, Cursor::new(ndjson_bytes), None)`
against a per-test tmp store-dir, with the count read back via `read`/`stats`
against the same `data_dir`. (DD-6 specifies this in-process shape; it is the
CLI driving port exercised through the library, equivalent to spawning the
binary and piping NDJSON on stdin, and strictly simpler.)

It asserts, per the 5 AC:

- **parse-error-commits-nothing** — 3 valid + malformed line 4 at
  `batch_size=3`: `Err(Error::ParseRecord { line: 4, .. })` AND a follow-up
  `read`/`stats` count **UNCHANGED at 0**.
- **re-run-no-double** — same still-malformed input a SECOND time: again
  `Err(ParseRecord { line: 4 })`, count **STILL 0**.
- **corrected-file-ingests-once** — line 4 fixed (4 valid at `batch_size=3`):
  `Ok(IngestStats { records_ingested: 4, batches_flushed: 2, .. })`, count
  **exactly 4**.
- **valid-file-negative-control** — 250 valid at `DEFAULT_BATCH_SIZE=100`:
  `Ok(IngestStats { 250, 3, 3 })`, count **250**, byte-equivalent stderr
  summary.
- **malformed-first-line boundary** — `Err(ParseRecord { line: 1 })`, count
  **0**.

**Verdict: fully deterministic, no flake surface.** Every assertion is a
typed-`Result` value (the library-boundary equivalent of an exit code) plus a
committed-state COUNT read back. There are **no signals** (unlike the beacon
SIGHUP tests), **no crash target**, **no wall-clock / p95 / sleep**, and **no
concurrency**. The same `cargo test --workspace --all-targets --locked`
invocation runs it in **both** the local pre-commit hook
(`scripts/hooks/pre-commit` Step 4) **and** CI gate-1-test — identical command,
identical determinism. This sidesteps the overnight p95 flake class entirely
(`project_p95_wallclock_flakes_overnight`): there is no timing assertion here.

## A3 — Instrumentation: none new

Per `discuss/outcome-kpis.md` "Handoff to DEVOPS": all four outcome KPIs
(OK1 parse-error-commits-nothing as North Star, OK2 re-run-no-double, OK3
corrected-once, OK4 valid-file guardrail) are verified at **build time** by
`tests/ingest_atomic.rs` plus the locked roundtrip suite, not by production
telemetry. No new runtime instrumentation, dashboard, or alert is designed.
The existing `--observe-otlp` commit-side metric stream is unaffected — a
failed ingest now emits zero commit-side metric lines, consistent with
committing zero records. No `kpi-instrumentation.md` is warranted.

## A4 — Rollback

`git revert` of the single-file commit. One file, no migration, no persistent
schema change, no deploy. The change is a pure re-ordering of in-memory
parse-vs-flush; reverting restores the prior interleaved loop with no
data-state cleanup. By the feature's own definition, an all-or-nothing ingest
leaves no partial commit behind, so there is nothing to compensate on revert.

## Artefacts written

- `docs/feature/cli-ingest-atomic-v0/devops/environments.yaml` (NEW — slim:
  `clean` + `ci`, no deploy surface).
- `docs/feature/cli-ingest-atomic-v0/devops/wave-decisions.md` (this file).
- No `.github/workflows/ci.yml` edit (A1). No `kpi-instrumentation.md` (A3).

## Self-review (reviewer dispatch)

`@nw-platform-architect-reviewer` was dispatched; if not invocable from this
subagent context, a structured self-review was performed and a top-level
reviewer run is flagged in the parent report (INCLUDING the nWAVE-order
reminder, so the reviewer does not mistake the not-yet-existing
`tests/ingest_atomic.rs` and `lib.rs` re-ordering for a defect). Self-review
summary: simplest-infrastructure-first holds (zero new components — the change
REMOVES a footgun and reuses existing gates); rollback-first satisfied
(A4, git revert, no data cleanup); SLO/observability proportionate (no runtime
surface, build-time KPIs per A3); determinism is strong (A2 — typed-error +
count-readback, no flake class present); the no-new-job claim is cited to the
exact ci.yml lines (A1). No critical/high issues found.
