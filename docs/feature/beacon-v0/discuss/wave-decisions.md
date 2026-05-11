# DISCUSS Decisions — beacon-v0

## Key decisions

- **[D1] Library + binary.** Beacon v0 ships a Rust crate (`beacon`)
  exposing the evaluator API, plus a `beacon-server` binary that
  wires the evaluator to a Prometheus HTTP backend and the
  integration sinks. The same shape as Aperture (library +
  service). The library is reusable for hosts that want to embed
  Beacon's evaluator without the daemon wrapping. (See:
  user-stories.md system constraint 1.)
- **[D2] PromQL HTTP backend at v0.** The only data source is the
  Prometheus HTTP API (`/api/v1/query`). Pulse and Lumen are not
  yet built; an operator runs Beacon against the same Prometheus /
  Mimir backend Prism queries. The ADR for the HTTP client shape is
  shared with Prism ADR-0027. (See: user-stories.md system
  constraint 3.)
- **[D3] CUE files on disk; SIGHUP reload.** Loom is the eventual
  Git-backed authority for rule definitions, but Loom is itself a
  later feature. Beacon v0 reads a directory of `.cue` files at
  startup and reloads on `SIGHUP`. The catalogue is in-memory only
  at v0; persisted state is each sink's accumulated firing/resolved
  log via Beacon's OTLP telemetry. (See: user-stories.md system
  constraint 4.)
- **[D4] Five sink kinds scoped.** Webhook, SMTP, Mattermost,
  Zulip, OnCall. Each is an adapter behind a single `Sink` trait.
  Per-rule routing. Secrets via environment variables named in
  CUE. (See: user-stories.md system constraint 5.)
- **[D5] Single tenant at v0.** No tenant scoping. Aegis is the
  tenancy authority and lands separately; until then Beacon is
  single-tenant by construction. The integration test corpus uses
  a single rule namespace. (See: user-stories.md system constraint
  3.)
- **[D6] Google SRE workbook MWMBR.** SLO burn-rate synthesis uses
  the workbook's published five-window multi-burn-rate
  configuration with the canonical threshold values (14.4, 6, 3,
  1). The workbook URL is cited in a Rust constant comment in the
  code generator. (See: slice-05 brief.)
- **[D7] Inhibition + grouping at v0.** The 20-rule storm scenario
  is the named load-bearing case. Without inhibition + grouping
  Beacon is a one-rule toy; with them it is a credible alerting
  layer. (See: slice-03 brief.)
- **[D8] No telemetry-on-telemetry.** Beacon emits OTLP traces and
  metrics to the operator's own Aperture deployment per the
  architecture doc's §A.2 contract. No third-party telemetry
  endpoints, no phone-home, no anonymous usage stats. (See:
  outcome-kpis.md cross-KPI guardrail.)
- **[D9] AGPL-3.0-or-later.** Same licensing as every platform
  component. The `LICENSING.md` posture is preserved. (See:
  user-stories.md system constraint 2.)
- **[D10] Evaluator is a pure function.** `(rule_set, fetch_fn,
  now)` → `EvaluationResult`. The binary wires `fetch_fn` to the
  real backend and a scheduler ticks the evaluator at the rule's
  interval. Same shape as Prism's reducer + Scheduler seam. (See:
  user-stories.md system constraint 10.)

## Requirements summary

- Primary user need: programmable alerting for a Kaleidoscope or
  operator-existing OTel backend, with declarative rule
  authoring, deterministic inhibition + grouping, and reliable
  delivery to standard incident sinks.
- Walking skeleton scope: one CUE rule → one Prometheus query →
  one webhook emission. (Slice 01.)
- Feature type: backend (service + library, no UI).

## Constraints established

- Beacon v0 cannot depend on Pulse, Lumen, Aegis, or Loom (none
  exist).
- The CUE schema is the contract; an unknown field or missing
  required field fails load with an operator-readable diagnostic.
- The integration test fixture is a digest-pinned `prom/prometheus`
  container (same posture as Prism's E2E fixture).

## Upstream changes

None. DISCOVER (the architecture doc) named Beacon as the alerting
layer over a PromQL-compatible backend; this DISCUSS reaffirms the
architecture without proposing changes to it.
