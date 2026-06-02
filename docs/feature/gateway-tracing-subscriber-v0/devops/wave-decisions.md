# Wave Decisions — gateway-tracing-subscriber-v0 / DEVOPS

- **Wave**: DEVOPS (slim, doc-only)
- **Author**: Apex (`nw-platform-architect`)
- **Mode**: slim. This feature installs a `tracing` subscriber EARLY in the
  fourth (write/ingest side) Kaleidoscope binary, `kaleidoscope-gateway`, as
  the first statement of its `main` (`OnceLock`-guarded, JSON to stderr,
  `EnvFilter` keyed on `RUST_LOG`, `try_init` so the install is panic-free
  and idempotent). It closes an ordering gap: `gateway_starting`
  (`main.rs:89`) and `health.startup.refused` (`main.rs:102`, the
  `probe_or_refuse` fail arm) fire BEFORE `aperture::spawn` installs the
  subscriber at `main.rs:116`, so both are dropped today. The gateway aligns
  to the aperture write-side posture, NOT to `query-http-common` (the read
  tier crate); the anti-coupling invariant stays clean. Origin: EDD verifier
  issue 005 moves from `partial` (read tier resolved) to RESOLVED. The change
  touches `crates/kaleidoscope-gateway` only: one early `init_tracing()` call,
  one inline `init_tracing` fn, one `tracing-subscriber` dependency line, and
  the acceptance test. No new crate, no new workspace member, no new binary,
  no new CI job, no new ADR. Shape and brevity mirror the immediate sibling
  slim precedent at
  `docs/feature/read-api-tracing-subscriber-v0/devops/wave-decisions.md`.

## DEVOPS Decisions

| D# | Topic | Value |
|----|-------|-------|
| DD1 | deployment_target | N/A (write-side composition binary already deployed as `kaleidoscope-gateway`; no new deployable artefact, no new container) |
| DD2 | container_orchestration | N/A (this feature produces no container image; no Dockerfile is added or amended) |
| DD3 | cicd_platform | inherit GitHub Actions per ADR-0005; the five-gate workspace contract is unchanged |
| DD4 | existing_infrastructure | extend; the only addition is one direct dependency edge (`tracing-subscriber` 0.3) on `crates/kaleidoscope-gateway/Cargo.toml`; no new infra, no new CI job |
| DD5 | observability | this feature IS observability: it installs the subscriber that renders the gateway's own lifecycle events; it improves operability rather than leaving it invariant; no new metric, no new dashboard, no new alert (events were already emitted, just dropped pre-install) |
| DD6 | deployment_strategy | N/A (pure trunk-based; recovery is fix-forward / git revert; the change is additive to startup stderr only and the ingest contract is byte-identical) |
| DD7 | continuous_learning | N/A (no live observability stack at v0; stderr is the operator's surface) |
| DD8 | git_branching | inherit pure trunk-based (project default; main has no required-status-checks and no enforce_admins) |
| DD9 | mutation_testing | inherit per-feature, 100% kill rate (CLAUDE.md, ADR-0005 Gate 5); covered by `gate-5-mutants-kaleidoscope-gateway` (line 2318) via `--in-diff`. The `OnceLock`-guarded idempotence guard of `init_tracing` is pinned by a unit test the way the read tier pinned its `test_init_tracing_is_idempotent_and_never_panics` |

## CI Inheritance

The ADR-0005 five workspace gates (Gate 1 `cargo test --workspace`, Gate 2
`cargo public-api`, Gate 3 `cargo semver-checks`, Gate 4 `cargo deny`,
Gate 5 `cargo mutants`) are inherited unchanged. No workflow file edit. No
new or amended job.

The relevant Gate 5 job already exists and path-filters its own crate via
`--in-diff`, so it covers this feature's modified files automatically:

- `gate-5-mutants-kaleidoscope-gateway` at `.github/workflows/ci.yml:2318`
  (CONFIRMED, name "Gate 5 — cargo mutants (kaleidoscope-gateway)"). It runs
  `cargo mutants (kaleidoscope-gateway, in-diff)` at line 2353 with the same
  `origin/main → HEAD~1 → full` baseline cascade as the sibling jobs, and an
  empty diff short-circuits to a zero-second exit. The `--in-diff` filter
  points the runner at the new inline `init_tracing()` fn and its `main` call
  site in `crates/kaleidoscope-gateway/src/main.rs`. It uploads
  `mutants-out-kaleidoscope-gateway`. Added in the `gate-5-mutants-batch-v0`
  batch; it remains the binding gate-5 signal for this crate.

No gate is added or removed; the job needs `gate-2-public-api` and
`gate-3-semver` exactly as it does today.

## No new tooling

Zero new workspace crate. Zero new binary. Zero new public event name (the
three event names already exist; this feature only makes the two dropped
ones render). Zero new graduation tag. Zero new `deny.toml` policy change.

One new dependency edge on `crates/kaleidoscope-gateway/Cargo.toml`
`[dependencies]`:

- `tracing-subscriber = { version = "0.3", default-features = false,
  features = ["fmt", "json", "env-filter", "registry"] }` (MIT/Apache-2.0),
  matched verbatim to aperture's existing line. The gateway already declares
  `tracing` 0.1.

The specifier is non-wildcard, so Gate 4 `cargo deny` raises no wildcard-pin
concern. The workspace `Cargo.lock` already resolves a `tracing-subscriber`
0.3.x via aperture and `query-http-common`, so the new edge adds only the
edge itself with no fresh transitive resolution and near-zero `Cargo.lock`
churn. No version is bumped, and nothing approaches a 1.0.0 promise.

## Observability note

This feature is itself an observability improvement, which is why DD5 reads
"improves" rather than "invariant". The gateway already emits its lifecycle
events through `tracing::`, but the subscriber is installed only inside
`aperture::spawn`, so the two events that fire before that call are dropped
and the operator's stderr is empty for them. Installing the subscriber as the
first statement of `main` renders `gateway_starting` (with `pillar_root`) on a
clean start and `health.startup.refused` (with `substrate` and `reason`)
before the non-zero exit on a fail-closed start; `listener_bound` continues to
render as the regression guard. This closes EDD verifier issue 005 from
`partial` to RESOLVED, completing the fourth-binary coverage the read tier left
open, and aligns the gateway to the aperture write-side posture: one
subscriber format (JSON to stderr, flattened, `event` field, no target/span
noise), one filter (`EnvFilter` / `RUST_LOG`), one rendered line shape across
the whole Kaleidoscope surface.

The double install is handled idempotently. The gateway's early `try_init`
under its `OnceLock` is the effective install on the gateway path; aperture's
in-spawn `install_subscriber` (`observability.rs:145`) is ALSO `OnceLock`-
guarded and `try_init`-based, so it observes a default already set, returns
`Err`, and aperture discards it with `let _ =`. No panic, order-independent.
aperture standalone never runs the gateway code, so its install stays the
first and only one and its behaviour is byte-for-byte unchanged. The signal is
the stderr contract itself; at v0 there is no separate metrics or dashboard
layer to wire, and the black-box acceptance run plus the EDD verifier are the
consumers of that contract.

## Inherited from slim precedent

This wave inherits the structure and per-decision shape of
`docs/feature/read-api-tracing-subscriber-v0/devops/wave-decisions.md` (slim
DEVOPS). That sibling installed the same subscriber posture in the three read
binaries via the shared `query_http_common::init_tracing()` helper, verified
against the same ADR-0005 contract with no new crate and no new CI job. The
DEVOPS posture is identical at the workflow and deployment layers: inherit the
five gates, edit no workflow file, rely on the per-crate `--in-diff` gate-5
job to cover the modified files. The shape differences are two: the gateway is
the write side, so it aligns to aperture and replicates the builder inline
rather than importing the read-tier helper (zero edge to `query-http-common`);
and coverage leans on a single crate job (`gate-5-mutants-kaleidoscope-gateway`,
line 2318) rather than the four the read tier used, because the change is
confined to one binary.

## Upstream Changes

None. Zero DISCUSS assumptions changed by this DEVOPS wave. Zero DESIGN
assumptions changed; the DESIGN handoff at `../design/wave-decisions.md`
("No new crate, no new workspace member, no new CI job ... the Gate 5
mutation job already exists: `gate-5-mutants-kaleidoscope-gateway`") is
ratified verbatim: the job exists at line 2318, covers via `--in-diff`, and
no CI edit is required. The feature composes additively on top of ADR-0009
(aperture posture) and aperture's already-idempotent install without altering
either crate's source.
