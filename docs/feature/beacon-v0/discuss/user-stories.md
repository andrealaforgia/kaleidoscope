# Beacon v0 — user stories

Five LeanUX user stories with mandatory Elevator Pitches per the nWave
DISCUSS template. Personas drawn from `acme-observability`, the same
fictional team Spark, Sieve, Codex, and Prism have been built for.

The principal user is **Sasha, a platform engineer** authoring the
alert and SLO rule catalogue that runs in production. Sasha's job is
to encode the team's "things we want to wake someone up about" as
versioned, reviewable CUE that the rest of the team can audit.

The secondary user is **Riley, an SRE** on the receiving end of
Beacon's incident emissions. Riley pages at 03:14 with the alert
body in their hand and needs every field — name, severity, query
URL, runbook link — to be exactly what Sasha wrote, with no
re-formatting and no truncation.

System constraints (apply to every story):

1. Library plus binary. Beacon v0 ships as a Rust crate (`beacon`)
   exposing a public evaluator API, plus a thin binary
   (`beacon-server`) that wires the evaluator to a Prometheus
   PromQL HTTP backend and the integration sinks.
2. AGPL-3.0-or-later. Same licensing posture as Aperture, Sieve, and
   Prism per `LICENSING.md`.
3. The PromQL HTTP backend is the only data source at v0. Pulse and
   Lumen are not yet built; an operator runs Beacon against the same
   Prometheus / Mimir backend Prism queries. ADR pinning the
   `prometheus_http_query_range` shape is shared with Prism's
   ADR-0027.
4. Rule definitions live in CUE files on disk at v0. Loom's
   Git-backed authority is a v1 deliverable; v0 reads a directory
   of `.cue` files and re-loads on `SIGHUP` (per the operator's
   cadence preference). No tenant scoping at v0 — Aegis is the
   tenancy authority and lands separately.
5. Five integration sinks are scoped for v0: webhook (universal),
   SMTP (operational fall-back), Mattermost, Zulip, and Grafana
   OnCall (AGPL-3.0). Each sink ships as its own adapter under
   `crates/beacon/src/sinks/`. Adapter selection is per-rule.
6. SLO burn-rate evaluation follows Google SRE workbook §14.4 / §6 /
   §1: multi-window-multi-burn-rate (MWMBR) with the standard
   five-window configuration (1h/5m, 6h/30m, 1d/2h, 3d/6h, plus the
   page/ticket distinction).
7. Inhibition and grouping prevent storms. The grouping key is the
   rule label set; inhibition rules are CUE-declared (`inhibited_by`)
   and resolved before emission.
8. Schema validation. Beacon's CUE rule schema is the contract; an
   unknown field, missing required field, or type mismatch fails
   load with an operator-readable diagnostic that names the file,
   line, and offending construct (mirror of Codex's lint posture).
9. No telemetry-on-telemetry. Beacon does not phone home. Its own
   observability is OTLP traces and metrics emitted to the same
   Aperture deployment the operator already runs (the recursive
   bootstrap: Kaleidoscope observing Kaleidoscope, per the
   architecture doc §A.2).
10. The evaluator is a pure function. Given `(rule_set, fetch_fn,
    now)` it returns `EvaluationResult { fired, pending, resolved,
    inhibited }`. The binary wires `fetch_fn` to the real backend
    and a scheduler ticks the evaluator at the rule's configured
    interval. Same shape as Prism's reducer + Scheduler seam.

---

## US-BE-01 — Walking skeleton: one CUE rule, one webhook

### Elevator Pitch

- **Before**: there is no Beacon code; Sasha wants a programmable
  alerting layer that reads from the team's existing Prometheus
  backend, but their current path is to keep hand-editing Grafana
  alert rules through the UI, which has no review trail and
  silently breaks when Grafana upgrades.
- **After**: run `beacon-server --rules ./rules/`, where `rules/`
  contains a single CUE file `down.cue` declaring an alert
  `service_down` with `query: "up == 0"`, `for: "1m"`, `webhook:
  {url: "https://ops.acme/alerts"}` → sees Beacon evaluate the
  rule every 30 s against `http://localhost:9090`, and when the
  test `prometheus_mock` returns `up == 0` for 1 minute, emit a
  single POST to the webhook URL with a JSON body containing the
  rule name, query, and start timestamp.
- **Decision enabled**: Sasha confirms Beacon's load → evaluate →
  emit loop works end-to-end against a real Prometheus, and the
  team can build the rest of the rule schema and sinks on top.

### Acceptance criteria

- AC-1.1 — On startup, `beacon-server` loads every `.cue` file under
  the `--rules` directory. Load failures (parse error, schema
  violation) print a diagnostic with file, line, and offending field
  to stderr and exit with non-zero status.
- AC-1.2 — A single rule loaded from CUE produces a `Rule` struct
  with the canonical fields: `name`, `query`, `for_duration`,
  `interval`, `severity`, `labels`, `sinks`.
- AC-1.3 — The evaluator queries the Prometheus HTTP API at the
  configured `interval` (default 30 s) using the `query` and
  classifies the result into one of:
  `Inactive | Pending(since) | Firing(since) | Resolved(at)`.
  `Pending` transitions to `Firing` when the condition has held for
  the rule's `for_duration`.
- AC-1.4 — On `Firing` transition, exactly one POST is emitted to
  the webhook URL with a JSON body containing `name`, `query`,
  `severity`, `labels`, and `started_at` ISO-8601.
- AC-1.5 — On `Resolved` transition, exactly one POST is emitted
  containing the same fields plus `resolved_at`.
- AC-1.6 — The webhook payload is byte-identical for repeat firings
  of the same rule against the same data — no `now()` leakage
  except in `started_at` and `resolved_at`.

### KPI anchor

- KPI 1 (Time-to-first-alert): the walking skeleton must fire a
  webhook within `interval + for_duration + 5 s` of the underlying
  condition being true, on a 60-row test harness.

---

## US-BE-02 — CUE rule catalogue: many rules, one diagnostic

### Elevator Pitch

- **Before**: Sasha can ship one CUE rule. The team wants 35 rules
  covering all of acme-observability's services, but hand-loading
  35 files one at a time is impractical and a single typo in one
  file should not silently disable the entire catalogue.
- **After**: run `beacon-server --rules ./rules/` with 35 CUE files
  under `rules/` → sees the loader summarise `loaded 34 rules,
  rejected 1 with detail: rules/payments-checkout.cue:12: unknown
  field "thresehold" (did you mean "threshold"?)`; the 34 good rules
  are evaluated and the 1 bad rule is rejected with the operator
  shown exactly what to fix. Adding an `severity: critical` field
  loads cleanly on next start.
- **Decision enabled**: Sasha confirms Beacon's CUE schema is
  defensive enough to scale to the full alert catalogue without
  silent failures, and the team commits to migrating off Grafana's
  alert UI.

### Acceptance criteria

- AC-2.1 — The CUE schema for a rule is documented in
  `crates/beacon/cue/rule.cue` and includes: `name` (required,
  string), `query` (required, PromQL string), `for_duration`
  (optional, default "1m"), `interval` (optional, default "30s"),
  `severity` (required, one of `info | warning | critical`),
  `labels` (optional map), `annotations` (optional map containing
  `summary` and `runbook_url`), `sinks` (required list of sink
  references).
- AC-2.2 — An unknown field at any depth fails load with a
  diagnostic naming the file, line, and `nearest_blessed_match`
  using Codex's edit-distance suggestion (mirrors `tenat.id` →
  `tenant.id`).
- AC-2.3 — A missing required field fails load with the same shape:
  file, line, missing field name.
- AC-2.4 — A type mismatch (e.g. `severity: 42`) fails load with a
  diagnostic naming the file, line, and expected type.
- AC-2.5 — Multiple rejected rules each produce one diagnostic line;
  good rules continue loading. Exit code 0 if at least one rule
  loaded; exit code 1 if all rules rejected.
- AC-2.6 — Reloading the catalogue via `SIGHUP` triggers the same
  validation pass without restarting the process; the previous
  catalogue stays active until the new one validates.

### KPI anchor

- KPI 2 (Catalogue diagnostic recall): on a 50-rule corpus where 5
  are intentionally broken in different ways, every broken rule
  produces a diagnostic with file + line + field name. Zero false
  positives on the 45 valid rules.

---

## US-BE-03 — Evaluation engine with grouping and inhibition

### Elevator Pitch

- **Before**: with 34 rules loaded, a backend outage trips 20 alerts
  at once. Riley's pager goes off 20 times in 90 seconds and they
  cannot read any single alert in the storm. The team needs Beacon
  to group related alerts and suppress noise alerts when an upstream
  rule fires.
- **After**: with CUE declaring `service_down` as `inhibits:
  ["high_latency", "elevated_errors"]` for the same service, when a
  Prometheus outage trips `prometheus_unavailable` (severity:
  critical), Riley's pager gets one notification naming the
  upstream rule, with the 19 inhibited alerts listed in the body —
  not 20 separate notifications. Resolution of the upstream rule
  emits a single "all clear" with the resolved inhibitees.
- **Decision enabled**: Sasha confirms Beacon's storm-suppression
  primitives are sufficient for the team's incident-response shape,
  and the team can disable the Grafana-side grouping that was
  causing duplicate noise.

### Acceptance criteria

- AC-3.1 — Each rule emits incidents keyed by a `grouping_key`
  derived from the rule's `labels` and matching `query` labels. Two
  incidents with the same `grouping_key` are grouped into one
  emission per sink per cycle.
- AC-3.2 — A rule may declare `inhibits: [other_rule_names...]` in
  CUE. When the inhibiting rule is `Firing` and the inhibited rule
  is also `Firing`, the inhibited rule's emissions are suppressed.
  The grouping key still records the inhibited rule for the
  upstream notification's body.
- AC-3.3 — When the inhibiting rule resolves, the previously
  inhibited rules emit their state as of the resolution timestamp.
  No "phantom alerts" — only rules still `Firing` after un-inhibition
  emit.
- AC-3.4 — Grouping and inhibition logic is deterministic: the same
  set of inputs at time T produces byte-identical sink emissions.
  This is proven by a property test exercising 50 randomly generated
  rule sets.

### KPI anchor

- KPI 3 (Storm reduction ratio): on a 20-rule simultaneous failure
  scenario with one upstream rule declared as the inhibitor, the
  number of webhook emissions is `1 + (resolutions)`, not 20.

---

## US-BE-04 — Multiple integration sinks: webhook + SMTP + Mattermost + Zulip + OnCall

### Elevator Pitch

- **Before**: Beacon emits to one webhook URL. The team uses
  Mattermost for low-severity alerts, OnCall for paging, and SMTP
  as the cross-system fallback when both are down. They need
  per-rule sink routing.
- **After**: a rule declaring `sinks: [{kind: "mattermost",
  channel: "#alerts-info"}, {kind: "oncall", team: "platform"}]`
  emits to both sinks on `Firing`; the same rule with `severity:
  critical` adds the SMTP fallback automatically per the CUE
  default. Each sink's emission carries the same canonical incident
  payload, formatted appropriately for that sink (Markdown for
  Mattermost, plain text for SMTP, OnCall's JSON schema for
  OnCall).
- **Decision enabled**: Sasha confirms Beacon's adapter discipline
  scales to the team's notification topology without compromising
  the canonical incident-payload contract.

### Acceptance criteria

- AC-4.1 — Beacon's sink trait `crates/beacon/src/sinks/mod.rs`
  defines `async fn emit(&self, incident: &Incident) -> Result<(),
  SinkError>` with one implementation per sink kind: `WebhookSink`,
  `SmtpSink`, `MattermostSink`, `ZulipSink`, `OnCallSink`.
- AC-4.2 — Each sink is configured via CUE: per-instance fields
  (URL, channel, recipient list) live next to the rule's `sinks:`
  array. Secret material (SMTP password, OnCall token) is read from
  environment variables named in the CUE, never from the CUE itself
  — the operator-readable rule catalogue contains no secrets.
- AC-4.3 — Sink failures are independent: a failure to deliver to
  one sink does not block delivery to the others. Each emission
  records `(sink_kind, status, latency_ms, error_class)` to Beacon's
  own OTLP telemetry.
- AC-4.4 — Per-sink formatting is locked: Mattermost emits
  Markdown with a code block for the failing query; SMTP emits a
  multipart message with plain-text and HTML alternatives; OnCall
  emits the documented OnCall JSON schema; webhook emits the
  canonical incident JSON; Zulip emits a topic-keyed message.
- AC-4.5 — Header redaction (ADR-0027 §6, shared with Prism):
  no auth-header value flows into the canonical incident or any
  sink emission's body. The sink layer's redaction discipline is
  pinned by a 5-arm property test exercising every sink kind.

### KPI anchor

- KPI 4 (Sink delivery rate): on a 60-incident burst test against
  a fake-sink harness, every incident reaches every configured
  sink at least once. Retry on transient failure (3 attempts with
  exponential backoff 1s / 5s / 30s) is in scope.

---

## US-BE-05 — SLO burn-rate alerting (Google SRE multi-window-multi-burn-rate)

### Elevator Pitch

- **Before**: simple threshold alerts catch instant failures but
  miss slow burns — a service whose error rate climbs from 0.1% to
  0.5% over four hours never trips a threshold but burns through the
  service's monthly error budget in a single day. The team has been
  hand-computing burn-rates in PromQL with `error_budget` recording
  rules and the maths is hard to get right.
- **After**: a CUE SLO declaration with `target_availability: 0.999`,
  `error_budget_period: "30d"`, and `service: "payments-api"`
  generates the standard five-window multi-burn-rate alerts (page on
  1h/5m burn-rate ≥ 14.4, ticket on 6h/30m ≥ 6, etc.) per Google's
  SRE workbook. Sasha writes one SLO; Beacon synthesises the rule
  set. Riley pages only when the burn-rate truly warrants response.
- **Decision enabled**: Sasha confirms Beacon's SLO primitives match
  Google's published methodology byte-for-byte, and the team
  migrates the four most-critical services to SLO-backed alerting
  in the same review cycle.

### Acceptance criteria

- AC-5.1 — The CUE schema for an SLO is documented in
  `crates/beacon/cue/slo.cue` with required fields:
  `service` (string), `sli_good_events` (PromQL),
  `sli_total_events` (PromQL), `target_availability` (float in
  (0,1)), `error_budget_period` (Prometheus duration).
- AC-5.2 — Beacon synthesises the five MWMBR rule windows per
  Google SRE workbook §14.4 Table 14-3 (the canonical multi-window
  table): page-level at 1h/5m (burn-rate threshold 14.4) and
  6h/30m (threshold 6); ticket-level at 1d/2h (threshold 3) and
  3d/6h (threshold 1). The exact thresholds match the workbook
  table for a 30-day budget.
- AC-5.3 — The synthesised PromQL expressions for each window use
  the canonical form `(sli_total_events - sli_good_events) /
  sli_total_events` with the appropriate `[window]` and
  `[long-window]` aggregations. The expressions are deterministic
  and re-emitted byte-equal across runs.
- AC-5.4 — Each synthesised rule carries `slo_service` and
  `slo_window` labels so the operator can correlate the firing
  rule with its SLO declaration. Annotations link back to the
  source CUE file path.
- AC-5.5 — Cross-validation: for a known time series with a known
  burn-rate, Beacon's synthesised rules fire at the same
  wall-clock as a hand-authored PromQL recording rule computing
  the burn-rate directly. The acceptance test asserts byte-equal
  firing decisions across a 24-hour synthetic test trace.

### KPI anchor

- KPI 5 (Burn-rate fidelity): a synthetic 24-hour trace with a
  controlled 0.5% error rate (above the 0.1% target) produces
  exactly the expected pattern of page-level and ticket-level
  emissions per the workbook table. Zero spurious pages on a
  control trace with 0.05% error rate (below target).
