# Releasing

This document records the release process for Kaleidoscope's
per-crate version tags. It is a tool for the maintainer to execute
when the workspace is in a tagsable state, not a documentation of
automation. There is no automation; tagging is a deliberate act.

## Tag format

Per-crate tags. Each tag is `<crate>/<semver>`. Examples:

- `aperture/v0.1.0`
- `lumen/v1.0.0`
- `cinder/v1.0.0`
- `kaleidoscope-cli/v0.1.0`

Tags are immutable once pushed. Deleting a tag is a destructive
operation and should be avoided.

## When to tag a crate at v0.1.0

A crate is graduating to its first publicly-referenced version when
all of these hold:

1. Its public API has been stable for several features without
   contract churn (the ADR-0001 pattern for `kaleidoscope-cli`,
   ADR-0038 / ADR-0039 for `self-observe`, the trait pin in
   `cinder` for tier policy, and the equivalent pin in `lumen`).
2. Acceptance tests under `tests/` are green; the workspace
   `cargo test` passes; `cargo fmt --check` and `cargo clippy
   -- -D warnings` pass.
3. The per-package mutation gate (Gate 5) is meeting its kill rate.
4. The crate is referenced by ADRs or by another crate as a stable
   surface.

## When to tag a crate at v1.0.0

A crate graduating to v1 has had its v0 surface ship in production
or in an equivalent operational substrate, with the data-shape and
durability invariants (`WAL + snapshot`, recovery KPI) measured.
Each v1 ship has a `tests/v1_slice_NN_*.rs` suite under it.

## Candidate set as of 2026-05-19

The five candidates currently sitting at `0.1.0` in their
`Cargo.toml` but un-tagged. Pre-flight check column shows the
single deliberate verification per crate before tagging.

| Crate | Tag | Pre-flight check |
|-------|-----|------------------|
| `lumen` | `lumen/v1.0.0` | `cargo test -p lumen` green; `tests/v1_slice_01_*.rs` and `tests/v1_slice_02_*.rs` both pass; KPI 1 budget already bumped to CI-realism (commit 5ac7c67); KPI 2 bumped to 2.5 s (commit 0cee88c). |
| `cinder` | `cinder/v1.0.0` | `cargo test -p cinder` green; v1 WAL + snapshot tests pass; KPI 2 budget bumped to 2.5 s (commit ebffa3d); evaluate_at API confirmed exposed via `kaleidoscope-cli evaluate-policy` (commit 26350cc). |
| `sluice` | `sluice/v1.0.0` | `cargo test -p sluice` green; v1 suite shipped; no CLI surface but the durable-buffer contract is referenced by ADR-0005. |
| `self-observe` | `self-observe/v0.1.0` | `cargo test -p self-observe` green; ADR-0038 (Pulse-side bridges) and ADR-0039 §1-§8 (OTLP-JSON bridges) are the locked public surface; the §2 atomic-pair correction has shipped (commit 5daae6d). |
| `kaleidoscope-cli` | `kaleidoscope-cli/v0.1.0` | `cargo test -p kaleidoscope-cli` green; fourteen subcommand features shipped in this redo cycle; every `TieringStore` trait method exposed; every `LogStore` trait method exposed. |

## Suggested order

If tagging all five in one session:

1. `lumen/v1.0.0` first. It is the bottom-most storage adapter in
   the dependency graph for the others; tagging it first makes
   the upstream tag visible when the others are checked.
2. `cinder/v1.0.0` next, for the same reason.
3. `sluice/v1.0.0` next.
4. `self-observe/v0.1.0` next. It depends on `lumen` and `cinder`
   types being stable.
5. `kaleidoscope-cli/v0.1.0` last. It depends on all four.

## Commands to run

Each tag is created locally, signed with the maintainer's GPG
key (if configured), and pushed individually. Verify each tag
lands on the remote before moving on.

```bash
# From the workspace root, on `main`, with the working tree clean.

git tag -a lumen/v1.0.0          -m "Lumen v1.0.0"
git push origin lumen/v1.0.0

git tag -a cinder/v1.0.0         -m "Cinder v1.0.0"
git push origin cinder/v1.0.0

git tag -a sluice/v1.0.0         -m "Sluice v1.0.0"
git push origin sluice/v1.0.0

git tag -a self-observe/v0.1.0   -m "self-observe v0.1.0"
git push origin self-observe/v0.1.0

git tag -a kaleidoscope-cli/v0.1.0 -m "kaleidoscope-cli v0.1.0"
git push origin kaleidoscope-cli/v0.1.0
```

Each `git tag -a` opens an editor for an annotated tag message.
The single-line `-m` form above is the minimum; if you want a
longer message, drop the `-m` and let the editor open.

## After tagging

1. Verify each tag on the remote:
   `git ls-remote --tags origin | grep <tag>`
2. Update `CHANGELOG.md` if the project has one (currently not
   shipped; if you start one, this is the moment).
3. Move on to the next development cycle. The next `0.x.y` work
   on `kaleidoscope-cli` becomes a `kaleidoscope-cli/v0.2.0`
   candidate.

## What is not in scope here

- Crates.io publication. Kaleidoscope's licence and trademark
  posture intentionally does not target crates.io for the
  platform components. Public-API crates (SDKs, protocol
  libraries) may publish at a later stage.
- Docker image tags. The Dockerfile under
  `crates/kaleidoscope-cli/` builds `kaleidoscope-cli`; image
  tagging is a separate concern.
- GitHub Releases. A release page on GitHub can be created from
  a tag manually if useful for distributing artefacts; not done
  automatically.

## Why no automation

Tagging is irreversible-ish: a tag can be deleted but should not
be. Automating tagging is an attractive idea that historically
shipped at least one bad release per major OSS project that did
it. Kaleidoscope's posture is "tag is a deliberate act": the
maintainer reads this document, executes the commands, watches
the remote. The few minutes that costs is the audit trail.
