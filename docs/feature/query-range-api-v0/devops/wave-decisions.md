# query-range-api-v0 - DEVOPS wave decisions

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-21
- **Wave**: DEVOPS
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default; pure
  trunk-based, no required-status-checks, admins not enforced, per
  memory `project_kaleidoscope_pure_trunk_based`)
- **Predecessor handoff**: `design/wave-decisions.md` DEVOPS handoff
  annotation; `design/application-architecture.md`; ADR-0042;
  `discuss/outcome-kpis.md` (KPI-1..KPI-4)
- **Direct precedent**: `docs/feature/aperture-storage-sink-v0/devops/`
  and `docs/feature/strata-v1/devops/` - each added a single new
  `gate-5-mutants-<crate>` job, byte-mirrored from
  `gate-5-mutants-self-observe`. This wave repeats that move for the
  net-new `query-api` crate. The query-api operational shape (a new HTTP
  deployable) follows the gateway's operator-side runtime shape in
  `aperture-storage-sink-v0/devops/environments.yaml`.

## Posture

`query-range-api-v0` inherits the five-gate workspace CI contract from
ADR-0005. Gates 1-4 carry forward; Gate 5 gains ONE new job for the
net-new crate. One new crate arrives in DELIVER (it does not exist
today): `query-api`, a library crate carrying the minimal PromQL
selector parser and the Pulse-to-Prometheus-matrix translation, plus a
thin `[[bin]]` composition root that opens the Pulse store read-only,
probes, and binds an axum listener for `GET /api/v1/query_range`. This
is therefore inheritance for Gates 1-4 plus exactly one new Gate 5 job.

## A1 - NEW `gate-5-mutants-query-api` job (the one real change)

**Verdict: ADD a new per-package Gate 5 job, `gate-5-mutants-query-api`.
The thin query-api binary needs NO separate Gate 5 job (the lib carries
all the mutable logic; the binary is composition / wiring).**

Grep verified, 2026-05-21:
`grep -c "gate-5-mutants-query-api" .github/workflows/ci.yml` returns 0.
The crate is not mutation-gated because it does not exist yet; Crafty
creates it in DELIVER.

### Why the query-api crate gets a gate

`query-api` carries the feature's net-new mutable logic, all of it in
the lib (DD1):

- the `selector` module's bare-metric-name parser - the executable v0
  PromQL boundary: accept `[a-zA-Z_:][a-zA-Z0-9_:]*` after trimming,
  reject everything else (empty, `{`-matcher, `[`-range-vector,
  `(`-function, operators, whitespace-separated tokens) as HTTP 400
  (DD3, ADR-0042 Decision 3);
- the `matrix` module's Pulse-rows-to-Prometheus-matrix translation:
  the label-set merge rule (`__name__` + resource_attributes +
  point.attributes, point wins on collision, DD4a), the time conversion
  (`time_unix_nano / 1_000_000_000` to integer seconds, DD4b), and the
  value conversion (`f64` to minimal-decimal string, `NaN` to `"NaN"`,
  `0.0` to `"0"`, DD4c);
- the half-open `[start, end)` range conversion and the bounds /
  numeric validation that drive the error arms (DD5, DD6);
- the fail-closed tenant resolution in the `TenantResolver` port (DD7).

This is exactly the dense branching logic mutation testing exists to
police: a flipped collision-precedence in the label merge, an off-by-one
in the half-open bound, a dropped `NaN` branch, or a loosened selector
regex are correctness regressions a surviving mutant would silently
introduce. With zero mutation coverage this would be the largest
mutation-coverage gap in the workspace. The per-feature MT strategy in
`CLAUDE.md` (100% kill rate, scoped to modified files, ADR-0005 Gate 5)
cannot be honoured without a job to run it.

### Why the binary gets NO separate gate

The query-api `[[bin]]` is composition / wiring, not logic (DD1, DD8,
DD9). Its body is: resolve the tenant from `KALEIDOSCOPE_QUERY_TENANT`
(fail-closed), open the Pulse store read-only at `pillar_root/pulse`,
run the startup `probe()`, then bind the axum listener and serve. Every
testable behaviour lives elsewhere: the parser and translation are in
the lib (gated by A1); the probe contract is enforced by the lib's
subtype boundary, an AST structural check, and a behavioural gold-test
against a store that lies (open succeeds, read fails) (DD9, ADR-0042
Verification); the `axum::serve` seam is the same one aperture already
owns. A mutant in pure wiring either fails to compile or is caught by
the integration test that boots the binary; there is no branching
translation logic in the binary for a mutation to corrupt undetectably.
A gate over it would gate code that has nothing for mutation testing to
find. **Decision: no `gate-5-mutants-query-api-bin`.** If DELIVER lands
real branching logic in the binary, that is the signal to revisit;
record it as a post-merge correction.

### Why mirror self-observe specifically

`gate-5-mutants-self-observe` (`ci.yml:862-947`) is the canonical
per-package Gate 5 template and the byte source the pulse, ray, strata,
and aperture-storage-sink jobs were all mirrored from. It encodes the
current-best baseline cascade (`origin/main -> HEAD~1 -> full`), the
empty-diff short-circuit, the precompiled-binary install, and 30-day
artefact retention. Mirroring it inherits the latest conventions with
zero drift.

### The six substitutions (and ONLY these six)

The new job is `gate-5-mutants-self-observe` copied verbatim with
exactly six string substitutions. Everything else - `runs-on`,
`needs: [gate-2-public-api, gate-3-semver]`, `timeout-minutes: 30`, the
checkout/toolchain/cache/install step shapes, the baseline cascade,
`--in-diff`, `--no-shuffle --jobs 2`, the 30-day retention - is
byte-for-byte identical.

| # | Field | self-observe value | query-api value |
|---|-------|--------------------|-----------------|
| 1 | job key | `gate-5-mutants-self-observe` | `gate-5-mutants-query-api` |
| 2 | step `name` | `Gate 5 — cargo mutants (self-observe)` | `Gate 5 — cargo mutants (query-api)` |
| 3 | `--in-diff` path filter | `crates/self-observe/**` | `crates/query-api/**` |
| 4 | `--package` arg | `--package self-observe` | `--package query-api` |
| 5 | cache key suffix | `...-cargo-mutants-self-observe-...` | `...-cargo-mutants-query-api-...` |
| 6 | artefact name | `mutants-out-self-observe` | `mutants-out-query-api` |

The cache-step display name and the cache `restore-keys` prefix follow
substitution 5 mechanically. The diff-echo log strings and the step
comment naming the crate follow substitutions 3/4 mechanically. These
are cosmetic consequences of the six, not additional changes. The
`--in-diff` baseline cascade is preserved verbatim. The full
byte-for-byte YAML snippet is in `ci-cd-pipeline.md` for Crafty to
copy-paste.

### Landing discipline

This DEVOPS wave does **NOT** edit `ci.yml`. `@nw-software-crafter`
(Crafty) lands the new job atomic with the crate's source in the DELIVER
commit, so the job and the code it gates arrive together and the first
CI run on the implementation commit exercises the new gate. Insert it
adjacent to the other Gate 5 jobs (e.g. after
`gate-5-mutants-kaleidoscope-cli`, the current last Gate 5 job).

## A2 - Gate 1 auto-discovers the new crate's tests

Gate 1 (`cargo test --workspace --all-targets --locked`) carries forward
UNCHANGED. The DELIVER commit adds `query-api` as a workspace member and
adds the per-slice acceptance `[[test]]` blocks to
`crates/query-api/Cargo.toml` (US-01 serve matrix, US-02 calm empty,
US-03 reject unparseable, US-04 fail-closed tenancy, US-05 hold scope
boundary). `--workspace --all-targets` discovers these automatically;
the workflow invocation needs no edit. The consumer-driven contract test
(the four pinned response shapes - success, empty, parse-error, 5xx -
asserted against Prism's own `isPromSuccess` / `isPromError` validators,
ADR-0042 / DESIGN handoff) runs under Gate 1 and IS the measurement of
KPI-1. The E2E ingest-to-query-to-render test (KPI-2) and the timed
latency test (KPI-3) also run under Gate 1, as does the behavioural
probe gold-test (store-that-lies -> `health.startup.refused`, DD9). See
`kpi-instrumentation.md`.

## A3 - Dependencies: zero new external crates in the workspace graph

The new crate depends on existing in-tree workspace crates and on
external crates already resolved in `Cargo.lock`. I verified the root
workspace manifest and aperture's manifest on 2026-05-21:

- **In-tree (REUSE, not external):** `pulse` (the `MetricStore::query`
  read surface and `FileBackedMetricStore::open`), `aegis`
  (`TenantId`). Both are workspace members; not external.
- **`axum` (0.7)** is already in aperture's `[dependencies]`
  (`crates/aperture/Cargo.toml:51`). query-api reuses the identical
  `Router` + `axum::serve(listener, router)` shape (DD2). Already
  resolved.
- **`hyper` (1.4)** is present via aperture (`crates/aperture/Cargo.toml:109`,
  underpinning the axum server type). Already resolved.
- **`tokio` (1.40)** is already in aperture's `[dependencies]`
  (`crates/aperture/Cargo.toml:44`). The binary needs it for the async
  runtime and the SIGTERM loop. Already resolved.
- **`serde_json` (1)** is a `[workspace.dependencies]` entry (root
  `Cargo.toml:58`) and is also in aperture's `[dependencies]`
  (`crates/aperture/Cargo.toml:65`). query-api serialises the
  Prometheus JSON matrix response through it. Already resolved.
  **Confirmed: serde_json is present as a workspace dependency; no new
  crate.**

**Conclusion: zero new external crates enter the workspace dependency
graph.** query-api adds `[workspace = true]` (or version-pinned path)
references to crates already in `Cargo.lock`, the same pattern strata-v1
and aperture-storage-sink-v0 used. **Gate 4 (`cargo deny check`) carries
forward UNCHANGED and is a no-op-for-this-feature pass**: `cargo deny`
operates on the resolved workspace graph; since every needed crate is
already in the graph, the licence / advisory / ban checks see no new
crate. No `deny.toml` change is required. (Sibling path-dep version
pins, the Gate-4 `bans.wildcards = "deny"` idiom, apply per the aperture
precedent if query-api declares `pulse`/`aegis` by path.)

## A4 - No new toolchain pin

Gates carry forward on the existing `stable` toolchain
(`rust-toolchain.toml`), identical to every other Gate 5 job. The lib is
pure `std` plus the already-resolved axum / serde_json / pulse / aegis;
the binary is `tokio` + the pulse open path. No MSRV bump (memory
`feedback_msrv_creep_is_ecosystem_reality` does not trigger - no
transitive dep raises its `rust-version`), no nightly feature, no new
component. The new `gate-5-mutants-query-api` job uses the same
`dtolnay/rust-toolchain` stable step as its self-observe template.

## Gates NOT modified (summary)

| Gate | Status | Reason |
|------|--------|--------|
| Gate 1 (`cargo test --workspace`) | UNCHANGED | new member + `[[test]]` blocks auto-discovered (A2) |
| Gate 2 (`cargo public-api`) | UNCHANGED | the new crate is not in the Gate 2 scope set {harness, spark, sieve, codex}; not graduated by this feature |
| Gate 3 (`cargo semver-checks`) | UNCHANGED | same scope as Gate 2; not graduated |
| Gate 4 (`cargo deny check`) | UNCHANGED | zero new external crates in the resolved graph (A3) |
| Gate 5 (`cargo mutants`) | **NEW JOB** | `gate-5-mutants-query-api` added (A1); no separate binary gate |
| Prism Gates 6-11 (TS/React) | UNCHANGED | Rust-only commit; path filter excludes it |

## Pre-commit and pre-push hooks

| Hook | Action required |
|------|-----------------|
| `scripts/hooks/pre-commit` | None. Runs `cargo test --workspace` (mirrors Gate 1); the new crate's tests are auto-discovered (A2). |
| `scripts/hooks/pre-push` | None. The per-pkg loop for Gates 2/3 iterates the graduated set {harness, spark, sieve, codex}; query-api is not graduated by this feature. |

The pre-push hook does NOT run `cargo mutants` (mutation testing is a
CI-and-peer-review concern, not a per-push gate, per the per-feature MT
strategy). The new Gate 5 job therefore needs no local-hook mirror.

## DORA framing (one new deployable, no Kaleidoscope-side deploy)

- **Deployment frequency**: N/A on the Kaleidoscope side (no deploy).
  Analog: merge-to-main; this feature targets the per-slice DELIVER
  merges. Operator-side: query-api is a long-lived process the operator
  deploys; operator-determined.
- **Lead time**: commit to merged-on-main = time-to-merge, bounded by
  the five gates' aggregate wall-clock. The new `gate-5-mutants-query-api`
  job runs in parallel with the other Gate 5 jobs (independent `needs`),
  so it does not lengthen the critical path beyond the existing slowest
  Gate 5 job.
- **Change failure rate**: failed Gate 1 or Gate 5 over the next
  query-api-touching commits. Target 0%. The new Gate 5 job makes the
  parser and translation regressions observable for the first time.
- **Time to restore**: revert-and-fix-forward per memory
  `feedback_fix_forward_post_merge_correction`.

## Earned-trust note

query-api's single driven dependency is the durable Pulse store, opened
read-only via `FileBackedMetricStore::open`. The catalogued substrate
lie is "the store opens but cannot be read" (the open succeeds, a query
fails). The probe (DD9) issues a trivial `query` against the resolved
tenant for a sentinel metric over an empty range after open; a failure
refuses startup with `event=health.startup.refused` and a non-zero exit,
never a half-up listener. The behavioural-layer gold-test drives this
against a store-that-lies double (Gate 1). The new
`gate-5-mutants-query-api` job is the test-quality probe that proves the
contract, round-trip, and refusal tests can distinguish the correct
parser + translator from a behaviourally-mutated one - including the
DD4a label-merge precedence, the DD5 half-open bound, and the DD7
fail-closed tenancy invariant (the guardrail against cross-tenant leak).
