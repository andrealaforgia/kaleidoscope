# Codex v0 — Technology choices

## Runtime closure

Codex v0 has zero runtime dependencies beyond `std`. The crate is
a pure-Rust library with no async runtime, no network surface, no
serde, no tracing emit (per ADR-0024 §3 and §4).

| Layer | Choice | Licence | Rationale |
|---|---|---|---|
| Standard library | `std` (Rust stable per workspace MSRV) | n/a | All Codex needs at runtime is `Vec`, `HashSet`-equivalent lookup, `OnceLock` (in Spark's integration), `String`. |

## Build-time / dev / xtask

| Tool / crate | Where | Licence | Rationale |
|---|---|---|---|
| `opentelemetry-semantic-conventions = "=0.27"` | xtask only (regenerator) | Apache-2.0 | Source of the corpus; consumed at maintainer-trigger time, output checked into `crates/codex/src/generated/semconv_0_27.rs`. Per ADR-0023 + ADR-0024 §1. |
| Workspace test infra (cargo, rustc, clippy, fmt, deny, public-api, semver-checks, mutants) | dev / CI | various permissive | Same as the rest of the workspace; ADR-0005 Gate 1-5. |

## Cross-feature dependencies

| From | To | Direction | Notes |
|---|---|---|---|
| Spark | Codex | Spark adds Codex as runtime dep at slice 06 DELIVER | Path-resolved with version pin; AGPL crate inside Spark's runtime closure (Spark itself is Apache-2.0; the AGPL on Codex applies to Codex's source, not virally to downstream Spark consumers). |
| Codex | Spark | none | Codex does not depend on Spark; the dependency arrow is one-way. |
| Codex | Aperture | none at v0 | Aperture-side lint integration is a follow-up feature, not Codex v0 scope. |

## Licence audit

Codex's runtime-closure entry in `THIRD-PARTY-LICENSES.md` is a
single line: "no third-party runtime dependencies". The xtask
regenerator's transitive closure (the `opentelemetry-semantic-conventions`
crate plus its deps) is build-time-only and audited separately
under the workspace's xtask rules.

`cargo deny check` (Gate 4) sees zero new entries when Codex
v0 lands (the BSL-1.0 entry from Sieve's xxhash-rust covers the
last new licence; Codex adds none).

## Performance / memory budget

Codex's `validate(&[(&str, &str)])` runs in-process inside
`spark::init`, after Resource composition and before any OTel SDK
type is constructed. Budget per ADR-0022 §6 and ADR-0024 §3:

- Typical Resource (~10 attributes): under 1 ms wall-clock.
- Full upstream OTel semconv 0.27 fixture: under 10 ms.
- Memory: a few hundred bytes per Levenshtein call; otherwise
  zero allocations on the validate path (the catalogue is shared
  via `&'static SchemaCatalogue` from the `OnceLock` in Spark's
  init).

## Forward-compatibility

| When | What | Where |
|---|---|---|
| OTel semconv pin moves | Maintainer runs the xtask regenerator; `generated/semconv_0_27.rs` becomes `generated/semconv_0_28.rs` (or similar); the Cargo file's xtask dep pin updates. PR diff visible. | ADR-0023 |
| Codex v1 (gRPC daemon) | A separate `crates/codex-server` (or similar) ships alongside the library. Spark's runtime dep stays on the library; operators who run the daemon do so via Loom config, not via Spark. | Out of v0 scope; v1+ ADR. |
| Per-tenant overlays | Aegis lands; Codex's catalogue gains a per-tenant extension surface; the lint takes a tenant-id parameter. | Out of v0 scope; v1+ ADR. |
