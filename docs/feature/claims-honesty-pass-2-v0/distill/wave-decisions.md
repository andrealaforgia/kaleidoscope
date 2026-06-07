<!-- markdownlint-disable MD013 MD024 -->

# Wave Decisions — claims-honesty-pass-2-v0 (DISTILL)

- **Wave**: DISTILL (nWave)
- **Acceptance Designer**: Quinn (nw-acceptance-designer)
- **Date**: 2026-06-07
- **Mode**: autonomous, subagent. No questions returned to the operator.
- **Upstream**: DESIGN `wave-decisions.md` (the authoritative 9-locus
  overstatement->truth->proving-code table; the prism-e2e MARK decision; the
  anti-over-correction guardrails) + DISCUSS `user-stories.md` (US-01/02/03,
  ACs).
- **Does NOT proceed into DELIVER.** Does NOT commit (operator commits).

## Walking-skeleton strategy — structural assertion over the committed docs

This is a **prose/comment/config-honesty feature**. There is no service to stand
up, no driving port to invoke, no process to spawn. The observable outcome the
reader (Devin Okafor, evaluator/contributor) wants lives entirely in committed
**doc-comments, a Cargo manifest description, the platform README, a prism
README, and a playwright config**. So the acceptance is **STRUCTURAL** — a
`std::fs` string-parsing test that reads each corrected file and asserts content,
mirroring the established `integration-suite` precedent
(`v0_perf_kpi_ci_non_gating_structure.rs` ADR-0070,
`v0_fast_precommit_structure.rs` ADR-0072).

The "driving surface" the reader actually uses IS the committed file: `cargo doc`
renders the pulse rustdoc; the GitHub-rendered README is the brand front door;
the playwright config is read on the CI page. The structural test that reads
those exact files is the falsifiable regression net. There is no behaviour to
exercise (the feature builds nothing, changes no production-logic line), so no
runtime walking skeleton applies — the **single structural guard test IS the
walking skeleton**: it answers "does the reader see claims that match the code?"
through the only surface the reader touches.

### Both-directions guard (the DESIGN mandate, encoded)

For EACH overstatement locus the test asserts BOTH:

- **(a) the FALSE phrase is ABSENT** — so a non-correction (leaving the lie)
  cannot pass.
- **(b) the TRUE phrase is PRESENT** — so an over-correction (deleting the claim,
  or swinging it into a new lie) cannot pass either.

This is the DESIGN anti-over-correction requirement made executable: neither
drift direction (the honesty thesis's twin failures — over-trust and
under-trust) can slip through.

## Reconciliation against the DESIGN 9-locus table

Each DESIGN row maps to a test function; the 9 loci collapse into 5 RED
behavioural scenarios (grouped by locus per the brief) + 3 GREEN controls.

| DESIGN row | Locus | Scenario (test fn) | Tag |
|---|---|---|---|
| 1 | `pulse/src/lib.rs:46` (volatility under-claim) | `pulse_doc_states_durable_store_survives_restart_not_crate_wide_volatility` | `#[ignore]` RED |
| 1b | `pulse/src/lib.rs:37` ("no daemon, no network" — TRUE) | NOT asserted against (DESIGN: accurate, left as-is) | — |
| 2 | `pulse/src/lib.rs:20-22,41` (columnar shipped over-claim) | `pulse_docs_name_durable_adapter_and_future_tense_columnar` | `#[ignore]` RED |
| 3 | `pulse/Cargo.toml:7` (description) | `pulse_docs_name_durable_adapter_and_future_tense_columnar` | `#[ignore]` RED |
| 4 | `gateway/src/main.rs:62-63` ("RED-ready NO-OP") | `gateway_comments_and_test_prose_describe_the_green_delivered_code` | `#[ignore]` RED |
| 5 | `gateway/src/main.rs:118-120` + `:24-25` ("force stub") | `gateway_comments_and_test_prose_describe_the_green_delivered_code` | `#[ignore]` RED |
| 6 | `gateway/tests/slice_01_tracing_subscriber.rs:42,207,280` | `gateway_comments_and_test_prose_describe_the_green_delivered_code` | `#[ignore]` RED |
| 7 | `README.md:184` (Prism row) | `readme_prism_row_and_cost_line_match_the_single_metric_reality` | `#[ignore]` RED |
| 8 | `README.md:222` (cost line) | `readme_prism_row_and_cost_line_match_the_single_metric_reality` | `#[ignore]` RED |
| 9 | `apps/prism/playwright.config.ts:19` + `apps/prism/README.md:35` | `prism_e2e_browser_matrix_gate_is_marked_scaffold_not_advertised_live` | `#[ignore]` RED |

### GREEN controls (un-ignored, pass NOW and AFTER)

| Control (test fn) | Guards | DESIGN basis |
|---|---|---|
| `prism_module_readme_stays_the_single_metric_source_of_truth` | the already-honest `apps/prism/README.md` "single PromQL query panel" stays the SSOT the platform README is aligned TO; if the module README were itself inflated, this reds | rows 7/8 (alignment anchor) |
| `gateway_fixed_port_ac_01_ignore_attributes_are_not_removed` | both fixed-port AC-01 `#[ignore]` attributes survive — DELIVER edits PROSE ONLY; over-correction (de-ignoring a real port-flake ignore) reds | row 6 guardrail |
| `prism_e2e_digest_ssot_and_readd_roadmap_are_preserved` | the `PROMETHEUS_IMAGE_DIGEST` constant + the slice-by-slice "Re-add per slice landing:" roadmap survive — MARK (annotate) is not silently turned into REMOVE (delete) | row 9 MARK guardrail |

## Falsifiability — the false phrases are present TODAY (RED proven)

Verified at HEAD (2026-06-07) by reading each file, then by running the test:

- `pulse/src/lib.rs:46` reads exactly `//! - In-memory only at v0; restart loses points.`
- `pulse/src/lib.rs:20-22` reads the present-tense columnar "lives behind the same trait".
- `pulse/Cargo.toml:7` reads "the columnar (... ) adapter lands at v1 behind the same trait".
- `gateway/src/main.rs:62` reads "the body is a RED-ready NO-OP that Crafty fills in DELIVER".
- `gateway/src/main.rs:118` reads "Force `sink.kind = stub`"; `:24-25` "forces `sink.kind = stub` internally".
- `gateway/tests/slice_01_tracing_subscriber.rs:42` reads "a wired NO-OP"; `:207,:280` "RED against the no-op subscriber".
- `README.md:184` reads "Unified query and visualisation frontend"; `:222` "The compliance dashboards in Prism are open templates."
- `apps/prism/playwright.config.ts:19` reads the unqualified "Gate 7 (Prism E2E across the browser matrix)."; no "scaffold"/"NOT YET IMPLEMENTED" anywhere in the config.

**Run evidence (this binary only):**

- `cargo test -p integration-suite --test v0_claims_honesty_pass_2_structure`
  → `ok. 3 passed; 0 failed; 5 ignored` (default GREEN: controls pass, RED ignored).
- `cargo test -p integration-suite --test v0_claims_honesty_pass_2_structure -- --ignored`
  → `FAILED. 0 passed; 5 failed; 0 ignored` (each RED fails on its false-phrase-ABSENT assertion).

A guard that cannot fail proves nothing; these 5 fail today, which proves the
guard is real (Earned-Trust shape). DELIVER removes each `#[ignore]` when its
matching edit lands.

## `#[ignore]`-until-DELIVER (nWave ordering)

DISTILL precedes DELIVER. The doc/comment/config corrections do not exist yet, so
the 5 correction scenarios are tagged `#[ignore = "RED until DELIVER: ..."]`.
Default `cargo test` stays GREEN on the current tree; `--ignored` shows them
FAILING. This is the EXPECTED structural-RED for a DISTILL wave, identical to the
two sibling structural tests.

## Mandate 7 — RED, not BROKEN

The test reads existing files (`read_repo_file` = `fs::read_to_string` +
`unwrap_or_else(panic!)` with a clear message) and asserts content. No production
symbol is referenced, so it COMPILES and links; the RED assertions FAIL
behaviourally (the false phrases are present), they do not error on setup. A
genuinely missing file would report a clean FAILED with a path message, not an
opaque ERROR. Confirmed: the `--ignored` run shows 5 clean FAILED (behavioural
panics on the assert), zero ERROR.

## Path-resolution note (DEVOPS watch-item)

`CARGO_MANIFEST_DIR` is `<repo>/crates/integration-suite`; `repo_root()` goes two
parents up to the repo root. ALL targets — `crates/pulse/*`,
`crates/kaleidoscope-gateway/*`, `README.md`, `apps/prism/README.md`,
`apps/prism/playwright.config.ts` — are resolved from there, so the test is
robust regardless of the caller's working directory. This matches the
DEVOPS-flagged resolution used by both sibling structural tests (README / apps
paths resolved two parents up from the integration-suite crate dir).

## Mixed-ownership note (carried from DESIGN, for DELIVER)

The `.rs` doc-comment edits (`pulse/src/lib.rs`, `gateway/src/main.rs`,
`gateway/tests/slice_01_tracing_subscriber.rs`) + this structural guard test are
crafter / `integration-suite` territory. The `README.md`, `pulse/Cargo.toml`
description, `apps/prism/README.md`, and `playwright.config.ts` are non-crafter
docs/metadata/config. The single structural test reads ALL of these (including
the non-`.rs` files via `std::fs`) and is the unifying regression net — it lives
in `integration-suite`.

## Coordination for DELIVER's exact wording

The PRESENT assertions are KEY-CLAIM substrings, not exact full sentences, so
DELIVER has wording latitude while still satisfying the guard. DELIVER's
corrected text MUST contain:

- pulse `lib.rs`: the literal `FileBackedMetricStore`, both `survives` and
  `restart` in the durability claim, and `future` for columnar; and must DROP the
  exact line `In-memory only at v0; restart loses points.` and the present-tense
  `(Arrow + Parquet + DataFusion + Prometheus TSDB block) lives behind the same trait`.
- pulse `Cargo.toml`: `FileBackedMetricStore` (or `file-backed`) and `future`;
  drop the exact "lands at v1 behind the same trait" columnar clause.
- gateway `main.rs`: `installs the real JSON-to-stderr`, `relies on`, `Stub
  default`; drop "RED-ready NO-OP that Crafty fills in DELIVER" and "Force/forces
  `sink.kind = stub`".
- gateway test prose: `installs the real JSON-to-stderr` and `GREEN`; drop "wired
  NO-OP" and "RED against the no-op subscriber".
- `README.md`: a `single-metric PromQL` / `single PromQL` / `single-metric`
  framing for Prism, keep `Prism` truthfully in the cost surface; drop "Unified
  query and visualisation frontend" and "The compliance dashboards in Prism are
  open templates.".
- `playwright.config.ts`: `scaffold` or `NOT YET IMPLEMENTED`; drop the
  unqualified "Gate 7 (Prism E2E across the browser matrix)." advertisement.
- `apps/prism/README.md`: `scaffold` in the `pnpm playwright` note; KEEP "single
  PromQL query panel" (control).

## Peer review

Self-review against `nw-ad-critique-dimensions` recorded in
`acceptance-test-scenarios.md` (the `nw-acceptance-designer-reviewer` is not
separately nested-invocable from this sub-agent context — critique-dimensions
applied directly, mirroring the DESIGN wave's posture). Verdict: APPROVED.
