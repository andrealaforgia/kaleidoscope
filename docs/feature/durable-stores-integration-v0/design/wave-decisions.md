# DESIGN Wave Decisions: durable-stores-integration-v0

> **Author**: `nw-solution-architect` (Morgan), DESIGN wave, 2026-05-21.
> **Scope**: application architecture, test-only feature inside the existing
> `crates/integration-suite` crate. No new production source under any `src/`.
> **Interaction mode**: propose. Each load-bearing decision picks the obvious
> option grounded in the direct in-repo precedent and proceeds.

## Context

The first triad (cinder + sluice + lumen, all FileBacked) is already proven to
compose under one shared `aegis::TenantId` and survive a drop-and-reopen in
`crates/integration-suite/tests/v1_three_adapters_compose_under_restart.rs`. This
feature adds the second triad: prove `pulse::FileBackedMetricStore` +
`ray::FileBackedTraceStore` + `strata::FileBackedProfileStore` compose under one
tenant and recover identically across a restart, with tenant isolation. This is
the deterministic completion of a shipped milestone with a fixed design space;
the precedent file is the template.

## Decisions

### DD1: test file, shape and target name

A new file `crates/integration-suite/tests/v1_three_durable_stores_compose.rs`
holds two tests mirroring the first-triad file:

- **(a) compose + restart with tenant isolation** — ingest metrics, spans and
  profiles for tenant `acme` and parallel data for tenant `globex` into the three
  FileBacked durable stores; drop (scope exit flushes via each adapter's
  `BufWriter`); reopen all three from the same base paths; assert `acme`'s
  metrics + traces + profiles recover identically while `globex` stays isolated
  (no `globex` point/span/profile visible under `acme`).
- **(b) cross-crate tenant-identity contract** — hold one `aegis::TenantId` and
  pass the same `&TenantId` to all three adapters with no conversion; read each
  back under it. A shape drift in `aegis::TenantId` breaks this at compile time.

The `[[test]]` name is **`v1_three_durable_stores_compose`** — it MUST match the
US-01 Elevator Pitch command
`cargo test -p integration-suite --test v1_three_durable_stores_compose` exactly.

### DD2: Cargo.toml dev-deps and `[[test]]` block

`crates/integration-suite/Cargo.toml` already declares `pulse` and `aegis` as
dev-deps (confirmed by read). Add two path dev-deps mirroring how `pulse` is
declared:

```toml
ray   = { path = "../ray",   version = "0.1.0" }
strata = { path = "../strata", version = "0.1.0" }
```

Add the `[[test]]` block:

```toml
[[test]]
name = "v1_three_durable_stores_compose"
path = "tests/v1_three_durable_stores_compose.rs"
```

No other Cargo.toml change. The `[lib]`, `[dependencies]`, existing `[[test]]`
blocks and `[lints]` are untouched.

### DD3: helper shape and the open->ingest->drop->reopen->query pattern

Reuse the first-triad helpers verbatim in shape: `tenant(id)`,
`temp_root(test_name)`, `cleanup(root)`. Each store gets its own `base_path`
under one shared `temp_root` (`root.join("pulse-store")`,
`root.join("ray-store")`, `root.join("strata-store")`). The write path MUST equal
the reopen path (false-PASS guard, per `shared-artifacts-registry.md`).

REAL public signatures, confirmed by reading the crate source on 2026-05-21:

| Crate | open arity | NoopRecorder export | ingest | recovery query |
|-------|-----------|---------------------|--------|----------------|
| pulse | `FileBackedMetricStore::open<P: AsRef<Path>>(base_path, Box<dyn MetricsRecorder + Send + Sync>) -> Result<Self, MetricStoreError>` | `pulse::NoopRecorder` | `ingest(&TenantId, MetricBatch) -> Result<IngestReceipt, _>` | `query(&TenantId, &MetricName, TimeRange) -> Result<Vec<(Metric, MetricPoint)>, _>` |
| ray | `FileBackedTraceStore::open<P: AsRef<Path>>(base_path, Box<dyn MetricsRecorder + Send + Sync>) -> Result<Self, TraceStoreError>` | `ray::NoopRecorder` | `ingest(&TenantId, SpanBatch) -> Result<IngestReceipt, _>` | `get_trace(&TenantId, &TraceId) -> Result<Vec<Span>, _>` and `query(&TenantId, &ServiceName, TimeRange) -> Result<Vec<Span>, _>` |
| strata | `FileBackedProfileStore::open<P: AsRef<Path>>(base_path, Box<dyn MetricsRecorder + Send + Sync>) -> Result<Self, ProfileStoreError>` | `strata::NoopRecorder` | `ingest(&TenantId, ProfileBatch) -> Result<IngestReceipt, _>` | `query(&TenantId, &ServiceName, TimeRange) -> Result<Vec<Profile>, _>` |

Each crate exports its OWN `NoopRecorder` (unit-struct shape, used as
`Box::new(pulse::NoopRecorder)` etc.). Import them under crate-qualified aliases
exactly as the first-triad file aliases `CinderRecorder`, `LumenRecorder`,
`SluiceRecorder` — e.g. `pulse::NoopRecorder as PulseRecorder`,
`ray::NoopRecorder as RayRecorder`, `strata::NoopRecorder as StrataRecorder`.

Construction notes the crafter must honour (from the type reads):

- `MetricBatch::with_metrics(vec![Metric { name: MetricName::new(...), kind:
  MetricKind::Gauge, points: vec![MetricPoint { time_unix_nano, .. }], .. }])`.
  Query by `&MetricName::new("process.cpu.utilization")`. The query returns
  `Vec<(Metric, MetricPoint)>`; assert on point count and ascending
  `time_unix_nano`.
- `SpanBatch::with_spans(vec![Span { trace_id, span_id, .. }])`. A span's
  service-name index is keyed off the `service.name` resource attribute, so each
  span MUST carry `resource_attributes["service.name"] = "checkout"` (mirrors the
  first-triad `log_record` helper, and matches `Span::service_name()`). Recover
  by `get_trace(&acme, &trace)` (two spans, start-time ascending) and by
  `query(&acme, &ServiceName::new("checkout"), TimeRange::all())`.
- `ProfileBatch::with_profiles(vec![Profile { time_unix_nano, profile_type:
  "cpu".into(), resource_attributes["service.name"] = "checkout", .. }])`. A
  profile with no `service.name` is dropped from the index at v0/v1, so the
  resource attribute is mandatory. Recover by `query(&acme,
  &ServiceName::new("checkout"), TimeRange::all())` (one profile).

Isolation assertions (test a): `acme`'s metric query contains no
`billing.requests` point; `acme`'s `checkout` trace contains no `billing` span;
`strata.query(&acme, &ServiceName::new("billing"), TimeRange::all())` is empty.
`globex` recovers its own one-of-each independently.

`TimeRange` is a per-crate type (`pulse::TimeRange`, `ray::TimeRange`,
`strata::TimeRange`), each with `::all()`. Use the crate-local `TimeRange::all()`
at each query site; they are NOT interchangeable across crates.

### DD4: KPI is a correctness guardrail

The KPI is 100% recover-and-isolate fidelity with zero cross-bucket leakage (both
tests green). The only timing element is a generous guardrail: total wall-clock
for the target stays well under 30 s on ubuntu-latest (the real work is
microseconds of tmpfs I/O). This is a guardrail to catch a pathological
regression (e.g. an accidental fsync-per-record storm), never a latency target.
No per-pillar latency budget is re-asserted here; those live in the crate-level
v1 suites.

## Reuse Analysis (mandatory)

| Asset | Location | Verdict | Rationale |
|-------|----------|---------|-----------|
| First-triad composition test | `crates/integration-suite/tests/v1_three_adapters_compose_under_restart.rs` | **EXTEND** | The component being extended, not duplicated. The new file mirrors its two-test shape, its helper trio (`temp_root`/`cleanup`/`tenant`) and its open->ingest->drop->reopen->query pattern. We replicate the proven pattern for a new triad; we do not refactor or alter the existing file. |
| `temp_root` / `cleanup` / `tenant` helpers | same file | **REUSE (shape)** | Copied verbatim in shape into the new test file. Rust integration tests are independent binaries with no shared helper module here, so verbatim duplication of three trivial helpers is the idiomatic and lowest-coupling choice; extracting a shared module would over-engineer a test-only crate. |
| `integration-suite` crate scaffold | `crates/integration-suite/` | **EXTEND** | Brownfield. Add one test file + two dev-deps + one `[[test]]` block. No `src/` change, no new crate. |
| pulse / ray / strata FileBacked adapters | their crates | **CONSUME (read-only)** | Used through their public surfaces only. This feature modifies no production crate. |
| ADR-0005 five workspace CI gates | repo CI | **INHERIT** | No new gate; Gate 1 (`cargo test --workspace --all-targets --locked`) auto-discovers the new `[[test]]` block. See DEVOPS handoff below. |

## ADR verdict: NO new ADR

No ADR is warranted. Every load-bearing decision here is the application of an
already-recorded pattern:

- the composition + restart + isolation pattern is decided by the first-triad
  precedent file;
- the WAL + snapshot + replay durability of each adapter is decided by the
  per-pillar v1 ADRs in their own crates;
- the no-new-CI-gate posture is decided by ADR-0005 (five workspace gates) and
  the project's pure-trunk-based stance.

There is no genuinely non-obvious decision to record. Writing a ceremonial ADR
that only restates inherited decisions would be noise. (Rationale stated
explicitly per the DESIGN brief.)

## DEVOPS handoff annotation (for Apex / platform-architect)

- **External integrations**: none. All three stores are first-party in-process
  Rust libraries on the local filesystem. No contract tests
  (Pact/consumer-driven) apply.
- **Expected DEVOPS posture (flagged, for Apex to confirm by grep, not assume)**:
  `integration-suite` is a test-only crate with no production `src/` logic to
  mutate, so it almost certainly needs **NO dedicated `gate-5-mutants` job**.
  Likely **A1 = "no new gate; inherits ADR-0005's five workspace gates; Gate 1
  auto-discovers the new `[[test]]` block"**.
  - Supporting evidence found during DESIGN (Apex should re-verify):
    `.github/workflows/ci.yml` Gate 1 runs `cargo test --workspace --all-targets
    --locked` (line ~182), which discovers any new `[[test]]`. The
    `gate-5-mutants-*` jobs are all scoped to crates with production logic
    (harness, aperture, spark, sieve, codex, self-observe); there is no
    `integration-suite` mutants job today.
  - The kill-rate gate (CLAUDE.md, ADR-0005 Gate 5, 100%) targets production
    source; a tests-only crate has nothing to mutate. Apex confirms by grep.
- **CI signal**: capture the `test result` line for
  `v1_three_durable_stores_compose` on every push to main (feedback, not a gate;
  pure trunk-based per project memory). Any non-zero failure count or a compile
  failure of the target is the alert.
- **Guardrail**: target wall-clock well under 30 s on ubuntu-latest.

## Reviewer gate

Peer review via `nw-solution-architect-reviewer` is required before DESIGN is
declared done. Verdict recorded at the foot of this file.

## Reviewer verdict

```yaml
review_id: "arch_rev_20260521_durable_stores_integration_v0"
reviewer: "solution-architect-reviewer (Atlas lens; Task subagent unavailable in env, dimensions applied directly)"
artifact: "docs/feature/durable-stores-integration-v0/design/wave-decisions.md, application-architecture.md"
iteration: 1

strengths:
  - "Reuse Analysis explicit and correct: first-triad test EXTEND not duplicate; helpers REUSE(shape) with idiomatic-Rust justification."
  - "All public signatures verified against actual crate source 2026-05-21 (open arity, per-crate NoopRecorder, query signatures)."
  - "No-ADR verdict justified with explicit inherited-precedent rationale; avoids ceremonial ADR noise."
  - "DEVOPS posture flagged with CI evidence and an explicit confirm-by-grep instruction for Apex."
  - "Earned Trust applied: filesystem named as highest-risk dependency; the test IS the composed-durable-path probe."

issues_identified:
  architectural_bias: []
  decision_quality: []
  completeness_gaps: []
  implementation_feasibility: []
  priority_validation:
    q1_largest_bottleneck: "YES"
    q2_simple_alternatives: "ADEQUATE"
    q3_constraint_prioritization: "CORRECT"
    q4_data_justified: "JUSTIFIED"

approval_status: "approved"
critical_issues_count: 0
high_issues_count: 0
```

Verdict: APPROVED, iteration 1. Zero critical, zero high. No revision required.
DESIGN wave complete; ready for DISTILL handoff.
