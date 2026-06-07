# Kaleidoscope — agent guidance

This file is the bootstrap pointer for AI agents working in this
repository. It is intentionally short. Per-feature waves keep their
deeper context under `docs/feature/<feature-id>/`.

## Development Paradigm

Rust idiomatic: data + free functions + traits where polymorphism is
genuinely needed. No class-style inheritance hierarchies (Rust has none),
no `dyn Trait` indirection where direct generic monomorphisation
suffices, composition over inheritance throughout. This shape matches
Morgan's DESIGN brief for `otlp-conformance-harness-v0` and is the
natural shape for validation-and-decode libraries in the wider Rust
ecosystem (`serde_json`, `prost`, `regex` all expose this shape).

For implementation work, use `@nw-software-crafter`. The crafter agent
is the only agent that writes production source under
`crates/<name>/src/`. All other agents write specifications, ADRs, peer
reviews, or workflow YAML.

## Mutation Testing Strategy

This project uses per-feature mutation testing. Runs after refactoring
during each delivery, scoped to modified files. Kill rate gate: 100%
(per ADR-0005 Gate 5).

## CI watch

**What.** `scripts/ci-watch.sh` reports the latest `main` CI run's
conclusion + URL + short SHA and, on a red, classifies the failed jobs —
explicitly calling out `gate-1-test` (the deep `cargo test --workspace
--all-targets --locked` suite) and any `gate-5-mutants*` (mutation) reds.
These are the two deep gate families the local pre-commit hook no longer
pre-runs (ADR-0072). Invoke directly: `scripts/ci-watch.sh` (or
`scripts/ci-watch.sh 10`); it is never wired into a git hook.

**Why.** Per ADR-0072 the local hook now runs only the fast unit subset
(`cargo test --workspace --lib --locked`); the deep suite gates in CI.
ci-watch.sh is the safety net that keeps eyes on the deep coverage now off
the local commit block — the honesty trade is only honest because this
cadence is real.

**Cadence.** Run `scripts/ci-watch.sh` after every push to main, and poll
on a periodic tick while working a multi-slice task. Target: a deep-only
regression surfaces within one cadence interval (same session / < 1 h),
not days. On a red, fix-forward (project memory
`feedback_fix_forward_post_merge_correction`).

**Honest degradation.** If `gh` is missing / unauthenticated / the network
is down, the script exits non-zero with a remediation message — it never
reports green on an un-probed substrate.
