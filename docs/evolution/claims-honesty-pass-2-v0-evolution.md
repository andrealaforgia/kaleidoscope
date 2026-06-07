# Evolution archive — claims-honesty-pass-2-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md`,
`store-fsync-durability-v0-evolution.md`,
`tls-config-reject-v0-evolution.md`,
`claims-honesty-pass-v0-evolution.md`,
`beacon-sighup-reload-v0-evolution.md`,
`cli-ingest-atomic-v0-evolution.md`,
`cinder-wal-error-surfacing-v0-evolution.md`,
`aperture-serve-loop-error-surfacing-v0-evolution.md`,
`beacon-slo-operator-path-v0-evolution.md`,
`aegis-ingest-auth-v0-evolution.md`,
`spark-ingest-auth-v0-evolution.md`,
`perf-kpi-ci-non-gating-v0-evolution.md`,
`aperture-presubscriber-probe-stderr-v0-evolution.md` and
`speed-up-local-precommit-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below. This is a documentation-correction feature (doc
comments, a `Cargo.toml` description, the platform README, a prism README,
a playwright config, plus ONE structural guard test, no production-logic
source), so the record is deliberately proportionate to that scope.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: none. Like `claims-honesty-pass-v0`'s non-ADR loci, every
  correction here aligns prose to already-honest code; nothing reached the
  bar of an architecturally significant decision. The pass-v0 precedent
  (an ADR only for a real scope statement, DOCUMENT for everything else)
  was mirrored exactly, and no locus in this pass needed a scope statement.
- Closes: the residual Q2 "doc says X, code does not-X" overstatements
  that the four-quadrants reports flagged and that `claims-honesty-pass-v0`
  did NOT cover. pass-v0 corrected the README Components rows for
  Spark/Strata/Cinder/Loom and a cluster of stale-over-green crate/test
  headers; it did not touch the Prism row, the pulse crate doc, the gateway
  comments, or the prism playwright config. This is pass-2.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| discuss | `f919c59` | correct the residual doc/comment overstatements |
| design | `dc8995a` | correct-in-place, MARK the prism e2e, no ADR |
| devops | `5bdd036` | existing CI covers it, no semver, mutation N/A |
| distill | `926bf56` | both-directions structural guard over the overstatements |
| deliver | `80f8949` | correct 9 overstatement loci to match the code; un-ignore the 5 structural RED scenarios |
| docs | `0c340e5` | narrative + slide closure |

The DISCUSS, DESIGN, DEVOPS and DISTILL artefacts landed on `main` ahead
of DELIVER, each from its own wave agent; the as-built facts below are
read from the DELIVER commit `80f8949`.

## The problem, in Earned-Trust framing

Earned-Trust turned on the project's own words a SECOND time. pass-v0
proved the discipline once: the four-quadrants stale-prose family was not
a code problem but a prose problem, the lie having migrated from the code
to the outer surfaces. A data-driven re-scan of the four-quadrants reports
on 2026-06-07, re-grounded against the live code at HEAD, found residual
Q2 overstatements pass-v0's sweep had not reached. The code is truth; each
named claim was read against the exact source that makes its corrected
form true.

The residue came in two directions, and the second direction is the
finding worth recording.

The first direction is the familiar over-claim, prose advertising a
capability the code does not have. The prism platform-README row called
prism a "unified visualisation frontend" with Grafana-parity dashboards
(a HIGH overstatement) when the code is a single-metric PromQL
query/chart explorer; and the prism playwright config advertised a Gate 7
"browser-matrix e2e" gate vacuously, naming a gate that runs no spec
today. The gateway carried stale comments of the same class: a "RED-ready
NO-OP that Crafty fills in DELIVER" beside an `init_tracing` that already
installs the real JSON-to-stderr subscriber, and a "force `sink.kind =
stub`" comment beside code that relies on the `Config::builder()` Stub
default and forces nothing.

The second direction is the inverse drift, and it is the one that makes
this pass more than a repeat. pulse's crate doc said "in-memory only at
v0; restart loses points." That sentence was true when it was written,
before the durability work. The durability lineage (`store-fsync-
durability-v0` and the fsync probe) shipped `FileBackedMetricStore`, a
JSON-over-WAL adapter with atomic snapshot and per-record fsync that
SURVIVES process restart. So the doc UNDERSOLD the code: it told users
their data was volatile when the code had since made it durable. An
under-trust lie is still a substrate lie. A reader who believes the
volatility note throws away durable data, or builds a redundant external
store, on the strength of prose the code has outgrown.

This is the same class of lie the Earned-Trust principle exists to forbid,
turned inward on the project's own claims a second time, and across both
the over-trust and under-trust axes at once.

## The decision

The decision is inherited verbatim from pass-v0: correct the CLAIM to
match the CODE, do not build the missing feature, do not weaken any real
behaviour. No behaviour change, no feature-building. No new ADR (mirrors
pass-v0; no locus reached architectural significance). Two sub-decisions
shaped the pass.

- The prism-e2e gate was MARKed, not REMOVEd. The playwright config and
  prism README that advertised the browser-matrix e2e were annotated as
  NOT YET IMPLEMENTED / scaffold, rather than deleted. The honest move for
  a not-yet-built gate is to say it is not built, while preserving the
  apparatus that lets it be built later: the `PROMETHEUS_IMAGE_DIGEST`
  constant (the SSOT pin) and the slice-by-slice re-add roadmap both
  survive. MARK keeps the truth and the intent; REMOVE would have thrown
  away the roadmap and made the next slice re-derive it.
- The guard is both-directions. Because the residue ran both ways
  (over-claim AND under-claim), a one-sided guard that only checked false
  phrases were ABSENT would let an over-correction through: deleting a
  claim entirely, or swinging it into a new lie in the opposite direction,
  would pass a false-absent-only test. So the structural guard asserts BOTH
  that the false phrase is ABSENT and that the true phrase is PRESENT. This
  is the methodological advance over pass-v0, which corrected in place
  without a structural guard. pass-v0 trusted the diff; pass-2 leaves a
  test that reddens if either drift direction reappears.

## The as-built shape

Nine overstatement loci corrected, collapsing into the diff
(`README.md`, `apps/prism/README.md`, `apps/prism/playwright.config.ts`,
`crates/kaleidoscope-gateway/src/main.rs`,
`crates/kaleidoscope-gateway/tests/slice_01_tracing_subscriber.rs`,
`crates/pulse/Cargo.toml`, `crates/pulse/src/lib.rs`, and the structural
test that un-ignored its 5 RED scenarios).

- pulse `src/lib.rs`: the unscoped crate-wide "In-memory only at v0;
  restart loses points." is dropped. The doc now names the durable
  `FileBackedMetricStore` (JSON-over-WAL plus atomic snapshot,
  fsync-durable) that SURVIVES process restart, and scopes volatility to
  `InMemoryMetricStore` where it is genuinely true. The columnar story
  (Arrow / Parquet / DataFusion / TSDB) is reframed from present-tense
  shipped to a genuine FUTURE direction, because the crate's deps carry no
  Arrow/Parquet/DataFusion/TSDB crate.
- pulse `Cargo.toml` description: names the durable file-backed adapter as
  shipped, and columnar as a future direction (was "lands at v1").
- gateway `src/main.rs`, two comments: "RED-ready NO-OP that Crafty fills
  in DELIVER" becomes the truth that `init_tracing` installs the real
  JSON-to-stderr subscriber; "force / forces `sink.kind = stub`" becomes
  the truth that the code relies on the `Config::builder()` Stub default
  and does not force.
- gateway `tests/slice_01_tracing_subscriber.rs`: PROSE ONLY. "wired
  NO-OP" / "RED against the no-op subscriber" becomes the GREEN reality
  (the subscriber is installed; AC-02 asserts the JSON line IS present and
  passes). The fixed-port AC-01 `#[ignore]` attributes are UNTOUCHED, kept
  for port-flake determinism. The correction changes what the test prose
  SAYS, not what the test DOES.
- README.md: the Prism row "Unified query and visualisation frontend"
  becomes "a single-metric PromQL query/chart explorer"; the cost line
  ("compliance dashboards in Prism") is restated to keep the true
  no-upsell economics without inventing a feature.
- `apps/prism/playwright.config.ts` and `apps/prism/README.md`: the Gate 7
  browser-matrix e2e is MARKed NOT YET IMPLEMENTED / scaffold (no spec
  runs today). The config is NOT removed: the digest SSOT and the
  slice-by-slice re-add roadmap are preserved.

Columnar is kept FUTURE-tense throughout, not deleted, because it is a
genuine direction the crate may take; the honest form is "future", not
"gone". No semver bump: doc comments plus a `Cargo.toml` description are
metadata, not API; pulse, gateway and prism are not in the Gate 2 /
Gate 3 public-API surface; all crates stay at 0.1.0 (CLAUDE.md;
`semver_one_zero_is_andreas_call`).

## The proof and its boundary

- The acceptance is STRUCTURAL, in
  `crates/integration-suite/tests/v0_claims_honesty_pass_2_structure.rs`:
  8 scenarios, all green, 0 ignored after DELIVER. Five are the
  both-directions correction scenarios (each asserting the false phrase
  ABSENT and the true phrase PRESENT, cross-read against the cited code):
  the pulse durable-survives-restart correction, the pulse durable-adapter
  plus future-tense-columnar correction (covering both `lib.rs` and the
  `Cargo.toml` description), the gateway comments-and-test-prose
  correction, the README Prism-row-and-cost-line correction, and the
  prism-e2e marked-scaffold correction. Three are GREEN controls that pass
  before and after: the prism module README stays the single-metric honest
  SSOT the platform README is aligned TO; the gateway fixed-port AC-01
  `#[ignore]` attributes are NOT removed (prose-only edit guard); and the
  prism-e2e digest SSOT plus re-add roadmap are preserved (MARK, not
  REMOVE, guard).
- RED before, green after: the false phrases were verified PRESENT at HEAD
  on 2026-06-07 (`In-memory only at v0` in `pulse/src/lib.rs`,
  `RED-ready NO-OP` in `gateway/src/main.rs`, `no-op subscriber` in the
  gateway test prose, `Unified query and visualisation frontend` in the
  README, the unqualified Gate 7 line in the playwright config). DISTILL
  authored the 5 scenarios `#[ignore]`d (RED-ready); DELIVER un-ignored
  them as each matching surface was corrected.
- Build and lints clean: `cargo build --workspace` ok; `cargo test
  --workspace --lib` 26/26 ok; `cargo fmt --all` clean; `cargo clippy
  --workspace --all-targets -D warnings` clean; `cargo deny` ok. The
  gateway slice_01 `#[ignore]` attributes are confirmed intact.
- Mutation (ADR-0005 Gate 5): N/A. The diff is doc comments, a `Cargo.toml`
  description, config, README prose and test PROSE plus the structural
  guard. There is no production-logic surface to mutate, so there is no
  mutant to kill; mutation is correctly silent rather than vacuously
  green, exactly as on pass-v0's pure-prose slices and on the ADR-0070 /
  ADR-0072 structural-only features.
- SemVer (Gate 2 / Gate 3): none. No crate version change; never 1.0.0.
- DEVOPS confirmed the standing five-gate pipeline absorbs the work with
  no new CI job and no edit to any existing job: the one net-new artefact,
  the structural guard test, lands in the `integration-suite` `tests/`
  directory and runs inside the existing CI gate-1.

## The lesson

Honesty is not a one-time pass. It decays as the code changes. The
durability work turned pulse's once-true "in-memory only at v0" into a lie
without anyone editing that line: the code moved underneath the prose, and
the prose did not move with it. The same drift that pass-v0 chased in the
over-trust direction reappeared, this time including the under-trust
direction, because the work that made the code MORE honest left a doc that
was now LESS honest about it. A prose-honesty pass that corrects in place
and trusts the diff (pass-v0) fixes the lie of the day; it does not stop
the next one. The durable defence is a structural test that reddens the
moment a doc and its code part ways, in EITHER direction. That
both-directions guard is the methodological advance of this pass: pass-v0
restored the truth, pass-2 left a tripwire that fires when truth slips
again, whether the prose oversells the code or undersells it.

## Note for the operator

This feature adds no deployment precondition and changes no runtime
behaviour. The diff is documentation, metadata, config and test prose plus
one structural guard test. Its only consequences are at the reader / CI
boundary: the pulse rustdoc, the platform README, the prism README and the
playwright config now describe what the code does, and a new structural
test in `integration-suite` reds if any corrected claim drifts back out of
truth (in either direction). No crate version moved; no gate was added or
edited; the existing CI gate-1 runs the new guard.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them. The residual Q2 doc-overstatement
cluster pass-v0 left untouched (the Prism row, the pulse crate doc, the
gateway comments, the prism playwright config) is CLOSED by this feature.

1. faster-test-fsync-backend-v0. The fsync-bound durability bins remain
   I/O-bound in CI, paying the honest per-record `sync_all` of
   ADR-0049/0060. A future feature could speed them with a faster
   test-fsync backend or a batched-fsync test mode behind an env guard.
   Open.

2. read-path auth (the next aegis wire). The query / log-query /
   trace-query read APIs are still unauthenticated; aperture-storage-sink
   reaches through `.inner` and read-path tenant authority is deferred.
   Open.

3. ingest role-gating. ingest auth is authentication-only: any valid
   catalogued token may ingest. Rejecting a valid `viewer` on the write
   path is the deferred authorization decision; the `TenantContext.role`
   is already threaded, so the follow-up is one
   `if ctx.role != Operator { reject }` gate with no re-plumbing. Open.

4. aegis "JWKS"-vs-HS256 doc-fix. `aegis/src/lib.rs` overstates "JWKS";
   the validator is HS256 pre-shared-key only. The same doc-honesty class
   as this feature, not covered by either pass; disposition: a `docs:`
   fix-forward or a trivial micro-wave. Open.

5. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap needs its own slice. Open.

6. sluice wiring. sluice remains UNWIRED: no gateway/server `src` path
   constructs or drives `FileBackedQueue`. The wiring is a separate,
   still-open slice. Open.

7. sluice torn-tail migration. sluice still carries the inline
   parse-or-die recovery loop; its migration to the shared
   `replay_wal_tolerating_torn_tail` routine is the tracked ADR-0059 §5
   follow-up. Open.

8. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
   doubles the store, because lumen has no idempotency key. The designed
   extraction (ADR-0064 DD-3): success-case dedup earns its own slice.
   Open.

9. ingest-bounded-memory. The buffer-all-then-flush design (ADR-0064)
   holds the whole input's records in RAM before commit. A future feature
   lifts it with a temp-WAL staging stage or a max-records streaming cap.
   Open.

10. ADR-0059 Decision 8 layer b, the AST structural check, remains
    UNWIRED. The structural pre-commit check asserting in-scope stores
    delegate to the shared wal-recovery routine and carry no `let _ =`
    swallow; the tool choice was deferred and remains deferred. It is
    feedback, not a gate, consistent with the pure trunk-based,
    no-required-checks posture; when wired it belongs in the local
    pre-commit stage (now the fast `--lib` stage). Open.

11. OTLP partial_success never populated. The OTLP `partial_success`
    response field is never populated, so partial-accept signalling is not
    surfaced to clients. Open.

12. prism dashboarding and the prism browser-matrix e2e. The Prism row was
    corrected to its single-metric reality and the Gate 7 e2e was MARKed
    scaffold with its digest SSOT and re-add roadmap preserved. Building
    the unified-visualisation dashboarding and standing up the
    browser-matrix e2e specs are each future features that would retire
    their respective scaffold marks. Open only if wanted.

13. pulse columnar adapter. The Arrow / Parquet / DataFusion / TSDB
    columnar story was reframed FUTURE-tense rather than built. A future
    feature that ships it behind the existing `MetricStore` trait would
    move the columnar claim from future to present. Open only if wanted.

14. The two claims-honesty-pass-v0 DOCUMENT items remain future features
    if wanted. The actual Prometheus-stepped grid for `query_range` (a
    query-api feature) and real gRPC-prefix honouring for `harness`
    (`Framing::GrpcProtobuf`) were documented as v0 reality rather than
    built; each would retire its respective pin. Open only if wanted.

15. beacon non-30d error budget periods. v0 supports ONLY a 30d error
    budget period. Other windows (7d, 90d) would each need their own
    `MWMBR_TABLE` row set and earn their own slice. Open only if wanted.
