# KPI Instrumentation — `otlp-conformance-harness-v0` (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-05-03.
> **Author**: Apex.
> **Source of truth for KPIs**:
> `docs/feature/otlp-conformance-harness-v0/discuss/outcome-kpis.md`.
> **CI workflow**: `.github/workflows/ci.yml`.

---

## Framing

The harness has no runtime, so the seven outcome KPIs are **build-time
metrics**, not runtime metrics. There is no Prometheus scrape, no
StatsD emit, no OTel collector. Every metric below is computed from
one of three data sources:

1. **CI workflow output** — the `gate-1-test` job's structured output
   plus the `verdict-counts.json` artefact.
2. **Repository state at HEAD** — the corpus directory, the harness's
   `Rule` enumeration, the README's signal-coverage table.
3. **External signal** — GitHub issues, discussions, and inbound
   references from third-party repositories.

This document specifies, for each of the seven KPIs, the data source,
the collection mechanism, the storage location, the reading cadence,
and the alerting (if any). North-star and guardrail markings come
from `outcome-kpis.md`'s metric hierarchy; they are not relitigated
here.

## Per-KPI specification

### KPI 1 — North star: false-positive rate on the accept-path corpus

> "The harness, against its own accept-path corpus, wrongly rejects
> bytes that conform to the OTLP wire spec — 0 cases enforced as a CI
> invariant."

| Aspect | Specification |
|---|---|
| Data source | `crates/otlp-conformance-harness/tests/vectors/{logs,traces,metrics}/accept/*.bin`, walked by the corpus runner in slice 07. |
| Collection | The corpus runner asserts `Result::is_ok` for every accept vector. Any failure produces a panic; the panic propagates through `cargo test` and fails Gate 1. |
| Storage | The pass/fail bit lives in the GitHub Actions run history, queryable via the GitHub API or visible in the Actions UI. |
| Reading cadence | Every CI run; reviewed at PR-merge time. |
| Alert threshold | **Any non-zero false-positive count fails the build.** No degraded-but-acceptable threshold. |
| Owner | The harness crate maintainer. |

This KPI is the contract: the gate is binary, the threshold is zero,
the alert mechanism is the CI run going red.

### KPI 2 — Leading: reject-rule fidelity

> "The harness, against its own reject-path corpus, surfaces the named
> violation rule expected by the corpus descriptor — 100 % of reject
> vectors."

| Aspect | Specification |
|---|---|
| Data source | `crates/otlp-conformance-harness/tests/vectors/{logs,traces,metrics}/reject/*.bin` plus the sibling `*.expected.json` descriptors. |
| Collection | The corpus runner asserts `OtlpViolation::rule` matches `expected.rule` for every reject vector. |
| Storage | Same as KPI 1: GitHub Actions run history. |
| Reading cadence | Every CI run. |
| Alert threshold | Any mismatched rule fails the build. |
| Owner | The harness crate maintainer. |

The `verdict-counts.json` artefact (Gate 1) reports the per-rule count
of reject vectors so a quarterly review can scan for rule drift, but
the build-blocking signal is the per-vector assertion in the corpus
runner.

### KPI 3 — Leading: signal-type breadth

> "The harness, across the OTLP stable signal types, validates the
> signal type end-to-end — 3 of 3 currently in the OTLP stable spec."

| Aspect | Specification |
|---|---|
| Data source | (a) The slice 04, 05, 06 acceptance tests in `crates/otlp-conformance-harness/tests/`; (b) the signal-coverage table in `crates/otlp-conformance-harness/README.md`. |
| Collection | (a) Test pass/fail; (b) human-edited table refreshed each release. |
| Storage | (a) GitHub Actions run history; (b) committed README. |
| Reading cadence | Per release (not per commit) for the README; per CI run for the test pass/fail. |
| Alert threshold | Any of the three slice-04/05/06 test files failing fails the build. The README table drift is reviewed at release time, not gated. |
| Owner | The harness crate maintainer. |

### KPI 4 — Leading: CI gating on every commit

> "Kaleidoscope CI, on every commit touching the crate, runs the
> corpus and refuses to merge on any verdict change without a
> corresponding rule diff — every commit."

| Aspect | Specification |
|---|---|
| Data source | The `verdict-counts.json` artefact produced by the Gate 1 KPI capture step. |
| Collection | A small Python step in `gate-1-test` walks `tests/vectors/`, parses the `*.expected.json` descriptors, and writes a JSON document with per-signal accept/reject counts and per-rule reject-vector counts. |
| Storage | GitHub Actions artefact named `verdict-counts`, retained for **90 days**. |
| Reading cadence | Quarterly review (the 90-day retention covers a full quarter), and on demand when a maintainer wants to check rule-coverage drift. |
| Alert threshold | None at the artefact level (the build-blocking signal is the underlying per-vector assertions in Gate 1). The artefact is informational. |
| Owner | DEVOPS (this wave) plus the harness crate maintainer. |

The `verdict-counts.json` schema (v1):

```json
{
  "schema_version": 1,
  "generated_at_utc": "2026-05-03T12:34:56Z",
  "commit_sha": "abcd...",
  "workflow_run_id": "1234567890",
  "per_signal": {
    "logs":    { "accept": 1, "reject": 6 },
    "traces":  { "accept": 1, "reject": 3 },
    "metrics": { "accept": 1, "reject": 3 }
  },
  "per_rule": {
    "EmptyInput":      { "reject_vectors": 3, "signals": { "logs": 1, "traces": 1, "metrics": 1 } },
    "ProtobufDecode":  { "reject_vectors": 3, "signals": { "logs": 3 } },
    "SignalMismatch":  { "reject_vectors": 6, "signals": { "logs": 2, "traces": 2, "metrics": 2 } }
  },
  "totals": { "accept": 3, "reject": 12 }
}
```

The numbers in the example match the v0 corpus (17 vectors split per
ADR-0001's recommended layout: 3 accept, 14 reject after Crafty's
corpus shipped). The runtime computation derives all fields from the
filesystem and the descriptors; the artefact is regenerated every CI
run and overwrites no historical record (each run gets a fresh
artefact in its own retention window).

The schema is intentionally simple and self-describing
(`schema_version` field) so a future iteration that adds, e.g., a
verdict-by-verdict latency tally can extend the document without
breaking existing readers.

### KPI 5 — Lagging adoption signal

> "Third-party observability engineers running the harness use it to
> verify their OTLP emitter without reading the OTLP specification —
> at least one external user, post-Phase-0."

| Aspect | Specification |
|---|---|
| Data source | (a) GitHub issues filed against `andrealaforgia/kaleidoscope` whose body or title mentions `otlp-conformance-harness`; (b) GitHub Discussions threads in the same repository; (c) inbound references from third-party repositories listing the harness in their `Cargo.toml`. |
| Collection | Manual sweep at quarterly review. The maintainer runs `gh issue list --search 'otlp-conformance-harness in:title,body'`, `gh discussion list --search 'otlp-conformance-harness'`, and `cargo crev verify` (or a manual GitHub code-search) for inbound `Cargo.toml` references. |
| Storage | A short-form quarterly note in `docs/evolution/<yyyy-qN>-otlp-harness-adoption.md` (file does not exist yet; the first quarterly review creates it). |
| Reading cadence | Quarterly. |
| Alert threshold | None. KPI 5 is observed, not engineered. |
| Owner | Project maintainers. |

This KPI is deliberately not automated: the signal is scarce and
qualitative ("did anyone say this was useful?"), and a per-CI-run
data feed would produce noise without insight. A quarterly manual
sweep is appropriate.

### KPI 6 — Leading: corpus rule-coverage

> "The harness's reference corpus defends each violation rule with at
> least one reject vector — at least 1 reject vector per rule (3 rules
> in v0)."

| Aspect | Specification |
|---|---|
| Data source | (a) The `Rule` enumeration in `crates/otlp-conformance-harness/src/violation.rs`; (b) the `corpus.rs` integration test's `every_rule_variant_has_at_least_one_defending_reject_vector` test; (c) the `verdict-counts.json` artefact's `per_rule` map. |
| Collection | The `corpus.rs` test enumerates `Rule` variants reflectively (via the closed enumeration the maintainer updates in lockstep) and asserts each has at least one reject vector. |
| Storage | (a)–(b) committed source; (c) GitHub Actions artefact, 90-day retention. |
| Reading cadence | Every CI run. |
| Alert threshold | Any rule variant with zero reject vectors fails Gate 1. |
| Owner | The harness crate maintainer. |

The artefact's `per_rule` map gives a quick visual ("how many vectors
defend each rule?") for review; the gate is the test, which is
binary.

### KPI 7 — Guardrail (informational): validation latency

> "The harness, on a typical-sized OTLP record, validates a
> logs/traces/metrics export request — p99 latency tracked but not
> blocking for v0."

| Aspect | Specification |
|---|---|
| Data source | Criterion benchmark output under `target/criterion/` produced by the slice-07 benchmark. |
| Collection | `cargo bench -p otlp-conformance-harness` on demand (not in the regular CI run). |
| Storage | Local `target/criterion/` directories; uploaded as a non-required artefact when the benchmark step is enabled. |
| Reading cadence | On demand. v0 has no scheduled cadence. |
| Alert threshold | None. KPI 7 is informational; no SLA is enforced. |
| Owner | The harness crate maintainer. |

The benchmark is out of the v0 default CI run because (a) the
maintainer has not yet established a baseline, (b) `criterion`
benchmarks need stable runner-side wall-clock to be meaningful, and
(c) the v0 outcome KPIs document explicitly marks KPI 7 as not
blocking. A future iteration may add a non-required `cargo bench`
job to the workflow that uploads `target/criterion/` as a long-term
trend artefact.

## Summary table

| KPI | Type | Data source category | Build-blocking? | Cadence | Storage |
|---|---|---|---|---|---|
| 1 | North star | CI test outcome | Yes | Every commit | GitHub Actions run history |
| 2 | Leading | CI test outcome | Yes | Every commit | GitHub Actions run history |
| 3 | Leading | CI test outcome + README | Yes (tests); No (README) | Every commit (tests); per release (README) | Run history; committed source |
| 4 | Leading | CI artefact | No (artefact); Yes (underlying tests) | Every commit | Workflow artefact (90 days) |
| 5 | Lagging adoption | External | No | Quarterly | `docs/evolution/<yyyy-qN>-otlp-harness-adoption.md` |
| 6 | Leading | CI test outcome + artefact | Yes | Every commit | Run history; artefact |
| 7 | Guardrail | On-demand bench | No | On demand | `target/criterion/` |

Five of seven KPIs are CI-output-driven. One (KPI 5) is external.
One (KPI 7) is on-demand. None are runtime-driven, by design.

## Dashboards

There are no dashboards. The metrics are build-time, observed once
per CI run, and read by humans at PR-merge time and quarterly review
time. Building a Grafana dashboard for "did the build pass?" is
ceremony without value; the GitHub Actions UI already provides the
view.

If a future iteration introduces runtime telemetry — which it cannot,
per the no-telemetry-on-telemetry commitment — dashboards would
re-enter the picture. For v0, the dashboard surface is empty by
design.

## Alerting

Three alerts only:

1. **Build failure on `main`** — GitHub Actions sends an email to
   the commit author (default behaviour, no configuration needed).
   The author's response is to revert or fix forward; per
   `branching-strategy.md` the time-to-restore target is < 1 hour.
2. **Build failure on a PR** — the PR's "checks" UI goes red; the
   author iterates until green.
3. **Quarterly review trigger** — calendar-driven, not automated.
   The maintainer adds a recurring calendar reminder; the project
   does not own a cron.

No PagerDuty, no Slack webhook, no email aggregator. The signal
volume is far too low to need any of that.

## Forward-compatibility

The KPI instrumentation in this wave is sized for v0 (a single Phase-0
crate, no runtime). When the next Phase-0 crate (Codex) lands, its
DEVOPS wave will:

- Inherit the verdict-counts artefact pattern and add Codex-specific
  fields to a per-Codex artefact.
- Add a per-crate KPI-instrumentation document under
  `docs/feature/<codex-feature-id>/devops/kpi-instrumentation.md`.
- Reuse this document's overall shape verbatim.

When the project reaches a runtime-bearing phase (Aperture, Phase 1),
the dashboards-and-alerting concern will re-emerge and a separate
runtime-telemetry KPI document will accompany the harness's
build-time one. That is a future wave's problem.
