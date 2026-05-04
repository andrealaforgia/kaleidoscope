# Platform Architecture — `otlp-conformance-harness-v0` (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-05-03.
> **Author**: Apex.
> **Companion documents**: `ci-cd-pipeline.md`, `branching-strategy.md`,
> `kpi-instrumentation.md`, `environments.yaml`, `wave-decisions.md`.

---

## Scope

The "platform" for the OTLP conformance harness is, in its entirety:

1. A Cargo crate at `crates/otlp-conformance-harness/` (already shipped
   by DELIVER, 73/73 tests green, 100 % mutation kill rate).
2. The Cargo workspace at the repository root, declared in
   `Cargo.toml`, with the harness as its first member.
3. A GitHub Actions CI workflow at `.github/workflows/ci.yml` that runs
   the five ADR-0005 gates on every push to `main` and every pull
   request.

That is the whole platform. The harness is a *library*, not a service.
There is no deployment target, no container, no orchestrator, no cloud
runtime, no observability surface, no on-call rotation. Distribution is
via the GitHub repository (and, eventually, crates.io once a v0.1 or
later cuts a release).

This is deliberately and structurally small. The roadmap's
no-telemetry-on-telemetry commitment (Section A.2) and the harness's
DISCUSS-locked library-not-service framing (D1) jointly remove every
piece of infrastructure a typical DEVOPS wave would otherwise produce.

## What the platform is *not*

| Concern | Status | Rationale |
|---|---|---|
| Deployment target | None | Library, not service. |
| Container orchestration | None | Library, not service. |
| Observability stack (Prometheus, OTel collector, log aggregator) | None | The harness emits no telemetry by design (Section A.2). |
| Alerting | None | No runtime to alert on. |
| Secrets management | None | Crate has no runtime credentials. |
| External SaaS integrations | None | Roadmap A excludes bundled commercial SaaS. CI runs on GitHub Actions because the repository lives there; that is not an integration. |
| Multi-environment promotion (dev → staging → prod) | None | Source is a Cargo crate; "environments" means CI runner OS images and developer workstations. See `environments.yaml`. |
| Backups, DR, capacity planning | None | No state. |

The DEVOPS wave is therefore unusual in shape: most of its skill
template's surface area is non-applicable. What remains — and what this
wave delivers — is the CI contract's runner-specific YAML, the branch
protection posture, the KPI instrumentation, and the documented
toolchain provisioning rules.

## Components

```
+-----------------------------------------------------------------+
|                      Kaleidoscope repository                    |
|                                                                 |
|  +-------------------------------+   +------------------------+ |
|  |  Cargo workspace              |   |  CI surface            | |
|  |  Cargo.toml                   |   |  .github/workflows/    | |
|  |  rust-toolchain.toml (1.78)   |   |    ci.yml              | |
|  |  deny.toml                    |   |                        | |
|  |  CLAUDE.md                    |   |  Trigger:              | |
|  |                               |   |    push to main        | |
|  |  crates/                      |   |    pull_request to main| |
|  |    otlp-conformance-harness/  |   |                        | |
|  |      Cargo.toml               |   |  Gates (ADR-0005):     | |
|  |      src/                     |   |    4 cargo deny        | |
|  |      tests/                   |   |    1 cargo test        | |
|  |        vectors/               |   |    2 cargo public-api  | |
|  |      examples/                |   |    3 cargo semver      | |
|  |      README.md                |   |    5 cargo mutants     | |
|  +-------------------------------+   +------------------------+ |
|                                                                 |
+-----------------------------------------------------------------+
                              |
                              v
                    GitHub Actions runner (ubuntu-latest)
                    Stable Rust 1.78 (per rust-toolchain.toml)
                    Pinned nightly (NIGHTLY_PIN env var) for Gates 2-3
                    Cache: ~/.cargo/registry, ~/.cargo/git, target/
                    Artefacts:
                      verdict-counts.json (KPI 4, 90-day retention)
                      mutants.out/        (Gate 5, 30-day retention)
```

The GitHub Actions runner is the only "compute" the platform owns, and
it owns it for at most the wall-clock duration of a CI run. Outside
that window the platform is a directory of files in version control.

## Toolchain provisioning

| Tool | Source | When installed |
|---|---|---|
| Rust 1.78 (stable) | `rust-toolchain.toml` honoured by `dtolnay/rust-toolchain@stable` | Every CI job that builds or tests |
| Pinned nightly (`NIGHTLY_PIN`) | `dtolnay/rust-toolchain@master` with explicit `toolchain:` input | Gates 2 and 3 only |
| `cargo-deny` | `EmbarkStudios/cargo-deny-action` (SHA-pinned) | Gate 4 |
| `cargo-public-api` | `cargo install --locked` from crates.io | Gate 2 |
| `cargo-semver-checks` | `obi1kenobi/cargo-semver-checks-action` (SHA-pinned) | Gate 3 |
| `cargo-mutants` | `cargo install --locked` from crates.io | Gate 5 |

All four supplementary cargo tools are MIT/Apache-2.0 (verified in
ADR-0005). None are on the disqualified-licence list (Roadmap A.1).

## Storage and state

The platform has three categories of persistent state, all
version-controlled:

1. **Source of truth in git**: `Cargo.toml`, `Cargo.lock`,
   `rust-toolchain.toml`, `deny.toml`, `CLAUDE.md`, the harness crate,
   the workflow YAML, the corpus vectors. Every change goes through
   the CI pipeline before merging to `main`.
2. **Workflow artefacts** (ephemeral, GitHub-hosted):
   `verdict-counts.json` (KPI 4, 90-day retention) and `mutants.out/`
   (Gate 5 audit trail, 30-day retention).
3. **Cache** (ephemeral, GitHub-hosted): the Cargo registry, git
   index, and `target/` directory, keyed by `Cargo.lock` hash.
   Best-effort speed optimisation; correctness does not depend on the
   cache.

There is no database, no object store, no message queue, no
configuration server. The harness is stateless at compile time and at
runtime.

## Security posture

| Concern | Mechanism |
|---|---|
| Supply-chain hygiene (transitive licences) | `cargo deny check` (Gate 4) — RustSec advisories + licence allow-list. |
| Supply-chain hygiene (action versions) | Third-party actions pinned to commit SHAs in `ci.yml`. First-party actions (`actions/checkout`, `actions/cache`, `actions/upload-artifact`) pinned to major version tags maintained by GitHub. |
| Pinning policy (ADR-0003) | `cargo deny check` `bans` table verifies `opentelemetry-proto` is exact-pinned. |
| Public-surface drift | `cargo public-api` (Gate 2) refuses unannounced changes. |
| SemVer correctness | `cargo semver-checks` (Gate 3). |
| Test quality | `cargo mutants` (Gate 5), 100 % kill rate gate. |
| Branch protection | See `branching-strategy.md`. |
| Secrets in code | None to leak; the crate has no secrets. CI workflow declares no secrets, no `permissions: write-all`. Default `permissions: read` token. |

The posture is "supply chain in, no runtime out". The harness contains
no credentials, talks to no service, makes no network call. The CI
workflow's only outbound dependencies are crates.io (via cargo) and
the actions cache.

## Open questions for future waves

None blocking this DEVOPS wave. Two non-urgent items recorded for
later:

1. **crates.io publication**: when the harness reaches v0.1 (or v1.0),
   a release workflow will be added to publish to crates.io on tag.
   That is a future iteration's deliverable, not v0's. The current
   crate is `publish = false` in `Cargo.toml`.
2. **Workspace-level `cargo metadata` consistency check**: deferred per
   `shared-artifacts-registry.md > otlp_wire_format`. The harness is
   the only OTLP-proto consumer in v0, so the check is a no-op.

The crates.io publication question is the only piece of infrastructure
this v0 leaves unbuilt that v0.1 will need; everything else this
DEVOPS wave produces is sufficient through the harness's life.
