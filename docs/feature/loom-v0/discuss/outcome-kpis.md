# Loom v0 — outcome KPIs

Four outcome KPIs grounded in the user stories. Each has a numeric
target, a measurement plan, and a slice anchor. Convention follows
the Aperture / Sieve / Codex / Prism / Beacon feature pattern.

---

## KPI 1 — Feedback latency

**What it measures**: the wall-clock from invoking
`loom validate --rules <dir>` to the process exit. The pre-commit
hook depends on this being fast enough that operators do not
disable it.

**Target**: ≤ 100 ms p95 on a 50-rule corpus.

**How measured**: Acceptance test
`tests/slice_01_validate.rs` writes 50 rule files to a temp dir,
runs `loom::validate(dir)` in a loop of 100 invocations, asserts
the p95 wall-clock is ≤ 100 ms.

**Slice anchor**: US-LO-01.

---

## KPI 2 — Plan determinism

**What it measures**: whether `loom plan` produces byte-equal
output across successive invocations on the same inputs.

**Target**: 100% byte-equality across 100 invocations on a fixed
50-rule corpus.

**Why**: Loom's primary value is auditable change control. A
non-deterministic plan output defeats PR review because two
reviewers might see different diffs.

**How measured**: Acceptance test
`tests/slice_02_plan.rs` runs `loom::plan(from, to)` 100 times
on the same inputs, hashes each output, asserts all hashes
match.

**Slice anchor**: US-LO-02.

---

## KPI 3 — Apply idempotency

**What it measures**: whether `loom apply` becomes a no-op when
called twice on the same inputs.

**Target**: zero file writes on the second invocation; exit code
0; output reports `0 added, 0 removed, 0 changed`.

**Why**: the operator's mental model is "git push → deployment
follows". An idempotent applier means a stale CI pipeline that
re-runs apply does not churn the deployment.

**How measured**: Acceptance test
`tests/slice_03_apply.rs` runs `loom::apply(from, to)`, captures
the destination directory's file mtimes, runs apply again,
asserts every mtime is unchanged.

**Slice anchor**: US-LO-03.

---

## KPI 4 — Operator-readable diagnostics

**What it measures**: whether `loom validate` diagnostics are
parseable by standard Unix tools (grep, awk).

**Target**: 100% of diagnostics match the regex
`^.+:[0-9]+: <message>` so CI can `grep -E` for them.

**Why**: tooling integration is part of Loom's value. CI engineers
should not have to escape JSON to surface diagnostics in PR
comments.

**How measured**: Acceptance test
`tests/slice_04_diagnostics.rs` constructs 5 broken rule files in
representative ways (typo, missing field, type mismatch, bad
duration, broken sink shape), runs `loom::validate`, asserts
every diagnostic line matches the regex above.

**Slice anchor**: US-LO-04.

---

## Cross-KPI guardrails

| Guardrail | Threshold | Rationale |
|---|---|---|
| Binary size | ≤ 8 MB release | Loom is a CLI; large binaries slow CI clone + cache. |
| No telemetry-on-telemetry | 0 third-party endpoints | Per architecture doc §A.2; absence of `phone-home` in the codebase is the test. |
| AGPL licence-header coverage | 100% of `.rs` files | Same posture as every prior feature. |
| `loom apply` writes only `.toml` | enforced by code | Loom must not touch files outside its scope (binaries, scripts, etc. that may sit in the deployment dir). |
