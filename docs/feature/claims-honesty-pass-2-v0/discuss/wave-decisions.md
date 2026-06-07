# Wave Decisions — claims-honesty-pass-2-v0 (DISCUSS)

- **Wave**: DISCUSS (nWave)
- **Analyst**: Luna (nw-product-owner)
- **Date**: 2026-06-07
- **Feature type**: Cross-cutting (documentation honesty across a few crates +
  the platform README + one CI/e2e config)
- **Mode**: Autonomous overnight run. All interactive decisions made by Luna and
  recorded here. No questions returned to the operator.
- **Sequel to**: `claims-honesty-pass-v0` (same Earned-Trust job). This pass
  corrects the residual doc/comment overstatements the four-quadrants reports
  flagged that pass-v0 did NOT cover. pass-v0 corrected the README **Components**
  table rows for Spark/Strata/Cinder/Loom and a cluster of stale-over-green
  crate/test headers; it did **not** touch the Prism row, the pulse crate doc,
  the gateway comments, or the prism playwright config.

## Origin and verification posture

Backlog item #3 of the four-quadrants programme is the **claims-honesty** family
(after `store-fsync-durability-v0` and `tls-config-reject-v0`, both shipped, and
after `claims-honesty-pass-v0`). The substantive *code* findings the reports
flagged (durability, swallowed errors, the tls-lie, unwired auth/SLO,
diagnostics) are already fixed in prior features. What remains is a doc-honesty
cluster: prose/comments/config that claim a capability the code does not have, or
(inverted) tell the reader the data is volatile when it is durable.

**The code is truth.** Luna re-grounded every named overstatement against the
live code on 2026-06-07 by direct archaeology. The verified inventory is the
table below; each row cites the exact code that makes the corrected claim true.

The fix principle, inherited verbatim from `claims-honesty-pass-v0`:

> **Correct the CLAIM to match the CODE. Do NOT build the missing feature. Do
> NOT weaken any real behaviour.** The per-crate / module-local docs that are
> ALREADY honest are the source of truth the overstated surfaces are aligned TO.

## The job (JTBD)

> "When I read what Kaleidoscope claims — a crate doc, a comment, the platform
> README, a CI config — it matches what the code actually does, so I neither
> over-trust (Prism = Datadog) nor under-trust (pulse loses my data on restart)
> the system."

This is the same Earned-Trust job as `claims-honesty-pass-v0`, applied to the
project's own prose. For an honesty-thesis project, a claim that overstates the
code is the sharpest possible self-inflicted wound; a claim that *understates*
the code (the pulse "volatile" inversion) is the same failure mirror-imaged — it
makes the reader distrust a capability that genuinely ships.

## Verified-against-code overstatement inventory

Each row: the false claim → the code truth (cited) → the corrected claim it will
be aligned to → verdict. The "corrected claim" is the truth DISCUSS hands to
DISTILL as the guard target (false string ABSENT, corrected string PRESENT,
verifiable by reading the cited code).

| # | Claim locus | The false claim | Code truth (verified 2026-06-07) | Corrected claim (aligned to) | Verdict |
|---|-------------|-----------------|----------------------------------|------------------------------|---------|
| 1 | `pulse/src/lib.rs:37` and `:46` | "Library only at v0. No daemon, no network." (`:37`) and **"In-memory only at v0; restart loses points."** (`:46`) | `pulse/src/lib.rs:65` re-exports `FileBackedMetricStore`; `file_backed.rs:17,75-82` is a durable WAL+snapshot adapter; `Cargo.toml:21-28,58-63` + the v1 slice tests (`v1_slice_01_wal_durability`, `v1_slice_06_snapshot_atomicity`) confirm it is crash-durable (WAL crash-durable under ADR-0049; fsync via `RealFsyncBackend`). Pulse ships a durable, restart-surviving store. | The crate doc states a durable `FileBackedMetricStore` ships alongside the in-memory adapter and **survives process restart** (fsync-durable per the store-fsync-durability work). The "restart loses points" claim is true ONLY of `InMemoryMetricStore` and must be scoped to it. | **INVERTED-VOLATILITY** (doc understates durability) |
| 2 | `pulse/Cargo.toml:7` description + `pulse/src/lib.rs:20-21,41` | The v1 adapter is **"columnar (Arrow + Parquet + DataFusion + Prometheus TSDB block)"**; and (Cargo.toml) "v0 ships … an InMemoryMetricStore adapter" with the columnar adapter "at v1" | The shipped v1 adapter (`file_backed.rs:47-69`) is **line-delimited JSON over a WAL** (`WalRecord`/`Snapshot` are `serde`/`serde_json` shapes); there is NO Arrow / Parquet / DataFusion / TSDB-block code in the crate (deps are `serde`, `serde_json`, `wal-recovery`, `aegis` — `Cargo.toml:15-19`). The durability half is real; the columnar half is not. | Describe the actual shipped durable adapter: a JSON-over-WAL file-backed store. The columnar substrate (Arrow/Parquet/DataFusion/TSDB) may be named only as a genuine **future/aspiration (future-tense)**, never as shipped. Reconcile the Cargo.toml "v0 ships … InMemoryMetricStore" to also name the durable file-backed adapter. | **OVERSTATED** (promises an undelivered columnar substrate) |
| 3a | `kaleidoscope-gateway/src/main.rs:62-63` | "the body is a **RED-ready NO-OP that Crafty fills in DELIVER**" | `init_tracing()` (`main.rs:153-173`) installs a real JSON-to-stderr `tracing_subscriber` registry guarded by a `OnceLock` + `try_init`. The body is delivered and live, not a no-op. | The comment states `init_tracing` installs the real JSON-to-stderr subscriber as the first statement of `main`. | **STALE-OVER-GREEN** |
| 3b | `kaleidoscope-gateway/src/main.rs:118-120` (and module doc `:24-25`) | "**Force `sink.kind = stub`**" / "The config forces `sink.kind = stub` internally" | The very next line is `Config::builder().build()?` (`main.rs:121`). The code does NOT force the kind; it **relies on the `Stub` builder default**. | The comment states the gateway relies on the `Config::builder()` `Stub` default (aperture forwards Stub-kind sinks unchanged), rather than forcing the kind. | **STALE/INACCURATE** (overstates what the code does) |
| 3c | `kaleidoscope-gateway/tests/slice_01_tracing_subscriber.rs:42-51` (RED-not-BROKEN module note) and `:206-208`, `:280` (per-test "RED against the no-op subscriber" notes) | "At DISTILL close `init_tracing()` … is a **wired NO-OP** … installs no subscriber … this scenario … is RED … **RED against the no-op subscriber**" | `main.rs:153-173` now installs the subscriber; the fail-closed AC-02 tests (`:191`, `:245`) assert the `health.startup.refused` JSON line IS present on stderr — i.e. they are GREEN, not RED. | The test-module docs describe the GREEN reality: the subscriber is installed, the refusal event renders, the always-run scenarios pass. (The `#[ignore]`d fixed-port AC-01 scenarios stay `#[ignore]`d — that ignore is about port-flake determinism, not about the subscriber being absent; correct only the "no-op / RED" wording, not the ignore.) | **STALE-OVER-GREEN** (test-module prose lies about a now-GREEN suite) |
| 4a | `README.md:184` Components table | **Prism**: "Unified query and visualisation frontend" / Replaces "Datadog dashboards, NR One, **Grafana**" | `apps/prism/README.md:3-6` (the honest source of truth): "v0 ships a **single PromQL query panel**" — a single-metric line-chart explorer for the "see-the-shape-of-the-signal" job. There is no dashboarding, no multi-panel layout, no "unified visualisation". | The README Prism row describes a **single-metric PromQL query/chart explorer** (aligned to `apps/prism/README.md`), with unified dashboards marked as future. The "Replaces" cell qualified so it does not imply present-tense Grafana/Datadog-dashboard parity. | **OVERSTATED** (platform README overstates Prism; module-local README is honest) — **not touched by pass-v0** |
| 4b | `README.md:222` cost table | "The **compliance dashboards in Prism** are open templates." | Prism ships a single PromQL chart panel (`apps/prism/README.md:3-6`); there are **no compliance dashboards** in Prism at all. | Either drop the line or restate it so it does not assert a Prism capability (compliance dashboards) that does not exist. The honest cost-model point (no per-seat / no "contact sales" gate) can stand without inventing a feature. | **OVERSTATED** (asserts a non-existent Prism feature) |
| 5 | `apps/prism/playwright.config.ts:19,28-34,50,63-67` | The config header advertises **"Gate 7 (Prism E2E across the browser matrix)"** with Chrome/Firefox/Safari projects + a pinned Prometheus digest, implying a passing browser-matrix e2e gate | `testMatch: ['__no-spec-matches-yet__.spec.ts']` (`:50`) matches **no spec**; every `e2e/*.spec.ts` body throws `UNIMPLEMENTED` (per the slice-by-slice re-add comment `:43-49`). The "gate" runs zero real assertions. The infrastructure IMPLIES a gate that does not exist. | Stop advertising a gate that does not run. Mark the e2e/browser-matrix gate clearly as **not-yet-implemented / scaffold** (config header + the README `pnpm playwright` script note), so no reader believes a browser-matrix e2e gate is green. (Do NOT build playwright e2e — that is a real feature out of scope.) | **OVERSTATED / VACUOUS** (advertises a non-existent gate) |

## Items from the brief that turned out STALE / ALREADY-FIXED (excluded — do NOT "correct")

- **(Brief item 6) `kaleidoscope-cli/src/lib.rs:43-45` "Both adapters survive
  process restarts"** — **STALE four-quadrants MED; the doc is now ACCURATE.**
  Verified: `lumen::FileBackedLogStore::open` and
  `cinder::FileBackedTieringStore::open` both default to `RealFsyncBackend`
  (`lumen/src/file_backed.rs:97,319`; `cinder/src/file_backed.rs:136,433`):
  `sync_all`/`fsync_file` on every WAL append and tmp+fsync+rename+fsync-dir
  atomic snapshots (ADR-0049/0060). After `store-fsync-durability-v0` the stores
  survive **crashes**, not merely graceful restarts — so "survive process
  restarts" is true and arguably understated. The four-quadrants MED predates the
  fsync work. **Adding a "may lose data on crash" caveat would be a FALSE caveat;
  no correction is made.** (If anything, a future pass could strengthen the claim
  to "survive crashes", but DISCUSS does not invent scope: the existing claim is
  not misleading, so it is left.)
- **query-api `step` "Prometheus-shaped / accepted but not honoured"**
  (`README.md:105-109,197-199`) — **ALREADY HONEST.** This was DESIGN flag #1 of
  `claims-honesty-pass-v0`; the README now plainly states "`step` is accepted but
  not honoured (no grid re-sampling) … raw points … (ADR-0062)". No residual
  overstatement. **Not in scope.**
- **README Spark / Strata / Cinder / Loom Components rows + Strata cost line**
  — already corrected by `claims-honesty-pass-v0` (verified at `README.md:174,
  182, 183, 188, 217`). **Not re-touched.**
- **README "not a re-skinned Grafana" prose (`:44`)** — a *provenance* claim
  (first-party code, not a wrapper), NOT a *capability* claim. It is true and is
  left untouched. The Prism overstatement is the **capability** wording in the
  table row (`:184`) and the cost line (`:222`), not the provenance prose.
- **pulse README durability `Status` claims** — pulse has no separate platform
  README durability row beyond the crate doc (item 1) and the Components row
  (`README.md:179` "Time-series metrics engine", which is accurate). No extra
  surface to correct.
- **Genuinely in-flight `#[ignore]`d / RED scaffolds** — the crash-durability
  proving tests, the gateway/log-query tracing-subscriber `#[ignore]`d fixed-port
  AC-01 scenarios (the ignore is about port-flake, the subscriber IS installed),
  the `pulse-crash-target` SCAFFOLD bin (`pulse/Cargo.toml:21-28`, genuinely a
  proving harness), and the prism slice-by-slice e2e re-add plan — these describe
  a TRUE current state. **Item 5 marks the playwright config as not-yet-running;
  it does NOT touch the genuinely in-flight per-spec scaffolds, only the false
  "Gate 7 … across the browser matrix" advertisement that implies the gate is
  live.**

## Slicing decision (Elephant Carpaccio — by locus)

Carpaccio taste tests applied:

- **Independently shippable?** Each locus (pulse / gateway / prism) is a separate
  set of files with no cross-dependency. Yes.
- **Verifiable in one session?** Each is a grep-guard (false string absent,
  corrected string present) cross-read against cited code. Yes.
- **Delivers a coherent reader-outcome?** Each locus is one reader experience
  ("I read the pulse docs and they're honest", "I read the gateway comments and
  they're honest", "I read what the platform claims about Prism and it's honest").
  Yes.

Decision: **carpaccio by locus**, three slices, ordered by sharpness of the lie:

1. **US-01 — pulse** (the inverted-volatility + columnar overstatement). Notable
   because it tells users their data is volatile when it is durable, AND promises
   an undelivered columnar substrate. Two claims, one crate, one reader.
2. **US-02 — gateway** (three stale-over-green / inaccurate comments + the test
   module prose). One binary, one reader (the contributor reading `main.rs`).
3. **US-03 — prism** (platform-README overstatement + cost line + the vacuous
   browser-matrix e2e advertisement). The README row is the loudest brand
   surface; the playwright config is the most misleading CI advertisement.

Three slices is within right-size; no further split needed (see story-map
Scope Assessment).

## Decisions (autonomous, per the overnight brief)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Feature type | Cross-cutting (documentation honesty) | Prose/comments/config across pulse, gateway, prism + platform README. |
| Walking skeleton | **No** | Brownfield docs; no end-to-end flow to thread. The three slices are independent. |
| UX research | Lightweight | The "user" is a reader: an operator / integrator / contributor reading the claims. One persona (Devin, carried from pass-v0). |
| JTBD | The Earned-Trust honesty job above | Same job as pass-v0; recorded as the single job all stories trace to (N:1). |
| DIVERGE | Absent (confirmed) | No `docs/feature/claims-honesty-pass-2-v0/diverge/`. Job grounded directly in the four-quadrants reports + the structural-honesty thesis + verified code, mirroring pass-v0. Recorded as a risk below. |
| Slicing | Elephant carpaccio by locus (pulse / gateway / prism), sharpest lie first | Each slice leaves the docs strictly more honest and is independently shippable. |
| Correct-the-claim only | **Enforced** | No feature-building, no behaviour change. The columnar substrate, the e2e browser matrix, and any "force the kind" are NOT built; the claims are corrected to match the code. |
| prism-e2e: remove vs mark | **Recommend MARK (not remove)** — DESIGN flag | See "Flagged decision" below. The honest minimum is to stop claiming the gate works; whether to delete the config or annotate it as scaffold is a DESIGN call. DISCUSS recommends MARK (keeps the genuine slice-by-slice plan visible) over REMOVE (loses the roadmap). |
| Mutation testing | N/A for pure doc/comment/config changes | Per CLAUDE.md mutation is per-feature on modified files; doc-comment changes in `src` add no mutable production-logic lines, and the README / Cargo.toml / playwright.config.ts changes are metadata/prose. Recorded as a guardrail. |
| Gate 2/3 (public-API / semver) | Confirmed **not triggered** | A doc-comment change is not a public-API change; a `Cargo.toml` `description` change is package metadata, not API. pulse/prism/gateway public surfaces are unchanged. NEVER 1.0.0. |
| Acceptance shape | Guard-style: false string ABSENT + corrected string PRESENT, cross-read against cited code | A prose-honesty correction is verified by a doc-lint / grep guard that the false claim is gone and the true claim is present and matches the code. Recorded for DISTILL. |

## Flagged decision (for DESIGN) — prism-e2e: remove vs mark

`apps/prism/playwright.config.ts` advertises "Gate 7 (Prism E2E across the
browser matrix)" with three browser projects and a pinned Prometheus digest, but
`testMatch` matches no spec and every spec throws `UNIMPLEMENTED`. The honesty
fix is to stop claiming a gate that does not run. Two honest options:

- **Option A (MARK, recommended)** — Annotate the config header and the README
  `pnpm playwright` script note to state plainly that the browser-matrix e2e gate
  is **not yet implemented / a scaffold** (no spec runs today; specs land slice
  by slice). Keeps the genuine slice-by-slice re-add plan and the digest-SSOT
  rule visible for the future feature that actually builds the e2e.
- **Option B (REMOVE)** — Delete the false advertisement (the "Gate 7 …"
  header, the browser projects, the digest) until the e2e feature is actually
  built. Cleaner, but discards a real roadmap artefact and the digest-SSOT
  invariant that a future e2e feature would re-derive.

DISCUSS recommendation (non-binding): **Option A (MARK)**. It satisfies the
honesty job (no reader believes the gate is green) at the lowest cost and without
destroying the legitimate scaffold/roadmap. "Build the playwright e2e" is a real
feature, explicitly out of scope for an honesty pass.

## Risk register

| Risk | Prob | Impact | Mitigation |
|------|------|--------|------------|
| No DIVERGE artifacts present (`docs/feature/claims-honesty-pass-2-v0/diverge/` absent) | High (confirmed absent) | Low | Job grounded directly in the four-quadrants reports (prism/pulse/gateway/cli) + the structural-honesty thesis + verified code. JTBD recorded; no ODI re-run for a prose-correctness sequel. Mirrors the sibling `store-fsync-durability-v0` and pass-v0 posture. |
| Adding a FALSE caveat where the code is now durable (the stale cli MED) | Medium | Medium | Explicitly verified the cli stores are fsync-durable; the "survive restart" claim is accurate. Excluded from scope; recorded in the stale/already-fixed list. No false caveat is added. |
| Over-correcting pulse to "no columnar ever" when columnar IS a genuine future direction | Medium | Medium | The corrected claim names the columnar substrate as a **future/aspiration in future tense**, not as nonexistent — it removes only the *shipped/present-tense* promise. Aligned to the real roadmap, not invented. |
| Touching a GENUINELY in-flight `#[ignore]`d / RED marker by mistake (would make an honest in-flight marker lie) | Medium | High | The inventory separates stale-over-green (items 3a–3c, 4, 5-advertisement) from genuinely in-flight markers (crash-durability proving tests, the `#[ignore]`d fixed-port AC-01 scenarios whose ignore is about port-flake not the subscriber, the per-spec e2e `UNIMPLEMENTED` scaffolds). Each correction names the exact file+line and asserts the covered code is GREEN before the marker is touched. Item 5 corrects only the false "gate works" advertisement, not the per-spec scaffolds. |
| The prism-e2e remove-vs-mark choice pre-empted in DISCUSS | Low | Low | DISCUSS only flags and recommends (Option A); the decision is DESIGN's. |
| A "corrected" claim is itself subtly wrong | Medium | Medium | Every correction uses the module-local already-honest doc as canonical truth (pulse→`file_backed.rs` JSON+WAL durable adapter; gateway→the actual `init_tracing`/`Config::builder().build()` code; prism→`apps/prism/README.md` "single PromQL query panel"). The README/comment/config is aligned TO the honest source, not invented. |
| A pure-doc slice carries no mutation target and a naive gate flags it | Low | Low | Mutation N/A for doc/comment/config (no mutable production-logic surface). Trunk-based, CI-is-feedback (project memory). Recorded as a guardrail. |

## What this feature does NOT do

- Does **not** build the columnar (Arrow/Parquet/DataFusion/TSDB) pulse adapter.
- Does **not** build the Prism dashboarding / unified-visualisation capability.
- Does **not** build the playwright browser-matrix e2e gate.
- Does **not** change any store / handler / gateway / validator BEHAVIOUR.
- Does **not** weaken any real behaviour (no real durability, no real subscriber
  install, no real probe is removed or softened).
- Does **not** add a FALSE caveat to the cli durability doc (it is now accurate).
- Does **not** re-correct claims pass-v0 already fixed (Spark/Strata/Cinder/Loom
  rows, query-api `step`) or the now-true README durability section.
- Does **not** touch genuinely in-flight `#[ignore]`d / RED scaffolds.
- Does **not** re-run ODI / opportunity scoring (prose-correctness sequel with a
  job pre-validated by the four-quadrants reports).
- Does **not** make the prism-e2e remove-vs-mark decision (that is DESIGN's).
- Does **not** proceed into DESIGN (per the brief).

## Peer review

Peer review (nw-product-owner-reviewer) run at end of DISCUSS; result recorded in
`dor-validation.md`. Handoff to DESIGN is NOT performed by this wave (brief: "Do
NOT proceed into DESIGN").
