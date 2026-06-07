<!-- markdownlint-disable MD013 MD024 -->

# Wave Decisions — claims-honesty-pass-2-v0 (DESIGN)

- **Wave**: DESIGN (nWave, LIGHT)
- **Architect**: Morgan (nw-solution-architect)
- **Date**: 2026-06-07
- **Mode**: PROPOSE, autonomous overnight. No questions returned to the operator.
- **Sequel to**: `claims-honesty-pass-v0` (DESIGN was LIGHT: ONE ADR only for a
  real scope statement, DOCUMENT for everything else, grep-guard acceptance,
  no C4). This DESIGN mirrors that precedent exactly.
- **Scope**: A prose/comment/config-honesty feature. DESIGN owns ONLY one
  document-vs-implement decision (the prism-e2e MARK-vs-REMOVE flag, US-03) plus
  the final corrected wording for each locus and the falsifiability seam. The
  three slices need no architecture — they are doc/string changes grounded in
  already-honest code. **Deliberately not over-built.**

## Proportionality stance (PROPOSE-mode posture)

This is a correct-the-claim-only documentation pass. It builds NOTHING, changes
NO behaviour, adds NO component, and touches NO production-logic line. The
architect's job here is narrow and explicit:

1. Confirm each false claim against the LIVE code (done — "the code is truth").
2. Pin the corrected wording that is VERIFIABLY true of the cited code (and is
   not itself a new overstatement in the opposite direction).
3. Resolve the one DESIGN flag (prism-e2e MARK vs REMOVE).
4. Specify the falsifiability seam (structural string test) for DISTILL.
5. Confirm no ADR, no semver, no C4, no contract test.

## Verification posture — the code was read directly (2026-06-07)

Every locus below was read at HEAD, not trusted from the DISCUSS inventory:

- **pulse** — `lib.rs:65` re-exports `FileBackedMetricStore`; `file_backed.rs:47-82`
  is a durable adapter whose `WalRecord`/`Snapshot` are `serde`/`serde_json`
  shapes (line-delimited JSON over a WAL) with fsync via `RealFsyncBackend`
  (`fsync_probe`). `Cargo.toml:15-19` deps are `aegis`, `serde`, `serde_json`,
  `wal-recovery` — **no** Arrow/Parquet/DataFusion/TSDB crate anywhere. So:
  durability is REAL (the doc understates it); columnar is UNBUILT (the doc +
  description overstate it). Both confirmed.
- **gateway** — `init_tracing()` (`main.rs:153-173`) installs a real
  JSON-to-stderr `tracing_subscriber::registry()` behind `OnceLock` + `try_init`;
  it is called as `main`'s first statement (`main.rs:64`). The next line after the
  "Force `sink.kind = stub`" comment is `Config::builder().build()?`
  (`main.rs:121`) — it relies on the builder's Stub default, it does NOT force.
  The always-run fail-closed tests (`slice_01_tracing_subscriber.rs:209-213,
  261-265`) assert the `health.startup.refused` JSON line IS present — i.e. they
  are GREEN, contradicting the module note's "RED against the no-op subscriber"
  (`:42-51, 206-208, 280`). Confirmed.
- **prism** — `apps/prism/README.md:3-6` (the honest source) says Prism "v0 ships
  a single PromQL query panel". The platform `README.md:184` row says "Unified
  query and visualisation frontend" / Replaces "Grafana"; the cost line
  `README.md:222` says "compliance dashboards in Prism". `playwright.config.ts:19`
  advertises "Gate 7 (Prism E2E across the browser matrix)" but `testMatch`
  (`:50`) is `['__no-spec-matches-yet__.spec.ts']` — zero specs run. Confirmed.

The DISCUSS inventory holds exactly against the live code.

## Final overstatement → truth → proving-code table (the DISTILL guard target)

Each row: the false string to assert ABSENT, the corrected wording to assert
PRESENT, and the live code that makes the corrected wording true. The corrected
wording is the truth; it is aligned TO the module-local honest source, never
invented. Direction of each lie is named so the correction does not over-swing.

| # | Locus (file:line) | False string (assert ABSENT) | Corrected wording (assert PRESENT) | Proving code (verified true of) | Lie direction |
|---|-------------------|------------------------------|------------------------------------|---------------------------------|---------------|
| 1 | `crates/pulse/src/lib.rs:46` | "In-memory only at v0; restart loses points." (unscoped, crate-wide) | "v0 ships the in-memory `InMemoryMetricStore` **and** a durable `FileBackedMetricStore` (JSON-over-WAL + atomic snapshot) that **survives process restart** (fsync-durable). `InMemoryMetricStore` is volatile — it loses points on restart; `FileBackedMetricStore` does not." | `lib.rs:65` (`pub use FileBackedMetricStore`); `file_backed.rs:75-82` (durable adapter, fsync backend); v1 slice tests `v1_slice_01_wal_durability`, `v1_slice_06_snapshot_atomicity` | UNDER (doc hides real durability) |
| 1b | `crates/pulse/src/lib.rs:37` | "Library only at v0. No daemon, no network." | Leave the "library only / no daemon / no network" fact (it is TRUE — pulse is a library crate). DESIGN note: this line is ACCURATE and is NOT corrected; it is listed only because the brief paired it with `:46`. The volatility correction is `:46` only. | `Cargo.toml [lib]` (library crate, no `[[bin]]` daemon, no network dep) | NONE — accurate, left as-is |
| 2 | `crates/pulse/src/lib.rs:20-22, 41` | "The v1 columnar + durable adapter (Arrow + Parquet + DataFusion + Prometheus TSDB block) lives behind the same trait." (present-tense, shipped) | "v0 ships the [`MetricStore`] trait, the in-memory adapter, **and** the durable JSON-over-WAL [`FileBackedMetricStore`]. A columnar substrate (Arrow / Parquet / DataFusion / Prometheus TSDB block) **is a future direction**, not yet built, behind the same trait." (future tense; durable adapter named as shipped) | `file_backed.rs:47-69` (JSON `WalRecord`/`Snapshot`); `Cargo.toml:15-19` (no columnar dep) | OVER (promises unbuilt columnar) |
| 3 | `crates/pulse/Cargo.toml:7` (`description`) | "v0 ships the MetricStore trait + an InMemoryMetricStore adapter; the columnar (Arrow + Parquet + DataFusion + Prometheus TSDB block) adapter lands at v1 behind the same trait." | "v0 ships the MetricStore trait, an in-memory adapter, **and a durable JSON-over-WAL FileBackedMetricStore** behind the same trait; a columnar substrate (Arrow / Parquet / DataFusion / Prometheus TSDB block) **is a future direction**. OTLP-shaped, per-tenant, gauge + sum number points, time-range and predicate query." | same as rows 1+2 | OVER + UNDER (names only in-memory; promises columnar as v1) |
| 4 | `crates/kaleidoscope-gateway/src/main.rs:62-63` | "the body is a RED-ready NO-OP that Crafty fills in DELIVER (see `init_tracing` below)." | "`init_tracing` installs the real JSON-to-stderr `tracing_subscriber` (registry + JSON stderr layer, `OnceLock` + `try_init`-guarded) as the first statement of `main`, so `gateway_starting` and `health.startup.refused` render rather than dropping." | `init_tracing` body `main.rs:153-173` | STALE-OVER-GREEN |
| 5 | `crates/kaleidoscope-gateway/src/main.rs:118-120` and module doc `:24-25` | "Force `sink.kind = stub`" / "The config forces `sink.kind = stub` internally" | "The gateway **relies on the `Config::builder()` `Stub` default** so aperture's composition root forwards the injected `StorageSink` unchanged (Stub-kind sinks are forwarded as-is); it does not override the kind." | next line `Config::builder().build()?` `main.rs:121` | OVER (overstates what the code does) |
| 6 | `crates/kaleidoscope-gateway/tests/slice_01_tracing_subscriber.rs:38-51, 206-208, 280` | "wired NO-OP", "installs no subscriber", "is RED", "RED against the no-op subscriber" | The module/per-test notes describe the GREEN reality: "`init_tracing()` installs the real JSON-to-stderr subscriber; the always-run fail-closed AC-02 scenarios assert the `health.startup.refused` JSON line is present and PASS (GREEN)." The `#[ignore]`d fixed-port AC-01 note explains the ignore is **port-flake determinism**, not an absent subscriber. | `init_tracing` `main.rs:153-173`; the AC-02 asserts `slice_01_tracing_subscriber.rs:209-213, 261-265` | STALE-OVER-GREEN |
| 7 | `README.md:184` Prism row | "Unified query and visualisation frontend" / Replaces cell asserting present-tense "Grafana" dashboard parity | Role: "Single-metric PromQL query/chart explorer (unified dashboards: future)". Replaces cell qualified so it does NOT imply present-tense Grafana/Datadog-dashboard parity (e.g. "Grafana (single-panel explore; full dashboarding: future)"). | `apps/prism/README.md:3-6` ("a single PromQL query panel") | OVER |
| 8 | `README.md:222` cost line | "The compliance dashboards in Prism are open templates." | Restate so it asserts NO non-existent Prism feature — keep the true economic point (no per-seat / no "contact sales" gate) without inventing compliance dashboards. E.g. "Compliance reporting needs no 'contact sales' upsell tier; the platform is fully FOSS." (DESIGN: prefer RESTATE over DELETE — the cost-model row pairing with "Contact sales for compliance reports" should keep an answer.) | `apps/prism/README.md:3-6` (no compliance dashboards exist) | OVER (asserts non-existent feature) |
| 9 | `apps/prism/playwright.config.ts:19` header + the prism README `pnpm playwright` note (`apps/prism/README.md:35`) | "Gate 7 (Prism E2E across the browser matrix)" advertised as a live gate (no qualifier) | A header that plainly states the browser-matrix e2e gate is **NOT YET IMPLEMENTED / scaffold-only**: no spec runs today (`testMatch` matches none); specs land slice by slice (the existing re-add roadmap stays); the digest-SSOT rule stays. README `pnpm playwright` note marked "(scaffold; no e2e spec runs yet — see playwright.config.ts)". | `testMatch: ['__no-spec-matches-yet__.spec.ts']` `:50`; e2e specs throw `UNIMPLEMENTED` | OVER / VACUOUS — resolved as **MARK** (see flag below) |

### Anti-over-correction guardrails (so the fix is not a new lie)

- Row 1b: `lib.rs:37` "no daemon, no network" is TRUE and stays. Do NOT delete it.
- Row 2/3: columnar is named as a GENUINE future direction (future tense), not as
  "never" and not as "removed". The roadmap intent is preserved; only the
  present-tense/shipped promise is removed.
- Row 6: only the "no-op / RED" wording changes. The `#[ignore]` attributes on the
  fixed-port AC-01 scenarios are **untouched** — their ignore is real (port-flake),
  and the corrected note states that real reason.
- Row 9: only the false "gate works" advertisement is marked. The per-spec
  `UNIMPLEMENTED` e2e bodies and the slice-by-slice re-add plan are **untouched**
  (genuinely in-flight). The Prometheus digest-SSOT invariant stays visible.

## DESIGN flag resolution — prism-e2e: MARK (not REMOVE)

**Decision: MARK.** Annotate `apps/prism/playwright.config.ts` and the prism
README `pnpm playwright` note as not-yet-implemented / scaffold-only. Do NOT
delete the config, the browser projects, the digest constant, or the re-add
roadmap.

| Criterion | MARK (chosen) | REMOVE (rejected) |
|-----------|---------------|-------------------|
| Satisfies the honesty job | Yes — no reader believes a browser-matrix e2e gate is green | Yes |
| Lowest honest cost | Yes — a header edit + one README line | Larger diff; deletes projects/digest/webServer/globalSetup |
| Preserves the legitimate roadmap | Yes — the slice-by-slice re-add plan (`:43-49`) stays visible | No — discards the roadmap |
| Preserves the digest-SSOT invariant | Yes — the `PROMETHEUS_IMAGE_DIGEST` ↔ CI `gate-11` atomic-bump rule (`:23-34, 80-82`) stays | No — a future e2e feature must re-derive it |
| Risk of re-introducing the lie | Low — the scaffold is clearly labelled | None, but at the cost of losing a real artefact |

**Justification.** The honesty minimum is "stop claiming a gate that does not
run." Both options achieve that. MARK achieves it at the lowest cost while
preserving two genuine engineering artefacts a future "build the prism e2e"
feature will rely on: the slice-by-slice re-add plan and the Prometheus
digest-SSOT invariant (the `playwright.config.ts` digest MUST equal the CI
`gate-11-prism-prometheus-contract` digest, bumped atomically). Deleting them
would force a future feature to re-discover an invariant that is correct today.
This matches the DISCUSS recommendation and the pass-v0 precedent (both flags
there resolved to the lighter DOCUMENT option). Building the playwright e2e is a
separate, out-of-scope feature.

**Exact MARK annotation (the corrected wording to assert PRESENT).** A config
header block stating, in substance:

> NOT YET IMPLEMENTED — scaffold only. The Prism E2E browser-matrix gate (Gate 7)
> does NOT run any spec today: `testMatch` (below) deliberately matches no spec
> and every `e2e/*.spec.ts` body throws `UNIMPLEMENTED`. The browser projects,
> the Prometheus digest-SSOT, and the slice-by-slice re-add plan are kept as the
> roadmap for the future feature that builds the e2e. Do not read this config as
> a passing quality gate.

The literal word "scaffold" or "NOT YET IMPLEMENTED" near the former "Gate 7"
advertisement is the PRESENT guard; the unqualified "Gate 7 … across the browser
matrix" advertised-as-live phrasing is the ABSENT guard. The prism README note
becomes: `pnpm playwright` — Playwright E2E suite (scaffold; no e2e spec runs
yet — see `playwright.config.ts`).

## ADR decision — NO new ADR

**No ADR is created.** Rationale, mirroring the pass-v0 precedent exactly:

- claims-honesty doc corrections are NOT architecturally significant. They change
  no component boundary, no technology choice, no integration pattern, no quality
  attribute. They align prose to code that already exists.
- pass-v0 set the precedent: it created exactly ONE ADR (ADR-0062), and ONLY
  because that flag (`query_range` step) was a real cross-cutting SCOPE statement a
  future implementer + Prism would rely on. None of pass-2's loci is such a
  statement — the pulse columnar-is-future framing is already captured by the
  existing roadmap, the gateway corrections are comment-to-code alignment, and the
  prism MARK decision is a local config annotation, not a cross-cutting boundary.
- The prism-e2e MARK decision and all corrected wordings are captured fully in
  THIS document, exactly as pass-v0 captured its FLAG #2 (no ADR) here.

If a reviewer disagrees: the only candidate would be "pulse durable
JSON-over-WAL adapter is shipped; columnar is future" — but that is already the
de-facto recorded state (ADR-0049 WAL crash-durability, ADR-0051 cardinality,
ADR-0060 snapshot atomicity already govern the durable adapter). No NEW
architectural decision is being made; a doc is being corrected to match decisions
already taken. Default and final: **NO ADR.**

## Reuse Analysis (MANDATORY)

This feature **creates nothing**. It is a pure in-place edit of existing
docs/comments/config, aligning the overstated/inverted surfaces TO the
already-honest module-local source of truth.

| Surface to correct | Aligned TO (already-honest canonical source) | Created? |
|--------------------|----------------------------------------------|----------|
| `pulse/src/lib.rs:20-22,41,46` (doc) | `pulse/src/file_backed.rs` (the real durable JSON+WAL adapter) | No — edit in place |
| `pulse/Cargo.toml:7` (description) | same `file_backed.rs` + the dep list | No — edit metadata in place |
| `kaleidoscope-gateway/src/main.rs:24-25,62-63,118-120` (comments) | the actual `init_tracing` (`:153-173`) + `Config::builder().build()` (`:121`) | No — edit comments in place |
| `kaleidoscope-gateway/tests/slice_01_tracing_subscriber.rs` (prose) | the GREEN AC-02 asserts in the same file | No — edit prose in place |
| `README.md:184,222` (Prism row + cost line) | `apps/prism/README.md:3-6` ("single PromQL query panel") | No — edit in place |
| `apps/prism/playwright.config.ts:19` + `apps/prism/README.md:35` | the `testMatch` reality (`:50`) | No — annotate in place (MARK) |
| The falsifiability guard (structural string test) | the `integration-suite` / `std::fs` structural-test precedent (perf-kpi / fast-precommit) | **Reuse the existing structural-test harness pattern**; add one test module — no new framework |

**Reuse verdict: CORRECT-IN-PLACE.** Every corrected surface is aligned to an
existing honest source. Zero new components, zero new dependencies, zero new
behaviour. The single net-new artefact is one structural guard test, and even
that reuses the established `std::fs`-reads-source-and-asserts-substrings
precedent rather than inventing a mechanism.

## Test seam for DISTILL — making the doc claims FALSIFIABLE

The honesty corrections become regression-protected by a **structural string
test** that reads each corrected file from disk (`std::fs::read_to_string`) and
asserts, per locus: the FALSE phrase is ABSENT and the corrected TRUE phrase is
PRESENT. This mirrors the existing perf-kpi / fast-precommit structural-test
precedent (`integration-suite`, `std::fs`-reads-source).

**The RED-today.** Before the edits, every false phrase is still present (verified
2026-06-07). So the structural test, written first, FAILS today — that is the RED
that proves the guard is real, exactly the Earned-Trust shape (a guard that
cannot fail proves nothing).

Suggested guard assertions (the DISTILL acceptance-designer pins the exact
strings; these are the shape):

| File read | ABSENT assertion (false) | PRESENT assertion (true) |
|-----------|--------------------------|--------------------------|
| `crates/pulse/src/lib.rs` | "In-memory only at v0; restart loses points." (unscoped); "(Arrow + Parquet + DataFusion + Prometheus TSDB block) lives behind the same trait" (present tense) | "FileBackedMetricStore" durability claim; columnar named as "future" |
| `crates/pulse/Cargo.toml` | "an InMemoryMetricStore adapter; the columnar … adapter lands at v1" | "FileBackedMetricStore" named as shipped; columnar as "future direction" |
| `crates/kaleidoscope-gateway/src/main.rs` | "RED-ready NO-OP that Crafty fills in DELIVER"; "Force `sink.kind = stub`" / "forces `sink.kind = stub`" | "installs the real JSON-to-stderr"; "relies on the `Config::builder()` … Stub default" |
| `crates/kaleidoscope-gateway/tests/slice_01_tracing_subscriber.rs` | "wired NO-OP"; "RED against the no-op subscriber" | "installs the real JSON-to-stderr subscriber"; "GREEN" / "PASS" |
| `README.md` | "Unified query and visualisation frontend"; "compliance dashboards in Prism" | "single-metric PromQL" (or equivalent); the restated cost line |
| `apps/prism/playwright.config.ts` | unqualified "Gate 7 (Prism E2E across the browser matrix)" advertised-as-live | "NOT YET IMPLEMENTED" / "scaffold" |

**Behaviour guardrails (runtime, not string):** the pulse durability + snapshot
tests still pass; the gateway always-run tracing AC-02 scenarios still pass; no
`#[ignore]` attribute changed; the prism per-spec `UNIMPLEMENTED` bodies unchanged.

**Routing note for DISTILL/DELIVER (mixed ownership).** The structural guard test
and the `.rs` doc-comment edits are crafter/`integration-suite` territory (Rust
source). The `README.md`, `pulse/Cargo.toml` description, `apps/prism/README.md`,
and `playwright.config.ts` are NON-crafter docs/metadata/config. This is a
mixed-ownership feature: the crafter owns the `.rs` edits + the guard test; the
non-`.rs` edits are documentation edits. The single structural test that reads
ALL of these files (including the non-`.rs` ones via `std::fs`) is the unifying
regression net and belongs in the integration-suite.

## Public-API / semver confirmation

**No semver bump. Gate 2/3 not triggered.** Verified:

- `cargo public-api` (Gate 2) and `cargo semver-checks` (Gate 3) are scoped in
  `.github/workflows/ci.yml:385-406, 449-465` to **`otlp-conformance-harness`,
  `spark`, `sieve`, `codex` ONLY**. pulse, gateway, and prism are NOT in the
  public-surface lock. So even an actual API change to pulse would not trip those
  gates.
- Independently: a `Cargo.toml` `description` change is package **metadata**, not
  the Rust API surface. `cargo public-api` diffs the public Rust items (types,
  fns, traits, re-exports), not the manifest `description`. So a description edit
  is not an API diff EVEN IF pulse were gated.
- A doc-comment (`//!` / `///`) change is not a public-API change.
- No `pub` item is added, removed, or changed in any of the three crates.
- **NEVER 1.0.0** (project memory: semver 1.0.0 is Andrea's call). All three
  crates stay at `0.1.0`.

## Constraints (carried from DISCUSS, reaffirmed)

- The module-local already-honest doc is the canonical truth; the
  README/comment/config/description are aligned TO it, never invented fresh.
- Correct-the-claim only. No feature built (no columnar adapter, no prism
  dashboarding, no playwright e2e). No behaviour weakened (durable stores, real
  subscriber, startup probe all stay).
- Touch ONLY markers proven to sit over GREEN code. Genuinely in-flight
  `#[ignore]`d / RED scaffolds and the per-spec `UNIMPLEMENTED` e2e bodies are
  untouched.
- Trunk-based, CI-is-feedback (project memory): no CI gate blocks a doc-only
  change; the structural guard test is the regression net.
- Mutation: N/A (no mutable production-logic surface added). Per CLAUDE.md
  per-feature mutation is on modified production files; doc/comment/metadata/config
  changes add none.
- No external integration introduced → no contract-test recommendation.
- No C4 produced — the topology is byte-identical before and after; a doc feature
  changes no container, component, or boundary. (C4 omission is deliberate and
  recorded per the methodology's "L3 only for complex subsystems; doc feature →
  none".)

## Upstream changes (to brief.md)

A single one-line pointer is added to `docs/product/architecture/brief.md`
recording that the residual pulse/gateway/prism overstatements are corrected and
guarded by a structural string test (the falsifiability seam), with a DEVOPS
handoff line on the mixed crafter/non-crafter ownership. No heavy section — the
architecture is unchanged.

## DEVOPS handoff line

Inherits the five gates from the prior waves; no new gate. The deliverable is
doc/comment/metadata/config edits + ONE structural guard test. Routing:
the `.rs` doc-comments + the structural test → crafter/`integration-suite`; the
`README.md` + `pulse/Cargo.toml` description + `apps/prism/playwright.config.ts` +
`apps/prism/README.md` → non-crafter docs/metadata/config. Mixed ownership noted
for DELIVER. No semver bump; Gate 2/3 untouched; mutation N/A.

## What this DESIGN wave does NOT do

- Does NOT create an ADR (precedent: pass-v0 created one only for a real scope
  statement; none here qualifies).
- Does NOT produce C4 (topology unchanged).
- Does NOT build the columnar pulse adapter, prism dashboarding, or the playwright
  e2e.
- Does NOT change behaviour or weaken any real capability.
- Does NOT touch genuinely in-flight `#[ignore]`d / RED scaffolds or per-spec
  `UNIMPLEMENTED` e2e bodies.
- Does NOT proceed into DEVOPS (per the brief).
- Does NOT commit, and does NOT touch `docs/evolution/`.

## Peer review

Self-review against `nw-sa-critique-dimensions` recorded below
(`nw-solution-architect-reviewer` not separately nested-invocable from this
sub-agent context — critique-dimensions applied directly, mirroring the pass-v0
DESIGN posture). Handoff to DISTILL/DEVOPS is NOT performed by this wave.

### Self-review (nw-sa-critique-dimensions)

| Dimension | Assessment |
|---|---|
| **Architectural bias** (resume-driven / latest-tech) | None. The feature refuses to inflate a doc sweep into a behavioural feature; the prism-e2e flag is resolved to the LIGHTER option (MARK, not build). Anti-bias by construction. No new tech introduced. |
| **ADR quality** | NO ADR is the decision; justified twice (not architecturally significant + the pass-v0 precedent). The decision is recorded with its alternative (a pulse-columnar ADR) explicitly considered and rejected. |
| **Completeness** (quality attributes) | The relevant quality attribute is **maintainability/honesty of documentation** (ISO 25010 maintainability: analysability). Addressed directly: every corrected claim is cited to the proving code and guarded by a falsifiable structural test. No performance/security/reliability attribute is in play (zero behaviour change). Not a gap. |
| **Implementation feasibility / testability** | Every locus has a concrete observable (false string ABSENT + true string PRESENT, cross-read against cited code). The structural test reuses an existing `std::fs` precedent. RED-today confirmed (false phrases still present). Feasible; no team-capability or budget concern. |
| **Priority validation** | Q1 (largest problem): YES — the verified inventory IS the data; 9 overstatement loci, each cited to file:line and the code it contradicts. Q2 (simpler alternatives): ADEQUATE — MARK chosen over REMOVE and over build; DOCUMENT over implement. Q3 (constraint prioritisation): CORRECT — the fix is the <effort solution to the honesty problem, not a >50% solution to a <30% problem (no over-build). Q4 (data-justified): JUSTIFIED — every claim re-grounded against HEAD on 2026-06-07. |

**Self-review verdict: APPROVED — no critical/high/medium issues.** The single
watch-item is the over-correction risk (touching an in-flight marker, or
over-swinging a correction into a new lie) — pinned by the explicit
anti-over-correction guardrails (rows 1b, 2/3, 6, 9) and the both-directions
structural guard (false ABSENT *and* true PRESENT, cross-read against code). Each
correction is verifiably true of the cited code; the prism-e2e MARK decision is
sound and roadmap-preserving; the Reuse is correct-in-place; proportionality is
held (no ADR, no C4, no feature-building, no semver bump).
