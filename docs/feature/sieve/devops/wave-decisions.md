# DEVOPS Decisions — `sieve` v0

- **Wave**: DEVOPS
- **Author**: Bea (orchestrator), executing the DEVOPS work directly
  using the Aperture and Spark precedents as the template. Forge will
  peer-review on the first CI run after this lands.
- **Date**: 2026-05-06

## Key decisions

### [D1] Gate 1 (`cargo test`) — exclude Sieve during DISTILL/DELIVER

- `cargo test --workspace` becomes `cargo test --workspace --exclude
  sieve` in the local pre-commit hook and the CI workflow's Gate 1
  step.
- Already applied at Scholar's DISTILL landing (`98b89d0`). Mirrors
  Aperture's and Spark's RED-phase precedent.
- Sieve graduates to `--workspace` (no exclude) at the close of its
  v0 DELIVER cycle.

### [D2] Gate 2 (`cargo public-api`) — graduate Sieve immediately

- Add a `cargo public-api -p sieve ...` invocation alongside the
  harness and spark invocations in both branches of the gate (origin
  /main baseline available, baseline absent).
- Rationale: Sieve IS a consumer-facing library. ADR-0018 locks the
  seven-item public surface plus two doc-hidden test seams as a
  contract that downstream stages (Aperture's pipeline, Sluice's
  future pipeline) hold against.
- Mirrors Spark's graduation pattern. Aperture stays out of this gate
  (its only library surface is `aperture::testing`, a dev-only seam).

### [D3] Gate 3 (`cargo semver-checks`) — graduate Sieve immediately

- Add `--package sieve --baseline-rev origin/main` alongside the
  harness and spark invocations.
- Rationale: same as D2.
- On the first commit (no `origin/main` baseline) the gate
  short-circuits with a clear note. After the first push, semver
  enforcement on Sieve's public surface is active.

### [D4] Gate 5 (`cargo mutants`) — new parallel job for Sieve

- Add `gate-5-mutants-sieve` as a new GitHub Actions job mirroring
  `gate-5-mutants-aperture` and `gate-5-mutants-spark` exactly:
  30-minute timeout; `--in-diff` filter against an `origin/main →
  HEAD~1 → full` baseline cascade; upload of `mutants.out/` artefact.
- Rationale: Sieve's mutation suite will grow with each DELIVER
  slice. The split-per-package precedent set by Aperture keeps each
  crate within its own 30-minute budget.
- During DISTILL/DELIVER's RED phase, individual mutations may
  survive (the `unimplemented!()` panic short-circuits before any
  mutation can be observed). Each slice's DELIVER pass turns its own
  mutants 100% killed per ADR-0005 Gate 5.

### [D5] Pre-push hook — graduate Sieve to Gates 2 and 3

- The pre-push hook's Gate 2 and Gate 3 loops both iterate
  `[otlp-conformance-harness, spark, sieve]` instead of running over
  just the harness and spark.
- Rationale: keeps the local hook coverage symmetric with the CI
  workflow.
- Aperture stays out for the same reason it is out of the CI Gates 2
  and 3.

### [D6] `deny.toml` — BSL-1.0 entry already added

- Added at Scholar's DISTILL landing (commit `98b89d0`) so the
  pre-commit `cargo deny` gate could pass with the new
  `xxhash-rust =0.8` runtime dep. ADR-0019 §"Licence audit" is the
  authority; the deny.toml comment links to the rationale.
- Treated as in-scope for this DEVOPS wave (the licence audit is a
  hand-off Atlas's slice-mapping flagged for DEVOPS); applied early
  to unblock subsequent commits.

### [D7] No new wave-specific gates

- Aperture had three new gates added at its DEVOPS wave
  (`gate-6-aperture-architectural-rules`,
  `gate-7-aperture-no-telemetry`, `gate-8-aperture-probe-gold`)
  driven by Aperture-specific architectural invariants.
- Sieve's invariants (the public-surface lock, the
  `OtlpSink + Probe` decorator pattern, the `target = "sieve"` event
  filter, the cross-counter race acceptability) are enforced by:
  - `tests/invariant_public_api_smoke.rs` (compile-time public-API
    type-system invariants).
  - `tests/invariant_sampling_sink_is_otlp_sink_and_probe.rs`
    (compile-time trait-implementation invariants — the existing
    Aperture xtask AST walk that verifies "every OtlpSink type also
    implements Probe" covers `SamplingSink` automatically per ADR-0021).
  - `cargo deny check` Gate 4 (licence containment).
  - The Aperture pipeline's no-telemetry-on-telemetry filter, which
    Sieve does not contradict at v0 (Sieve does not emit through the
    OTel pipeline; its own `target = "sieve"` events go to the
    application's `tracing` facade).
- No new gate types are required.

## Infrastructure summary

- **Deployment target**: not applicable. Sieve is a library at v0;
  no deployment.
- **CI/CD platform**: GitHub Actions; same workflow as the harness,
  Aperture, and Spark.
- **Observability**: not applicable to the library itself. Sieve's
  diagnostic events go to the application's `tracing` facade per
  ADR-0020 §4.
- **Mutation testing**: per-feature (per-slice during DELIVER); 100%
  kill rate gate per ADR-0005 Gate 5.
- **Branching strategy**: pure trunk-based, same as the rest of
  Kaleidoscope.

## Constraints established

- Sieve contributes to the workspace test gate only after graduation
  at v0 DELIVER close.
- Sieve's public-API surface (seven items per ADR-0018 plus the two
  doc-hidden test seams) is now under CI-enforced semver discipline.
- ADR-0019's runtime dep `xxhash-rust =0.8` flowed through Gate 4
  (`cargo deny check`) at DISTILL landing; the BSL-1.0 entry holds.

## Upstream changes

None. DESIGN's ADRs and DISTILL's test infrastructure already
specify the surface and contracts the CI gates enforce. DEVOPS is
purely the wiring.

## Outputs

- `.github/workflows/ci.yml` — Gate 2 and Gate 3 graduate Sieve; new
  `gate-5-mutants-sieve` job mirroring `gate-5-mutants-aperture` and
  `gate-5-mutants-spark`.
- `scripts/hooks/pre-push` — Gates 2 and 3 graduated to cover Sieve.
- `deny.toml` (already at commit `98b89d0`) — BSL-1.0 allow entry.
- (no change) `scripts/hooks/pre-commit` — Gate 1 already excludes
  Sieve at commit `98b89d0`.

## Forge peer review

Pending. Forge will run on this wave's outputs once the workflow's
first run on Sieve-touching commits comes back green.
