# Story Map: claims-honesty-pass-2-v0

## User: Devin Okafor

Senior platform engineer at Northwind Logistics, evaluating Kaleidoscope to
replace a five-figure Datadog bill. Devin reads claims — a crate doc, a comment,
the platform README, a CI config — then opens the code to verify them. A claim
that **overstates** the code makes Devin over-trust (Prism = Datadog); a claim
that **understates** the code makes Devin under-trust (pulse loses my data on
restart). For an honesty-thesis project, either drift is the worst possible
failure.

## Goal

Read any honesty-relevant surface of Kaleidoscope (pulse crate doc, gateway
comments, the platform README's Prism claims, the prism e2e config) and find that
it matches what the code actually does — neither over- nor under-stated.

## Backbone

The "journey" is reading-to-verify, walked locus by locus. Each activity is a
surface a reader encounters; each task is one claim that must match the code.

| Read the pulse docs | Read the gateway comments | Read what the platform claims about Prism |
|---------------------|---------------------------|-------------------------------------------|
| Crate doc names the durable store (not "restart loses points") | `init_tracing` comment matches the real subscriber install | README Prism row matches the single-metric reality |
| Crate doc + Cargo.toml don't promise an undelivered columnar substrate | "sink.kind = stub" comment matches `Config::builder().build()` (relies on default) | Cost table doesn't claim a non-existent "compliance dashboards in Prism" |
| | Test-module prose matches the GREEN suite (not "no-op / RED") | Playwright config doesn't advertise a browser-matrix e2e gate that doesn't run |

---

### Walking Skeleton

**None.** This is a brownfield documentation-honesty feature: there is no
end-to-end runtime flow to thread. The three slices are independent reading
surfaces. (Decision and rationale in `wave-decisions.md`.)

### Release: all three slices ship together (one honesty pass)

There is one release. The three slices are independently shippable in any order
but together constitute "the residual doc-honesty cluster from the four-quadrants
reports". Ordered below by sharpness of the lie.

### Slice US-01 — pulse docs stop lying about volatility and columnar

- **Tasks**: correct `pulse/src/lib.rs:37,46` (inverted-volatility) and
  `pulse/src/lib.rs:20-21,41` + `pulse/Cargo.toml:7` (undelivered columnar).
- **Target outcome (KPI)**: a reader of the pulse docs encounters zero claims
  that contradict the shipped `FileBackedMetricStore` — neither understating its
  durability nor overstating an undelivered columnar substrate.
- **Rationale**: this is the *notable* finding — it tells users their data is
  volatile when it is durable (under-trust), and simultaneously promises a
  columnar engine that is not built (over-trust). Both directions of the job in
  one crate. Sharpest lie → first.

### Slice US-02 — gateway comments match the code

- **Tasks**: correct `main.rs:62-63` (no-op→real subscriber), `main.rs:118-120`
  + module doc `:24-25` (force→relies-on-default), and the test-module prose in
  `tests/slice_01_tracing_subscriber.rs:42-51,206-208,280` (no-op/RED→GREEN).
- **Target outcome (KPI)**: a contributor reading `kaleidoscope-gateway`
  comments/test docs reads a status that matches the delivered, green code.
- **Rationale**: contributor-facing (not the headline brand), three comments +
  test prose. Medium sharpness; cheap and self-contained.

### Slice US-03 — the platform README and prism config match the single-metric reality

- **Tasks**: correct `README.md:184` (Prism row), `README.md:222` (cost-table
  compliance-dashboards line), and `apps/prism/playwright.config.ts:19,28-34,50`
  + the prism README `pnpm playwright` note (the vacuous browser-matrix gate).
- **Target outcome (KPI)**: an evaluator reading what the platform claims about
  Prism understands it is a single-metric PromQL chart explorer, not a unified
  dashboarding product, and does not believe a browser-matrix e2e gate is green.
- **Rationale**: the README row is the loudest brand surface (an evaluator reads
  it first), and the playwright config is the most misleading CI advertisement.
  Touches the most-read surface, so it carries a DESIGN flag (remove vs mark).

---

## Priority Rationale

Priority is by **sharpness of the lie × reader reach**, not by effort (all three
are comparable, low effort). Walking-skeleton tie-break does not apply (no
skeleton). Riskiest-assumption tie-break: the pulse inverted-volatility claim is
the riskiest to the *thesis* because it makes a durable capability look volatile —
the exact opposite of what an honesty project wants — so it leads.

| Priority | Slice | Target outcome | KPI | Rationale |
|----------|-------|----------------|-----|-----------|
| 1 | US-01 (pulse) | pulse docs match the durable JSON+WAL store | KPI-1 | Notable inverted-volatility + an undelivered columnar promise; both job directions in one crate. Sharpest, riskiest-to-thesis. |
| 2 | US-02 (gateway) | gateway comments + test prose match the green code | KPI-2 | Three stale/inaccurate comments over green code; contributor-facing; cheap and isolated. |
| 3 | US-03 (prism) | platform README + prism config match the single-metric reality | KPI-3 | Loudest brand surface (README) + most misleading CI advertisement; carries the remove-vs-mark DESIGN flag. |

Dependencies: none between slices (independent loci). All three trace to the
single Earned-Trust JTBD (N:1). No orphan stories.

---

## Scope Assessment: PASS — 3 stories, 3 loci (pulse / gateway / prism), estimated ~1 day

Elephant-Carpaccio gate (oversized if any 2+ signals):

- Stories: **3** (≤10). PASS.
- Bounded contexts / modules touched: **3** (pulse, kaleidoscope-gateway,
  prism+README) (≤3). PASS.
- Walking-skeleton integration points: **0** (no skeleton; doc-only). PASS.
- Estimated effort: **~1 day** total (prose/comment/config edits + guards) (≤2
  weeks). PASS.
- Independent user outcomes that could ship separately: 3, but they are one
  coherent honesty pass and each is already a thin, independently-shippable
  slice. No further split needed.

Verdict: **right-sized**. No split required. (If US-03's prism-e2e turned into
"build the playwright browser-matrix e2e", that would be a separate feature, not
a slice of this honesty pass — explicitly out of scope.)
