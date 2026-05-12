# DEVOPS Decisions — beacon-v0

- **Wave**: DEVOPS
- **Author**: Bea (autonomous DEVOPS dispatch)
- **Date**: 2026-05-11

## Key decisions

### [D1] Gate 1 (`cargo test`) — exclude Beacon during DISTILL/DELIVER

- Mirrors the Aperture / Sieve / Codex / Prism precedent. The
  workspace `cargo test --workspace` runs with `--exclude beacon
  --exclude beacon-server` during the RED state; graduates to full
  workspace at v0 close.

### [D2] Gate 2 (`cargo public-api`) — graduate Beacon immediately

- Add `-p beacon` to the public-api check alongside harness /
  aperture / spark / sieve / codex. ADR-0033 locks the library's
  public surface; the gate holds it.
- `beacon-server` is a binary; no public-api check applies.

### [D3] Gate 3 (`cargo semver-checks`) — graduate Beacon immediately

- Add `-p beacon --baseline-rev origin/main`. Same scope as D2.

### [D4] Gate 5 (`cargo mutants`) — new parallel job

- Add `gate-5-mutants-beacon` mirroring the existing Sieve / Codex
  jobs: 30-minute timeout, `--in-diff` filter against the
  `origin/main → HEAD~1 → full` baseline cascade.
- `beacon-server` is excluded from mutation testing (D7).

### [D5] Pre-push hook — graduate Beacon to Gates 2 and 3

- The hook's Gate 2 and Gate 3 loops iterate
  `[otlp-conformance-harness, spark, sieve, codex, beacon]`.

### [D6] No new wave-specific gates at this DEVOPS pass

- Slice 01 MAY introduce a Gate 9 (Beacon prometheus contract) per
  the slice-mapping. The judgement is deferred to slice 01's
  DELIVER pass; if the integration test fits the default `cargo
  test` harness, no new CI job is needed.

### [D7] `beacon-server` is excluded from mutation testing

- The binary is a thin orchestration shell: `tokio::main`, signal
  handler, scheduler loop. Mutation kill on these surfaces is not
  informative. The crate is still covered by Gates 1, 4, and the
  in-process integration tests under `crates/beacon-server/tests/`.

### [D8] Mutation testing strategy: per-feature, 100% kill rate

- Per the workspace CLAUDE.md (`ADR-0005 Gate 5`). Each slice's
  DELIVER pass turns its own mutants 100% killed before review
  approval.

### [D9] Integration-test fixture: Prometheus container digest-pin

- Slice 01 onward uses `prom/prometheus:v2.55` digest-pinned in
  `crates/beacon/tests/fixtures/prom-image-digest.txt`. Same
  pattern as Prism's E2E Playwright fixture.

### [D10] No back-propagation to DESIGN

- The DEVOPS posture extends the existing CI shape without
  contradicting DESIGN. The Cargo workspace member list grows by
  two; nothing in DESIGN constrains that.

## Infrastructure summary

- Deployment: out of scope for v0 (operator-deployed on
  operator-supplied infrastructure).
- CI: GitHub Actions ubuntu-latest, gates 1-5 (plus optional
  Gate 9 if slice 01 introduces it).
- Branching: trunk-based, no feature branches.
- Mutation testing: per-feature, scoped to modified files via
  `--in-diff`.

## Constraints established

- Pre-DISTILL: no workspace `Cargo.toml` change yet; existing crates
  must not regress.
- At DISTILL: workspace gains two crates; CI workflow gains the
  exclude rules and the new mutation job in the same commit.
- At slice 01 DELIVER close: Gate 1 / Gate 5 graduate to include
  `beacon` (`beacon-server` graduates at v0 close).

## Hand-off to DISTILL

DISTILL creates the skeleton crates plus the acceptance test files,
and applies the CI workflow extensions documented here in the same
commit. The atomic commit keeps `main` GREEN.
