# Codex v0 — DESIGN wave decisions

- **Date**: 2026-05-07
- **Architect**: `@nw-solution-architect` (Morgan)
- **Wave**: DESIGN
- **Inputs**: `docs/feature/codex/discuss/` (nine locked decisions; six
  user stories; six slice briefs; outcome KPIs; journey YAML; shared-
  artefacts registry); Spark public surface at `crates/spark/src/`.
- **Outputs**: this file; four ADRs (ADR-0022 through ADR-0025); three
  C4 diagrams (context, container, component); technology choices;
  slice → ADR → module mapping.

---

## Mode

**Propose**. The DISCUSS wave locked nine scope decisions and Sentinel
left only DESIGN-scope flags (D1-D6 below). Morgan resolves each in an
ADR with two-or-more considered alternatives, and writes the design
package directly. No further user dialogue is required to lock the
public API; if any DISCUSS contract needs revision, a back-propagation
note routes back to Bea.

---

## Multi-architect context

This is a single-architect feature (Morgan). No prior application-
architecture sections exist for Codex v0. Spark and Aperture have prior
ADRs that this DESIGN wave amends, additively, post-DELIVER:

- ADR-0011 (Spark public API and crate layout) — pattern reference for
  the test-seam shape; Codex follows the same shape.
- ADR-0012 (Spark error type design) — gains a post-DELIVER amendment
  for the new `SparkError::SchemaValidation(LintReport)` variant; that
  amendment lands when Slice 06 completes, mirroring how Aperture's
  `--config` wiring landed at slice-08 completion.
- ADR-0013 (Spark dependency pinning) — gains a row in its licence-
  audit table for the new runtime `codex` dep at the same post-DELIVER
  moment.

The amendments are not part of the Codex DESIGN wave's deliverables.
They are flagged in `slice-mapping.md` and re-flagged in the Slice 06
completion summary so the orchestrator and the crafter route them
correctly.

---

## DESIGN decisions, summarised

The DISCUSS wave locked the contract; DESIGN locks the shape.

| ID | DISCUSS-flagged decision | DESIGN resolution | ADR |
|----|--------------------------|-------------------|-----|
| D1 | `BlessedAttribute` shape: enum or struct? | **Enum** with two variants `ExactMatch(&'static str)` and `Prefix(&'static str)`. The catalogue iteration loop expresses cleanly as a `match`, the static-slice form is zero-cost, and a third match-kind (e.g. regex, glob) lands cleanly as a third variant. | ADR-0022 |
| D2 | `validate` argument type? | **`&[(&str, &str)]`**. Smallest shape Spark can call without per-call allocation. Spark's already-typed Resource is a `Vec<(Cow<'static, str>, Value)>`-ish; mapping it to `&[(&str, &str)]` is one borrow loop, no heap traffic. `IntoIterator<Item = (&str, &str)>` deferred — no v0 caller benefits, and the slice form is the simplest implementation surface. | ADR-0022 |
| D3 | Multi-violation collect vs short-circuit? | **Collect all** per Q5. Implementation: a `Vec<LintViolation>` accumulator inside `validate`, returned wrapped in `LintReport` when non-empty, dropped (returned `Ok(())`) when empty. Iterator-chain alternative considered and rejected — accumulator is more legible at the corpus size in play. | ADR-0022 |
| D4 | Generated corpus regenerator shape? | **`xtask` Rust binary** in `crates/codex-xtask/` (or `xtask/src/bin/codex_regen.rs` if a workspace-wide xtask crate is preferred at the moment of crafting; the crafter chooses). Reads the upstream `opentelemetry-semantic-conventions =0.27` crate (already a transitive runtime dep of Spark via the OTel SDK), walks the resource-class attribute set, emits `crates/codex/src/generated/semconv_0_27.rs`. Shell+`jq` rejected — the upstream representation is Rust constants, not JSON; reading them in Rust is the natural shape. `cargo-script` rejected — adds a one-off tool to the contributor onboarding ritual for no benefit. | ADR-0023 |
| D5 | Levenshtein implementation? | **Pure function** `pub(crate) fn levenshtein(a: &str, b: &str) -> usize` with the two-row dynamic-programming matrix. Threshold 2 (locked at DISCUSS Q5). Stack-allocated row buffers via `SmallVec`-like inline storage rejected — `Vec<usize>` of length `b.chars().count() + 1` is fine for the corpus size; the simpler implementation is readable at review time. | ADR-0024 |
| D6 | Spark integration shape? | **`SparkConfig::with_strict_schema_lint(bool)`** builder method (default `false` = warn). In `spark::init`, after Resource composition, build `&[(&str, &str)]` from the Resource's attribute pairs and call `SchemaCatalogue::validate(...)`. On `Err(report)`: warn mode emits one `tracing::warn!(target = "spark", ?report, "schema validation failed")` event with the `Display` rendering attached as the message body; strict mode returns `Err(SparkError::SchemaValidation(report))`. The catalogue is built lazily once per process via `OnceLock<SchemaCatalogue>`, so multiple `spark::init` calls in the same process do not rebuild the static corpus. | ADR-0025 |

---

## Architectural style

**Modular library** with the same shape as `crates/sieve` and `crates/spark`:

- `pub` surface in `crates/codex/src/lib.rs` re-exports five types and
  nothing else: `SchemaCatalogue`, `BlessedAttribute`, `LintReport`,
  `LintViolation`, `ViolationKind`.
- Internal modules: `catalogue` (the `SchemaCatalogue` struct + lookup
  loop), `report` (the report and violation types + their `Display` /
  `Error` impls), `fuzzy` (the in-tree Levenshtein), `generated` (the
  hand-regenerated semconv 0.27 slice + the three house attributes).
- No async, no I/O, no allocations in the hot path beyond what the
  `LintReport` requires when violations are present. The clean path
  (every attribute blessed) is allocation-free after catalogue
  construction.

**Why this shape, not microservices / not gRPC service / not
build.rs-driven**:

- Single in-process consumer at v0 (Spark). Network surface is
  premature optimisation.
- Static corpus, single semconv version. Build-time regeneration via
  `build.rs` rejected at DISCUSS Q7 — checked-in artefact gives PR-
  diff visibility on corpus changes.
- The walking-skeleton-then-extend slice plan respects Conway's law
  (one crate, one team — Bea + Morgan + the crafter), so coordinated
  changes do not cross team boundaries.

**Enforcement**: Rust's module-visibility system enforces the public
surface (`pub` vs `pub(crate)`); CI runs `cargo doc --no-deps` to
catch accidental re-exports of upstream types. No ArchUnit-style
external tooling needed at this scale; if the surface grows in v1+
(multiple modules under `pub` API), `cargo-public-api` is the right
language-appropriate enforcement tool to add then.

---

## Earned-Trust posture

Codex v0 has no driven adapters in the principle-12 sense — there is no
filesystem, no subprocess, no vendor SDK called at runtime. The runtime
dependency surface is:

- `opentelemetry-semantic-conventions =0.27` — read at xtask-regen time,
  not at runtime. The "lie" surface is "the upstream crate has the
  attributes we think it has"; the regenerator's output (the checked-in
  `semconv_0_27.rs` file) is its own probe — every entry is visible in
  PR diffs, every regeneration produces a re-runnable test (`slice_02`)
  that exercises the full corpus.

- `tracing` (Spark side) — emits the warn event in warn mode. Probed by
  Spark's existing `tracing::subscriber::test::with_default` test
  pattern, which Slice 06's BDD scenario exercises.

The adapter that *does* warrant probing semantics is the **Spark→Codex
integration boundary** at `spark::init`: the cross-feature touch is
Earned-Trust-relevant because it is the first time Spark depends on a
sibling crate's typed `Result` for init success. The Slice 06
acceptance test (warn-mode emits exactly one event, strict-mode
returns `Err`, clean Resource emits zero events) is the probe. Nothing
about this requires the formal `probe()` method specification that
applies to filesystem / subprocess adapters; the typed return value
*is* the probe contract.

---

## Quality attributes (ISO 25010)

| Attribute | Strategy at v0 |
|-----------|----------------|
| **Functional suitability** | Six BDD scenarios per story (US-CO-01 to US-CO-06) cover happy + edge + error paths. The slice-by-slice carpaccio surfaces shape lock-in early (Slice 01) and integration-side risk last (Slice 06). |
| **Performance efficiency** | Levenshtein computation budget: <10 ms for a corpus of ~400 entries on typical hardware (per US-CO-05). Hot-path validation is `O(n×m)` where `n` is input attribute count and `m` is corpus size; for typical Spark Resources (~10 attributes × ~400 corpus entries) this is microseconds. Catalogue is built once per process via `OnceLock`. |
| **Compatibility** | Single semconv pin (`=0.27.x`) per Q3, mirroring Spark's `opentelemetry-proto` family pin via ADR-0013. The new `SparkError::SchemaValidation` variant lands additive on `#[non_exhaustive]` per ADR-0012. |
| **Reliability** | No I/O, no subprocess, no panics outside `unreachable!` paths. The `validate` function is total: every input either yields `Ok(())` or `Err(LintReport)`. |
| **Security** | AGPL-3.0-or-later, symmetric with Spark and Aperture. No data exfiltration surface (library is in-process, no network). Levenshtein input is operator-supplied attribute names, bounded in length by Spark's existing attribute-name validation. |
| **Maintainability** | Public surface is five types; internal modules are four. Mutation testing target 100% kill rate per ADR-0005 Gate 5 covers the lookup loop, the Levenshtein, the report-building loop, and the prefix-match shape. |
| **Testability** | The library's free-function shape (no async, no I/O) means every test is a synchronous `cargo test` invocation. No test seams beyond standard Rust visibility (`pub(crate)` for the Levenshtein helper). The walking-skeleton slice (US-CO-01) locks the testable shape on day one. |
| **Portability** | Pure Rust, MSRV tracks workspace floor. No platform-specific code. |

---

## Constraints honoured

- AGPL-3.0-or-later per LICENSING.md.
- Single pinned semconv version (`=0.27.x`).
- No new runtime deps beyond `opentelemetry-semantic-conventions`
  (which Spark already pulls transitively); in-tree Levenshtein.
- British English, library framing, no FTE/person-day estimates,
  AGPL-symmetric posture (Spark depends on Codex at runtime; same
  shape as Sieve↔Aperture).
- Mutation testing target 100% kill rate per ADR-0005 Gate 5.

---

## ADR table

| ADR | Title | Status |
|-----|-------|--------|
| ADR-0022 | Codex public API and crate layout | Proposed |
| ADR-0023 | Codex corpus regeneration ritual + generated-file shape | Proposed |
| ADR-0024 | Codex dependency pinning + in-tree Levenshtein | Proposed |
| ADR-0025 | Codex Spark integration: lint hook, warn-mode tracing, opt-in strict | Proposed |

All four ADRs follow the Nygard template (Status, Context, Decision,
Considered Alternatives, Consequences). Each carries at least two
considered alternatives with rejection rationale.

---

## Technology stack (summary; full table in `technology-choices.md`)

| Layer | Choice | Licence | Rationale |
|-------|--------|---------|-----------|
| Runtime crate | `crates/codex` | AGPL-3.0-or-later | Symmetric with Spark and Aperture |
| Semconv source | `opentelemetry-semantic-conventions =0.27` | Apache-2.0 | Already a transitive dep via Spark's OTel SDK; pin discipline mirrors Aperture's proto-family pin |
| Tracing (Spark side) | `tracing` (already in Spark) | MIT | No new dep; Codex itself has no tracing surface at v0 |
| Fuzzy matching | In-tree Levenshtein, ~30 lines | AGPL-3.0-or-later (own code) | Q8 lock; small corpus, simple algorithm, no licence-audit cost |
| Regenerator | `xtask` Rust binary reading the semconv crate | AGPL-3.0-or-later (own code) | Q7 lock for the artefact; Q4 for the mechanism |

No proprietary deps. No new runtime deps. All transitive licences
audited via `cargo deny` (already part of workspace CI).

---

## C4 diagrams

- `c4-context.md` — Level 1 (System Context): operator, Spark, Codex,
  upstream OTel semconv crate.
- `c4-container.md` — Level 2 (Containers): the four logical modules
  inside `crates/codex` plus the `xtask` regenerator and Spark's call
  site.
- `c4-component.md` — Level 3 (Components): the lookup loop and the
  Levenshtein component, the only sub-component decomposition warranted
  at this scale.

---

## Slice → ADR → module → CI invariant → KPI mapping

See `slice-mapping.md`. Summary:

| Slice | ADR | Module(s) touched |
|-------|-----|-------------------|
| 01 walking skeleton | ADR-0022 | `lib.rs`, `catalogue`, `report` |
| 02 OTel corpus | ADR-0023 | `generated` (regenerated artefact) |
| 03 house attributes | ADR-0022 | `catalogue` (prefix variant) |
| 04 unknown lint | ADR-0022 | `report`, `catalogue` |
| 05 fuzzy suggestions | ADR-0024 | `fuzzy` |
| 06 Spark integration | ADR-0025 | `crates/spark/src/init.rs`, `crates/spark/src/error.rs`, `crates/spark/src/config.rs` |

---

## Quality gates

- [x] DISCUSS contracts honoured (Q1-Q9 traced through to ADRs).
- [x] Public surface ≤ five types per Q5; verified via ADR-0022.
- [x] No new runtime deps per Q8; verified via ADR-0024.
- [x] Mutation testing target 100% per ADR-0005 Gate 5; carries through
      to every slice.
- [x] AGPL-3.0-or-later symmetric posture; verified in licence-audit.
- [x] C4 L1+L2 produced; L3 produced for the lookup-and-fuzzy
      components (the one place where component decomposition warrants
      it).
- [x] Architectural enforcement: `cargo doc --no-deps` for surface
      drift; `cargo deny` for licence drift; `cargo-mutants` for kill-
      rate gate. All language-appropriate, all already in workspace CI.

---

## Back-propagation

None. All DESIGN-flagged decisions resolve cleanly within DISCUSS
contracts; no DISCUSS lock requires revision. If `back-propagation.md`
later becomes necessary (e.g. during peer review by Atlas), it will
land at `docs/feature/codex/design/back-propagation.md`.

---

## Handoff

The next wave is DISTILL (acceptance test design). The handoff package
to Bea (orchestrator) for routing to `@nw-acceptance-designer`:

- `docs/feature/codex/design/wave-decisions.md` (this file)
- `docs/feature/codex/design/c4-context.md`
- `docs/feature/codex/design/c4-container.md`
- `docs/feature/codex/design/c4-component.md`
- `docs/feature/codex/design/technology-choices.md`
- `docs/feature/codex/design/slice-mapping.md`
- `docs/product/architecture/adr-0022-codex-public-api-and-crate-layout.md`
- `docs/product/architecture/adr-0023-codex-corpus-regeneration-ritual.md`
- `docs/product/architecture/adr-0024-codex-dependency-pinning.md`
- `docs/product/architecture/adr-0025-codex-spark-integration.md`

External integration annotations: none. Codex is in-process; the only
"external" surface is the upstream `opentelemetry-semantic-conventions`
crate, which is build-time-only via the xtask regenerator. No contract
tests are recommended.

Cross-feature ADR amendments (post-DELIVER, not part of this wave):

- ADR-0012 amendment for `SparkError::SchemaValidation(LintReport)`.
- ADR-0013 amendment for the new `codex` runtime dep row in the
  Spark licence-audit table.

These amendments land at the Slice 06 completion summary, mirroring
how Aperture's `--config` wiring landed at slice-08 completion.
