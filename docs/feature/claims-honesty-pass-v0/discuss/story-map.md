# Story Map: claims-honesty-pass-v0

## User: Devin — a senior engineer evaluating Kaleidoscope for adoption

Devin reads the README, the crate docs, the codenames, and the Cargo.toml
descriptions to decide whether Kaleidoscope does what it says. Every claim that
overstates a capability costs Devin trust the moment the code is opened — which
is fatal for a project whose entire thesis is *structural honesty against vendor
overstatement*.

## Goal: read any Kaleidoscope claim and have it match the code, or be clearly marked future/roadmap

---

## Backbone (reader's journey through the claims)

The "activities" are the surfaces Devin reads, left to right in the order a new
evaluator encounters them.

| A. Read the README headline table | B. Read a crate's source doc | C. Read a crate's test/build headers | D. Probe an endpoint's behaviour |
|------------------------------------|------------------------------|--------------------------------------|--------------------------------|
| Codename + role claims (Loom, Spark, Strata, Cinder) | Harness "wire spec" validation-depth claim | Codex stub headers over green code | query-api `step` honoured? |
| Cost-table "Continuous profiling … included" | Harness README "implementation absent" status | query-http-common module scaffold doc | Harness `GrpcProtobuf` acted on? |
| | | trace-query-api scaffold doc comments | |

---

## Scope Assessment: PASS — 9 stories, ~7 crates + 1 README, estimated ~4 days

Elephant-carpaccio assessment against the oversized signals (any 2+ → oversized):

- User stories: **9** (threshold >10) — under.
- Bounded contexts/modules touched: README + 6 crates (harness, codex,
  query-http-common, trace-query-api, query-api, + the README's 4 component
  rows). These are **independent doc surfaces**, not a coupled system; each slice
  touches one. Under the spirit of the >3 threshold because there is no shared
  behaviour or integration between them — they are parallel, not composed.
- Walking skeleton integration points: **0** (no skeleton; brownfield docs).
- Estimated effort: ~4 days of small, independent prose edits + 2 possible small
  code touches — under 2 weeks.
- Independent user outcomes: yes, and that is the POINT — each slice ships
  alone. This is carpaccio working as intended, not oversizing.

Verdict: **right-sized as a grouped doc sweep.** Each slice is independently
shippable, 1-3 days, 3-7 scenarios. No split required. The two code-touch items
are isolated into their own slices so the pure-prose majority ships without
waiting on a DESIGN decision.

---

## Slices (carpaccio order — cheapest/sharpest first)

There is **no walking skeleton** (brownfield docs, no end-to-end flow). Slices
are ordered by "cheapest + sharpest edge first": the codename overstatements and
the stale-over-green markers are the very cheapest and carry the sharpest
honesty edge.

### Slice 01 — README codename honesty (US-01) — CHEAPEST, SHARPEST

Correct the four false codename/role claims in the README "Components at a
glance" table + the cost-table profiling line, aligned to each crate's
already-honest `lib.rs`:

- Loom: "Dashboards-as-code, alert-rules-as-code" → "Rule-catalogue change
  control (TOML); dashboards-as-code is v1+".
- Spark: "Auto-instrumentation SDKs" → "Manual-init OTel SDK wrapper;
  auto-instrumentation is v0.2/v1".
- Strata: "Continuous profiling" → "Profile storage (passive sink); continuous
  scraping is future" (and the cost-table line aligned).
- Cinder: "cold-tier coordinator" → "Local tier-metadata coordinator;
  object-storage (S3/OpenDAL/Iceberg) cold tier is v2".

Pure prose. Outcome: the first table a reader sees stops overstating four
capabilities. Targets KPI-1 directly.

### Slice 02 — Codex stub-declaration honesty (US-02)

Correct the stale "DISTILL stub / Tests panic on `unimplemented!()`" declarations
over green code: `codex/Cargo.toml:17-24`, the five `tests/slice_0*.rs` headers,
and `tests/common/mod.rs:14-16`. The crate is fully delivered and green
(`lib.rs:43-48`). Pure prose. Outcome: codex stops declaring itself a stub.

### Slice 03 — Stale `__SCAFFOLD__`-over-green doc comments (US-03)

Remove/correct the stale scaffold doc comments sitting over fully-delivered green
bodies: `query-http-common/src/lib.rs:30-42` (module doc claiming all fns are
`unimplemented!`) and `trace-query-api/src/lib.rs:207-209,228-232` (handler
"`unimplemented!` scaffold" over the live `handle_traces_by_id`/`parse_trace_id`).
Guardrail: touch ONLY markers proven to sit over GREEN code; leave every
genuinely-RED / `#[ignore]`d in-flight scaffold untouched. Pure prose.

### Slice 04 — Harness validation-depth honesty (US-04)

Correct the harness "validates against the OTLP **wire specification**" overclaim
in `lib.rs:1-7`, `README.md:3-4`, and `Cargo.toml:11` to "**structural
decode-level** validation" (non-empty, first-tag-is-resource-field, decodes as
the asserted type, signal-mismatch fallback — NOT semantic). Also correct the
harness README status block (`README.md:8-16`) that still says "implementation
intentionally absent / every fn returns `unimplemented!()`" over the live code.
Pure prose.

### Slice 05 — query-api `step` honesty (US-05) — DOCUMENT-vs-IMPLEMENT (DESIGN flag #1)

The in-code field doc is already honest ("`step` accepted and ignored at v0");
the residual is the README "Prometheus-compatible" framing implying a stepped
grid. DESIGN decides document (qualify the README + add the black-box guard) vs
implement (build the stepped grid). DISCUSS recommends DOCUMENT. Carries a code
touch ONLY if DESIGN picks implement.

### Slice 06 — Harness `GrpcProtobuf` framing honesty (US-06) — DOCUMENT-vs-IMPLEMENT (DESIGN flag #2)

`Framing::GrpcProtobuf` is accepted but never acted on. DESIGN decides document
(flag the no-op framing at `lib.rs`/README level) vs honour (strip the gRPC
length prefix). DISCUSS recommends DOCUMENT. Carries a code touch ONLY if DESIGN
picks honour.

---

## Priority Rationale

Priority is by **honesty impact × reader-reach ÷ effort**, cheapest/sharpest
first, with the two document-vs-implement items deliberately LAST so the
pure-prose majority ships without blocking on a DESIGN decision.

| Priority | Slice | Target outcome | Effort | Rationale |
|----------|-------|----------------|--------|-----------|
| P1 | US-01 README codenames | The headline table stops overstating 4 capabilities | XS (prose) | Highest reader-reach (every evaluator reads it first) × sharpest edge (codenames ARE the brand) ÷ trivial effort. The single best honesty-per-keystroke move. |
| P2 | US-02 Codex stub headers | Codex stops declaring itself a stub | XS (prose) | Cheapest of all; a delivered crate calling itself unbuilt is the most embarrassing kind of stale prose. |
| P3 | US-03 Stale `__SCAFFOLD__`-over-green | Two delivered crates stop claiming `unimplemented!` bodies | S (prose, careful) | Cheap, but needs the guardrail (distinguish stale-over-green from in-flight RED), so it ranks just below the pure-table edits. |
| P4 | US-04 Harness validation depth | The conformance harness stops claiming semantic validation | S (prose) | Sharp edge (a *conformance* harness overstating conformance is the project's thesis in miniature), slightly more prose than a table row. |
| P5 | US-05 query-api `step` | The read endpoint's stepped-grid implication matches reality | S-M (DESIGN decision) | Document-vs-implement; deferred behind the pure-prose slices so it does not block them. The verifier's black-box is already in flight. |
| P6 | US-06 Harness framing | The harness's `GrpcProtobuf` claim matches behaviour | S-M (DESIGN decision) | Document-vs-implement; lowest reader-reach of the set (an internal enum variant), so last. |

Dependencies: all six slices are **independent** — any can ship alone. US-04 and
US-06 both touch the harness, so if shipped together they share one PR; US-06 may
be folded into US-04's PR at DELIVER's discretion (noted, not required).

There is no walking skeleton and no release-band layering: every slice IS a
release. The carpaccio is "one honest claim at a time."
