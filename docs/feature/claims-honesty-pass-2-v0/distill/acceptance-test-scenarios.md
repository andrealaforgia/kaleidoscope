<!-- markdownlint-disable MD013 MD024 -->

# Acceptance Test Scenarios — claims-honesty-pass-2-v0 (DISTILL)

Structural acceptance: one `std::fs` string-parsing test
(`crates/integration-suite/tests/v0_claims_honesty_pass_2_structure.rs`) that
reads each corrected file and asserts the both-directions honesty guard per
locus. Mirrors the `integration-suite` structural precedent. No service, no port,
no spawned process — the committed doc/comment/config IS the surface the reader
reads.

## The both-directions guard (note)

For EACH overstatement locus, the test asserts BOTH:

- **(a) FALSE phrase ABSENT** — a non-correction (leaving the lie) cannot pass.
- **(b) TRUE phrase PRESENT** — an over-correction (deleting the claim, or
  over-swinging into a new lie) cannot pass.

This encodes the DESIGN anti-over-correction mandate. The honesty thesis has two
failure modes (over-trust from over-claims, under-trust from under-claims);
asserting only ABSENT would let a deletion masquerade as a fix, asserting only
PRESENT would let the old lie linger beside the new truth. Both are required.

## Scenarios (RED — `#[ignore]`d until DELIVER)

### Scenario 1 — pulse-volatility

`pulse_doc_states_durable_store_survives_restart_not_crate_wide_volatility`

```gherkin
Given Devin reads the pulse/src/lib.rs crate documentation
When Devin reads the architectural-posture section
Then the durable FileBackedMetricStore is named and stated to survive process restart
And the unscoped crate-wide phrase "In-memory only at v0; restart loses points." is absent
```

- **ABSENT** (false): `In-memory only at v0; restart loses points.`
- **PRESENT** (true): `FileBackedMetricStore` AND `survives` AND `restart`
- `@ignore`: "RED until DELIVER: pulse/src/lib.rs still carries the unscoped
  'restart loses points' under-claim and lacks the durable survives-restart
  statement"
- Maps to: **US-01 AC-1**, DESIGN row 1. Guardrail 1b: line `:37` "no daemon, no
  network" NOT asserted against (it is TRUE, left as-is).

### Scenario 2 — pulse-columnar

`pulse_docs_name_durable_adapter_and_future_tense_columnar`

```gherkin
Given Devin reads pulse/src/lib.rs and pulse/Cargo.toml
When Devin reads what the durable / v1 adapter is
Then the durable JSON-over-WAL FileBackedMetricStore is named as shipped
And the columnar substrate is named only as a future direction
And no present-tense / "lands at v1" columnar shipped-promise remains
```

- **ABSENT** (false, lib.rs): `(Arrow + Parquet + DataFusion + Prometheus TSDB block) lives behind the same trait` (and the wrapped-comment variant)
- **ABSENT** (false, Cargo.toml): `the columnar (Arrow + Parquet + DataFusion + Prometheus TSDB block) adapter lands at v1 behind the same trait`
- **PRESENT** (true, lib.rs): `FileBackedMetricStore` AND `future`
- **PRESENT** (true, Cargo.toml): `FileBackedMetricStore` (or `file-backed`) AND `future`
- `@ignore`: "RED until DELIVER: pulse/src/lib.rs and pulse/Cargo.toml still
  present the columnar adapter as shipped/v1 rather than as a future direction,
  and do not name the durable JSON-over-WAL adapter"
- Maps to: **US-01 AC-2, AC-3**, DESIGN rows 2+3. Guardrail 2/3: columnar
  asserted PRESENT-as-`future`, never ABSENT (the roadmap is preserved).

### Scenario 3 — gateway-comments

`gateway_comments_and_test_prose_describe_the_green_delivered_code`

```gherkin
Given Devin reads kaleidoscope-gateway/src/main.rs and the slice_01 tracing test
When Devin reads the init_tracing comment, the config comment, and the test prose
Then the comments state init_tracing installs the real JSON-to-stderr subscriber
And the gateway relies on the Config::builder() Stub default (not "forces")
And the test prose describes the always-run scenarios as GREEN
And no "RED-ready NO-OP", "force sink.kind = stub", "wired NO-OP", or
    "RED against the no-op subscriber" claim remains
```

- **ABSENT** (false, main.rs): `RED-ready NO-OP that Crafty fills in DELIVER`;
  `Force `sink.kind = stub``; `forces `sink.kind = stub``
- **ABSENT** (false, test): `wired NO-OP`; `RED against the no-op subscriber`
- **PRESENT** (true, main.rs): `installs the real JSON-to-stderr`; `relies on` AND `Stub default`
- **PRESENT** (true, test): `installs the real JSON-to-stderr`; `GREEN`
- `@ignore`: "RED until DELIVER: gateway main.rs comments still say 'RED-ready
  NO-OP'/'Force sink.kind = stub' and slice_01 prose still says 'wired
  NO-OP'/'RED against the no-op subscriber'"
- Maps to: **US-02 AC-1, AC-2, AC-3**, DESIGN rows 4+5+6.

### Scenario 4 — prism-readme

`readme_prism_row_and_cost_line_match_the_single_metric_reality`

```gherkin
Given Devin reads the platform README.md table and cost-model table
When Devin reads the Prism row and the Prism cost line
Then the Prism row is described as a single-metric PromQL explorer
And the cost surface keeps an honest Prism answer (no compliance-dashboards claim)
And no "Unified query and visualisation frontend" or
    "The compliance dashboards in Prism are open templates." remains
```

- **ABSENT** (false): `Unified query and visualisation frontend`; `The compliance dashboards in Prism are open templates.`
- **PRESENT** (true): `single-metric PromQL` / `single PromQL` / `single-metric`; `Prism` retained truthfully in the cost surface
- `@ignore`: "RED until DELIVER: README.md still says 'Unified query and
  visualisation frontend' (Prism row) and 'The compliance dashboards in Prism are
  open templates.' (cost line)"
- Maps to: **US-03 AC-1, AC-2**, DESIGN rows 7+8.

### Scenario 5 — prism-e2e-mark

`prism_e2e_browser_matrix_gate_is_marked_scaffold_not_advertised_live`

```gherkin
Given Devin reads apps/prism/playwright.config.ts and the prism README pnpm-playwright note
When Devin reads how the browser-matrix e2e gate is described
Then the config marks the gate not-yet-implemented / scaffold
And the prism README pnpm-playwright note is marked scaffold
And no unqualified "Gate 7 (Prism E2E across the browser matrix)" advertisement remains
```

- **ABSENT** (false): `Gate 7 (Prism E2E across the browser matrix).`
- **PRESENT** (true): `scaffold` or `NOT YET IMPLEMENTED` (config); `scaffold` (prism README)
- `@ignore`: "RED until DELIVER: playwright.config.ts still advertises an
  unqualified 'Gate 7 (Prism E2E across the browser matrix)' and carries no
  scaffold/NOT-YET-IMPLEMENTED marker; the prism README pnpm-playwright note is
  unmarked"
- Maps to: **US-03 AC-3**, DESIGN row 9 (MARK).

## Controls (GREEN — un-ignored, pass NOW and AFTER)

| Control (test fn) | Asserts | DESIGN basis |
|---|---|---|
| `prism_module_readme_stays_the_single_metric_source_of_truth` | `apps/prism/README.md` keeps "single PromQL query panel" (the SSOT the platform README aligns TO) | rows 7/8 alignment anchor |
| `gateway_fixed_port_ac_01_ignore_attributes_are_not_removed` | both fixed-port AC-01 `#[ignore = "..."]` attributes survive (PROSE-ONLY edit guard) | row 6 guardrail |
| `prism_e2e_digest_ssot_and_readd_roadmap_are_preserved` | `PROMETHEUS_IMAGE_DIGEST` + "Re-add per slice landing:" roadmap survive (MARK ≠ REMOVE) | row 9 MARK guardrail |

## Test-fn → US/AC traceability map

| Test fn | US | AC | DESIGN row(s) | State |
|---|---|---|---|---|
| `pulse_doc_states_durable_store_survives_restart_not_crate_wide_volatility` | US-01 | AC-1 | 1 | RED |
| `pulse_docs_name_durable_adapter_and_future_tense_columnar` | US-01 | AC-2, AC-3 | 2, 3 | RED |
| `gateway_comments_and_test_prose_describe_the_green_delivered_code` | US-02 | AC-1, AC-2, AC-3 | 4, 5, 6 | RED |
| `readme_prism_row_and_cost_line_match_the_single_metric_reality` | US-03 | AC-1, AC-2 | 7, 8 | RED |
| `prism_e2e_browser_matrix_gate_is_marked_scaffold_not_advertised_live` | US-03 | AC-3 | 9 | RED |
| `prism_module_readme_stays_the_single_metric_source_of_truth` | US-03 | (anchor) | 7/8 | GREEN control |
| `gateway_fixed_port_ac_01_ignore_attributes_are_not_removed` | US-02 | AC-3/AC-4 (no `#[ignore]` change) | 6 | GREEN control |
| `prism_e2e_digest_ssot_and_readd_roadmap_are_preserved` | US-03 | AC-4 (scaffold untouched) | 9 | GREEN control |

US-01 AC-4 / US-02 AC-4 / US-03 AC-4 ("no behaviour weakened; no `#[ignore]`
changed; scaffolds untouched") are covered by the three GREEN controls plus the
PROSE-ONLY scope of every RED assertion (each reads doc/comment/config text, never
asserts on runtime behaviour). The pulse durability + snapshot runtime suites are
NOT re-asserted here — they are owned by the pulse crate's own always-run tests
and are out of this structural test's surface (the structural guard guarantees no
production-logic line is referenced).

## Mandate-7 / falsifiable self-review checklist

- [x] **Compiles & links** (no missing production symbol) — `cargo test` built
  the binary in 10.93s.
- [x] **RED, not BROKEN** — `--ignored` shows 5 clean behavioural FAILED (assert
  panics on false-phrase-ABSENT), zero ERROR. File reads `panic!` with a clear
  path message so a missing file is a clean FAILED.
- [x] **Default GREEN** — `cargo test ... ` → 3 passed, 0 failed, 5 ignored.
- [x] **RED scenarios genuinely fail today** — `--ignored` → 0 passed, 5 failed.
- [x] **Both-directions** — every RED scenario asserts FALSE-ABSENT AND
  TRUE-PRESENT.
- [x] **False strings present at HEAD** — verified by reading each file
  (2026-06-07) and by the `--ignored` failures.
- [x] **True substrings coordinated with DELIVER** — KEY-CLAIM substrings, wording
  latitude documented in `wave-decisions.md`.
- [x] **Path resolution robust** — `repo_root()` = two parents up from
  `CARGO_MANIFEST_DIR`; all six files resolved from there.
- [x] **Prism-already-honest control meaningful** — asserts the module README
  SSOT the platform README is aligned TO; reds if the anchor is inflated.
- [x] **Anti-over-correction controls** — `#[ignore]`-untouched + digest/roadmap
  preserved guard against MARK→REMOVE and prose→behaviour drift.
- [x] **fmt clean** (`cargo fmt -p integration-suite -- --check` exit 0).
- [x] **clippy clean** (`cargo clippy -p integration-suite --tests`, no warnings).
- [x] **No doc/comment/config file edited** (DELIVER owns those); no
  `crates/*/src` logic touched.

## Peer review — nw-ad-critique-dimensions (self-applied)

`nw-acceptance-designer-reviewer` not nested-invocable from this sub-agent
context; critique-dimensions applied directly.

| Dimension | Assessment |
|---|---|
| **1 Happy-path bias** | N/A in the conventional sense — this is a documentation-honesty feature with no error runtime path. The "error" analogue (over-correction / non-correction) is covered by the both-directions guard + 3 anti-over-correction controls. The guard ratio (every locus checked in both failure directions, 3 dedicated negative controls) exceeds the spirit of the 40% error-path mandate. PASS. |
| **2 GWT compliance** | Each scenario is single-Given/When/Then-shaped (see Gherkin above), one behaviour per scenario, observable outcome. PASS. |
| **3 Business-language purity** | The reader-facing scenarios are in domain terms (Devin reads a claim; the claim matches the code). Substrings asserted are the literal artefact text (unavoidable for a doc-honesty guard); the scenario PROSE is business-framed. PASS. |
| **4 Coverage completeness** | All 9 DESIGN loci + all 3 US (US-01/02/03) + every AC mapped to a scenario or control (traceability table above). No DESIGN row unmapped. PASS. |
| **5 WS user-centricity** | The "walking skeleton" here is the structural guard over the surface the reader actually reads (`cargo doc` / README / CI page) — user-goal framed ("does what the reader sees match the code?"), not layer-wiring. Declared in wave-decisions.md. PASS. |
| **6 Priority validation** | The DESIGN 9-locus table IS the gap data; the structural guard is the <effort solution (one test, reuses the precedent). No over-build (no behaviour test where no behaviour changes). PASS. |
| **7 Observable-behaviour assertions** | Every Then asserts on the committed file content the reader observes (the observable outcome for a doc feature), not on private runtime state. For a structural-honesty feature the file content IS the observable. PASS. |
| **8 Traceability** | Story-to-scenario: every US-01/02/03 + AC has ≥1 scenario (table). Environment-to-scenario: N/A — no runtime environment matrix (doc feature; DEVOPS inherits the five gates, no new env). PASS. |
| **9 WS boundary proof** | WS strategy declared (structural assertion over committed docs). No driven adapter / real-I/O concern (no runtime adapter exists). The "real I/O" here is the real `std::fs` read of the real committed files — no InMemory double. PASS. |

**Verdict: APPROVED.** Both-directions guards encoded, false strings proven
present today (5 RED fail under `--ignored`), true substrings coordinated with
DELIVER wording latitude, path resolution robust, the prism-already-honest control
and the two anti-over-correction controls are meaningful and green. Mandate 7 RED
not BROKEN confirmed. fmt/clippy clean. Does NOT proceed into DELIVER; does NOT
commit.
