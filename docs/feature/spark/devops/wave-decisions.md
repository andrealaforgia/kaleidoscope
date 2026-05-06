# DEVOPS Decisions — `spark` v0

- **Wave**: DEVOPS
- **Author**: Bea (orchestrator), executing the DEVOPS work directly
  after two consecutive specialist-agent dispatches stalled on stream
  watchdog. The CI changes mirror the Aperture precedent closely
  enough that direct execution preserved chain momentum without
  loss of fidelity.
- **Date**: 2026-05-06
- **Reviewer**: pending — Bea will dispatch `@nw-platform-architect-reviewer`
  (Forge) on this wave's outputs once the workflow's first run on
  Spark-touching commits comes back green.

## Key decisions

### [D1] Gate 1 (`cargo test`) — exclude Spark during DISTILL/DELIVER

- `cargo test --workspace --all-targets --locked` becomes
  `cargo test --workspace --exclude spark --all-targets --locked`.
- Rationale: Spark's acceptance tests panic on `unimplemented!()` by
  design (Strategy C "real local" RED-first posture per DISTILL
  wave-decisions). Including them would fail the gate on every push.
- Mirror of Aperture's DISTILL/DELIVER period (the `--exclude
  aperture` clause that was removed at Aperture's graduation on
  2026-05-05).
- Compile-only coverage of Spark is preserved by `cargo build -p
  spark --all-targets --locked` in the local pre-commit hook (per
  hook update at `3745f52`).
- Spark graduates to `--workspace` (no exclude) at the close of its
  v0 DELIVER cycle.

### [D2] Gate 2 (`cargo public-api`) — graduate Spark immediately

- Add a second `cargo public-api -p spark ...` invocation alongside
  the harness invocation, in both branches (origin/main baseline
  available, baseline absent).
- Rationale: Spark IS a consumer-facing library; ADR-0011 locks the
  four-item public surface as a contract third-party applications
  hold against. The semver / public-api discipline is part of that
  lock.
- This contrasts with Aperture, which is NOT graduated to this gate:
  Aperture's only library surface is `aperture::testing`, a dev-only
  seam.

### [D3] Gate 3 (`cargo semver-checks`) — graduate Spark immediately

- Add `--package spark --baseline-rev origin/main` alongside the
  harness invocation.
- Rationale: same as D2.
- On the first commit (no `origin/main` baseline), the gate
  short-circuits with a clear note. After the first push, semver
  enforcement on Spark's public surface is active.

### [D4] Gate 5 (`cargo mutants`) — new parallel job for Spark

- Add `gate-5-mutants-spark` as a new GitHub Actions job mirroring
  `gate-5-mutants-aperture` exactly (timeout-minutes: 30; `--in-diff`
  filter against an `origin/main → HEAD~1 → full` baseline cascade;
  upload of `mutants.out/` artefact).
- Rationale: Spark's mutation suite will grow with each DELIVER
  slice. The split-per-package precedent set by Aperture
  (commit history c3c7319 et al.) keeps each crate within its own
  30-minute timeout.
- During DISTILL/DELIVER's RED phase, individual mutations may
  survive (the `unimplemented!()` panic short-circuits before any
  mutation can be observed). Each slice's DELIVER pass turns its
  own mutants 100% killed per ADR-0005 Gate 5; the per-slice landing
  is the moment the kill rate is verified.

### [D5] Pre-push hook — graduate Spark to Gates 2 and 3

- The pre-push hook's Gate 2 and Gate 3 loops both iterate
  `[otlp-conformance-harness, spark]` instead of running once for
  the harness only.
- Rationale: keeps the local hook coverage symmetric with the CI
  workflow.
- Aperture stays out of pre-push for the same reason it is out of
  the CI Gates 2 and 3.

### [D6] No new wave-specific gates

- Aperture had three new gates added at its DEVOPS wave
  (`gate-6-aperture-architectural-rules`,
  `gate-7-aperture-no-telemetry`, `gate-8-aperture-probe-gold`)
  driven by Aperture-specific architectural invariants.
- Spark v0's invariants (single-init, no-telemetry-on-telemetry) are
  enforced by the existing `tests/invariant_*.rs` binaries running
  inside Gate 1 (per DELIVER's graduation moment) and the `cargo
  deny check` gate (for licence containment of the Apache-2.0 + AGPL
  dev-dep split).
- No new gate types are required. ADR-0017's
  `opentelemetry-appender-tracing` runtime dep is verified by
  `cargo deny check` (Gate 4) plus the licence-audit table in
  `docs/feature/spark/design/technology-choices.md` (DELIVER updates
  that table when adding the dep to `Cargo.toml`).

## Infrastructure summary

- **Deployment target**: not applicable. Spark is a library; no
  deployment.
- **CI/CD platform**: GitHub Actions, same workflow as the harness
  and Aperture (`.github/workflows/ci.yml`).
- **Observability**: not applicable to the SDK itself. Spark's
  diagnostic events go to the application's `tracing` facade
  (DISCUSS D5; the no-telemetry-on-telemetry invariant defends).
- **Mutation testing**: per-feature (per-slice during DELIVER); 100%
  kill rate gate per ADR-0005 Gate 5.
- **Branching strategy**: pure trunk-based, same as the rest of
  Kaleidoscope.

## Constraints established

- Spark contributes to the workspace test gate only after graduation
  at v0 DELIVER close.
- Spark's public-API surface (four items per ADR-0011) is now under
  CI-enforced semver discipline.
- ADR-0017's runtime dep `opentelemetry-appender-tracing =0.27` will
  flow through Gate 4 (`cargo deny check`) when DELIVER lands the
  Cargo.toml edit. Verified Apache-2.0 by Morgan's analysis before
  ADR-0017 was finalised.

## Upstream changes

None. DESIGN's ADRs and the `back-propagation-2.md` outputs already
specify the surface and contracts the CI gates enforce. DEVOPS is
purely the wiring.

## Outputs

- `.github/workflows/ci.yml` — Gate 1 excludes Spark; Gate 2 and 3
  add Spark; new `gate-5-mutants-spark` job mirroring aperture.
- `scripts/hooks/pre-push` — Gates 2 and 3 graduated to cover Spark.
- (no change) `scripts/hooks/pre-commit` — already updated at
  `3745f52` to exclude Spark from the test gate during DISTILL/DELIVER.
