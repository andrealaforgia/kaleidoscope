# Outcome KPIs — `otlp-conformance-harness-v0`

> Per the project's house style, effort is wall-clock and conceptual, never human-engineer-months. Targets in this document use rates and ratios; counts appear only where the count is itself the contract (e.g. number of signal types covered).
>
> The harness is built by AI agents, not human engineers. The KPIs measure the *contract the harness exposes to its consumers*, not the productivity of whoever produces it.

---

## Feature: OTLP conformance harness v0

### Objective

Lock in the OTLP wire contract for Kaleidoscope's Phase 0 in a form that every Phase-1+ component can depend on without rediscovering the spec, and that third-party OTel implementers can use to verify their own emitters.

### Outcome KPIs

| # | Who                                                | Does What                                                                                  | By How Much                                  | Baseline                  | Measured By                                                              | Type     |
|---|----------------------------------------------------|---------------------------------------------------------------------------------------------|----------------------------------------------|---------------------------|---------------------------------------------------------------------------|----------|
| 1 | The harness, against its own accept-path corpus    | Wrongly rejects bytes that conform to the OTLP wire spec                                   | 0 cases — enforced as a CI invariant         | n/a (greenfield)           | Corpus runner: every vector under `tests/vectors/*/accept/` returns Ok    | Leading  |
| 2 | The harness, against its own reject-path corpus    | Surfaces the named violation rule expected by the corpus descriptor                         | 100% of reject vectors                       | n/a (greenfield)           | Corpus runner: `OtlpViolation::rule` equals `expected.json/rule`          | Leading  |
| 3 | The harness, across the OTLP stable signal types   | Validates the signal type end-to-end (decode + asserted-type check + accept-or-reject)      | 3 of 3 currently in the OTLP stable spec     | 0 of 3 (greenfield)        | Slice 04, 05, 06 tests pass; signal-coverage table in README               | Leading  |
| 4 | Kaleidoscope CI, on every commit touching the crate| Runs the corpus and refuses to merge on any verdict change without a corresponding rule diff | Every commit                                 | n/a (greenfield)           | CI workflow runs `cargo test -p otlp-conformance-harness --all-targets`   | Leading  |
| 5 | Third-party observability engineers running the harness | Use the harness to verify their OTLP emitter without reading the OTLP specification themselves | At least one external user, post-Phase-0 | 0 (greenfield) | GitHub issue/discussion threads referencing the crate                | Lagging  |
| 6 | The harness's reference corpus                     | Defends each violation rule with at least one reject vector                                 | At least 1 reject vector per rule (3 rules in v0) | 0 (greenfield)         | Static count in `tests/vectors/*/reject/` enumerated in `corpus.rs`        | Leading  |
| 7 | The harness, on a typical-sized OTLP record        | Validates a logs/traces/metrics export request                                              | p99 latency tracked but not blocking for v0  | n/a (no profiling data)    | Criterion benchmark added in slice 07; result recorded, no SLA enforced     | Guardrail (informational) |

### Metric Hierarchy

- **North Star**: KPI 1 — false-positive rate of 0% on the accept-path corpus. The whole point of the harness is that conforming bytes are not rejected. If the harness ever rejects a record that the OpenTelemetry SDK considers valid, the contract is broken and downstream consumers stop trusting it.
- **Leading indicators**:
  - KPI 2 (reject rules surface as expected) — predicts that consumers can pattern-match on violations without regret.
  - KPI 3 (signal-type breadth) — predicts the harness's usefulness across the full OTLP surface area Kaleidoscope cares about.
  - KPI 4 (CI gating) — predicts that KPI 1 stays at 0% over time.
  - KPI 6 (reject-vector coverage per rule) — predicts that every rule the harness exposes is defended by a vector, not just claimed in a comment.
- **Guardrail metric**:
  - KPI 7 (validation latency) — must not silently degrade to the point that a Phase-1 Aperture cannot embed the harness on the hot path. Tracked, not enforced, until profiling data exists.
- **Lagging adoption signal**:
  - KPI 5 (third-party use) — observed not engineered. The harness's value to third parties is a signal, not a target.

### Measurement Plan

| KPI | Data source                                                                | Collection method                                              | Frequency                | Owner                          |
|-----|----------------------------------------------------------------------------|----------------------------------------------------------------|--------------------------|--------------------------------|
| 1   | `tests/vectors/*/accept/` directory                                        | Corpus runner asserts `Result::is_ok` for each vector          | Every CI run             | Harness crate maintainer        |
| 2   | `tests/vectors/*/reject/` directory + `.expected.json` siblings            | Corpus runner asserts `OtlpViolation::rule` matches            | Every CI run             | Harness crate maintainer        |
| 3   | Slice 04, 05, 06 tests + a signal-coverage table in the crate README       | Tests pass; README table updated when signals are added         | Per release              | Harness crate maintainer        |
| 4   | CI workflow (e.g. GitHub Actions, configured in DEVOPS wave)               | Workflow run on every commit affecting the crate                | Every commit             | DEVOPS wave (`platform-architect`) |
| 5   | GitHub issues / discussions / referenced repositories                      | Manual sweep at quarterly review                                 | Quarterly                | Project maintainers              |
| 6   | Static enumeration in `corpus.rs`                                          | Compile-time check that each `Rule` variant is referenced       | Every build              | Harness crate maintainer        |
| 7   | Criterion benchmark output, written to `target/criterion/`                 | `cargo bench` baseline recorded; no CI gate                     | On demand; recorded only | Harness crate maintainer        |

### Hypothesis

We believe that a small, focused conformance harness consuming `opentelemetry-proto` and exposing a closed set of named violation rules will let Aperture, Codex, every storage engine, and third-party OTel implementers validate OTLP byte sequences with zero false positives on conforming inputs and a stable, machine-readable shape on rejections.

We will know this is true when:
- KPI 1 holds 0% across the lifetime of the crate.
- KPI 4 prevents any commit from breaking KPI 1 silently.
- KPI 3 reaches 3 of 3 signals before Phase 1 begins.

### Baseline note

Every KPI baseline is "n/a (greenfield)" because the project has no prior code. The baselines are established the first time the corresponding slice ships and the corresponding test passes for the first time.

### Effort framing

Per house style, no slice has a person-days estimate. Slice 01 ships in well under a day of wall-clock and is conceptually trivial. Slices 02–06 are low-difficulty, no integration surface beyond the upstream `opentelemetry-proto` crate. Slice 07 is the only slice that touches CI infrastructure; it is a half-day's work because the workflow is a few lines of YAML and the corpus runner is a single Rust file walking a directory.

### Handoff signals to DEVOPS

The DEVOPS wave (`platform-architect`) will need:
1. **Data collection requirement**: the CI workflow that runs the corpus must record verdict counts per signal and per rule per build. This belongs in the workflow's artifact upload step.
2. **Dashboard/monitoring need**: KPI 1 (false-positive rate) is the only metric that *must* trip a build; the others are informational. No real-time dashboard is required for v0 because the metric is build-time, not run-time.
3. **Alerting threshold**: any non-zero false-positive count fails the build. There is no degraded-but-acceptable threshold.
4. **Baseline measurement**: the corpus's first commit establishes the baseline; subsequent commits compare against it.
