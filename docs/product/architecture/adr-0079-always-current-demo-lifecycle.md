# ADR-0079: The always-current demo lifecycle — read-time synthesis, store-free, service-scoped

## Status

Accepted (DESIGN wave, `always-current-demo-v0`). Author: Morgan
(`nw-solution-architect`), PROPOSE mode. Read-only grounding on `main`; every
claim below is cited to source read this run (`file:line`) or named explicitly as
a DELIVER must-verify. Companion design doc:
`docs/feature/always-current-demo-v0/design/options.md`.

Builds on ADR-0076 (the consolidated runtime + the durable per-signal stores),
ADR-0077 (the experimentable stack + the `telemetrygen` seed), and ADR-0078
(same-origin Prism on the single auth-off tenant).

## Context

The managed consolidated-runtime instance is handed to an outsider as an
"always-current" experimentable stack. Today the demo is a **one-time** seed of
**fixed-timestamp** telemetry pushed once over OTLP
(`crates/kaleidoscope-telemetrygen/src/lib.rs:319-423`). Two grounded facts make
it go stale and un-refreshable:

1. **The stores append on ingest with no dedup.** ray:
   `entry(...).or_default().push(span.clone())`
   (`crates/ray/src/file_backed.rs:335-355`); lumen: `bucket.extend(...)`
   (`crates/lumen/src/file_backed.rs:230`); pulse: `entry.points.extend(points)`
   (`crates/pulse/src/file_backed.rs:474`). Re-seeding to refresh duplicates.
2. **No delete / reset / retention capability exists on any store port**
   (`crates/ray/src/store.rs:66-95`; `crates/lumen/src/store.rs:76-95`;
   `crates/pulse/src/store.rs:72-99` — only `ingest`/`query`/`query_with`), and
   the demo shares tenant `acme` with the Customer's own telemetry
   (`compose.yaml:39`), so the demo cannot be cleared without clearing her data;
   `make clean` wipes the whole shared volume (`Makefile:74-76`).

So a newcomer opening the instance a day after seeding sees an **empty** demo
(fixed timestamps fall outside the rolling window) and refreshing **accumulates
duplicates**.

A load-bearing read-path constraint shapes the solution: with auth **off** (the
deliberate local posture; `read_auth: None` at
`crates/kaleidoscope-runtime/src/main.rs:113`), each query router is pinned to
**one** query tenant (`crates/kaleidoscope-runtime/src/main.rs:104-109`) and
Prism is served same-origin against that one tenant (ADR-0078). A demo in a
separate `demo` tenant would **not** be visible in the Customer's Prism. The demo
must therefore live in the same `acme` tenant, distinguished by **service
identity**, not by tenant.

**Quality attributes (ISO 25010).** Primary: **reliability/recoverability** (real
tenant data must survive and must not be put at risk — the durability foundation
is the whole point) and **usability** (a current, non-empty demo on any day, no
operator action). Secondary: **maintainability/blast radius** (smallest surface,
no new durability-critical code) and **functional faithfulness** (close to the
real pipeline, but honestly tradeable against safety).

## Decision

**Synthesise the demo records at READ time, store-free, scoped to the demo
service identity, via a read-side decorator wired into the composition root.**

A new small read-only module — `DemoOverlay<S>` — implements the three store
ports (`MetricStore`/`LogStore`/`TraceStore`). It wraps the inner real store and
**delegates every call straight through**; additionally, for queries matching the
demo identity (`service.name = kaleidoscope-demo`, the demo trace ids
`4bf92f…` + the three healthy ids, the `request_count` metric, the
`checkout failed: card declined` cause log — the ADR-0077 sample vocabulary), it
synthesises records with timestamps computed `now - offset` and **merges** them
into the returned `Vec`. Nothing is ever written to a store.

It is wired in `spawn_consolidated` between the shared store `Arc` and the query
routers, **on the router side only** (`crates/kaleidoscope-runtime/src/lib.rs:307-384`);
the ingest **sink keeps the inner real store**, so the sole-writer durability
path and the shared-`Arc` read-your-write for real data (ADR-0076 DD2) are
byte-identical. The `telemetrygen` seed is **repositioned** (not deleted): it
remains available via `make demo` as the real-pipeline / `spark`-dogfood
demonstration; synthesis is the *always-current* layer.

This satisfies all four hard constraints **by construction**:

- **Always-current** — `now - offset` always lands inside a rolling window.
- **No accumulation** — nothing is stored.
- **Customer data untouched** — the demo has no write path, so it *cannot
  physically reach* her data; the strongest possible form of "untouched".
- **Reasonably faithful** — the real query routers (PromQL subset, label
  matchers, by-id lookup, `error=true` filter, trace-with-logs join) and Prism
  render the demo; only the demo's *write* path — the half a newcomer never sees
  — is synthetic.

No new store capability; **zero blast radius on the durability path**
(ADR-0076/0049/0060 untouched).

### Earned Trust (principle 12) — the overlay's own probe

`DemoOverlay` is a new read-side adapter, so it carries a first-class probe under
the runtime's existing "wire → probe → use" invariant
(`crates/kaleidoscope-runtime/src/lib.rs:320-336`). Stated as behavioural
acceptance criteria (Given-When-Then):

- **Currency probe (refuse-to-start on staleness).**
  *Given* `DemoOverlay` wired into the composition root, *when* the startup
  currency probe issues a demo query through the overlay, *then* the returned
  synthetic record's timestamp falls inside the rolling window; *and given* a
  synthetic record anchored **outside** the window (clock/timezone/window-math/
  offset bug — the exact failure that emptied the original demo), *when* the
  probe runs, *then* the runtime **refuses to start** with a structured
  `health.startup.refused` rather than booting a silently-empty demo.
- **Pass-through fidelity.**
  *Given* a **non-demo** read through the overlay, *when* the query is issued,
  *then* the result is byte-identical to the inner store's result (real
  read-your-write preserved).
- **Read-only invariant (principle 11).**
  *Given* the `DemoOverlay` source, *when* the read-only enforcement rule runs in
  CI, *then* it fails the build if `DemoOverlay` invokes any write/`ingest` method
  on its inner store. Rust mechanism (ArchUnit has no Rust equivalent): a CI
  static check — the project's existing structural-enforcement approach — asserting
  the overlay's `ingest` impls contain **only** a delegating call to the inner
  store and the synthesis branch never reaches a write. DELIVER picks the concrete
  tool (a `compile_fail` trybuild guard, a custom-lint/CI grep gate, or an
  enforced code-review checklist item) and records it in the slice; the AC is the
  *property*, not the tool.

The container host clock is assumed NTP-synchronised (or on a cloud time
service). The startup probe catches offset/window-math/timezone **code** bugs;
production **clock skew** greater than the window duration is out of scope for the
probe and is mitigated operationally (host NTP + the CI HTTP smoke on every
merge). This assumption is recorded in `compose.yaml` and the getting-started
docs (Slice D).

## Alternatives considered

### A1 — Tenant separation + per-tenant reset (REJECTED)

Put the demo in its own `demo` tenant; refresh by a periodic clear-and-reseed of
only that tenant. **Rejected**: (a) requires a NEW `reset(tenant)` capability on
all three store ports that is durability-honest — WAL-logged and replayed on
recovery, or a crash mid-reset resurrects the demo or a recovery bug reaches real
data — putting new code on the sensitivity-critical sole-writer path
(`crates/*/src/file_backed.rs`; ADR-0076/0049/0060); and (b) it **still** leaves
the demo invisible in the auth-off single-tenant Prism
(`crates/kaleidoscope-runtime/src/main.rs:104-109`) without turning auth on or
adding a second origin. Two hard problems to solve a soft one. Most faithful, but
the highest blast radius. Revisit only if the instance gains real multi-tenant
auth-on read for independent reasons.

### A2 — Store-level retention / TTL (REJECTED)

Age old data out so re-seed does not accumulate. **Rejected**: TTL is a
retention policy on the Customer's **real** durable telemetry, and the durability
foundation's whole purpose is that real telemetry survives (`make down` preserves
the volume; `Makefile:55-57`). It silently deletes real data and, crucially, does
**not** deliver currency — re-seed is still required. Wrong instrument.

### A3 — Re-seed + ingest-side dedup (upsert by identity) (REJECTED)

Make ingest overwrite by `span_id` / log identity so re-seed does not duplicate.
**Rejected**: it changes ingest semantics for **all** tenants on the most
sensitive code — every store's `apply_ingest` becomes upsert-by-identity, altering
the append-only WAL semantics, the snapshot, and ray's no-drift dual-index
guarantee plus its mutation gate (`crates/ray/src/file_backed.rs:327-358`). And it
still does not deliver currency: identical-identity re-seed is not current, while
current re-seed (new timestamps) accumulates unless ALSO deleted (back to A1/A2).
Highest blast radius, lowest payoff.

### A4 — Operational-only: a separate throwaway demo instance/volume (REJECTED)

A second compose stack on its own volume, periodically recreated. **Rejected**:
zero store risk, but it is **not "the same instance she uses"** — the demo sits on
a different origin she never opens, failing the literal "always-current demo on
the managed instance" intent; and currency is coarse (whole-stack recreate).

## Consequences

### Positive

- All four hard constraints met **structurally**, not by careful policy: current,
  non-accumulating, Customer-data-physically-unreachable, and faithful on the
  explored (read) half.
- **Zero durability blast radius** — no new store capability, no change to
  ingest/WAL/snapshot/fsync; the sensitivity-critical path is untouched.
- Smallest surface: one read-side module + a few composition-root lines; no
  change to pulse/lumen/ray, the sink, the routers, Prism, or the auth posture.
- Bonus: the demo is present even on a fresh empty volume (`make clean`) because
  it is store-free — a newcomer cannot land on an empty stack.
- The honest fidelity limit is contained and explicitly compensated: `make demo`
  remains the real-pipeline / dogfood path.

### Negative / trade-offs

- **The demo records do not exercise the real ingest→WAL→snapshot path.** This is
  the deliberate fidelity-for-safety trade. Compensated by keeping the real
  `telemetrygen` push on `make demo` and stating the split in the getting-started
  docs and `outcome-kpis.md`.
- The overlay adds a thin read-path branch (an identity match + a merge) to every
  query; it must short-circuit cleanly for non-demo reads so real-data latency and
  semantics are unaffected — guarded by the read-only enforcement rule and the
  pass-through tests.
- Currency depends on a correct clock + window math; mishandled, the demo could
  silently empty again. Mitigated by the mandatory startup currency probe above.

### Earned Trust discharge

The one new adapter (`DemoOverlay`) introduces one new probe obligation, fully
specified: a startup currency probe with an explicit fault scenario (record
anchored outside the window ⇒ `health.startup.refused`), plus a structural
read-only enforcement rule. No new external substrate is introduced; the
durability substrate (fsync, ADR-0049/0060) and its probes are unchanged because
the demo never writes.

## External integrations

**None requiring contract tests.** The overlay is first-party, in-process, and
store-free; it consumes no external API. Consistent with ADR-0076/0077, no
consumer-driven contract test is recommended.

## Enforcement

- **Store-free / read-only**: a Rust CI static check (concrete tool chosen in
  DELIVER — `compile_fail`/trybuild guard, a custom-lint/CI gate, or an enforced
  review-checklist item) asserting `DemoOverlay` never invokes a write/`ingest`
  method on its inner store. The AC is the property; ArchUnit is Java-only and
  has no direct Rust equivalent.
- **Pass-through fidelity**: tests asserting non-demo reads return the inner
  store's result unchanged (real read-your-write preserved).
- **Currency**: the mandatory startup currency probe (refuse-to-start on a
  stale/empty synthetic record) + the CI HTTP smoke (ADR-0077 F5) extended to
  assert now-relative demo data returns for all three signals.
- **Honest-claim discipline**: the fidelity trade ("the demo's write path is
  synthetic; `make demo` is the real pipeline") recorded in the ADR, the
  getting-started docs, and `outcome-kpis.md`.

## References

- ADR-0076 (consolidated runtime + durable stores), ADR-0049/0060 (fsync),
  ADR-0059 (WAL torn-tail), ADR-0077 (experimentable stack + seed), ADR-0078
  (same-origin Prism, single tenant).
- Design doc: `docs/feature/always-current-demo-v0/design/options.md`.
- Source read this run: `crates/ray/src/file_backed.rs:327-358`;
  `crates/lumen/src/file_backed.rs:230`; `crates/pulse/src/file_backed.rs:431-477`;
  `crates/ray/src/store.rs:66-95`; `crates/lumen/src/store.rs:76-95`;
  `crates/pulse/src/store.rs:72-99`;
  `crates/kaleidoscope-runtime/src/lib.rs:300-389`;
  `crates/kaleidoscope-runtime/src/main.rs:78-116`;
  `crates/kaleidoscope-telemetrygen/src/lib.rs:56-95,319-423`; `compose.yaml`;
  `Makefile`.
</content>
