# Releasing

This document records the release process for Kaleidoscope's
per-crate version tags. It is a tool for the maintainer to execute
when the workspace is in a tagsable state, not a documentation of
automation. There is no automation; tagging is a deliberate act.

## Tag format

Per-crate tags. Each tag is `<crate>/<semver>`. Examples:

- `aperture/v0.1.0`
- `lumen/v0.1.0`
- `cinder/v0.1.0`
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

## Milestone "v1" versus semver 1.0.0

The README and narrative call the durable storage pillars and the
durable alerting pillar "v1". That is a milestone designation (the
in-memory v0 adapter has been replaced by a file-backed durable
adapter behind the same trait, surviving restart). It is NOT the same
as a semver 1.0.0 release, which is a public promise of API stability.

Every crate's `Cargo.toml` still declares `version = "0.1.0"`, and the
platform is "implementation in progress". So graduation tags follow
the manifest version: each crate is tagged at `v0.1.0`, matching its
`Cargo.toml` and the convention the first five tags already set
(`aperture/v0.1.0`, `codex/v0.1.0`, `otlp-conformance-harness/v0.1.0`,
`sieve/v0.1.0`, `spark/v0.1.0`). Tagging a durable-milestone crate at
`v1.0.0` while its manifest says `0.1.0` would either make the tag
disagree with the manifest or force a major-version bump across every
dependent, and it would claim an API-stability guarantee the project
has not yet chosen to make.

A future semver `1.0.0` bump is a separate, deliberate decision: it
means committing to backwards compatibility, and it requires a
coordinated bump of the crate manifests plus every dependent's version
requirement plus `Cargo.lock`. That decision is the maintainer's to
make when the platform is ready to promise API stability; it is not
implied by reaching the durable-v1 milestone.

## Graduated set as of 2026-05-21

Graduation is complete. Every workspace crate now carries a `v0.1.0`
tag on `main` except `integration-suite`, which is a cross-crate test
harness with no shippable public surface (`publish = false`, tests
only) and is deliberately not tagged.

Tagged on 2026-05-21 (this round): `lumen`, `cinder`, `sluice`,
`pulse`, `ray`, `strata` (the six durable storage pillars), `beacon`
(alerting with durable rule state), `aegis` (the foundational
`TenantId` contract), `augur` (anomaly observers), `self-observe`
(MetricsRecorder bridges), `kaleidoscope-cli` (operator binary),
`beacon-server` (alerting binary), and `loom` (dashboards-as-code).

Already tagged in earlier rounds: `aperture`, `codex`,
`otlp-conformance-harness`, `sieve`, `spark`.

Tagged later on 2026-05-21, when the OTLP-to-durable pipeline shipped:
`aperture-storage-sink` (the storage `OtlpSink` that persists OTLP into
the pillars) and `kaleidoscope-gateway` (the runnable gateway binary
that wires the sink in). With these the platform runs end to end: a
client's OTLP reaches durable storage through a single deployable.

Tagged when the read loop closed: `query-api` (a Prometheus-compatible
`/api/v1/query_range` HTTP service over the durable Pulse store, the
read side prism queries). With it the loop is complete: ingest, store,
query, see.

Pre-flight for the whole round: `cargo build --workspace` green,
`cargo test --workspace` green, `cargo fmt --check` and `cargo clippy
-- -D warnings` green on `main`. The six storage pillars and beacon
each carry a `tests/v1_slice_NN_*.rs` durable suite; beacon and the
storage pillars meet their Gate 5 mutation kill rate.

## Commands used

```bash
# From the workspace root, on `main`, with the working tree clean.
# All tags at v0.1.0, matching each crate's Cargo.toml version.

for tag in \
  lumen/v0.1.0 cinder/v0.1.0 sluice/v0.1.0 \
  pulse/v0.1.0 ray/v0.1.0 strata/v0.1.0 \
  beacon/v0.1.0 aegis/v0.1.0 augur/v0.1.0 \
  self-observe/v0.1.0 kaleidoscope-cli/v0.1.0 \
  beacon-server/v0.1.0 loom/v0.1.0; do
    git tag -a "$tag" -m "$tag"
    git push origin "$tag"
done
```

Each `git tag -a` takes an annotated message; the messages used name
the crate's role and its durable adapter where it has one.

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
