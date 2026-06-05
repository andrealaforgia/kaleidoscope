# IO Strategy — claims-honesty-pass-v0 (DISTILL)

## Posture

This is a documentation-honesty feature. There is no new component, port,
adapter, or runtime environment (DESIGN: "no new architecture here"; DEVOPS:
`behaviour_change: none`, `deploy_surface: none`). Consequently there is no
adapter matrix to parametrise and no walking-skeleton real-I/O integration to
stand up. The IO strategy is therefore minimal and entirely deterministic.

## Two test shapes, two IO profiles

### 1. Doc-lint / grep guards (13 tests, RED-`#[ignore]`d)

- **IO**: pure file-read of a fixed workspace file, then a substring check.
  No network, no subprocess, no clock, no ordering. Deterministic by
  construction.
- **Path resolution (DEVOPS decision 3)**: the workspace-root `README.md` and
  cross-crate files are reached from inside a crate's `tests/` via
  `env!("CARGO_MANIFEST_DIR")` joined with `../../` — the same idiom
  `otlp-conformance-harness/tests/slice_07_lock_the_contract.rs` already uses
  for `tests/vectors`. NOT a CWD-relative path (cargo's CWD is not guaranteed
  across runners) and NOT a hard-coded absolute path. The crate-local guards
  (codex, query-http-common, trace-query-api) use a plain
  `CARGO_MANIFEST_DIR`-relative path (no `../../` hop — the target file is in
  the same crate).
- **Consolidation**: per DEVOPS, all README + cross-crate greps live in ONE
  dedicated docs-guard file (`harness/tests/slice_08`). One portable anchor,
  one place to maintain the relative hop.

### 2. Behaviour tests (8 tests, GREEN today)

- **US-04 / US-06 (harness)**: pure functions over in-process-synthesised
  bytes. `validate_traces` / `validate_logs` take `&[u8]` and return
  `Result<_, OtlpViolation>`. The fixtures are built with
  `prost::Message::encode_to_vec` (the same way the existing slice_04/05/06
  acceptance suites build corpus bytes). No I/O, no clock. Deterministic.
- **US-05 (query-api)**: drives the single public driving port
  `query_api::router(store, tenant, static_dir)` via `tower::ServiceExt::oneshot`
  against the axum `Router` — no network port bound. The store is a REAL
  `FileBackedMetricStore` opened in a unique tempdir (the `open_durable_store`
  helper the slice_01 suite already uses), seeded with fixed points at exact
  second boundaries. The window/`start`/`end` are FIXED inputs, never "now",
  so there is no wall-clock dependence. Deterministic.

## Why no fixture-matrix parametrisation (Mandate 4 / CM-D)

There are no environment variants to sweep. The corrections are pure file
content (asserted by reading the file) and the behaviour is pure over fixed
inputs (bytes for the harness; a seeded tempdir store for query-api). The one
real-I/O touch — the `FileBackedMetricStore` tempdir — is the SAME adapter the
existing query-api acceptance suite uses; it is not parametrised across
environments because the behaviour under test (`step` invariance) is
environment-independent. No pure-function extraction was required because the
behaviours already live behind the existing driving ports / public functions.

## Determinism contrast with the known flake

None of these tests resemble the lumen/pulse p95 KPI tests that flake under
overnight load (project memory `p95_wallclock_flakes_overnight`): there is no
timing surface, no p95, no ordering guarantee under contention. All 21 tests
run identically in the local `clean` pre-commit hook and in `ci` (DEVOPS
`environments.yaml`).

## Run evidence (DISTILL commit)

`cargo test --workspace --all-targets --locked` -> exit 0, ZERO failed, ZERO
errors.

- Touched-crate focused run (`-p otlp-conformance-harness -p query-api -p codex
  -p query-http-common -p trace-query-api`): all green.
- New files:
  - `harness/tests/slice_08`: 8 ignored (RED doc guards) + 1 passed (US-03
    in-flight GREEN half).
  - `harness/tests/slice_09`: 3 passed (US-04 + US-06 behaviour).
  - `codex/tests/slice_06`: 3 ignored (RED) + 1 passed (GREEN guardrail).
  - `query-http-common/tests/slice_01`: 1 ignored (RED) + 1 passed (GREEN
    guardrail).
  - `trace-query-api/tests/slice_04`: 1 ignored (RED) + 1 passed (GREEN
    guardrail).
  - `query-api/tests/slice_06`: 1 passed (US-05 step invariance GREEN).

The three `query-api` "function never used" warnings on the new test binary are
pre-existing: the shared `tests/common/mod.rs` carries helpers the new binary
does not call, exactly as the existing `slice_02` binary does. No
`deny(warnings)` is configured workspace-wide; the warnings are benign.
