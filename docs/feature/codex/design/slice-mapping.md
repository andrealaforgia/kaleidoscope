# Codex v0 — slice mapping

Per-slice trace from user story → ADR → module → CI invariant → KPI.

| Slice | Story | ADRs | Modules touched | CI invariants | KPI |
|---|---|---|---|---|---|
| 01 walking skeleton | US-CO-01 | ADR-0022 (public surface) + ADR-0024 (zero runtime deps) | `crates/codex/Cargo.toml`, `crates/codex/src/lib.rs`, `crates/codex/src/catalogue.rs` (seeded with 2 entries), `crates/codex/src/lint.rs` (LintReport stub) | Gate 1 (cargo test) on the walking-skeleton integration test; Gate 2 (cargo public-api) on the new crate; Gate 4 (cargo deny) confirms empty runtime closure | KPI 1 (canonical Resource validates clean); KPI 6 (sub-1ms validation) |
| 02 OTel semconv 0.27 corpus | US-CO-02 | ADR-0023 (regeneration ritual) + ADR-0024 §1 (build-time dep) | `xtask/regenerate_codex_corpus/` (new), `crates/codex/src/generated/semconv_0_27.rs` (generated artefact), `crates/codex/src/catalogue.rs` (seeded from generated) | Gate 1 includes the full-corpus fixture; the xtask runs in CI to confirm the checked-in artefact matches the upstream pin | KPI 2 (full upstream corpus blessed) |
| 03 house attribute completeness | US-CO-03 | ADR-0022 §"BlessedAttribute" (ExactMatch + Prefix variants) | `crates/codex/src/catalogue.rs` (Prefix matching for `feature_flag.{key}`); generated/semconv_0_27.rs unchanged; the three house attributes added to a separate `static` slice | Gate 1 includes the house-attribute fixture; mutation testing on the prefix-matching loop | KPI 1 (canonical Resource includes house attrs and validates clean) |
| 04 unknown attribute lint | US-CO-04 | ADR-0022 (LintReport / LintViolation / ViolationKind) | `crates/codex/src/lint.rs` (Display impl, multi-violation collection), `crates/codex/src/catalogue.rs` (validate's Err path) | Gate 1 includes single, multi, and prefix-empty cases; Gate 5 mutation testing on the Err path | KPI 3 (structured violations) |
| 05 fuzzy suggestions | US-CO-05 | ADR-0024 §2 (in-tree Levenshtein) | `crates/codex/src/fuzzy.rs` (new module), `crates/codex/src/catalogue.rs` (call into `nearest_blessed_match`) | Gate 1 includes typo fixtures; Gate 5 mutation testing on the Levenshtein function and the threshold check; Gate 4 confirms no new licence entries | KPI 4 (close-typo suggestions populate) |
| 06 Spark integration | US-CO-06 | ADR-0025 (full integration shape) + post-DELIVER amendments to ADR-0011/-0012/-0013 | `crates/spark/Cargo.toml` (Codex dep), `crates/spark/src/init.rs` (lint hook + OnceLock), `crates/spark/src/error.rs` (new `SchemaValidation` variant), `crates/spark/src/config.rs` (with_strict_schema_lint builder) | Gate 1 (cross-crate test in `crates/spark/tests/`) asserts warn mode and strict mode; Gate 2 confirms the new SparkError variant is non-breaking under `#[non_exhaustive]`; Gate 5 mutation testing on the new branch in `init.rs` | KPI 5 (Spark integration surfaces violations) |

## Module ownership

| Module | Owner |
|---|---|
| `crates/codex/src/lib.rs` | Codex public surface (slice 01) |
| `crates/codex/src/catalogue.rs` | Slices 01-03 (catalogue + matching) |
| `crates/codex/src/lint.rs` | Slice 04 (LintReport / Violation / Kind) |
| `crates/codex/src/fuzzy.rs` | Slice 05 (Levenshtein + nearest_blessed_match) |
| `crates/codex/src/generated/semconv_0_27.rs` | Slice 02 (regenerator output) |
| `xtask/regenerate_codex_corpus/` | Slice 02 (xtask binary, maintainer ritual) |
| `crates/spark/src/init.rs` | Slice 06 (lint hook + OnceLock catalogue) |
| `crates/spark/src/error.rs` | Slice 06 (new SchemaValidation variant) |
| `crates/spark/src/config.rs` | Slice 06 (with_strict_schema_lint builder) |

## Hand-off boundary to acceptance-designer (DISTILL)

Scholar receives:

- The four ADRs (0022-0025) plus the cross-cutting amendments to
  ADR-0011/-0012/-0013 (post-DELIVER, not pre-DISTILL).
- The C4 L1/L2/L3 diagrams for the architecture context.
- This slice mapping table; per-slice IN/OUT scope from the
  individual slice briefs at `docs/feature/codex/slices/`.

Test posture: same Strategy C "real local" Sieve and Spark settled
on. The slice 06 integration test lives in `crates/spark/tests/`
(not `crates/codex/tests/`), per the cross-crate touch convention
the workspace already follows for `aperture::testing::RecordingSink`
fixtures. The Codex side of slice 06 is a small wrapper exercising
`SchemaCatalogue` against the same fixture set the unit-level slice
04-05 tests use.

## Hand-off boundary to platform-architect (DEVOPS)

Apex receives:

- New `[[test]]` declarations in `crates/codex/Cargo.toml` for the
  five Codex-side slice integration tests (the slice 06 test is in
  Spark).
- A new `gate-5-mutants-codex.yml` workflow mirroring the Sieve
  precedent; per-slice in-diff mutation testing with a 30-minute
  timeout.
- Gate 2 + Gate 3 extensions to cover Codex (mirroring how Spark and
  Sieve graduated).
- No deny.toml edits required at v0 (Codex's runtime closure is
  empty; the xtask's build-time deps are in a separate Cargo.toml
  that does not feed the cargo-deny audit on the published crate).

## External integrations

None. Codex v0 has no network surface and no external services. The
contract-test recommendation (per the agent principle for any
component with external integrations) does not apply.
