# Shared Artefacts Registry — `cinder-to-otlp-json-bridge-v0`

Every cross-step variable in `journey-observe-cinder-via-otlp-json.yaml`,
its single source of truth, its consumers, and the integration risk if
it drifts.

## Registry

```yaml
shared_artifacts:

  tenant_id:
    source_of_truth: |
      aegis::TenantId (constructed by operator binary; in tests, by the
      acceptance harness).
    consumers:
      - cinder API calls (place, migrate, evaluate_at)
      - writer.record_place / record_migrate / record_evaluate forwarders
      - emitted line: resource.attributes[0].value.stringValue
      - emitted line: dataPoints[0].attributes[i] where key="tenant_id"
        (point attribute, mirroring the Lumen writer's choice — collectors
        disagree on which one they prefer; emitting both is the safer
        interop choice; see wave-decisions.md D3)
      - sidecar forwarder (passes verbatim)
      - downstream OTLP/HTTP collector (groups time series by it)
      - operator dashboard (per-tenant panels)
    owner: aegis crate
    integration_risk: |
      HIGH. Tenant identity is the partition key all the way to the
      downstream dashboard. The writer MUST forward &TenantId unchanged
      AND emit it in BOTH resource attribute AND point attribute slots.
      Any silent transform (interning, lowercase, trim) leaks data
      across tenants in the downstream collector.
    validation: |
      Slice 01 includes a two-tenant test asserting acme/globex
      isolation on both resource and point attribute sides. Slice 02 +
      Slice 03 inherit the same invariant.

  otlp_log_path:
    source_of_truth: |
      Operator-supplied --observe-otlp <path> CLI flag (already shipped
      in commit 3af7e82; see kaleidoscope-cli/src/lib.rs:139-160). At v0
      of THIS feature, the path lives at the call site of the future CLI
      follow-up feature, not in this library.
    consumers:
      - LumenToOtlpJsonWriter::new (already wired in the CLI ingest path)
      - CinderToOtlpJsonWriter::new (to be wired in the CLI follow-up)
      - operator's sidecar tail/forwarder process
      - operator's downstream OTLP/HTTP collector (indirectly, via the
        sidecar)
    owner: operator (runtime) / kaleidoscope-cli crate (parsing + dispatch)
    integration_risk: |
      MEDIUM. The CLI follow-up MUST give BOTH writers the same path,
      or the operator gets a split stream. The library cannot enforce
      this — both writers take a generic W: Write. The follow-up
      feature's DISCUSS owns the same-path-for-both invariant.

  file_handle:
    source_of_truth: |
      std::fs::File obtained via
      OpenOptions::new().create(true).append(true).open(otlp_log_path).
      At v0 of THIS feature, the acceptance tests use
      SharedBuf(Arc<Mutex<Vec<u8>>>) instead — a Write implementation
      that buffers bytes in memory. Same harness pattern as the Lumen
      OTLP-JSON tests.
    consumers:
      - CinderToOtlpJsonWriter<W>::new (wraps it in an internal Mutex<W>)
      - all three writer.record_* methods (each acquires the Mutex,
        write_all the line bytes, write_all b"\n", flush, release Mutex)
    owner: |
      CLI follow-up feature (real File). At v0 of this feature, the
      acceptance harness owns the Vec<u8> sink.
    integration_risk: |
      LOW at the library level — W is generic and exercised against
      Vec<u8>. MEDIUM at the CLI integration level — real File
      semantics (O_APPEND atomicity, ENOSPC, EAGAIN on pipes, partial
      writes) are owned by the follow-up feature and by the Lumen
      writer's existing test coverage.

  metric_name:
    source_of_truth: |
      String literals in self_observe::cinder_otlp_json.rs. Three names,
      LOCKED IDENTICAL to the Pulse-sink sibling
      (cinder-to-pulse-bridge-v0 wave-decisions.md D1):
        - "cinder.place.count"
        - "cinder.migrate.count"
        - "cinder.evaluate.migrated.count"
    consumers:
      - writer emission (each record_* method writes this literal into
        scopeMetrics[0].metrics[0].name)
      - sidecar JSON parser (passes through verbatim)
      - downstream collector (indexes by name)
      - operator dashboard (queries by name)
      - acceptance tests (string-literal asserts)
      - operator runbooks (post-v0)
    owner: self-observe crate
    integration_risk: |
      HIGH. The metric name is the contract between emission and
      dashboard, AND the cross-bridge contract with the Pulse-sink
      sibling. A typo on either side returns silent empty dashboards.
      Acceptance tests in Slices 01/02/03 each assert the exact metric
      name string; the sibling feature's tests assert the same strings;
      drift between the two would be caught at the next code review
      that diffs both files.

  scope_name:
    source_of_truth: |
      String literal "kaleidoscope.cinder" in
      self_observe::cinder_otlp_json.rs. Parallel to "kaleidoscope.lumen"
      in lumen_otlp_json.rs line 138.
    consumers:
      - writer emission: scopeMetrics[0].scope.name
      - downstream collector: groups metrics by instrumentation scope
      - operator dashboard: enables "everything from kaleidoscope.cinder"
        panels
    owner: self-observe crate
    integration_risk: |
      MEDIUM. A wrong scope name does not break ingestion but breaks
      scope-based dashboard groupings. Locked by acceptance-test
      assertion in Slice 01 and re-asserted in Slices 02 + 03.

  tier_value:
    source_of_truth: cinder::Tier enum (Hot / Warm / Cold).
    consumers:
      - cinder.place argument
      - cinder.migrate arguments (from + to)
      - writer point attribute "tier" on cinder.place.count
      - writer point attributes "from"/"to" on cinder.migrate.count
      - downstream dashboard filters by tier value
    owner: cinder crate
    integration_risk: |
      MEDIUM. The writer must serialise Tier consistently. Convention,
      locked by cross-bridge contract: lowercase string. Tier::Hot ->
      "hot", Tier::Warm -> "warm", Tier::Cold -> "cold". Any deviation
      (uppercase, Debug repr "Hot", numeric) breaks dashboard filters
      that the operator already wrote for the in-process Pulse-sink
      sibling in their local testing.
    validation: |
      Slice 01 asserts the exact lowercase string for tier attribute on
      place lines. Slice 02 asserts the same convention for from/to on
      migrate lines.

  migrated_count:
    source_of_truth: |
      cinder::InMemoryTieringStore::evaluate_at (and its FileBacked
      sibling). Per-tenant aggregation; see crates/cinder/src/store.rs
      lines 218-230.
    consumers:
      - record_evaluate(tenant, migrated) — Cinder calls the writer
      - writer renders migrated.to_string() into the asInt field of
        dataPoints[0] on cinder.evaluate.migrated.count
      - downstream dashboard sum/avg queries over the metric
    owner: cinder crate
    integration_risk: |
      LOW. The .to_string() rendering is exact for any usize. OTLP-JSON
      encodes uint64 as a string (per the OpenTelemetry specification),
      so the writer pre-renders to a String at emission time. No
      precision loss possible.
    validation: |
      Slice 03 asserts the exact string value matches the per-tenant
      eligible-item count ("5" for acme, "2" for globex).

  emission_timestamp:
    source_of_truth: |
      SystemTime::now() inside the writer at emission time, converted
      to nanos-since-Unix-epoch as u64, then rendered as a String. Same
      pattern as lumen_otlp_json.rs:142-146.
    consumers:
      - dataPoints[0].timeUnixNano (string-encoded uint64 per OTLP-JSON)
      - downstream collector sort/window order
      - dashboard time-axis rendering
    owner: writer implementation (self-observe crate)
    integration_risk: |
      LOW. The timestamp is set by the writer, not by Cinder. Acceptance
      tests do not pin specific values; they assert that the field is a
      string AND that the string parses as u64. Same approach as the
      Lumen writer (lumen_to_otlp_json.rs:123-127).

  ndjson_line_terminator:
    source_of_truth: |
      The byte sequence b"\n" written immediately after every serialised
      ResourceMetrics line. Source: writer implementation
      (self-observe crate); same pattern as
      lumen_otlp_json.rs:184-186.
    consumers:
      - sidecar reader (typically `BufRead::lines` or `tail -F`)
      - any line-oriented log shipper (vector.dev, fluent-bit, etc.)
      - acceptance tests asserting the trailing-newline invariant
    owner: writer implementation
    integration_risk: |
      HIGH if broken (entire stream becomes one giant unparseable line).
      LOW in practice because the test substrate exercises it and the
      Lumen sibling's tests catch the same class of bug for the parallel
      writer.
    validation: |
      Slice 01 includes an explicit "buffer ends with \n" assertion.
      Slices 02 and 03 inherit it through every multi-line test.
```

## Consistency check (DISCUSS wave gate)

| Artefact | Source documented | Consumers documented | Risk classified | Validation pointer |
|----------|-------------------|---------------------|-----------------|-------------------|
| tenant_id | yes | yes | HIGH | Slices 01 + 02 + 03 |
| otlp_log_path | yes | yes | MEDIUM | post-v0 CLI follow-up |
| file_handle | yes | yes | LOW (lib) / MEDIUM (CLI) | all slices use SharedBuf |
| metric_name | yes | yes | HIGH | Slices 01/02/03 |
| scope_name | yes | yes | MEDIUM | Slices 01/02/03 |
| tier_value | yes | yes | MEDIUM | Slices 01 + 02 |
| migrated_count | yes | yes | LOW | Slice 03 |
| emission_timestamp | yes | yes | LOW | inherited from Lumen pattern |
| ndjson_line_terminator | yes | yes | HIGH (if broken) | Slice 01 explicit, others implicit |

All nine artefacts have a single source of truth, documented consumers,
a risk classification, and an acceptance-test validation pointer. The
DISCUSS-wave horizontal-coherence gate **passes**.

## Cross-feature artefact interactions

This bridge is library-only at v0. Its cross-feature artefact
interactions are:

1. **`metric_name`** is the cross-bridge contract with
   `cinder-to-pulse-bridge-v0`. Both bridges MUST emit the same three
   names. Drift between them would be caught by a code review that
   diffs `crates/self-observe/src/cinder_bridge.rs` against
   `crates/self-observe/src/cinder_otlp_json.rs`. The string literals
   are intentionally NOT extracted into a shared constants module at
   v0 (rule of three not yet reached; see wave-decisions.md D7).

2. **`scope_name`** is parallel to `kaleidoscope.lumen` in
   `crates/self-observe/src/lumen_otlp_json.rs:138`. The two writers
   use distinct scope names — that is the entire point of scope-based
   grouping in OTLP-JSON. The parallelism is naming-convention-only,
   not a shared value.

3. **`otlp_log_path`** and **`file_handle`** are shared with
   `LumenToOtlpJsonWriter` at the CLI follow-up's call site. The
   library does not constrain them — both writers take a generic
   `W: Write + Send + Sync`.

4. **`ndjson_line_terminator`** is the cross-writer atomicity contract
   established by the Lumen writer. The Cinder writer inherits the
   pattern verbatim. See `wave-decisions.md` D6.

No SSOT artefact is modified by this wave:

- `docs/product/journeys/incident-response.yaml` (orthogonal journey)
- `docs/product/jobs.yaml` (no new job promoted at v0)

The post-v0 CLI follow-up will likely promote a new SSOT job
(`operator-observes-platform-internals-cross-process`) or extend the
one promoted by the Pulse-sink CLI follow-up once an operator-visible
surface exists. That promotion is OUT of scope for this feature.
