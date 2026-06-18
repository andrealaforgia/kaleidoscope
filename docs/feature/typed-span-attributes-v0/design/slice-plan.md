# Typed span attributes — thin-slice plan

- **Feature**: `typed-span-attributes-v0`
- **Author**: `nw-solution-architect` (Morgan)
- **Companion**: `options.md`, `docs/product/architecture/adr-0080-typed-span-attribute-values.md`
- **Sequencing rule**: FIDELITY (typed round-trip) **before** THRESHOLD (numeric
  `attr_gte`) **before** the demo spread. Each slice is independently shippable and
  carries a falsifiable learning hypothesis.

Legend: **[DURABILITY-CRITICAL]** = touches the on-disk serde format that
`crates/ray/src/file_backed.rs` reads/writes — extra review + a pre-change-fixture gold
test mandatory.

---

## Slice 1 — `AttrValue` type + untagged serde round-trip (in `ray`)  **[DURABILITY-CRITICAL]**

**What.** Introduce `AttrValue { String, Bool, Int, Double }` in `crates/ray/src/span.rs`
with `#[serde(untagged)]` and declaration order `[String, Bool, Int, Double]`. Change
`Span.attributes` to `BTreeMap<String, AttrValue>` (`span.rs:196`). Drop `Eq` from
`Span` and `SpanBatch` (keep `PartialEq`). Add `AttrValue::as_f64()` and
`AttrValue::canonical_string()`. NO ingest/query/demo wiring yet — only the type and its
codec.

**Why first.** It is the riskiest assumption: that the untagged enum migrates existing
on-disk string data with no version field and no wipe. Validate it in isolation before
anything depends on it.

**Hypothesis (falsifiable).** A WAL+snapshot fixture written in the OLD string shape
(captured from current `main` before the change) loads under the new `Span` and every
attribute deserializes to `AttrValue::String(...)`; a freshly written `Int`/`Double`/`Bool`
round-trips through `serde_json` to the same variant; a JSON number never deserializes to
`String` and a JSON string never to `Int`/`Double`/`Bool`.

**Durability review focus.** The pre-change fixture is the load-bearing artefact — it
must be generated from the codec as it stands today (`file_backed.rs:197`/`:419`), not
hand-typed. Confirm `file_backed.rs` source is unchanged (it serializes `Span` by value;
the derive carries the change). Confirm NDJSON framing and fsync path (ADR-0049/0059/0060)
untouched.

**Gate.** `cargo public-api` baseline regenerated and reviewed against ADR-0080 (AttrValue
added, attributes type changed, `Eq` removed on `Span`/`SpanBatch`). Mutation 100% on the
new arms of `as_f64`/`canonical_string`. Gold reopen test green.

---

## Slice 2 — FIDELITY: typed ingest decode + faithful JSON round-trip  **[DURABILITY-CRITICAL]**

**What.** Add `fold_span_attributes` in `crates/aperture-storage-sink/src/translate.rs`
(StringValue→String, BoolValue→Bool, IntValue→Int, DoubleValue→Double; Bytes/Array/Kvlist
→ String via the existing `hex_lower`/`render_array`/`render_kvlist`; None/empty→String("")).
Switch only `translate_span`'s own `attributes` to it (`translate.rs:333`); leave
`resource_attributes`, `translate_events`, `translate_links` on `fold_attributes`. The
read path (`success_response`, `lib.rs:608`) needs no change — `Span`'s Serialize now
emits numbers.

**Why second.** Delivers the headline need (numeric round-trip) end to end: OTLP ingest →
ray store (WAL+snapshot) → `GET /api/v1/traces` JSON numbers. Depends on slice 1.

**Hypothesis.** Ingesting `payment.amount` of `9, 90, 99.99, 100, 250, 250.50, 500` as
OTLP Int/Double, then reading `GET /api/v1/traces`, returns JSON numbers
`9, 90, 99.99, 100, 250, 250.5, 500` (decimals preserved, ints not `.0`), and the values
survive a store reopen (WAL replay AND snapshot load).

**Durability review focus.** This is the first slice to write typed records to disk —
the reopen-after-typed-ingest test is mandatory and must exercise both the WAL-replay and
snapshot-load paths (`file_backed.rs:131`, `:140`). Confirm legacy string attributes
ingested under the old fold (if any in the fixture) still read back as `String`.

**Gate.** Mutation 100% on `fold_span_attributes` arms. Existing slice_10 exact-match and
all ray recovery suites (`v1_slice_01/02`) still green. `scripts/ci-watch.sh` watched
after merge (deep durability gate).

---

## Slice 3 — exact-match filter on canonical form (no behaviour change)

**What.** Update `retain_traces_with_attribute` (`lib.rs:309-324`) to compare the wire
`attr_value` against `AttrValue::canonical_string()` instead of `v == value`. Pure
refactor to preserve slice_10 semantics under the new type.

**Why third.** Keeps the existing `attr_key`/`attr_value` filter correct before adding the
new numeric one. Small, low-risk, no durability impact.

**Hypothesis.** Every slice_10 scenario passes byte-for-byte; `attr_value=alice` matches a
`String("alice")`, and `attr_value=100` matches an `Int(100)`.

**Gate.** slice_10 green unchanged; mutation 100% on `canonical_string` use.

---

## Slice 4 — THRESHOLD: numeric `attr_gte` filter

**What.** Add `attr_gte: Option<String>` to `TracesParams` (`lib.rs:219-222`), parse to
`f64` before the store (`lib.rs:405-408` posture), extend `AttributeFilter`/parse with the
both-or-neither + mutual-exclusion rules (Decision 6, ADR-0080), and add a
`retain_traces_with_attribute_gte` using `AttrValue::as_f64()`. Reasons never echo the raw
key/value.

**Why fourth.** The follow-on numeric compare. Depends on the value being stored
numerically (slices 1–2). No durability impact (read-side only).

**Hypothesis.** `attr_key=payment.amount&attr_gte=100` returns the traces with
`{100, 250, 250.50, 500}` and excludes `{9, 90, 99.99}` (anti-lexical); a string-valued
attribute under `gte` is excluded; `attr_gte` without `attr_key` is 400; `attr_value`
+`attr_gte` together is 400.

**Gate.** New acceptance slice green; mutation 100% on the `>=` compare and the mixed-type
`None` exclusion; redaction asserted (no raw key/value in any 400 body).

---

## Slice 5 — demo overlay emits typed `payment.amount`

**What.** Add a `payment.amount` `AttrValue` to each `DemoSpanSpec`
(`crates/kaleidoscope-demo-overlay/src/trace.rs:52-66`, `:77-154`) spread across all seven
values `{9, 90, 99.99, 100, 250, 250.50, 500}` (ints `Int`, decimals `Double`). Resolve
the 6-spec ↔ 7-value mismatch by **adding one 7th healthy spec** (carol's checkout), giving
a 1:1 value→trace mapping (DESIGN decision, pinned here so slice_5 acceptance is
deterministic):

| # | spec (trace.rs:77-154) | customer | status | `payment.amount` |
|---|------------------------|----------|--------|------------------|
| 1 | failed checkout | alice | Error | `Double(250.50)` |
| 2 | checkout | alice | Ok | `Int(9)` |
| 3 | cart | alice | Ok | `Int(90)` |
| 4 | products | bob | Ok | `Double(99.99)` |
| 5 | checkout | bob | Ok | `Int(100)` |
| 6 | cart | carol | Ok | `Int(250)` |
| 7 | checkout (NEW spec) | carol | Ok | `Int(500)` |

This straddles 100 and is discriminating: `attr_gte=100` returns the 4 traces
`{alice-failed 250.50, bob-checkout 100, carol-cart 250, carol-checkout 500}` and excludes
the 3 `{alice-checkout 9, alice-cart 90, bob-products 99.99}`. Composing
`customer.id=alice & attr_gte=100` narrows alice's three traces to exactly the one failed
checkout (`250.50`) — a coherent IDSEARCH+THRESHOLD story. Adding the 7th spec means the
existing count-6 assertions (`trace.rs:386`, `:438`, `:777-782`) move to 7 as part of this
slice. Overlay stays read-only.

**Why last.** Makes the numeric story demonstrable on the always-current demo; depends on
the full typed pipeline (1–4). No durability impact (synthesis is read-time only,
`trace.rs:24-26`).

**Hypothesis.** On the demo tenant+service, `attr_gte=100` returns the demo traces whose
`payment.amount >= 100` and excludes the rest; `customer.id=alice&attr_gte=100` composes
to the expected subset; the demo is never persisted (two reads each return the same set).

**Gate.** Demo overlay tests green incl. the read-only no-accumulation invariant
(`trace.rs:761-784` shape); narrative/slide updated per wave-closure (project memory).

---

## Cross-cutting

- **Durability-critical slices are 1 and 2.** Both require a pre-change on-disk fixture
  and a reopen test across WAL replay + snapshot load. Extra review there; the rest are
  read-side or synthesis-only.
- **No `file_backed.rs` source edit** is expected in any slice. If a slice finds it needs
  one, stop and re-review against ADR-0080 §Decision 3 — the design premise is that the
  derive carries the format change.
- **Public-api baseline** changes once, in slice 1; later slices must not move it further
  (trait signatures, predicate, logs/metrics unchanged).
- **Paradigm**: data + free functions + traits; no new `dyn`, no inheritance (CLAUDE.md).
