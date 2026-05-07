# DEVOPS Decisions — `codex` v0

- **Wave**: DEVOPS
- **Author**: Bea (orchestrator), executing the DEVOPS work directly
  using the Sieve precedent as the template. Forge will peer-review
  on the first CI run after this lands.
- **Date**: 2026-05-07

## Key decisions

### [D1] Gate 1 (`cargo test`) — exclude Codex during DISTILL/DELIVER

- Already applied at Scholar's DISTILL landing (`0dc6f68`). Mirrors
  the Aperture / Spark / Sieve precedent.
- Codex graduates to `--workspace` (no exclude) at the close of its
  v0 DELIVER cycle.

### [D2] Gate 2 (`cargo public-api`) — graduate Codex immediately

- Add `cargo public-api -p codex ...` invocation alongside
  harness / spark / sieve in both branches of the gate.
- Rationale: Codex IS a consumer-facing library; ADR-0022 locks
  the five-item public surface (plus the doc-hidden `__test_*`
  seams Sieve/Spark established) as a contract Spark holds against.

### [D3] Gate 3 (`cargo semver-checks`) — graduate Codex immediately

- Add `--package codex --baseline-rev origin/main` alongside
  harness / spark / sieve.
- Rationale: same as D2.

### [D4] Gate 5 (`cargo mutants`) — new parallel job for Codex

- Add `gate-5-mutants-codex` mirroring the existing `aperture` /
  `spark` / `sieve` jobs byte-for-byte: 30-minute timeout,
  `--in-diff` filter against the `origin/main → HEAD~1 → full`
  baseline cascade, mutants.out artefact upload.
- During DISTILL/DELIVER's RED phase, individual mutations may
  survive (the `unimplemented!()` panic short-circuits before any
  mutation can be observed). Each slice's DELIVER pass turns its
  own mutants 100% killed per ADR-0005 Gate 5.

### [D5] Pre-push hook — graduate Codex to Gates 2 and 3

- The hook's Gate 2 and Gate 3 loops now iterate
  `[otlp-conformance-harness, spark, sieve, codex]`.

### [D6] No new wave-specific gates

- Aperture had three new gates added at its DEVOPS wave
  (`gate-6-aperture-architectural-rules`,
  `gate-7-aperture-no-telemetry`, `gate-8-aperture-probe-gold`)
  driven by Aperture-specific architectural invariants.
- Codex's invariants (the public-surface lock, the AGPL containment,
  the corpus regeneration ritual) are enforced by:
  - `tests/invariant_public_api_smoke.rs` (compile-time public-API
    type lock).
  - `cargo deny check` Gate 4 (zero new entries for Codex; runtime
    closure is empty per ADR-0024).
  - The xtask regenerator drift check at slice 02 DELIVER.
- No new gate types are required.

### [D7] No deny.toml changes

- Codex's runtime closure is empty (zero deps per ADR-0024 §3).
- The xtask regenerator's build-time deps live in a separate
  Cargo.toml that does not feed the cargo-deny audit on the
  published crate.

## Infrastructure summary

- **Deployment target**: not applicable. Codex is a library at v0.
- **CI/CD platform**: GitHub Actions; same workflow as the harness,
  Aperture, Spark, Sieve.
- **Observability**: not applicable to the library itself. Codex
  emits no telemetry; the warn-mode tracing event for Spark
  integration lives in Spark per ADR-0025 §3.
- **Mutation testing**: per-feature (per-slice during DELIVER);
  100% kill rate gate per ADR-0005 Gate 5.
- **Branching strategy**: pure trunk-based.

## Constraints established

- Codex contributes to the workspace test gate only after graduation
  at v0 DELIVER close.
- Codex's public-API surface (five items per ADR-0022) is now under
  CI-enforced semver discipline.

## Upstream changes

None. DESIGN's ADRs and DISTILL's test infrastructure already
specify the surface and contracts the CI gates enforce. DEVOPS is
purely the wiring.

## Outputs

- `.github/workflows/ci.yml` — Gate 2 and Gate 3 graduate Codex;
  new `gate-5-mutants-codex` job mirroring the
  `aperture` / `spark` / `sieve` precedents.
- `scripts/hooks/pre-push` — Gates 2 and 3 graduated to cover Codex.
- (no change) `scripts/hooks/pre-commit` — Gate 1 already excludes
  Codex at commit `0dc6f68`.
- (no change) `deny.toml` — no new entries; Codex's runtime closure
  is empty.

## Forge peer review

Pending. Forge will run on this wave's outputs once the workflow's
first run on Codex-touching commits comes back green.
