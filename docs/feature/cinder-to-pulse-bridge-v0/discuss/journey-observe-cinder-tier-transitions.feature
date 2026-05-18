# Acceptance scenarios for cinder-to-pulse-bridge-v0.
#
# These scenarios are the canonical Gherkin extracted from the YAML
# journey + the per-slice acceptance criteria. The acceptance-designer
# (DISTILL wave) translates them into executable Rust tests under
# crates/self-observe/tests/cinder_to_pulse.rs.
#
# Persona: Priya the platform operator.
# Bridge type under test: self_observe::CinderToPulseRecorder.
# Test substrate: cinder::InMemoryTieringStore + pulse::InMemoryMetricStore.

Feature: Cinder tier events land as queryable Pulse metric points
  # Platform: library (Rust). No CLI in v0.
  # Key heuristics: Nielsen #1 (visibility of system status), #2 (match
  # system and real world — same query API as Lumen).

  Background:
    Given a Pulse InMemoryMetricStore wrapped in Arc<dyn MetricStore + Send + Sync>
    And a CinderToPulseRecorder constructed with that pulse store
    And a Cinder InMemoryTieringStore constructed with that bridge as its recorder

  # ---------- Slice 01: place events ------------------------------------

  Scenario: Single place event lands as one cinder.place.count point under same tenant
    Given tenant acme has no prior tier metadata
    When cinder.place(&acme, &item("trade-2026-05-18-001"), Tier::Hot, t0) is called
    Then pulse.query(&acme, &MetricName::new("cinder.place.count"), TimeRange::all()) returns exactly 1 point
    And that point's value equals 1.0
    And that point's attributes contain the entry tier="hot"

  Scenario: Place events for different tiers land with correct tier attribute
    When cinder.place is called with Tier::Hot, then Tier::Warm, then Tier::Cold (three different items, same tenant acme)
    Then pulse.query for acme on cinder.place.count returns exactly 3 points
    And the tier attribute values across the three points are exactly {"hot", "warm", "cold"}

  Scenario: Place events isolate per tenant
    When cinder.place is called once for tenant acme (Hot) and twice for tenant globex (both Hot)
    Then pulse.query for acme on cinder.place.count returns exactly 1 point
    And pulse.query for globex on cinder.place.count returns exactly 2 points

  # ---------- Slice 02: migrate events ----------------------------------

  Scenario: Migrate event preserves source and destination tier as attributes
    Given tenant acme has placed item("trade-2026-05-18-001") in Tier::Hot
    When cinder.migrate(&acme, &item("trade-2026-05-18-001"), Tier::Warm, t1) succeeds
    Then pulse.query for acme on cinder.migrate.count returns exactly 1 point
    And that point's value equals 1.0
    And that point's attributes contain from="hot" and to="warm"

  Scenario: Failed migrate (unknown item) emits no metric point
    Given tenant acme has placed nothing
    When cinder.migrate(&acme, &item("ghost"), Tier::Warm, t1) returns Err(UnknownItem)
    Then pulse.query for acme on cinder.migrate.count returns an empty Vec

  Scenario: Migrate events isolate per tenant
    Given tenant acme has placed item("a1") in Hot and tenant globex has placed item("g1") in Hot
    When cinder.migrate is called for acme (Hot -> Warm) and for globex (Hot -> Cold)
    Then pulse.query for acme on cinder.migrate.count returns 1 point with attrs from=hot, to=warm
    And pulse.query for globex on cinder.migrate.count returns 1 point with attrs from=hot, to=cold

  # ---------- Slice 03: evaluate events ---------------------------------

  Scenario: Evaluate that migrates N items for one tenant emits N migrate points AND 1 evaluate point
    Given tenant acme has placed 5 items in Hot at t0 (all older than the policy threshold at t_now)
    And the tier policy migrates Hot items older than 24h to Warm
    When cinder.evaluate_at(t_now, &policy) is called
    Then cinder.evaluate_at returns 5 (the total migration count)
    And pulse.query for acme on cinder.migrate.count returns exactly 5 points, each with attrs from=hot, to=warm
    And pulse.query for acme on cinder.evaluate.migrated.count returns exactly 1 point with value 5.0

  Scenario: Evaluate with zero eligible items emits no evaluate point for that tenant
    Given tenant acme has placed 3 items in Hot but all are newer than the policy threshold
    When cinder.evaluate_at(t_now, &policy) is called
    Then cinder.evaluate_at returns 0
    And pulse.query for acme on cinder.evaluate.migrated.count returns an empty Vec
    And pulse.query for acme on cinder.migrate.count returns an empty Vec

  Scenario: Evaluate across two tenants emits per-tenant evaluate points
    Given tenant acme has 5 items eligible to migrate Hot -> Warm
    And tenant globex has 2 items eligible to migrate Hot -> Warm
    When cinder.evaluate_at(t_now, &policy) is called once
    Then pulse.query for acme on cinder.evaluate.migrated.count returns 1 point with value 5.0
    And pulse.query for globex on cinder.evaluate.migrated.count returns 1 point with value 2.0
    And the migrate.count metric also reflects the 5 acme + 2 globex per-item migrations

  # ---------- Cross-cutting properties ----------------------------------

  Scenario: No Cinder event means no Pulse metric point
    When no Cinder operation is called after wiring the bridge
    Then pulse.query for any tenant on any cinder.* metric name returns an empty Vec

  @property
  Scenario: The bridge is Send + Sync
    Given the CinderToPulseRecorder type
    Then the compile-time assertion `fn assert_send_sync<T: Send + Sync>(); assert_send_sync::<CinderToPulseRecorder>();` compiles
