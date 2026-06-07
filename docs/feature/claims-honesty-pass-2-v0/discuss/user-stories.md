<!-- markdownlint-disable MD024 -->

# User Stories — claims-honesty-pass-2-v0

Sequel to `claims-honesty-pass-v0`. Same Earned-Trust job; corrects the residual
doc/comment/config overstatements the four-quadrants reports flagged that pass-v0
did not cover. The job every story below traces to (N:1):

> **JTBD**: "When I read what Kaleidoscope claims — a crate doc, a comment, the
> platform README, a CI config — it matches what the code actually does, so I
> neither over-trust (Prism = Datadog) nor under-trust (pulse loses my data on
> restart) the system."

Persona for all stories: **Devin Okafor**, a senior platform engineer at a
mid-size SaaS (Northwind Logistics) evaluating Kaleidoscope to replace a
five-figure Datadog bill. Devin reads a claim, then opens the code to verify it.
A claim that overstates the code makes Devin over-trust; a claim that understates
the code makes Devin under-trust. Either drift, on an honesty-thesis project,
collapses trust in the whole system.

## System Constraints (cross-cutting)

- **Correct-the-claim only.** Do NOT build the missing feature (the columnar
  pulse adapter, Prism dashboarding, the playwright e2e). Do NOT weaken any real
  behaviour (the durable stores, the real subscriber, the startup probe all
  stay). Inherited verbatim from `claims-honesty-pass-v0`.
- **The module-local already-honest doc is the canonical truth.** Each correction
  aligns the louder/wronger surface TO the quiet honest source: pulse →
  `file_backed.rs` (the real JSON+WAL durable adapter); gateway → the actual
  `init_tracing` / `Config::builder().build()` code; prism → `apps/prism/README.md`
  ("a single PromQL query panel").
- **Touch ONLY markers proven to sit over GREEN code.** Genuinely in-flight
  `#[ignore]`d / RED scaffolds (crash-durability proving tests, the gateway
  fixed-port AC-01 scenarios whose `#[ignore]` is about port-flake not the
  subscriber, the prism per-spec `UNIMPLEMENTED` e2e bodies) describe a TRUE
  current state and MUST NOT be touched. Item US-03 corrects only the false
  "Gate 7 … browser matrix" *advertisement*, not the per-spec scaffolds.
- **Solution-neutral.** Stories state the honest claim and the testable guard,
  not the edit mechanics. The one document-vs-implement-flavoured choice (prism-e2e
  remove-vs-mark) is DESIGN's; DISCUSS recommends MARK.
- **Acceptance shape** (for DISTILL): a prose-honesty correction is verified by a
  guard that the false string is ABSENT and the corrected claim is PRESENT
  (grep / doc-lint), AND the corrected claim is true of the cited code
  (cross-read). No behaviour test is needed because no behaviour changes.
- **Mutation: N/A.** Doc/comment/config changes add no mutable production-logic
  surface. Per CLAUDE.md mutation is per-feature on modified files; there is
  nothing to mutate. Gate 2/3 untouched (a doc-comment is not a public API; a
  `Cargo.toml` `description` is metadata).
- **Trunk-based, CI-is-feedback** (project memory): no CI gate blocks a doc-only
  change; the guard tests are the regression net.

---

## US-01: The pulse docs stop telling readers their metrics are volatile and stop promising an undelivered columnar engine

### Problem

Devin opens `pulse` to evaluate the metrics engine. The crate doc
(`pulse/src/lib.rs`) says, in its architectural-posture list, **"In-memory only
at v0; restart loses points."** (`:46`) and "Library only at v0. No daemon, no
network." (`:37`). But pulse SHIPS `FileBackedMetricStore` — a durable,
restart-surviving, fsync'd WAL+snapshot adapter, re-exported at `lib.rs:65` and
implemented in `file_backed.rs`. The doc tells Devin his data is volatile; the
code keeps it durably. For an honesty project, an *under*-claim is as damaging as
an over-claim: Devin discards a capability that genuinely ships. Worse, the same
crate doc (`:20-21,41`) and the `Cargo.toml` description (`:7`) promise the v1
adapter is **"columnar (Arrow + Parquet + DataFusion + Prometheus TSDB block)"** —
but the shipped adapter is line-delimited JSON over a WAL (`file_backed.rs`); the
crate's only deps are `serde`, `serde_json`, `wal-recovery`, `aegis`. The
durability half is real; the columnar half is not.

### Who

- Devin Okafor | senior platform engineer evaluating adoption | reads the pulse
  crate doc + Cargo.toml description, then opens `file_backed.rs` and the v1
  slice tests — and catches both the volatility under-claim and the columnar
  over-claim.

### Elevator Pitch

- **Before**: Reading `pulse`'s crate doc, Devin is told metrics are "in-memory
  only … restart loses points" and that the v1 adapter is a columnar
  Arrow/Parquet/DataFusion/TSDB engine — so Devin under-trusts the durability and
  over-trusts a columnar substrate that does not exist.
- **After**: Reading the same `pulse/src/lib.rs` rustdoc (and `cargo doc` /
  `Cargo.toml` description), Devin sees that a durable `FileBackedMetricStore`
  ships and survives process restart (fsync-durable), that "restart loses points"
  is scoped to the in-memory adapter, and that the actual durable adapter is a
  JSON-over-WAL store — with columnar named only as a future direction.
- **Decision enabled**: Devin counts pulse as a restart-surviving durable metrics
  store in the adoption assessment, and scopes any columnar expectation to the
  roadmap rather than to what ships today.

### Solution

Correct the pulse crate doc and Cargo.toml so the prose matches the shipped code:

1. State that a durable `FileBackedMetricStore` ships alongside
   `InMemoryMetricStore` and **survives process restart** (fsync-durable per the
   store-fsync-durability work); scope "restart loses points" to the in-memory
   adapter only.
2. Describe the actual shipped durable adapter as a JSON-over-WAL file-backed
   store; name the columnar substrate (Arrow/Parquet/DataFusion/TSDB) only as a
   **future/aspiration in future tense**, never as shipped. Reconcile the
   Cargo.toml "v0 ships … InMemoryMetricStore" to also name the durable adapter.

Aligned to `file_backed.rs` and the v1 slice tests — the already-truthful code.

### Domain Examples

#### 1: Happy Path — Devin reads the corrected durability posture

Devin reads `pulse/src/lib.rs` and sees "v0 ships the in-memory
`InMemoryMetricStore` AND a durable `FileBackedMetricStore` (JSON-over-WAL +
atomic snapshot) that survives process restart". Devin opens `file_backed.rs:75-82`
(`FileBackedMetricStore` with a WAL writer + fsync backend) and the
`v1_slice_01_wal_durability` / `v1_slice_06_snapshot_atomicity` tests, and the
claim matches. Trust preserved; the durability is no longer hidden.

#### 2: Edge Case — Devin checks the scoped volatility claim

Devin reads the corrected posture line and sees "restart loses points" now
applies explicitly to `InMemoryMetricStore` only. Devin confirms against
`store.rs` (`InMemoryMetricStore`, no persistence) and `file_backed.rs` (durable)
that the scoping is correct: the in-memory adapter is indeed volatile, the
file-backed one is not.

#### 3: Error/Boundary — Devin greps for "columnar" / "Arrow" / "Parquet"

Devin greps the pulse crate for `Arrow`, `Parquet`, `DataFusion`, `TSDB`. The
only hits are now future-tensed in the doc ("a columnar substrate is a future
direction"), and there is no such dependency in `Cargo.toml`. The over-claim is
gone; the future direction is honestly labelled, not deleted.

### UAT Scenarios (BDD)

#### Scenario: The pulse crate doc states the durable store survives restart

Given Devin is reading the `pulse/src/lib.rs` crate documentation
When Devin reads the architectural-posture / public-surface section
Then it states a durable `FileBackedMetricStore` ships and survives process restart
And the phrase "In-memory only at v0; restart loses points" is absent as an
unscoped crate-wide claim
And any "restart loses points" wording is scoped explicitly to `InMemoryMetricStore`
And the corrected durability claim is true of `file_backed.rs` (verifiable by reading it)

#### Scenario: The pulse docs do not promise an undelivered columnar substrate

Given Devin is reading the `pulse/src/lib.rs` crate doc and the `Cargo.toml` description
When Devin reads what the v1 / durable adapter is
Then it describes the actual JSON-over-WAL durable adapter
And it does not present a columnar (Arrow + Parquet + DataFusion + Prometheus
TSDB block) adapter as shipped
And any columnar substrate is named only in future tense as a future direction
And the absence of columnar code is verifiable by reading `file_backed.rs` and
the `Cargo.toml` dependency list

#### Scenario: The Cargo.toml description names the durable adapter, not only the in-memory one

Given Devin is reading `pulse/Cargo.toml`
When Devin reads the `description` field
Then it names both the in-memory adapter and the durable file-backed adapter as shipped
And it does not present a columnar substrate as a shipped v1 adapter

#### Scenario: The correction did not weaken any real behaviour

Given the pulse crate before and after the correction
When the pulse test suite runs
Then the durable store, its WAL durability, and its snapshot atomicity tests
still pass unchanged
And no production-logic line was altered (doc/comment/metadata only)

### Acceptance Criteria

- [ ] `pulse/src/lib.rs` states a durable `FileBackedMetricStore` ships and
  survives process restart; the unscoped "restart loses points" claim is absent;
  any residual "loses points" wording is scoped to `InMemoryMetricStore`. The
  claim matches `file_backed.rs`. (scenario 1)
- [ ] `pulse/src/lib.rs` + `pulse/Cargo.toml` do not present a columnar
  (Arrow/Parquet/DataFusion/TSDB) adapter as shipped; columnar appears only in
  future tense; matches the absence of such code/deps. (scenario 2)
- [ ] `pulse/Cargo.toml` `description` names the durable file-backed adapter, not
  only the in-memory one, and does not claim a shipped columnar substrate.
  (scenario 3)
- [ ] No production-logic line changed; the pulse durability + snapshot tests
  still pass. (scenario 4)

### Outcome KPIs

- **Who**: evaluators / integrators reading the pulse crate docs and Cargo.toml.
- **Does what**: read a durability+substrate posture that matches the shipped
  `FileBackedMetricStore` — neither under-stating durability nor over-stating
  columnar.
- **By how much**: 0 of the pulse doc surfaces still claim crate-wide volatility
  or a shipped columnar substrate; was 2 inverted/over-stated claims (volatility
  + columnar) across `lib.rs` + `Cargo.toml`.
- **Measured by**: grep/doc-lint guard asserting the false phrases are ABSENT and
  the corrected phrases PRESENT, cross-read against `file_backed.rs` + the dep
  list; pulse suite stays green.
- **Baseline**: 1 inverted-volatility claim (`lib.rs:46`) + 1 columnar over-claim
  (`lib.rs:20-21,41` + `Cargo.toml:7`).

### Technical Notes

- Doc-comment + Cargo.toml `description` only; no production code change. The
  `Cargo.toml` description change is package metadata, not a public-API change
  (Gate 2/3 untouched). Mutation N/A.
- Constraint: do NOT build the columnar adapter (out of scope); name it as future
  only. Do NOT weaken the durable store. Depends on nothing; pulse is delivered.

---

## US-02: The gateway comments and test prose match the delivered, green code

### Problem

Devin (here in contributor mode) reads `kaleidoscope-gateway/src/main.rs` to
understand the host binary. Two comments lie about the code two lines below them.
`main.rs:62-63` says `init_tracing`'s "body is a RED-ready NO-OP that Crafty
fills in DELIVER" — but `init_tracing` (`:153-173`) installs a real JSON-to-stderr
`tracing_subscriber` registry behind a `OnceLock` + `try_init`. And `main.rs:118-120`
(echoed in the module doc `:24-25`) says the gateway "Force[s] `sink.kind = stub`"
— but the next line is `Config::builder().build()?` (`:121`), which does NOT force
the kind; it relies on the builder's `Stub` default. The matching test module
(`tests/slice_01_tracing_subscriber.rs:42-51,206-208,280`) still describes a
"wired NO-OP", a scenario "RED against the no-op subscriber" — but the always-run
fail-closed tests assert the `health.startup.refused` JSON line IS present, i.e.
they are GREEN. A contributor reading these comments concludes the gateway's
observability is half-built and that it forces a config it does not.

### Who

- Devin Okafor | contributor reading the gateway source to understand or extend
  it | trusts the prominent comment over the code beneath it, and is misled about
  both the subscriber (thinks it's a no-op) and the config (thinks it's forced).

### Elevator Pitch

- **Before**: Reading `kaleidoscope-gateway/src/main.rs` and its tracing test
  module, Devin is told `init_tracing` is a RED-ready no-op, that the gateway
  forces `sink.kind = stub`, and that the tracing scenarios are RED against a
  no-op subscriber.
- **After**: Reading the same comments and test-module docs, Devin sees that
  `init_tracing` installs a real JSON-to-stderr subscriber as `main`'s first
  statement, that the gateway relies on the `Config::builder()` Stub default
  (aperture forwards Stub-kind sinks unchanged), and that the always-run
  fail-closed scenarios are GREEN.
- **Decision enabled**: Devin trusts the gateway's startup observability as
  delivered and reasons correctly about the composition (default-Stub, not
  forced) when extending it.

### Solution

Correct the three stale/inaccurate gateway comments and the test-module prose to
describe what the code actually does:

1. `main.rs:62-63` — state `init_tracing` installs the real JSON-to-stderr
   subscriber as `main`'s first statement (no "no-op / Crafty fills in DELIVER").
2. `main.rs:118-120` + module doc `:24-25` — state the gateway relies on the
   `Config::builder()` `Stub` default (aperture forwards Stub-kind sinks
   unchanged), not that it "forces" the kind.
3. `tests/slice_01_tracing_subscriber.rs:42-51,206-208,280` — describe the GREEN
   reality (subscriber installed, refusal event renders, always-run scenarios
   pass). Leave the `#[ignore]` attributes on the fixed-port AC-01 scenarios
   intact — their `#[ignore]` is about port-flake determinism, not about the
   subscriber being absent; only the "no-op / RED" wording is corrected.

### Domain Examples

#### 1: Happy Path — Devin reads the corrected init_tracing comment

Devin reads `main.rs:56-64` and sees the comment now states `init_tracing`
installs the real JSON-to-stderr subscriber as `main`'s first statement. Devin
reads the `init_tracing` body (`:153-173`) — a `tracing_subscriber::registry()`
with a JSON stderr layer behind a `OnceLock`/`try_init` — and the comment matches.

#### 2: Edge Case — Devin reads the corrected "stub" comment then the next line

Devin reads `main.rs:118-120` (now: "relies on the `Config::builder()` Stub
default; aperture forwards Stub-kind sinks unchanged") and the next line
`Config::builder().build()?` (`:121`). Comment and code agree: nothing is forced;
the default is relied upon. The module doc `:24-25` agrees.

#### 3: Error/Boundary — Devin runs the tracing suite after reading the corrected test docs

Devin reads the corrected test-module note (no "no-op / RED against the no-op
subscriber") and runs `cargo test -p kaleidoscope-gateway slice_01`. The
always-run fail-closed scenarios pass (GREEN), matching the corrected prose. The
`#[ignore]`d fixed-port AC-01 scenarios remain `#[ignore]`d for port-flake — and
the corrected note explains that reason, not a missing subscriber.

### UAT Scenarios (BDD)

#### Scenario: The init_tracing comment describes a real subscriber install

Given Devin is reading `kaleidoscope-gateway/src/main.rs` around `init_tracing`
When Devin reads the comment above the `init_tracing()` call
Then it states the subscriber is installed (the real JSON-to-stderr subscriber)
And it contains no "RED-ready NO-OP that Crafty fills in DELIVER" claim
And the comment matches the `init_tracing` body (verifiable by reading `main.rs:153-173`)

#### Scenario: The config comment describes relying on the Stub default, not forcing it

Given Devin is reading the `main.rs` config-construction comment and module doc
When Devin reads how `sink.kind` is set
Then it states the gateway relies on the `Config::builder()` `Stub` default
And it contains no claim that the gateway "forces" `sink.kind = stub`
And the comment matches the `Config::builder().build()` line beneath it

#### Scenario: The tracing test-module prose describes the green suite

Given Devin is reading `tests/slice_01_tracing_subscriber.rs` module + per-test docs
When Devin reads the status notes
Then they describe the installed subscriber and the always-run scenarios as green
And they contain no "wired NO-OP" / "RED against the no-op subscriber" claim
And the `#[ignore]` on the fixed-port AC-01 scenarios is left intact (its reason
is port-flake determinism, which the corrected note states)

#### Scenario: The correction touched only stale-over-green prose

Given the gateway crate before and after the correction
When the gateway test suite runs
Then the always-run tracing scenarios still pass and behaviour is unchanged
And no production-logic line and no `#[ignore]` attribute was altered

### Acceptance Criteria

- [ ] `main.rs:62-63` describes the real subscriber install; no "no-op / Crafty
  fills in DELIVER" claim; matches `init_tracing` (`:153-173`). (scenario 1)
- [ ] `main.rs:118-120` + module doc `:24-25` describe relying on the
  `Config::builder()` Stub default; no "force `sink.kind = stub`" claim; matches
  `Config::builder().build()` (`:121`). (scenario 2)
- [ ] `tests/slice_01_tracing_subscriber.rs` prose describes the green suite; no
  "wired NO-OP" / "RED against the no-op subscriber" claim; `#[ignore]` attrs
  unchanged. (scenario 3)
- [ ] No production-logic line or `#[ignore]` attribute changed; the gateway
  tracing suite still passes. (scenario 4)

### Outcome KPIs

- **Who**: contributors reading the `kaleidoscope-gateway` source comments + test
  docs.
- **Does what**: read comments/test prose that match the delivered, green code
  (real subscriber, default-Stub config, green always-run scenarios).
- **By how much**: 0 of the 3 gateway comment loci + the test-module prose still
  misstate the code; was 3 stale/inaccurate comment loci + 1 stale test-module
  prose block over green code.
- **Measured by**: grep guard asserting the false phrases are ABSENT and the
  corrected phrases PRESENT, cross-read against `init_tracing` /
  `Config::builder().build()`; gateway suite stays green.
- **Baseline**: 3 stale/inaccurate comment loci + 1 stale test-module prose block.

### Technical Notes

- Doc/comment only; no production code change, no `#[ignore]` change, no behaviour
  change. Mutation N/A. Gate 2/3 untouched.
- Guardrail: confirm the always-run tracing scenarios are GREEN before editing the
  test-module prose (they are: the fail-closed AC-02 tests assert the refusal
  event is present). Depends on nothing; the gateway is delivered.

---

## US-03: What the platform claims about Prism matches the single-metric reality, and no browser-matrix e2e gate is advertised that does not run

### Problem

Devin reads the platform `README.md` "Components at a glance" table first — it is
the brand. The **Prism** row (`:184`) says "Unified query and visualisation
frontend" and claims it replaces "Datadog dashboards, NR One, **Grafana**". But
`apps/prism/README.md:3-6` (the honest, module-local source of truth) says Prism
"v0 ships **a single PromQL query panel**" — a single-metric line-chart explorer
for the on-call "see-the-shape-of-the-signal" job. It is not a dashboarding
product. The cost table (`:222`) compounds it: "The **compliance dashboards in
Prism** are open templates" — Prism has no compliance dashboards at all. Separately,
`apps/prism/playwright.config.ts:19` advertises "Gate 7 (Prism E2E across the
browser matrix)" with Chrome/Firefox/Safari projects and a pinned Prometheus
digest — but `testMatch` matches no spec (`:50`) and every e2e spec throws
`UNIMPLEMENTED`. The infrastructure implies a passing browser-matrix e2e gate
that does not exist. (pass-v0 corrected the Spark/Strata/Cinder/Loom rows but left
the Prism row; the module-local README is honest.)

### Who

- Devin Okafor | evaluator reading the README table first, then spot-checking the
  module README and the CI config | over-trusts Prism as a Grafana-class
  dashboarding product, and believes a green browser-matrix e2e gate exists.

### Elevator Pitch

- **Before**: The README tells Devin Prism is a "unified query and visualisation
  frontend" that replaces Grafana/Datadog dashboards (with "compliance dashboards
  … open templates"), and the playwright config advertises a "Gate 7 … browser
  matrix" e2e — none of which the v0 code delivers.
- **After**: Reading the same README row and cost table, Devin sees Prism as a
  single-metric PromQL query/chart explorer (matching `apps/prism/README.md`),
  with dashboards marked future and the non-existent "compliance dashboards" line
  removed/restated; and reading the playwright config, Devin sees the
  browser-matrix e2e gate clearly marked not-yet-implemented (no spec runs today).
- **Decision enabled**: Devin scopes the Prism evaluation to a single-metric
  explorer (not a dashboarding replacement) and does not count a browser-matrix
  e2e gate as a delivered quality signal.

### Solution

Align the loud platform surfaces TO the honest `apps/prism/README.md`:

1. `README.md:184` — correct the Prism row to describe a single-metric PromQL
   query/chart explorer; mark unified dashboards as future; qualify the "Replaces"
   cell so it does not imply present-tense Grafana/Datadog-dashboard parity.
2. `README.md:222` — drop or restate the "compliance dashboards in Prism" cost
   line so it asserts no Prism capability that does not exist.
3. `apps/prism/playwright.config.ts:19,28-34,50` + the prism README `pnpm
   playwright` note — mark the browser-matrix e2e gate as not-yet-implemented /
   scaffold (no spec runs today; specs land slice by slice). **DESIGN flag:**
   MARK (recommended) vs REMOVE — DISCUSS recommends MARK to keep the legitimate
   slice-by-slice roadmap and the digest-SSOT rule visible; either is honest.
   Do NOT build the playwright e2e (out of scope), and do NOT touch the per-spec
   `UNIMPLEMENTED` scaffolds (genuinely in-flight) — correct only the false "gate
   works" advertisement.

### Domain Examples

#### 1: Happy Path — Devin reads the corrected Prism row

Devin reads `README.md:184` and sees Prism described as a "single-metric PromQL
query/chart explorer (unified dashboards: future)". Devin opens
`apps/prism/README.md:3-6` ("a single PromQL query panel") and the two match.
Devin scopes the evaluation correctly.

#### 2: Edge Case — Devin reads the cost-table line and the e2e config

Devin reads the corrected cost line (`:222`) and finds it no longer claims
"compliance dashboards in Prism" (Prism has none). Devin opens
`apps/prism/playwright.config.ts` and sees the header now states the
browser-matrix e2e gate is not yet implemented (no spec runs), so the
Chrome/Firefox/Safari projects + digest are scaffold, not a live gate.

#### 3: Error/Boundary — Devin verifies the genuine scaffold was not touched

Devin greps `apps/prism/e2e/*.spec.ts` for `UNIMPLEMENTED` and finds the per-spec
bodies intact (genuinely in-flight). The correction touched only the false "Gate
7 … browser matrix" advertisement, not the honest slice-by-slice scaffold —
confirming no in-flight marker was made to lie.

### UAT Scenarios (BDD)

#### Scenario: The README Prism row matches the single-metric reality

Given Devin is reading the platform `README.md` "Components at a glance" table
When Devin reads the Prism row
Then it describes a single-metric PromQL query/chart explorer
And it contains no present-tense "unified query and visualisation frontend" claim
And unified dashboards are marked as a future capability
And the row is consistent with `apps/prism/README.md` ("a single PromQL query panel")

#### Scenario: The cost table does not claim a non-existent Prism feature

Given Devin is reading the README "How Kaleidoscope defeats the cost model" table
When Devin reads the Prism-related line
Then it does not assert that Prism has compliance dashboards
And any retained cost-model point about Prism is true of the single-panel reality

#### Scenario: The playwright config does not advertise a gate that does not run

Given Devin is reading `apps/prism/playwright.config.ts` and the prism README
`pnpm playwright` note
When Devin reads how the browser-matrix e2e gate is described
Then it is clearly marked not-yet-implemented (no spec runs today; specs land
slice by slice)
And it does not advertise a passing "Gate 7 — Prism E2E across the browser matrix"
And the `testMatch` reality (`__no-spec-matches-yet__`) is consistent with the
corrected description

#### Scenario: Genuinely in-flight scaffolds were not touched

Given the prism e2e per-spec `UNIMPLEMENTED` bodies and any `#[ignore]`d / RED markers
When the correction is applied
Then the per-spec scaffold bodies are unchanged
And only the false "gate works / browser matrix" advertisement was corrected

### Acceptance Criteria

- [ ] README Prism row (`:184`) describes a single-metric PromQL explorer;
  unified dashboards are future-tensed; no present-tense "unified … frontend"
  overclaim; consistent with `apps/prism/README.md`. (scenario 1)
- [ ] README cost line (`:222`) asserts no non-existent Prism compliance
  dashboards. (scenario 2)
- [ ] `apps/prism/playwright.config.ts` + the prism README `pnpm playwright` note
  mark the browser-matrix e2e gate as not-yet-implemented; no advertised passing
  "Gate 7 … browser matrix"; consistent with `testMatch`. (scenario 3)
- [ ] The per-spec e2e `UNIMPLEMENTED` scaffolds and any RED markers are
  unchanged. (scenario 4)

### Outcome KPIs

- **Who**: evaluators reading what the platform claims about Prism (README table,
  cost model) and the prism CI config.
- **Does what**: scope Prism as a single-metric PromQL explorer (not a Grafana-
  class dashboarding product) and do not count a non-existent browser-matrix e2e
  gate as a quality signal.
- **By how much**: 0 of the 3 prism overstatement loci (README row + cost line +
  e2e advertisement) still overstate; was 3 (1 README row + 1 cost line + 1
  vacuous e2e gate advertisement).
- **Measured by**: grep/doc-lint guard asserting the false phrases are ABSENT and
  the corrected phrases PRESENT, cross-read against `apps/prism/README.md` and the
  `testMatch` reality.
- **Baseline**: 1 overstated README Prism row + 1 overstated cost line + 1
  advertised-but-non-existent browser-matrix e2e gate.

### Technical Notes

- README + Cargo-adjacent config (playwright.config.ts) + prism README only; no
  application code change, no e2e built. Mutation N/A.
- **DESIGN flag — prism-e2e remove vs mark.** DISCUSS recommends MARK (annotate
  as scaffold; keep the slice-by-slice roadmap + digest-SSOT rule) over REMOVE
  (delete the advertisement). Either satisfies the job; MARK is the lower-cost,
  roadmap-preserving honest minimum. Building the playwright browser-matrix e2e is
  a separate feature, out of scope.
- Guardrail: do NOT touch the per-spec `UNIMPLEMENTED` e2e scaffolds (genuinely
  in-flight). Depends on nothing; aligns to the already-honest `apps/prism/README.md`.
