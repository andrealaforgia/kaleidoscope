# ADR-0080 — Typed span attribute values: an untagged `AttrValue` with migration-on-read, span attributes only

- **Status**: Proposed
- **Date**: 2026-06-18
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `typed-span-attributes-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0041 (`aperture-storage-sink-translation-and-tenancy` — the OTLP→pillar
  translation, §6.4/6.5 attribute fold and `any_value_to_string`; this ADR adds a
  span-specific typed fold beside the shared string fold, NOT modified). ADR-0048
  (`ray-trace-query-api-contract` — the traces read contract whose JSON array response
  now carries typed numbers; cited, NOT modified). ADR-0059
  (`earned-trust-wal-torn-tail-recovery` — the NDJSON torn-tail framing; this ADR does
  NOT change the framing, only the per-record `attributes` value shape; cited, NOT
  modified). ADR-0060 (`earned-trust-store-fsync-durability` — per-record fsync +
  atomic snapshot for ray; this ADR touches no codec/fsync logic in
  `crates/ray/src/file_backed.rs`; cited, NOT modified). ADR-0001 (public API surface;
  this ADR records an intentional surface evolution of `ray`). ADR-0005 (the five CI
  gates: Gate 2 `cargo public-api` byte identity, Gate 5 100% mutation kill on modified
  files). ADR-0079 (`always-current-demo-lifecycle` — the demo overlay that now also
  emits a typed `payment.amount`; cited, NOT modified).

## Context

Span attribute values in the traces pillar are string-only end to end. The field is
`ray::Span.attributes: BTreeMap<String, String>` (`crates/ray/src/span.rs:196`), and the
OTLP ingest coerces every scalar `AnyValue` to a `String` at a single point —
`any_value_to_string` (`crates/aperture-storage-sink/src/translate.rs:550-561`):
`IntValue(i).to_string()`, `DoubleValue(d).to_string()`, `BoolValue(b).to_string()` —
folded into the map by `fold_attributes` (`translate.rs:537-543`). Consequently:

1. **FIDELITY gap** — a numeric span attribute such as `payment.amount` of
   `9, 90, 99.99, 100, 250, 250.50, 500` returns from `GET /api/v1/traces` as the JSON
   strings `"9" … "500"`, not the numbers `9 … 500` (decimals `99.99`, `250.5`). The
   read side serializes `Span` faithfully (`crates/trace-query-api/src/lib.rs:608-610`),
   so the loss is entirely in the value type, not the serializer.
2. **THRESHOLD gap (follow-on, already named)** — a numeric
   `attr_key=payment.amount&attr_gte=100` filter must compare NUMERICALLY (`>=100`
   includes `{100, 250, 250.50, 500}`, excludes `{9, 90, 99.99}` — the anti-lexical
   case). The existing attribute filter is exact string match only
   (`retain_traces_with_attribute`, `lib.rs:309-324`). A numeric compare needs the value
   stored numerically.

The on-disk durable format is the `serde_json` derive of the `Span` struct itself, both
in the WAL (one fsynced JSON object per line — `file_backed.rs:419-427`, record
`WalRecord::Ingest { tenant, spans: Vec<Span> }`, `file_backed.rs:51-55`) and in the
snapshot (`serde_json::to_writer` of `Snapshot { traces: Vec<TraceBucket> }` via
`wal_recovery::atomic_write_snapshot`, `file_backed.rs:57-69`, `:193-201`). Recovery
reads them back with `from_reader::<Snapshot>` (`file_backed.rs:131`) and
`replay_wal_tolerating_torn_tail::<WalRecord, _>` (`file_backed.rs:140-151`). **Changing
the attribute value type changes that on-disk format**, so the change must not corrupt
or lose existing stored data.

ADRs in this repository are immutable (superseded, never edited). ADR-0080 is the next
free number (the highest existing was 0079, verified by
`ls docs/product/architecture/adr-*.md`).

## Decision

### 1. A typed `AttrValue` enum in `ray`, mirroring the OTLP scalar variants

`Span.attributes` becomes `BTreeMap<String, AttrValue>` where

```rust
pub enum AttrValue {
    String(String),   // OTLP StringValue (+ Bytes/Array/Kvlist rendered to string)
    Bool(bool),       // OTLP BoolValue
    Int(i64),         // OTLP IntValue   — kept DISTINCT from Double
    Double(f64),      // OTLP DoubleValue
}
```

`Int` is kept distinct from `Double` (a deliberate divergence from the metric path,
which casts `as_int → f64`, `translate.rs:226-234`) so an integer attribute round-trips
as `100`, not `100.0`. The type lives in `crates/ray/src/span.rs`, the home of the
OTLP-shaped boundary types (`span.rs:17-22`).

### 2. `#[serde(untagged)]` — the JSON value token is the discriminator, and IS the migration

`AttrValue` (de)serializes untagged: `String → "x"`, `Bool → true`, `Int → 100`,
`Double → 99.99`. Declaration order `[String, Bool, Int, Double]` makes untagged
deserialization correct for every new value AND backward-compatible for every existing
value: a JSON **string** token matches `String` first (a number-looking string stays a
string; serde_json never coerces a number token into `String`), a JSON `true` matches
`Bool`, a bare integer matches `Int`, a fractional number falls through `Int` (which
rejects it) to `Double`. This is the hand-rolled, dependency-free serde posture already
used for `TraceId`/`SpanId` (`span.rs:67-113`) — no new crate.

### 3. On-disk format evolution: migration-on-read, no version field, no wipe

Existing WAL and snapshot records hold every attribute value as a JSON **string**
(everything was coerced by `any_value_to_string`). On read, the untagged `String`
variant matches first, so an existing value deserializes to `AttrValue::String(...)` —
the prompt's option (a) "old string attrs read as `AttrValue::String`" — achieved with
**no record-version branch, no migration pass, and no data wipe**. New records write
typed values; the JSON token type selects `Int`/`Double`/`Bool` on read, with no
ambiguity between a new `String("100")` and a new `Int(100)` because serde_json
preserves the quoted-vs-bare token.

The NDJSON framing (one JSON object per line, newline-terminated — the ADR-0059
torn-tail boundary) is **unchanged**; this is NOT a WAL-format change in the
ADR-0059/0060 C8 sense (which pins the *framing*, not the per-record *schema*). The
codec logic in `crates/ray/src/file_backed.rs` changes by **zero source lines** — it
serializes `Span` by value and the derive does the rest.

**Honest limit**: data already coerced to a string before this change (e.g. an int
stored as `"100"` by the old ingest) stays `AttrValue::String("100")`; a type destroyed
at the old ingest cannot be recovered. FIDELITY applies to newly-ingested data; existing
data is faithfully preserved as the strings it already was, and is **never wiped**. The
numeric `attr_gte` filter treats such legacy strings as non-numeric and excludes them
(Decision 6). The managed instance is re-seeded with typed numbers, so the limit is
invisible there.

### 4. Scope: span `attributes` only this iteration

Only `Span.attributes` (`span.rs:196`) becomes typed. `resource_attributes`
(`span.rs:199`), span event `attributes` (`span.rs:169`), span link `attributes`
(`span.rs:177`), and all `lumen`/`pulse` attributes stay `BTreeMap<String, String>`.
`translate_span` switches the span's own attributes to a new `fold_span_attributes`;
its `resource_attributes`, `translate_events`, and `translate_links` keep calling the
shared `fold_attributes` (`translate.rs:333-336`, `:410-439`). This confines the blast
radius: `fold_attributes` keeps serving four of five call-sites unchanged, and the
`Eq`-drop (Decision 5) is confined to the two types that transitively carry the typed
map.

### 5. Drop the `Eq` marker on `Span` and `SpanBatch` (keep `PartialEq`)

`f64` is not `Eq`, so `AttrValue` cannot derive `Eq`, and `Span` (`span.rs:183`) and
`SpanBatch` (`span.rs:217`) must drop the `Eq` *marker*, keeping `PartialEq`. This is
**functionally safe**: a repo-wide search found no `HashSet<Span>`/`BTreeSet<Span>` and
`Span` is never a map key (the ray indices key on `TraceId`/`ServiceName` and hold
`Span` as the value — `store.rs:110-111`, `file_backed.rs:86-87`); every
`assert_eq!(... spans)` needs only `PartialEq`. `SpanEvent`, `SpanLink`, `SpanStatus`,
`SpanKind`, `StatusCode` keep `Eq` (their attributes stay `String`). The removed
`Eq impl` for `Span`/`SpanBatch` is an intentional `cargo public-api` (Gate 2) surface
diff recorded by this ADR; the crafter regenerates the baseline.

### 6. Filters: exact-match on canonical form, new numeric `attr_gte`

- **Exact match** (`retain_traces_with_attribute`, `lib.rs:309-324`) compares the wire
  string against `AttrValue::canonical_string()` (`String`→itself; `Int`→decimal;
  `Double`→ryu form; `Bool`→`"true"`/`"false"`). `attr_value=alice` still matches
  `String("alice")` — slice_10 behaviour preserved.
- **Numeric `attr_gte`** — a new `attr_gte: Option<String>` param parsed to `f64`
  before the store, beside `attr_value` (`lib.rs:219-222`, `:405-408`). A trace
  qualifies when any span's `attr_key` value has `AttrValue::as_f64() = Some(v)` with
  `v >= threshold` (`Int`→`as f64`, `Double`→itself, `String`/`Bool`→`None`). Mixed
  type: a string-valued attribute under `gte` is `None` → excluded.
- Param validation extends the existing both-or-neither posture (`lib.rs:286-300`):
  `attr_key`+`attr_value` = exact (today); `attr_key`+`attr_gte` = numeric (new);
  `attr_gte` without `attr_key` = 400; `attr_value`+`attr_gte` together = 400. Reasons
  never echo the raw key/value (redaction, `lib.rs:282-299`).

### 7. Demo overlay emits a typed `payment.amount`

`DemoTraceOverlay::synthesize_span` (`crates/kaleidoscope-demo-overlay/src/trace.rs:198-228`)
gains a `payment.amount` `AttrValue` per `DemoSpanSpec`, spread across
`{9, 90, 99.99, 100, 250, 250.50, 500}` (ints `AttrValue::Int`, decimals
`AttrValue::Double`), straddling 100 so `attr_gte=100` discriminates on the demo. The
overlay stays read-only (`trace.rs:24-26`, `:242-248`).

## Alternatives considered

### Value model A (rejected): keep `String`, add a sidecar typed map

A second `attributes_typed: BTreeMap<String, AttrValue>` field alongside the string map.
For: no change to the existing field, additive. Against: doubles attribute storage on
disk and in memory, needs a precedence rule on read (which map wins), and leaves two
sources of truth that drift. Rejected — the untagged enum migrates the single existing
field in place with no duplication.

### Value model B (rejected): a `Double(F64Bits)` newtype to keep `Eq`

Store `Double` as a `u64` bit-pattern newtype deriving `Eq`/`Ord` by bits, so `Span`
keeps its `Eq` marker. For: no public-api `Eq` diff. Against: gives NaN==NaN and
`-0.0 != 0.0` equality that diverges from `f64` semantics, needs a custom number
(de)serializer to emit a bare JSON number, and adds complexity to preserve a marker that
no code uses (no `HashSet<Span>` anywhere). Rejected — dropping `Eq` is the honest,
idiomatic Rust consequence of an `f64` field.

### Value model C (rejected): cast everything to `f64` like the metric path

Mirror `number_point_value` (`translate.rs:226-234`) and store one numeric type. For:
matches an existing precedent, no `Int`/`Double` split. Against: loses the int-vs-double
distinction (`100` would round-trip as `100.0`), and forces non-numeric attributes
(strings, bools) into a separate field anyway. Rejected — span attributes are
heterogeneous; the metric precedent is for inherently-float metric points.

### Format evolution (rejected): versioned records with a migration pass

Add a schema-version tag to each WAL record / snapshot and migrate old records on read.
For: explicit, self-describing. Against: adding a version tag is *itself* a format
change to records that today have none, and the untagged-enum token discrimination
already achieves backward-compatible read with zero new metadata. A migration pass is
unnecessary machinery for a change the type system migrates for free. Rejected in favour
of migration-on-read (Decision 3).

### Format evolution (rejected): one-time wipe / re-seed

Drop the on-disk store and re-seed. For: simplest possible; the managed instance is
re-seedable. Against: it discards REAL ingested data on any non-demo deployment — the
durability promise (ADR-0049/0059/0060) exists precisely so acked data survives.
Rejected as the default; re-seeding the *demo* is fine because the overlay synthesises
at read time (`trace.rs:24-26`), but the store itself must not be wiped.

### Scope (rejected): promote resource/event/link attributes too, this iteration

For: consistency — one attribute value type everywhere. Against: it spreads the `Eq`
drop to `SpanEvent`/`SpanLink`, rewrites four more `fold_attributes` call-sites, and
enlarges the public-api and mutation surface for fields with no numeric-filter need
(`service.name` etc. are semantic-convention strings). Rejected this iteration; a
follow-up ADR can promote them once the typed shape proves out (Consequences).

## Consequences

### Positive

- **Numeric fidelity and a numeric threshold, with no data loss**. New numeric span
  attributes round-trip as JSON numbers (ints as `100`, doubles as `99.99`/`250.5`),
  and `attr_gte` compares numerically (anti-lexical case correct), while ALL existing
  stored data survives via migration-on-read.
- **Zero change to the durability-critical codec**. `crates/ray/src/file_backed.rs`
  (WAL append, atomic snapshot, replay, open) is unmodified; the fsync discipline
  (ADR-0049/0060) and torn-tail framing (ADR-0059) are untouched. The risk is confined
  to serde format-compatibility, which the untagged round-trip and a pre-change-fixture
  gold test cover.
- **Least-invasive surface**. One new type, one field type change, one new ingest fold,
  one new query param, plus the demo attribute. Trait signatures (`TraceStore`),
  predicate (`span_name`/`kind`/`status`), and logs/metrics translation are unchanged;
  existing test doubles stand.
- **Parse-free numeric compare**. `as_f64` on a stored `Int`/`Double` avoids per-compare
  string parsing a lexical workaround would need.

### Negative

- **`Span`/`SpanBatch` lose the `Eq` marker** — an intentional `cargo public-api`
  (Gate 2) diff. Mitigated: functionally unused (no `HashSet<Span>`), `PartialEq`
  retained, baseline regenerated under this ADR.
- **Pre-change string data keeps its string type** — values coerced before the change
  are not retroactively numeric and are excluded from `attr_gte`. Mitigated: re-seed on
  the managed instance; the limit is documented and honest, and never costs data.
- **Two attribute value types coexist in `Span`** (typed span attributes; string
  resource/event/link attributes) until a follow-up consistency ADR. Accepted as the
  scope-discipline trade.

### Trade-off summary

The change is intentionally narrow: promote ONE field (`Span.attributes`) to a typed
untagged `AttrValue`, let the JSON token discriminate on read so existing data migrates
for free, and add the numeric filter and demo spread on top. The trade is "keep `Eq` and
a uniform attribute type (simpler surface, but no numeric fidelity)" against "drop the
unused `Eq` marker and accept one typed field (numeric fidelity + numeric threshold, no
data loss)". v0 takes the latter — numeric fidelity is the entire point of the feature.

## Verification

- **Earned-Trust (three orthogonal layers, per the methodology and the ADR-0049/0059/0060
  precedent)**: (a) **subtype/type check** — `AttrValue` is consumed where `Span` is;
  `cargo check` + `cargo public-api` (Gate 2) catch the type and `Eq` surface change
  (the `mypy`-equivalent). (b) **structural check** — an AST pre-commit pin asserts
  `translate_span` folds the span's own attributes through `fold_span_attributes` (the
  typed fold), not the shared string `fold_attributes`. (c) **behavioural gold-test** —
  a fixture WAL+snapshot written in the OLD string shape loads under the new binary and
  yields `AttrValue::String` (the migration-on-read probe — "the on-disk substrate does
  not lie after the type change"), AND a new typed record round-trips through reopen as
  numbers (the fidelity probe). A single-layer bypass is caught by at least one of the
  other two.
- **Self-application of Earned-Trust**: the gold round-trip test is the probe that the
  durable store actually preserves both old strings and new numbers across a real
  reopen — not merely that the type compiles. Asking "what happens when the old on-disk
  data meets the new type?" is answered by the migration-on-read gold test with a
  pre-change fixture.
- **Mutation testing** (`cargo mutants --in-diff`, 100% kill on modified files, ADR-0005
  Gate 5; CLAUDE.md): primary targets — the per-variant arms of `fold_span_attributes`
  (an arm collapsed to `String` must be caught), `AttrValue::as_f64` (the
  `String`/`Bool` → `None` arms and the `Int as f64` cast), `canonical_string`, and the
  `attr_gte` `>=` comparison and mixed-type exclusion. Scoped to `crates/ray/src/span.rs`,
  `crates/aperture-storage-sink/src/translate.rs`, `crates/trace-query-api/src/lib.rs`,
  `crates/kaleidoscope-demo-overlay/src/trace.rs` — NOT `file_backed.rs` (unmodified).
- **Gate 2 (`cargo public-api`)** records the intended `ray` surface change: `AttrValue`
  added, `Span.attributes` type changed, `Eq` removed from `Span`/`SpanBatch`. This ADR
  is the authority for accepting the new baseline.
- **CI watch** (CLAUDE.md): the durability gold test and the ray mutation job are deep
  gates; `scripts/ci-watch.sh` is the cadence that surfaces a deep-only regression after
  the FIDELITY (durability-touching) slice lands.

## External-integration handoff

None. The change is within the in-process traces pillar (ingest translation → ray store
→ trace-query-api JSON). No new network service, no third-party API. OTLP itself is the
inbound wire format and is already pinned (ADR-0003); the typed values make the read-side
JSON *more* faithful to the OTLP source types, not less. No consumer-driven contract test
recommendation.
