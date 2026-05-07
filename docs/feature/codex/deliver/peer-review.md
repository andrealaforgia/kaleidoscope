# Peer review — Codex v0 DELIVER

- **Date**: 2026-05-07
- **Reviewer**: `@nw-software-crafter-reviewer` (Crafty in review mode)
- **Wave**: DELIVER (Crafty's five slice landings 01, 02, 04, 05;
  slice 03 closed by construction at slice 02 corpus seeding)
- **Artefact set**: `crates/codex/src/`, `crates/codex/tests/`,
  `crates/codex/Cargo.toml`, `xtask/regenerate_codex_corpus/`,
  ADRs 0022-0025
- **Verdict**: **APPROVED** — graduate; coordinate tagging
- **Critical issues**: 0
- **Blocking issues**: 0
- **Iteration**: 1 of 2 — no revisions required

---

## Executive summary

Codex v0 DELIVER is exemplary. Five slices closed clean across 15
external acceptance tests plus 31 inline unit tests, with
disciplined RED → GREEN → REFACTOR cycles. Mutation kill rate is
100% on every slice's diff: 35 viable mutants total across the
five slices, all 35 caught. ADR fidelity is perfect across all four
ADRs (0022 public API, 0023 corpus regeneration ritual, 0024
dependency pinning + Levenshtein, 0025 Spark integration scaffolding).

Hexagonal boundary is pristine: every test imports from the
five-item public surface only, with zero internal-module reaches
verified by grep. The compile-time smoke test
(`invariant_public_api_smoke.rs`) locks the surface at Gate 1.

Crafty's back-propagation discipline at slice 02 (honouring
Scholar's `spark_canonical_resource_pair()` fixture and ADR-0023
§3 over the brief's misaligned sentence) is the discipline the
methodology rewards. Slice 03 closed GREEN by construction as a
side effect; the implementation matched the test's contract, not
the other way around.

---

## Quantitative validation

| Slice | Tests | Mutants | Caught | Status |
|---|---|---|---|---|
| 01 walking skeleton | 2 acceptance + 5 inline | 9 | 9 | GREEN, 100% |
| 02 OTel semconv corpus | 2 acceptance | 0 (LazyLock + as_slice; no operators) | n/a | GREEN, trivially 100% |
| 03 house attributes | 3 acceptance | 0 (closed by construction) | n/a | GREEN by construction |
| 04 Display impl | 4 acceptance + 8 inline | 2 | 2 | GREEN, 100% |
| 05 Levenshtein | 3 acceptance + 18 inline | 24 | 24 | GREEN, 100% |
| invariant smoke | 1 acceptance | n/a (compile-time) | n/a | GREEN |

**Cumulative**: 35 viable mutants across the five slices, 35
caught, zero missed. **100% kill rate**.

15 acceptance tests + 31 inline unit tests = 46 tests covering 5
user stories. The inline tests follow port-to-port discipline at
domain scope (each pure function's signature IS its driving port);
the acceptance tests exercise the five-type public surface. The
composition is disciplined.

---

## ADR fidelity

| ADR | Implementation | Verdict |
|---|---|---|
| ADR-0022 — public API | Five public types (`SchemaCatalogue`, `BlessedAttribute`, `LintReport`, `LintViolation`, `ViolationKind`); module split per layout; `forbid(unsafe_code)` in lib.rs | PASS |
| ADR-0023 — corpus regeneration ritual | xtask binary at `xtask/regenerate_codex_corpus/` enumerates upstream constants via real `use` imports; checked-in generated file at `crates/codex/src/generated/semconv_0_27.rs` (132 entries, alphabetically sorted); separate `HOUSE_ATTRIBUTES` keeps house attributes hand-maintained | PASS |
| ADR-0024 — dependency pinning + Levenshtein | Codex Cargo.toml has empty `[dependencies]`; xtask's `opentelemetry-semantic-conventions =0.27` is build-time-only; Levenshtein is in-tree (`fuzzy.rs::levenshtein`) with two-row DP matrix; `THRESHOLD = 2` named constant | PASS |
| ADR-0025 — Spark integration scaffolding | Codex side complete (LintReport Display rendering ready for Spark's warn-mode body); Spark side (slice 06) lands as separate Spark-side wave with ADR-0012/-0013 post-DELIVER amendments | PASS (Codex side); Spark side scheduled separately |

---

## Test fidelity check

`praise:` All 46 tests import from Codex's public surface only.
Grep confirms zero internal-module reaches in the test directory.
The compile-time smoke (`invariant_public_api_smoke.rs`) locks the
five-type surface at CI Gate 1.

`praise:` Inline unit tests in `catalogue.rs`, `fuzzy.rs`, and
`lint.rs` are exemplary of port-to-port domain-level testing. Each
test targets specific operator mutations (== flip, && flip, >
mutations, distance boundary, tie-break order). The slices'
acceptance tests complement the unit surface, proving end-to-end
user outcomes. The composition is correct.

`praise:` No test modifications detected during the RED → GREEN
trail. Zero assertion weakening. Zero deleted tests. Zero "TODO fix
later" markers. The discipline held across all five slice landings.

---

## RED → GREEN trail

Each slice's commit history shows the canonical pattern: RED tests
in place from DISTILL, production stub returning `unimplemented!()`,
implementation lands one piece at a time driven by the smallest
failing test. Two of the five slices (01 and 05) added pinning
commits to kill mutation survivors after the GREEN landed. The
slice 03 "no-op DELIVER" was correctly documented in the slice 04
commit message rather than producing an empty commit.

The pre-commit hook ran on every commit with all gates green. No
`--no-verify` was used. Every commit went directly to `main` per
pure trunk-based discipline. All eight commits pushed to origin.

---

## Hexagonal boundary

`praise:` Production code does not leak internals. The public
surface is exactly the five types named in ADR-0022. All tests
import only from `codex::` public re-exports. The xtask binary
lives in a separate workspace member with its own Cargo.toml and
its own dependency closure (build-time-only `opentelemetry-semantic-conventions`).

The only real infrastructure adapter is the xtask binary itself,
which is operator infrastructure, not part of Codex's runtime
closure. No database, filesystem, or network I/O in Codex itself.

---

## Defensive coding

`praise:` `#![forbid(unsafe_code)]` honoured throughout. No
`unwrap`/`expect` on user-input paths. Levenshtein is pure. The
LazyLock for the static catalogue is the right shape for "build
once at first access, share for the rest of the process". Drop
semantics are trivial (no resources to release).

The xtask binary's surface (a small main function plus
upstream-constant enumeration) is similarly defensive: it depends
on the upstream crate's API at compile time, so an upstream rename
breaks the build cleanly rather than producing a silent corpus
gap.

---

## Back-propagation discipline

`praise:` Crafty's handling of the slice 02 / slice 03 boundary
exemplifies the methodology's back-propagation discipline:

1. The slice 02 brief I wrote stated "no `feature_flag.` Prefix
   entry until slice 03".
2. Scholar's `spark_canonical_resource_pair()` fixture at DISTILL
   contained `feature_flag.checkout-v2` — which requires the
   Prefix entry to validate clean.
3. ADR-0023 §3 says "seed all three house attributes at slice 02"
   so slice 03 can close by construction.
4. The brief contradicted Scholar's fixture and the ADR.
5. Crafty followed Scholar's fixture and the ADR, not the brief.
6. Slice 03 closed GREEN by construction at slice 02 landing.

This is the discipline. Implementations match tests; tests match
ADRs; briefs that contradict either are amended (the slice 02
brief sentence was caught at the slice 02 commit message and
recorded for downstream awareness; the slice 03 dispatch was
correctly skipped).

---

## House style

British English in commits, comments, and rendering text verified
("catalogue", not "catalog"; "honoured", not "honored"). No FTE
estimates anywhere. AGPL containment is symmetric: Codex is
AGPL-3.0-or-later; the xtask is Apache-2.0 (operator tool, not
runtime); the Spark integration in slice 06 will keep Spark's
Apache-2.0 licence by consuming Codex through its public API.

---

## Cross-cutting invariants

`invariant_public_api_smoke.rs` enforces the five-type public lock
at compile time. If any of `SchemaCatalogue`, `BlessedAttribute`,
`LintReport`, `LintViolation`, `ViolationKind` is renamed or
removed, the binary fails to compile. Cheap and effective.

The 100% mutation kill rate gate (ADR-0005 Gate 5) held at every
slice's close.

---

## Workspace integrity

`cargo build --workspace --all-targets --locked` succeeds clean.
`cargo clippy --workspace --all-targets --locked -- -D warnings`
clean. `cargo deny --all-features check` clean (no new licence
audit entries; Codex's runtime closure is empty per ADR-0024 §3).
The harness, Aperture, Spark, Sieve crates' tests pass with no
regression.

---

## Suggestions (non-blocking, post-graduation)

`suggestion (non-blocking):` Slice 06 (Spark integration) lives in
Spark's wave queue. When it lands, ADR-0012 (Spark error type) and
ADR-0013 (Spark dep pinning) gain post-DELIVER amendments naming
the new `SparkError::SchemaValidation` variant and the Codex
runtime dep. Same shape as the Sieve / Aperture amendments at
their respective landings.

`suggestion (non-blocking):` The xtask's `use` imports
hand-enumerate the upstream semconv constants. When the upstream
crate adds a new resource attribute (semconv 0.28 or later), the
maintainer will need to add a corresponding `use` line. A small
README in `xtask/regenerate_codex_corpus/` documenting the
maintainer ritual would be helpful for first-time runners. Defer
to first time the upstream pin moves.

`suggestion (non-blocking):` The `BlessedAttribute::Prefix`
suggestion currently renders the prefix string (e.g.
`feature_flag.`) as the suggested name. The DISCUSS slice 05 brief
mentioned a prefix-rule reconstruction shape (`feature_flg.checkout`
→ `feature_flag.checkout`) as a possible v1 polish. Defer until a
consumer asks.

---

## Praise

`praise:` Mutation testing discipline. 35 viable mutants, all 35
killed. The inline unit tests target specific operator mutations
with surgical precision. Slice 05's tie-break refactor (commit
`d483e83`) flushed two surviving mutants by collapsing the
nearest_blessed_match implementation into an `iterator::min_by`
with a `(distance, name)` tuple. That kind of refactor-driven kill
is the canonical Outside-In TDD shape.

`praise:` Back-propagation discipline honoured cleanly. Crafty
detected the brief vs Scholar's fixture mismatch at slice 02 and
made the right call (test contract wins). This is judgment, not
rubber-stamping. The cost of getting this wrong (modifying the
test to match the implementation) is enormous; the cost of getting
it right (a small comment in the slice 04 commit message
documenting slice 03's no-op closure) is trivial.

`praise:` xtask infrastructure is the right shape. Real `use`
imports of upstream constants give compile-time audit signal: if
upstream renames or removes a constant, the xtask fails to compile
and the maintainer sees the signal at the next regeneration. The
generated file is alphabetically sorted for minimal PR diff. The
separation of `SEMCONV_0_27` (generated) from `HOUSE_ATTRIBUTES`
(hand-written) keeps the regeneration ritual from clobbering
hand-maintained entries.

`praise:` The five-type public lock plus the doc-hidden test seam
discipline (precedent set by Spark and Sieve) is now well-established
in Codex too. The `invariant_public_api_smoke` test is the
cheapest possible enforcement at CI time.

---

## Approval

**APPROVED** for graduation.

- Critical issues: 0
- Blocking findings: 0
- Iteration budget: 1 of 2 used. No revisions required.

Coordinate Codex v0 graduation:

1. Remove `--exclude codex` from `scripts/hooks/pre-commit` Gate 1.
2. Remove `--exclude codex` from `.github/workflows/ci.yml` Gate 1.
3. Tag `codex/v0.1.0`.
4. Update narrative + slides for the DELIVER closure (per the
   wave-by-wave cadence rule).
5. Slice 06 (Spark integration) is a separate Spark-side wave
   landing later; the post-DELIVER amendments to ADR-0012 and
   ADR-0013 route at that landing.

Forge's peer review on the DEVOPS workflow extensions can run
independently once the next CI run on Codex-touching commits comes
back green.
