# Acceptance scenarios for cinder-to-otlp-json-bridge-v0.
#
# These scenarios are the canonical Gherkin extracted from the YAML
# journey + the per-slice acceptance criteria. The acceptance-designer
# (DISTILL wave) translates them into executable Rust tests under
# crates/self-observe/tests/cinder_to_otlp_json.rs.
#
# Persona: Priya the platform operator.
# Writer type under test: self_observe::CinderToOtlpJsonWriter.
# Test substrate: cinder::InMemoryTieringStore wrapping the writer,
# with a SharedBuf(Arc<Mutex<Vec<u8>>>) as the inner Write — mirrors the
# Lumen OTLP-JSON test harness exactly.

Feature: Cinder tier events emit one OTLP-JSON ResourceMetrics line per event
  # Platform: library (Rust). No CLI in v0 of this feature.
  # Key heuristics: Nielsen #1 (visibility of system status — every event
  # produces an observable line), #2 (match system and real world — same
  # OTLP-JSON shape and same scope-name convention as the Lumen writer).

  Background:
    Given a SharedBuf(Arc<Mutex<Vec<u8>>>) that implements Write
    And a CinderToOtlpJsonWriter constructed with that SharedBuf
    And a Cinder InMemoryTieringStore constructed with that writer as its recorder

  # ---------- Slice 01: place events ------------------------------------

  Scenario: Single place event emits one OTLP-JSON ResourceMetrics line under same tenant
    Given tenant acme has no prior tier metadata
    When cinder.place(&acme, &item("trade-2026-05-18-001"), Tier::Hot, t0) is called
    Then the buffer contains exactly one non-empty line
    And that line parses as a JSON object
    And the JSON's resource.attributes[0].key equals "tenant_id"
    And the JSON's resource.attributes[0].value.stringValue equals "acme"
    And the JSON's scopeMetrics[0].scope.name equals "kaleidoscope.cinder"
    And the JSON's scopeMetrics[0].metrics[0].name equals "cinder.place.count"
    And the JSON's scopeMetrics[0].metrics[0].sum.aggregationTemporality equals 2
    And the JSON's scopeMetrics[0].metrics[0].sum.isMonotonic equals true
    And the JSON's scopeMetrics[0].metrics[0].sum.dataPoints[0].asInt equals "1"
    And the JSON's scopeMetrics[0].metrics[0].sum.dataPoints[0].attributes contains {key: "tier", value: {stringValue: "hot"}}
    And the JSON's scopeMetrics[0].metrics[0].sum.dataPoints[0].timeUnixNano is a string that parses as u64

  Scenario: Place events for different tiers emit lines with correct tier attribute
    When cinder.place is called with Tier::Hot, then Tier::Warm, then Tier::Cold (three different items, same tenant acme)
    Then the buffer contains exactly 3 non-empty lines
    And every line has metric name "cinder.place.count"
    And the set of tier attribute values across the three lines is exactly {"hot", "warm", "cold"}

  Scenario: Two tenants emit distinct resource attributes on place lines
    When cinder.place is called once for tenant acme (Tier::Hot) and twice for tenant globex (both Tier::Hot)
    Then the buffer contains exactly 3 non-empty lines
    And exactly 1 line has resource.attributes[0].value.stringValue equal to "acme"
    And exactly 2 lines have resource.attributes[0].value.stringValue equal to "globex"
    And every globex line has dataPoints[0].attributes containing {key: "tier", value: {stringValue: "hot"}}

  Scenario: No place call means no OTLP-JSON line
    Given Priya has wired the writer but called nothing on Cinder
    When the buffer is inspected
    Then the buffer contains zero bytes

  # ---------- Slice 02: migrate events ----------------------------------

  Scenario: Migrate event preserves source and destination tier as attributes
    Given tenant acme has placed item("trade-2026-05-18-001") in Tier::Hot
    When cinder.migrate(&acme, &item("trade-2026-05-18-001"), Tier::Warm, t1) succeeds
    Then the buffer's most recent line has metric name "cinder.migrate.count"
    And that line's dataPoints[0].asInt equals "1"
    And that line's dataPoints[0].attributes contains {key: "from", value: {stringValue: "hot"}}
    And that line's dataPoints[0].attributes contains {key: "to",   value: {stringValue: "warm"}}

  Scenario: Failed migrate (unknown item) emits no OTLP-JSON line
    Given tenant acme has placed nothing
    When cinder.migrate(&acme, &item("ghost"), Tier::Warm, t1) returns Err(UnknownItem)
    Then no line in the buffer has metric name "cinder.migrate.count"

  Scenario: Two tenants' migrate events emit isolated resource attributes
    Given tenant acme has placed item("a1") in Tier::Hot
    And tenant globex has placed item("g1") in Tier::Hot
    When cinder.migrate(&acme, &item("a1"), Tier::Warm, t) and cinder.migrate(&globex, &item("g1"), Tier::Cold, t) both succeed
    Then exactly one line has resource.tenant_id="acme" with metric "cinder.migrate.count" and attrs {from: hot, to: warm}
    And exactly one line has resource.tenant_id="globex" with metric "cinder.migrate.count" and attrs {from: hot, to: cold}

  # ---------- Slice 03: evaluate events ---------------------------------

  Scenario: Evaluate that migrates N items for one tenant emits N migrate lines AND 1 evaluate line
    Given tenant acme has placed 5 items in Tier::Hot at t0
    And the tier policy migrates Hot items older than 24h to Warm
    When cinder.evaluate_at(t0 + 25h, &policy) is called
    Then cinder.evaluate_at returns 5
    And exactly 5 lines in the buffer have metric name "cinder.migrate.count" with attrs {from: hot, to: warm} under tenant acme
    And exactly 1 line in the buffer has metric name "cinder.evaluate.migrated.count" under tenant acme
    And that evaluate line's dataPoints[0].asInt equals "5"

  Scenario: Evaluate with zero eligible items emits no evaluate line for that tenant
    Given tenant acme has placed 3 items in Tier::Hot at t0
    And the tier policy migrates Hot items older than 24h to Warm
    When cinder.evaluate_at(t0 + 1h, &policy) is called
    Then cinder.evaluate_at returns 0
    And no line in the buffer has metric name "cinder.evaluate.migrated.count" under tenant acme
    And no line in the buffer has metric name "cinder.migrate.count"   under tenant acme

  Scenario: Two-tenant evaluate emits per-tenant evaluate lines with correct asInt
    Given tenant acme has placed 5 items in Tier::Hot at t0
    And tenant globex has placed 2 items in Tier::Hot at t0
    And the tier policy migrates Hot items older than 24h to Warm
    When cinder.evaluate_at(t0 + 25h, &policy) is called
    Then exactly 1 line has resource.tenant_id="acme"   with metric "cinder.evaluate.migrated.count" and asInt "5"
    And exactly 1 line has resource.tenant_id="globex"  with metric "cinder.evaluate.migrated.count" and asInt "2"
    And exactly 5 lines have resource.tenant_id="acme"   with metric "cinder.migrate.count"
    And exactly 2 lines have resource.tenant_id="globex"  with metric "cinder.migrate.count"

  # ---------- Cross-cutting properties ----------------------------------

  Scenario: Output is NDJSON — every line is independently parseable JSON, every line newline-terminated
    Given Priya has wired the writer
    When any combination of N >= 1 Cinder events fire
    Then the buffer ends with a \n byte
    And the buffer split on \n yields N non-empty lines
    And every non-empty line parses as a complete JSON object

  Scenario: Lines are appended, never overwritten
    Given Priya has wired the writer
    When cinder.place is called, then later cinder.migrate is called
    Then the place line appears before the migrate line in the buffer

  @property
  Scenario: The writer is Send + Sync
    Given the CinderToOtlpJsonWriter<W> type for some W: Write + Send + Sync
    Then the compile-time assertion `fn assert_send_sync<T: Send + Sync>(); assert_send_sync::<CinderToOtlpJsonWriter<Vec<u8>>>();` compiles

  @property
  Scenario: scope.name is always "kaleidoscope.cinder"
    Given Priya has wired the writer
    When any Cinder event fires
    Then the emitted line's scopeMetrics[0].scope.name is exactly "kaleidoscope.cinder"

  @property
  Scenario: timeUnixNano is always a string-encoded uint64
    Given Priya has wired the writer
    When any Cinder event fires
    Then the emitted line's dataPoints[0].timeUnixNano is a JSON string
    And that string, parsed as u64, succeeds
