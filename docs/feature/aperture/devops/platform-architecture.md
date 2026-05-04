# Platform Architecture — `aperture` v0 (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-05-04.
> **Author**: Apex.
> **Companion documents**: `wave-decisions.md`, `ci-cd-pipeline.md`,
> `branching-strategy.md`, `kpi-instrumentation.md`,
> `observability-design.md`, `monitoring-alerting.md`,
> `environments.yaml`.

---

## Scope

The "platform" for Aperture v0 splits into two halves that the rest
of this document keeps clearly distinct:

1. **The Kaleidoscope-side platform** — the Cargo workspace, the
   GitHub Actions CI surface, the local quality gates, and the
   contributor environments. This is where this wave actually
   delivers configuration changes.
2. **The operator-side runtime** — the long-lived Aperture process
   under whatever orchestrator the operator runs (k8s sidecar, k8s
   deployment, systemd unit, bare-metal binary), the operator's log
   aggregator, the operator's configured downstream OTel-compatible
   backend. Kaleidoscope ships **none** of this; the v0 contract is
   "ship the binary, let the operator run it".

The harness's DEVOPS wave owned only half (1) because the harness is
a library. Aperture is a service, so half (2) exists; but the
operator owns it. The trick is to be honest about which side owns
what without over-engineering Kaleidoscope-side tooling for an
operator-side concern.

---

## What Kaleidoscope ships

```
Kaleidoscope side                                  Operator side
────────────────────────────────────────────       ──────────────────────────────────────
                                                                    
┌──────────────────────────────────────────┐                                            
│ Repository (CC0-1.0)                     │                                            
│                                          │                                            
│ Cargo workspace                          │      ┌──────────────────────────────────┐  
│  ├── crates/otlp-conformance-harness/    │      │ Operator's runtime environment   │  
│  └── crates/aperture/      <-- this wave │      │                                  │  
│        ├── src/                          │      │ ┌──────────────────────────────┐ │  
│        ├── tests/                        │      │ │ Aperture binary (operator-   │ │  
│        ├── Cargo.toml                    │      │ │ deployed)                    │ │  
│        └── (DELIVER fills src/ green)    │      │ │  ├── port 4317 (gRPC)        │ │  
│                                          │      │ │  ├── port 4318 (HTTP)        │ │  
│ rust-toolchain.toml (1.85)               │      │ │  ├── /healthz                │ │  
│ Cargo.lock                               │      │ │  ├── /readyz                 │ │  
│ deny.toml                                │      │ │  └── stderr (JSON-lines)     │ │  
│ CLAUDE.md                                │      │ └────────────┬─────────────────┘ │  
│ scripts/hooks/{pre-commit,pre-push}      │      │              │                   │  
│                                          │      │              │ ForwardingSink    │  
│ .github/workflows/ci.yml                 │      │              ▼                   │  
│  ├── gate-4-deny  (workspace-wide)       │      │ ┌──────────────────────────────┐ │  
│  ├── gate-1-test  (-p harness, evolves)  │      │ │ Operator's downstream OTel-  │ │  
│  ├── gate-2-public-api (-p harness)      │      │ │ compatible backend           │ │  
│  ├── gate-3-semver  (-p harness)         │      │ │  (Loki, Tempo, Mimir,        │ │  
│  ├── gate-5-mutants (-p harness, evolves)│      │ │   OTel Collector, etc.)      │ │  
│  │                                       │      │ └──────────────────────────────┘ │  
│  └── (future, DELIVER-wired)             │      │                                  │  
│      ├── gate-6-architectural-rules      │      │ ┌──────────────────────────────┐ │  
│      ├── gate-7-no-telemetry-on-telemetry│      │ │ Operator's log aggregator    │ │  
│      └── gate-8-probe-gold-runner        │      │ │  (Loki, Splunk, ELK,         │ │  
│                                          │      │ │   journald + journalctl,     │ │  
│ docs/feature/aperture/devops/            │      │ │   etc.)                      │ │  
│  ├── this directory's 8 artefacts        │      │ │                              │ │  
│                                          │      │ │ Captures Aperture's stderr   │ │  
└──────────────────────────────────────────┘      │ └──────────────────────────────┘ │  
              │                                   │                                  │  
              │ "ship the binary"                 │ ┌──────────────────────────────┐ │  
              ▼                                   │ │ Operator's monitoring &      │ │  
       (cargo build --release;                    │ │ alerting (operator-owned)    │ │  
        operator copies the binary;               │ │                              │ │  
        operator runs it under                    │ │ Documented as queries in     │ │  
        their orchestrator)                       │ │ monitoring-alerting.md;      │ │  
                                                  │ │ no Kaleidoscope-side         │ │  
                                                  │ │ infrastructure.              │ │  
                                                  │ └──────────────────────────────┘ │  
                                                  └──────────────────────────────────┘  
```

---

## What Kaleidoscope does *not* ship

| Concern | Kaleidoscope's posture | Where it really lives |
|---|---|---|
| A built binary distribution channel (e.g. crates.io publish, GitHub Releases artefacts) | None at v0 (`publish = false` in `Cargo.toml`) | Future release wave when v0.1 cuts; operators currently `cargo build --release` themselves |
| A reference container image (`Dockerfile`, OCI bundle) | None at v0 | Roadmap Phase Spark/Aegis |
| A reference k8s manifest (Deployment, Service, HPA, NetworkPolicy) | None at v0 | Operator-supplied; reference manifests are a future iteration |
| A reference `helm` chart or Kustomize overlay | None | Same |
| A reference `systemd` unit file | None | Operator-supplied |
| A monitoring agent or Prometheus exporter | **Forbidden** at v0 by no-telemetry-on-telemetry (DISCUSS Q6, ADR-0009) | Pulse (Phase 4) |
| A Grafana dashboard JSON | None | Documented as query shapes operators translate to their preferred system; see `monitoring-alerting.md` |
| Alerting rules in Prometheus / VictoriaMetrics / Splunk format | None | Documented as query shapes; see `monitoring-alerting.md` |
| A reference deployment-strategy automation (Argo Rollouts manifest, Flagger, etc.) | None | Operator's orchestrator-of-choice handles rolling deployments naturally |
| Backups / DR / capacity planning for an Aperture-hosted environment | None | Aperture is stateless; operators run as many replicas as their fleet demands |

The boundary is unambiguous: anything that runs in production is
operator-owned. Kaleidoscope ships source, CI gates, and
documentation. Future phases (Spark for packaging, Aegis for security,
Pulse for telemetry) extend the Kaleidoscope-side surface
deliberately, never opportunistically.

---

## Components (Kaleidoscope side)

### CI workflow (`.github/workflows/ci.yml`)

Five existing jobs (the harness's ADR-0005 gates) plus three future
Aperture-specific jobs (wired by DELIVER's per-slice commits per
`wave-decisions.md > A5`):

| Job | Owner | Scope at this wave's close | Scope after DELIVER closes |
|---|---|---|---|
| `gate-4-deny` | Workspace | Workspace-wide; covers Aperture transitively | Workspace-wide |
| `gate-1-test` | Harness | `-p otlp-conformance-harness --all-targets --locked` | `--workspace --all-targets --locked` |
| `gate-2-public-api` | Harness | `-p otlp-conformance-harness` | Add `-p aperture` (aperture's library surface is `aperture::testing` only — DESIGN ADR-0007 + DISTILL D2) |
| `gate-3-semver` | Harness | `-p otlp-conformance-harness` | Add `-p aperture` |
| `gate-5-mutants` | Harness | `--package otlp-conformance-harness` | Add `--package aperture` |
| `gate-6-architectural-rules` (future) | Aperture | Not wired | Wired at Slice 03 (xtask AST walk) |
| `gate-7-no-telemetry-on-telemetry` (future) | Aperture | Not wired | Wired at Slice 06 (network-namespace integration test) |
| `gate-8-probe-gold-runner` (future) | Aperture | Not wired | Wired at Slice 06 (behavioural-layer probe gold-test) |

The workflow's trigger surface, concurrency policy, permissions, and
caching are unchanged (inherited from the harness's DEVOPS wave; see
`ci-cd-pipeline.md` for the full job-graph and gate ordering).

### Cargo workspace

Already extended at DISTILL (root `Cargo.toml` lists
`crates/aperture` as a member; `crates/aperture/Cargo.toml` exists
with the production deps and dev-dependencies DELIVER will promote
slice by slice). This wave does **not** modify the workspace
membership; it makes one targeted edit to
`crates/aperture/Cargo.toml` to clear the cargo-deny wildcard
warning per `wave-decisions.md > A3`.

### Local quality gates (`scripts/hooks/`)

Inherited from the harness's DEVOPS wave. The pre-commit hook's
current `cargo test --workspace --exclude aperture` invocation is
the only Aperture-specific concession; DELIVER's last commit removes
the exclusion (per `wave-decisions.md > A6`). No file changes in
this wave.

### Documentation surface

This `docs/feature/aperture/devops/` directory holds the eight
artefacts named in `wave-decisions.md > DEVOPS wave summary`. The
peer-review-iteration files (post-review) live alongside them.

---

## Components (operator side)

These are documented for the operator's benefit but Kaleidoscope
neither hosts nor configures them. They are listed because the
"platform architecture" of a service is incomplete without them.

### Aperture process

A single static Rust binary built via `cargo build --release -p
aperture`. Operators run it as a long-lived process with:

- two listener ports bound (default 4317 gRPC, 4318 HTTP; both
  configurable via `bind_address` in TOML config or
  `APERTURE__GRPC__BIND_ADDRESS` / `APERTURE__HTTP__BIND_ADDRESS`
  env vars per ADR-0008);
- a TOML config file (path passed via `--config`);
- stderr piped into the operator's log aggregator (the configured
  log destination on the operator's side; Aperture writes JSON
  lines unconditionally and does not care where they go);
- SIGTERM honoured for graceful shutdown (drain-respecting per
  DISCUSS D8; `terminationGracePeriodSeconds: 30` in k8s aligns
  with Aperture's default `drain_deadline_ms`).

### Configured downstream OTel-compatible backend

When `sink = "forwarding"` in Aperture's config, Aperture POSTs
OTLP/HTTP/protobuf to the operator-supplied endpoint. The operator
chooses the backend — Loki, Tempo, Mimir, OpenTelemetry Collector,
proprietary OTLP receiver, etc. — and configures Aperture's
`forwarding.endpoint` and `forwarding.timeout_ms`.

The probe contract (ADR-0007 Earned-Trust section) catches
misconfiguration at startup: Aperture's `ForwardingSink::probe()`
does an OPTIONS request to the configured endpoint (with a fallback
to a known-empty `POST /v1/logs` for OTel Collectors that return 405
to OPTIONS) and refuses to start with `event=health.startup.refused`
if the downstream is not reachable / not honouring the contract.

### Operator's log aggregator

Operator-supplied. Captures Aperture's stderr. Examples:

- **k8s + Loki**: kubelet captures container stderr; `promtail`
  ships it to Loki; LogQL parses the structured JSON.
- **k8s + Splunk**: Splunk Connect for Kubernetes captures stderr;
  Splunk Search Processing Language parses JSON.
- **systemd + journald + journalctl**: `journalctl -u aperture
  --output=json` produces JSON-lines containing Aperture's stderr
  events embedded as a string field; alternatively, a
  `journalctl ... | jq` pipeline.
- **Vector / Fluent Bit / Filebeat → ELK**: any ingestion pipeline
  that handles JSON-lines.

The structured event vocabulary (DISCUSS D1, DESIGN ADR-0009) is
the schema the operator's queries target; see
`observability-design.md > Operator query patterns` for sample
queries per outcome KPI.

### Operator's monitoring and alerting

Same as the log aggregator: operator-supplied. The four DISCUSS
handoff alerting rules ("`sink_accepted/request_received` per
transport drops below 95% sustained for 5 min → page", etc.) are
documented in `monitoring-alerting.md` as queries operators translate
to their preferred alerting system. Aperture itself emits no alerts.

---

## Toolchain provisioning

Inherited from the harness's DEVOPS wave; **no Aperture-specific
additions**.

| Tool | Source | Used by |
|---|---|---|
| Rust 1.85 (stable) | `rust-toolchain.toml` honoured by `dtolnay/rust-toolchain@v1` | Every CI job that builds or tests; local development |
| Pinned nightly (`NIGHTLY_PIN`) | `dtolnay/rust-toolchain` with explicit `toolchain` input | Gates 2 and 3 |
| `cargo-deny` | `taiki-e/install-action` (precompiled binary) | Gate 4 |
| `cargo-public-api` | `taiki-e/install-action` | Gate 2 |
| `cargo-semver-checks` | `taiki-e/install-action` | Gate 3 |
| `cargo-mutants` | `taiki-e/install-action` | Gate 5 |
| `python3` | system-installed (used only by harness's KPI 4 capture step) | Gate 1 (harness only) |

Aperture's three future gates (`6`, `7`, `8`) use plain `cargo` and
`cargo run -p xtask` (when the xtask binary lands at Slice 03); no
new third-party actions, no new CI tools, no new toolchain
requirements. The Linux net-ns test in Gate 7 uses the `nix` crate
(MIT-licensed; transitive dep added in Slice 06's `[dev-dependencies]`).

---

## Storage and state

Same as the harness's posture (no runtime, no database, no message
queue, no configuration server **on the Kaleidoscope side**). The
operator-side runtime is stateless by design (per DISCUSS Q4: no
internal queue; per DESIGN: no on-disk persistence at v0).

| Category | Location | Retention |
|---|---|---|
| Source of truth | git (`main`) | Forever |
| Workflow artefacts | GitHub Actions storage | 30–90 days per artefact |
| Cache (CI) | GitHub Actions cache | Best-effort; correctness-independent |
| Aperture runtime state | RAM only (no disk) | Process lifetime |
| Aperture configuration | TOML file on operator's filesystem | Operator-managed |

---

## Security posture

Inherited from the harness's posture, with two Aperture-specific
additions:

| Concern | Mechanism (Kaleidoscope side) | Mechanism (operator side) |
|---|---|---|
| Supply-chain hygiene (transitive licences + advisories) | `cargo deny check` (Gate 4); workspace-wide; covers Aperture's deps | n/a |
| Supply-chain hygiene (action versions) | Third-party actions SHA-pinned in `ci.yml` | n/a |
| Public-surface drift | `cargo public-api` (Gate 2); covers Aperture once Gate 2 graduates per A2 | n/a |
| SemVer correctness | `cargo semver-checks` (Gate 3); same evolution | n/a |
| Test quality | `cargo mutants` (Gate 5); same evolution | n/a |
| Architectural rule enforcement (Aperture-specific) | xtask AST walks (Gate 6, future) | n/a |
| No outbound telemetry beyond ForwardingSink (Aperture-specific) | network-namespace test (Gate 7, future) | Operator's network monitoring (defence-in-depth) |
| Probe contract for ForwardingSink (Aperture-specific) | Probe gold-test (Gate 8, future) | Probe runs at startup; failure exits |
| Branch protection | See `branching-strategy.md` (relaxed; trunk-based) | n/a |
| Secrets in code | None to leak; Aperture has no embedded secrets | Operator's secrets management (TLS material in Phase 2 Aegis; v0 plaintext) |
| Plaintext at v0 | Forward-compatible TLS schema knob (DISCUSS Q5; ADR-0008) | Operator MAY front Aperture with an in-cluster TLS-terminating proxy until Aegis lands |
| Default user-agent on outbound (`ForwardingSink`) | `aperture/{version}` (ADR-0006 D8); identifiable in downstream logs | n/a |

The security posture is "supply chain in, deliberate scope out". v0
is plaintext by deliberate scope; the schema knob makes the Phase-2
Aegis migration non-breaking.

---

## Rollback posture

Aperture v0 has no Kaleidoscope-side rollback procedure because
Kaleidoscope does not deploy Aperture. The rollback contract for
operators is:

1. **Re-deploy the previous binary version under your orchestrator.**
   Aperture is a single static binary; the operator's orchestrator-of-
   choice (k8s Deployment image tag, systemd unit binary path, etc.)
   names which version is live.
2. **Aperture's drain-respecting shutdown supports rolling
   deployments naturally** (DISCUSS D8; ADR-0009). A rolling restart
   from version `v0.x` to `v0.y` is the default deployment strategy
   per `wave-decisions.md > D6`; in-flight requests drain before
   the previous instance exits, with `drain_deadline_ms` controlling
   the upper bound.
3. **Configuration rollback is symmetric**: configuration is read
   at startup only (no SIGHUP reload at v0; per DISCUSS Q4 implicit
   and DESIGN ADR-0008). Rolling a config change back is a process
   restart with the previous TOML.
4. **No data rollback exists**: Aperture is stateless. Records that
   were sink-accepted before rollback are durably handed to the
   downstream backend (per ForwardingSink contract); records still
   in-flight at SIGTERM either complete within the drain deadline or
   are explicitly named by `event=drain_deadline_exceeded
   dropped_count=N`. Either way there is no Aperture-side state to
   reconcile.

The four DISCUSS-handoff rollback-tier alerting rules (downstream-
acceptance ratio drop, `/healthz` non-200, `concurrency_cap_hit`
emergence, unexpected outbound network traffic) named in DISCUSS
US-AP-08's KPI 7 are the operator-side triggers for considering a
rollback. They are documented in `monitoring-alerting.md`.

This wave does not author a Kaleidoscope-side rollback runbook
because there is nothing on the Kaleidoscope side to roll back. A
future Phase-1+ wave that introduces a release pipeline (crates.io
or GitHub Releases binary artefacts) will own a release-cadence
rollback procedure for that channel.

---

## DORA metrics posture

The DORA metrics map onto Aperture v0 as follows. Like the harness's
mapping, "deployment" splits into "Kaleidoscope-side merge to `main`"
and "operator-side binary deployment"; the former is what
Kaleidoscope can measure, the latter is operator-side.

| Metric | Kaleidoscope-side | Operator-side |
|---|---|---|
| Deployment frequency | Merge-to-`main` frequency. Target: every conforming commit lands; rolling DELIVER-cycle target ~daily during DELIVER; structurally Elite-band by trunk-based discipline. | Operator-determined. Aperture's drain-respecting shutdown supports daily rollouts. |
| Lead time for changes | Time from author's `git push` to merged-on-`main` (gates passing). Target: < 30 minutes including all five gates; same as harness. | Operator-determined; bounded above by the operator's CI/CD cycle. |
| Change failure rate | % of `main` commits that subsequently require a revert. Target: 0 % structural via the gates; same as harness. (Branch protection is relaxed; the discipline is "main is socially always green" per harness's post-merge correction.) | Operator-determined. KPI 8 (graceful-restart drop ratio) is the Aperture-side input to operator change-failure rate during a rolling restart. |
| Time to restore | Wall-clock from a red `main` to green `main`. Target: < 1 hour, achieved by reverting / fix-forward; same as harness. | Operator-determined. KPI 8 + KPI 7 (downstream-acceptance ratio) are the Aperture-side inputs. |

Aperture is structurally in the **Elite** band on the Kaleidoscope-
side metrics for the same reasons the harness is. The operator-side
DORA metrics are not Kaleidoscope's to claim.

---

## Aperture-specific deployment-strategy guidance (operator-facing)

Per `wave-decisions.md > D6`, the recommended deployment strategy is
**rolling**, supported naturally by Aperture's drain-respecting
shutdown.

| Strategy | Suitability for Aperture v0 | Notes |
|---|---|---|
| **Rolling** | **Recommended.** | Aperture's `terminationGracePeriodSeconds`-aligned drain (DISCUSS D8) means k8s rolling deployments work out of the box. SDK retries (gRPC RESOURCE_EXHAUSTED, HTTP 503) handle transient saturation during the rollout. `maxUnavailable: 0` recommended; Aperture is fast to start (~50 ms per `outcome-kpis.md`'s KPI 8 budget note). |
| **Blue-green** | Possible. | Two Aperture replica sets behind a Service; switch the Service's selector. Aperture is stateless, so the switch is non-destructive. Useful if the operator has strict traffic-version-isolation requirements. Document only; not the default recommendation. |
| **Canary** | Possible but heavyweight at v0. | Requires a traffic-splitting proxy in front of Aperture (Argo Rollouts, Flagger, Linkerd, etc.). Aperture itself emits no metrics for canary analysis (no-telemetry-on-telemetry); the canary signal would have to come from the operator's downstream-side success ratio. Workable, but the operational complexity is disproportionate at v0. |
| **Progressive delivery** (feature flags + canary) | Not at v0. | Aperture has no feature-flag surface; configuration is restart-only. |

The default operator path is rolling. Blue-green is documented as a
workable alternative for risk-sensitive operators. Canary and
progressive delivery are flagged as Phase-1+ concerns once Pulse
provides telemetry-on-telemetry that canary analysis would consume.

---

## Open questions for future waves

1. **Release workflow**: when v0 reaches a tagged release (e.g.
   v0.1, or when Aperture is bundled into a Phase-1 distribution),
   a release workflow will be added. That workflow owns:
   (a) a GitHub Releases artefact upload (built binaries per OS
   target);
   (b) a crates.io publication (if Aperture ever becomes
   `publish = true`);
   (c) the KPI 5 / KPI 8 release-cadence load-test gates (per
   `wave-decisions.md > A8/Q5`);
   (d) a release-cadence rollback procedure for the publication
   channel.
2. **Reference container image / k8s manifest**: if pilot operators
   request reference deployment artefacts in Phase 1, a future wave
   adds a `Dockerfile` + reference manifests under
   `crates/aperture/deploy/`. Phase Spark / Aegis owns that surface.
3. **Per-OS CI matrix**: if a Windows-native or macOS-native
   deployment scenario emerges, add the corresponding runner to
   `gate-1-test`'s matrix. Not in v0.
4. **Pact-style contract test for the operator-supplied downstream
   OTLP backend**: deferred per `wave-decisions.md > Q4`.

None of the four blocks this wave's close.

---

## Summary

Aperture v0's platform architecture is deliberately split across the
Kaleidoscope-side and the operator-side. Kaleidoscope ships source,
CI gates, and documentation. Operators ship the binary, the runtime,
the dashboards, the alerts, and the rollback procedure. The boundary
is unambiguous and the documentation reflects it honestly.

The Kaleidoscope-side platform extends the harness's DEVOPS-wave
posture by one workspace member, three future-wired CI gates, and a
half-dozen operator-facing documents. No new infrastructure is
introduced; no new tooling is required; no new runtime is hosted.

The platform is small for the same reason the harness's was: the
service it ships does one job (validate-and-route OTLP) and does it
without bringing infrastructure baggage. Pulse, Aegis, Spark, and
Sluice will each grow the surface deliberately when their phase
arrives.
</content>
</invoke>
