# Journey: Operator observes Cinder via OTLP-JSON NDJSON

> Feature: `cinder-to-otlp-json-bridge-v0`
> Persona: **Priya the platform operator** — she runs the multi-tenant
> Kaleidoscope deployment for a fintech. She already runs the CLI with
> `--observe-otlp <path>` and consumes the resulting NDJSON via a sidecar
> that forwards to her org's OTLP/HTTP collector. The collector dashboards
> show `kaleidoscope.lumen` metrics today; she expects to see
> `kaleidoscope.cinder` next to them, with zero changes to the sidecar.
> Job: "I want my cross-process OTLP collector to show Cinder's tier
> movements alongside Lumen's ingest/query metrics, so my single existing
> dashboard becomes a complete platform view."

## Why this journey exists (the void Priya sees today)

The CLI's `--observe-otlp <path>` flag was shipped in commit `3af7e82`. It
wires `LumenToOtlpJsonWriter` into the Lumen store and produces NDJSON
lines under scope `kaleidoscope.lumen`. But the Cinder store in the same
`ingest` call still uses `NoopRecorder` (see
`crates/kaleidoscope-cli/src/lib.rs:163`).

Priya runs a workload, points her sidecar at the file, and refreshes her
collector dashboard. She sees:

- `lumen.ingest.count` per tenant
- `lumen.query.count` per tenant
- **nothing about Cinder**

Yet the same CLI invocation has just executed `cinder.place(...)` for
every batch (`kaleidoscope-cli/src/lib.rs:228`). Cinder's tier-management
activity is happening — invisibly. The sidecar is forwarding a stream
that lies by omission.

The in-process Pulse-sink sibling (`CinderToPulseRecorder`, shipped in
commit `4d20c31`) solves the in-process variant of this problem. It does
nothing for the cross-process operator, who has no Pulse query API at the
sidecar side; they only have OTLP/HTTP semantics. This feature ships the
cross-process sibling.

## The emotional arc

Three states, mirroring the Lumen→OTLP-JSON arc and the Cinder→Pulse arc:

```
   anxious           focused              relieved
      |                 |                     |
  "My collector       "Switch the CLI's      "Same dashboard,
   shows half the     Cinder recorder to     same query language,
   platform."         the OTLP-JSON writer.   now showing cinder.*
                      Same file, same         lines next to
                      sidecar."               lumen.* lines."
```

- **Entry**: anxious. Priya knows Cinder is doing work but her cross-
  process observability tool shows none of it. Her dashboard is a partial
  truth. She cannot diagnose "did the last hourly tier sweep migrate
  anything for `acme`?" from her standard tooling — she has to fall back
  to in-process tooling she does not have in production.
- **Middle**: focused. The CLI follow-up will wire
  `CinderToOtlpJsonWriter::new(file)` into the Cinder constructor instead
  of `NoopRecorder`. The sidecar does not change. The file format does
  not change. The collector does not change. Only the producer side
  changes — one line of CLI code, scheduled for the follow-up feature.
- **Exit**: relieved. The dashboard now shows `cinder.place.count`,
  `cinder.migrate.count`, `cinder.evaluate.migrated.count` with the same
  per-tenant resolution as the Lumen metrics. The cross-process view is
  complete.

Cross-feature invariant inherited from the Lumen OTLP-JSON writer:
**every line of the NDJSON file is a complete OTLP-JSON `ResourceMetrics`
envelope**. The Cinder writer MUST honour that invariant so the file
remains parseable when both writers contribute. See `wave-decisions.md` D6
for the cross-writer atomicity argument.

## Journey flow (ASCII)

```
+-----------------------------------------------------------------+
| Step 1: Wire the writer once at construction                    |
|                                                                 |
|   let file = OpenOptions::new()                                 |
|       .create(true).append(true).open(${otlp_path})?;           |
|   let writer = CinderToOtlpJsonWriter::new(file);               |
|   let cinder = FileBackedTieringStore::open(                    |
|       cinder_base(data_dir), Box::new(writer))?;                |
|                                                                 |
|   Emotion: focused. One line, type-checked, parallel to the     |
|   existing LumenToOtlpJsonWriter call site three lines above.   |
+-----------------------------------------------------------------+
                                |
                                v
+-----------------------------------------------------------------+
| Step 2: Cinder runs its normal API; lines append to the file    |
|                                                                 |
|   cinder.place(&acme, &item("trade-2026-05-18"), Tier::Hot, t); |
|   cinder.migrate(&acme, &item("trade-2026-05-18"),              |
|                  Tier::Warm, t+24h)?;                           |
|   cinder.evaluate_at(t+30d, &policy);                           |
|                                                                 |
|   The file gains lines:                                         |
|     {"resource":...,"scopeMetrics":[{"scope":{"name":           |
|        "kaleidoscope.cinder"},"metrics":[{"name":               |
|        "cinder.place.count",...,"attributes":[{"key":"tier",    |
|        "value":{"stringValue":"hot"}}],...}]}]}                 |
|     (one such line per Cinder event)                            |
|                                                                 |
|   Emotion: focused -> relieved. Cinder call sites are unchanged |
|   from before; the writer is invisible at the call site.        |
+-----------------------------------------------------------------+
                                |
                                v
+-----------------------------------------------------------------+
| Step 3: Sidecar tails the file; collector dashboard updates     |
|                                                                 |
|   $ tail -f /var/log/kaleidoscope/observe.ndjson | ./forwarder  |
|   [collector receives the line, ingests as ResourceMetrics]     |
|                                                                 |
|   Dashboard now shows:                                          |
|     kaleidoscope.lumen / lumen.ingest.count    [acme] 12345     |
|     kaleidoscope.lumen / lumen.query.count     [acme]   234     |
|     kaleidoscope.cinder / cinder.place.count   [acme]   789  *  |
|     kaleidoscope.cinder / cinder.migrate.count [acme]    45  *  |
|     kaleidoscope.cinder / cinder.evaluate.migrated.count        |
|                                                [acme]   123  *  |
|                                                                 |
|   (* = new this feature)                                        |
|                                                                 |
|   Emotion: relieved. Zero changes to sidecar, collector,        |
|   dashboard. The cross-process view is complete.                |
+-----------------------------------------------------------------+
```

## Per-step detail (with mockup output)

### Step 1 — Wire the writer

This step lands in the CLI follow-up feature, NOT in this feature. Shown
here so the library-level acceptance criteria for this feature are
grounded in the eventual call site.

```rust
use std::fs::OpenOptions;
use cinder::FileBackedTieringStore;
use self_observe::CinderToOtlpJsonWriter;

let file = OpenOptions::new()
    .create(true)
    .append(true)
    .open(otlp_log_path)?;
let cinder = FileBackedTieringStore::open(
    cinder_base(data_dir),
    Box::new(CinderToOtlpJsonWriter::new(file)),
)?;
```

The file may already be open and being written to by
`LumenToOtlpJsonWriter`. That is intentional. POSIX `O_APPEND`
(implicit in `OpenOptions::new().append(true)`) plus per-writer
in-process Mutex makes lines atomic at the NDJSON record boundary; see
`wave-decisions.md` D6.

Shared artefacts at this step:
- `otlp_log_path` — the file path. Operator-supplied via the CLI flag.
  Reused as the sink for BOTH Lumen and Cinder writers in the same CLI
  invocation.
- `file` — the `File` handle. The CLI follow-up may pass the same path
  to both `LumenToOtlpJsonWriter::new()` and `CinderToOtlpJsonWriter::new()`
  using `File::try_clone()` or two independent `OpenOptions::open()`
  calls; both end up with `O_APPEND` semantics on the same inode.

### Step 2 — Workload runs against Cinder; lines append

No new code from Priya's perspective. Cinder's existing `place`,
`migrate`, `evaluate_at` API is unchanged. Internally each call fans out
to the `MetricsRecorder` trait, which the writer implements.

Concrete line produced by `cinder.place(&acme, &trade_001, Tier::Hot, t0)`:

```
{"resource":{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}]},"scopeMetrics":[{"scope":{"name":"kaleidoscope.cinder"},"metrics":[{"name":"cinder.place.count","sum":{"aggregationTemporality":2,"isMonotonic":true,"dataPoints":[{"attributes":[{"key":"tier","value":{"stringValue":"hot"}}],"timeUnixNano":"1747569600123456789","asInt":"1"}]}}]}]}
```

Concrete line produced by `cinder.migrate(&acme, &trade_001, Tier::Warm, t1)`
(after the place above succeeded):

```
{"resource":{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}]},"scopeMetrics":[{"scope":{"name":"kaleidoscope.cinder"},"metrics":[{"name":"cinder.migrate.count","sum":{"aggregationTemporality":2,"isMonotonic":true,"dataPoints":[{"attributes":[{"key":"from","value":{"stringValue":"hot"}},{"key":"to","value":{"stringValue":"warm"}}],"timeUnixNano":"1747569603987654321","asInt":"1"}]}}]}]}
```

Concrete line produced by `cinder.evaluate_at(t2, &policy)` where `acme`
had 5 items migrated in this call (one of the N+1 lines for `acme`; the
other N are migrate lines):

```
{"resource":{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}]},"scopeMetrics":[{"scope":{"name":"kaleidoscope.cinder"},"metrics":[{"name":"cinder.evaluate.migrated.count","sum":{"aggregationTemporality":2,"isMonotonic":true,"dataPoints":[{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}],"timeUnixNano":"1747572000000000001","asInt":"5"}]}}]}]}
```

Inside the writer (sketch; DESIGN wave owns the final shape):

```rust
impl cinder::MetricsRecorder for CinderToOtlpJsonWriter<W> {
    fn record_place(&self, tenant: &TenantId, tier: Tier) {
        // emit one OTLP-JSON ResourceMetrics line with metric
        // "cinder.place.count", scope "kaleidoscope.cinder",
        // point attribute {tier: lowercase(tier)}, asInt = "1"
    }
    fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier) {
        // emit one line with "cinder.migrate.count", point
        // attributes {from: lowercase(from), to: lowercase(to)},
        // asInt = "1"
    }
    fn record_evaluate(&self, tenant: &TenantId, migrated: usize) {
        // emit one line with "cinder.evaluate.migrated.count",
        // asInt = migrated.to_string()
    }
}
```

### Step 3 — Sidecar consumes the NDJSON; collector dashboard updates

What an operator sees in their collector dashboard 5 seconds after a
workload that placed 3 items and ran an evaluate that migrated 2 of them
for tenant `acme`:

```
+-- Collector dashboard view: kaleidoscope.cinder for acme -------+
|                                                                 |
|   Metric: cinder.place.count                                    |
|   Time series (1m window):                                      |
|     1747569600 [tier=hot]   value=1                             |
|     1747569601 [tier=hot]   value=1                             |
|     1747569602 [tier=hot]   value=1                             |
|                                                                 |
|   Metric: cinder.migrate.count                                  |
|     1747572000 [from=hot,to=warm]   value=1                     |
|     1747572000 [from=hot,to=warm]   value=1                     |
|                                                                 |
|   Metric: cinder.evaluate.migrated.count                        |
|     1747572000 [tenant_id=acme]   value=2                       |
|                                                                 |
+-----------------------------------------------------------------+
```

Same dashboard, same query language. The only delta is that the
`kaleidoscope.cinder` rows are new. The sidecar script was not modified.
The collector was not reconfigured. The dashboard's PromQL/MQL queries
were not edited.

## Failure modes acknowledged

| What could go wrong | What the writer does today | DISTILL test |
|---------------------|---------------------------|--------------|
| Disk full / pipe broken / write fails | Writer silently swallows the error (matches Lumen writer, D5). The next event tries again. | Not explicitly tested at v0 (uninhabitable in unit tests against `Vec<u8>`). |
| Serde serialisation fails | Writer silently swallows. The hand-rolled structs make this practically impossible for `&str` and `String` fields. | Implicit: if serialisation broke, EVERY test would fail. |
| Cinder is constructed with `CinderToOtlpJsonWriter` but never used | Zero lines appended. File remains as it was before the CLI run. | `no_cinder_event_means_no_otlp_line` (Slice 01) |
| Tenant identity leak: `acme`'s event has `globex`'s tenant_id | Writer MUST pass `&TenantId` through to BOTH resource and point attribute unchanged. | `two_tenants_emit_distinct_otlp_resource_attributes` (Slice 01) — same shape as the Lumen writer's test of the same name. |
| Concurrent Cinder calls from multiple threads | `MetricsRecorder` is `Send + Sync`; `Mutex<W>` guards the file. | `the_writer_is_send_and_sync` compile-time check (Slice 01) |
| `evaluate_at` with 0 migrations for a tenant | Cinder does NOT call `record_evaluate` for tenants with 0 migrations (see `crates/cinder/src/store.rs:218-230` — only tenants in `per_tenant` map). | `evaluate_with_no_eligible_items_emits_no_evaluate_line` (Slice 03) |
| Cross-writer line interleaving (Lumen and Cinder both writing to the same file) | OS-level `O_APPEND` + per-writer in-process `Mutex<W>` makes per-line writes atomic for sizes below PIPE_BUF. | Not testable at the library level (the writer takes a `Write`, not a path). The CLI follow-up feature owns the OS-level test. Documented in D6. |
| Same Cinder store wrapped in TWO writers writing to ONE file (misconfiguration) | Each writer emits its own line. Result is the SAME logical content emitted twice. The bug is in the wiring, not the writer. | Not v0 scope. |

## Integration checkpoints

After Slice 01: `cinder.place.count` lines appended to the NDJSON sink
with `scope.name = "kaleidoscope.cinder"`, point attribute `tier`, and
resource + point `tenant_id` matching the call. Verifies the emission
path end-to-end with a single event type, the OTLP-JSON line shape, the
NDJSON newline-termination invariant, and per-tenant resource attribute
isolation.

After Slice 02: `cinder.migrate.count` lines appended with `from`/`to`
point attributes. Verifies multi-attribute emission. Verifies that
Cinder's failure path (`UnknownItem`) produces no line.

After Slice 03: `cinder.evaluate.migrated.count` lines appended with
`asInt = migrated_count.to_string()`. Verifies the value-encodes-count
convention, the dual-emission contract (per-item migrate + per-tenant
evaluate, both emitted to the SAME NDJSON stream), and the zero-migration
tenant case.

After Slice 03 the journey is complete at the LIBRARY level. The cross-
process operator-visible journey completes when the CLI follow-up feature
ships and switches `kaleidoscope-cli/src/lib.rs:163` from `NoopRecorder`
to `CinderToOtlpJsonWriter::new(file)`.
