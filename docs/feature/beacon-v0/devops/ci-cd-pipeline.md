# Beacon v0 — CI/CD pipeline

The Beacon v0 CI pipeline extends the existing five-gate Rust CI
workflow (per ADR-0005 §CI contract) to cover the two new crates
`beacon` (library) and `beacon-server` (binary).

The workflow file is `.github/workflows/ci.yml`. This document
describes the changes that DISTILL will apply when the skeleton
crates land (DISTILL creates the crates and the acceptance test
files; DEVOPS-prescribed CI extensions land in the same commit set
to keep `main` green).

## Existing gates

| Gate | What it runs | Beacon impact |
|---|---|---|
| Gate 1 | `cargo test --workspace --all-targets --locked` | Must include `beacon` and `beacon-server` once they exist. During DISTILL/DELIVER's RED state, the gate runs with `--exclude beacon --exclude beacon-server` per the Sieve / Codex / Prism precedent. Graduates to full workspace at Beacon v0 close. |
| Gate 2 | `cargo public-api` per package | Adds `-p beacon` once the library skeleton lands. `beacon-server` is a binary and does not have a public API to lock. |
| Gate 3 | `cargo semver-checks` per package | Adds `-p beacon`. Same scope as Gate 2. |
| Gate 4 | `cargo deny check` | No change — workspace-wide already. |
| Gate 5 | `cargo mutants` per package | New parallel job `gate-5-mutants-beacon` mirroring the existing Aperture / Spark / Sieve / Codex jobs (30-minute timeout, `--in-diff` filter against the `origin/main → HEAD~1 → full` baseline cascade). `beacon-server` is excluded from mutation testing because it is a thin orchestration shell; mutation kill on a binary's `tokio::main` is not informative. |

## New gates (potentially) introduced at slice landings

Beacon v0 may introduce one new gate, per the slice-mapping:

- **Gate 9 (Beacon prometheus contract)** — slice 01 fixture runs
  the binary against a digest-pinned `prom/prometheus` container
  and asserts the walking skeleton's webhook emission contract.
  Same posture as Prism's Gate 11.

The gate's introduction is conditional: if the slice 01 integration
test fits inside `cargo test`'s default harness, no new CI job is
needed. The DELIVER-time crafter judges; the DEVOPS posture is
"infrastructure is ready; new gates added as the slices warrant".

## Toolchain

No change. Stable Rust per `rust-toolchain.toml` (current floor 1.88).
Nightly pin `nightly-2026-04-15` for Gates 2 and 3.

## Caching

No change. The Cargo cache key extends naturally as new crates are
added to `Cargo.lock`.

## Pre-push hook

`scripts/hooks/pre-push` graduates `beacon` to Gates 2 and 3 at the
same point Gate 2/3 graduates in CI. Same shape as the Codex
graduation (D5 in Codex's DEVOPS wave-decisions).

## Mutation testing strategy

**Per-feature** per the workspace CLAUDE.md. Mutation testing runs
during each slice's DELIVER pass (refactoring + review), scoped to
modified files. Kill rate gate: 100% per ADR-0005 Gate 5.

The `cargo-mutants` configuration for the `beacon` crate lives at
`crates/beacon/.cargo/mutants.toml` and mirrors the Sieve config:
exclude generated code, exclude pure-derive impls, run with
`--shuffle` for test-order independence.

## Environment inventory

| Environment | Purpose | Beacon role |
|---|---|---|
| Local dev (macOS, Linux WSL2) | Crafter dispatch + manual verification | Operator runs `cargo run -p beacon-server -- --rules examples/` against a local Prometheus on `:9090`. |
| CI (GitHub Actions ubuntu-latest) | Gate runs | All five gates plus any new Beacon-specific gates. Slice 01 integration test runs against a docker-compose Prometheus container in CI. |
| Production | Operator-deployed | Out of scope for v0 — Beacon ships as a binary the operator runs on their own infra. Helm chart and Kubernetes operator are post-v0 deliverables. |

## Hand-off to DISTILL

DISTILL creates:

1. `crates/beacon/` and `crates/beacon-server/` skeleton crates with
   AGPL headers, `Cargo.toml`, and minimum `lib.rs` / `main.rs`
2. Five acceptance test files under `crates/beacon/tests/`, one per
   slice, with `unimplemented!()` panic bodies
3. The workspace `Cargo.toml` `members` extension
4. The CI workflow extension per this document (`--exclude` rules
   plus the new `gate-5-mutants-beacon` job)
5. The `rust-toolchain.toml` (no change — workspace floor applies)
6. The pre-push hook extension

All of the above lands in one DISTILL commit so `main` stays GREEN.
