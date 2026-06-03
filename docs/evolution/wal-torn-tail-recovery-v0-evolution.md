# Evolution archive — wal-torn-tail-recovery-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

First entry in `docs/evolution/`. Convention established here:
one file per feature, named `<feature-id>-evolution.md`, with the
sections below. Later features add their own sibling file.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0059
  (`docs/product/architecture/adr-0059-earned-trust-wal-torn-tail-recovery.md`).

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| discuss | `0a4b59c` | recover intact WAL prefix past a torn tail |
| design | `7ac7d38` | ADR-0059, shared wal-recovery seam |
| devops | `e5a50e3` | slim wave, one new per-crate mutation job |
| distill | `20d934c` | 15 RED-ready acceptance scenarios |
| feat | `0eb6227` | wal-recovery shared leaf crate (generic no-dyn replay, 100% mutants, gate-5 job atomic) |
| feat | `87d9363` | lumen rewire (+ fixed three slice_08 binary-harness defects) |
| feat | `188c6c2` | ray rewire |
| feat | `1886d94` | cinder rewire + corrected the false recovery doc (AC-7) |
| feat | `7c4a5e2` | pulse rewire (FLAG-1 cardinality reseed preserved) |
| docs | `ab8c826` | narrative + slide closure |

## The problem, in Earned-Trust framing

Every file-backed store (lumen, ray, cinder, pulse) recovered its
write-ahead log by replaying it parse-or-die: `serde_json::from_str` on
each line, and the first parse failure mapped to a `PersistenceFailed`
variant that aborted `open`. A single torn final line, the residue a
`kill -9` leaves between `write_all(bytes)` and `write_all(b"\n")` on an
fsync-honest append path (ADR-0049), therefore bricked recovery of the
entire intact, acked prefix that preceded it. The store would not start.

This was a claim contradicted by code, the exact class of substrate lie
the project's Earned-Trust principle exists to forbid. cinder made the
lie explicit: its module doc at `crates/cinder/src/file_backed.rs:36-38`
and its `open` doc at `:104-106` claimed a truncated last line "is
detected and ignored" while the code returned `PersistenceFailed` and
refused to start.

The feature is the third foot of Earned-Trust, ADR-0059 extending the
ADR-0040 (WAL plus snapshot plus replay recovery discipline), ADR-0049
(write path honours fsync) and ADR-0050 (read-side honest caps) lineage.
It closes the verifier's black-box issue 006.

## The architecture decision

### Shared crate, decided by the rule of three

Six stores carry the same replay loop: four in scope (lumen, ray,
cinder, pulse) and two out of scope but identical in shape (sluice,
strata). The four in-scope loops differ only in their record enum and
their error constructor. Replicating the new guard logic four times, and
two more times later, is exactly the copy-paste divergence ADR-0054
extracted `query-http-common` to end, and the latent-bug-via-copy that
ADR-0040 case B warned a recovery copy-paste produces.

Decision (ADR-0059 Decision 4): one new shared leaf crate
`crates/wal-recovery`, exposing a single generic free function
`replay_wal_tolerating_torn_tail`, generic over the record type
`R: DeserializeOwned` and the caller error `E`, monomorphised per pillar
with no `dyn Trait` indirection. Two closures, `apply` and
`on_parse_error`, are the seam that absorbs the per-pillar record payload
and error constructor. This is the Rust-idiomatic shape per CLAUDE.md:
data plus free functions plus generics where monomorphisation suffices,
no inheritance, no `dyn` where it is not needed. The three guard
conditions live at exactly one mutation site.

### Narrow tolerance with two negative guards

The newly tolerated shape is precisely one: a parse failure that is the
LAST line AND has no trailing newline. The function drops only that line,
recovers the intact prefix, and emits the warning. The value of the
tolerance is its narrowness, so it is pinned by two negative guards that
stay fail-closed:

- AC-5: any parse failure that is NOT the last line surfaces
  `PersistenceFailed`. Mid-file corruption is never swallowed.
- AC-6: a newline-terminated malformed final line surfaces
  `PersistenceFailed`. A complete-but-malformed record is not a tear.

A tolerance that swallowed either of these would be strictly worse than
the prior fail-closed behaviour. They are co-equal with the positive
path, not afterthoughts.

### The physical trailing-newline discriminator

The honest discriminator of a tear is the physical trailing byte:
a crash-torn record provably lacks the closing `\n` that the append path
writes last, and a complete malformed record provably has it. Detection
inspects the trailing byte (`ends_with_newline`) plus a line-index-vs-last
comparison. Inferring the tear from the `serde_json` error class was
considered and rejected (ADR-0059 alternative E) as fragile, because
`BufRead::lines()` strips the newline and hides whether the last line had
one. The micro-mechanism (read-whole-file vs one-line read-ahead vs
final-byte seek) was left to DELIVER within the three observable
constraints.

### The structured warning

One structured event on each tolerated drop:
`event="wal.recovery.torn_tail_dropped"`, `tracing::warn!`, at most once
per `open`, with fields `pillar` (one of `lumen`/`ray`/`cinder`/`pulse`),
`line` (1-based, matching the `idx+1` reason-text convention), and
`dropped_bytes` (the byte length of the torn line, excluding the absent
newline). It rides the existing structured tracing-to-stderr stream
established by `read-api-tracing-subscriber-v0` (the same stream that
carries `health.startup.refused` and `listener_bound`). No new metric,
dashboard or alert at v0; alerting on torn-tail frequency is operator
policy, not a feature requirement.

## The four-pillar rollout

All four in-scope pillars delegate to the shared function from their
`open` replay loop. The pillar-specific work survives unchanged around
the shared seam:

- lumen `FileBackedLogStore`: post-loop re-sort stays in the pillar.
- ray `FileBackedTraceStore`: the dual-index `apply_ingest` rebuild
  becomes the `apply` closure; `sort_all` stays in the pillar.
- cinder `FileBackedTieringStore`: `apply_to_entries` becomes `apply`.
- pulse `FileBackedMetricStore`: `apply_ingest` becomes `apply`, and the
  post-loop `tenant_counts` cardinality reseed (FLAG-1) reads the rebuilt
  series map, NOT the WAL, so dropping the torn tail is transparent to it
  (one fewer `apply_ingest`). The reseed is preserved exactly.

sluice and strata carry the identical loop shape but are out of scope for
this slice. sluice's `apply_record` is fallible, so its SHARE-LATER seam
needs a fallible-`apply` variant; this is recorded as a follow-up, not a
defect.

## The cinder doc correction (AC-7)

The false robustness claim in cinder was made true two ways: the code
now does what the doc said, and the prose at
`crates/cinder/src/file_backed.rs:36-38` and `:104-106` was corrected to
describe the actual behaviour. A project whose thesis is structural
honesty must not ship a doc claiming a robustness the code lacks. AC-7
was discharged behaviourally (the corrected doc's behaviour is proven by
cinder's reopen scenario plus its two negatives), not by a brittle
doc-string match test.

## Verification

- 15 acceptance scenarios across 5 crates, all real local I/O (Strategy
  C: real WAL files on a real tmp directory, real reopen, a real child
  process for the binary path, a real TCP query). The torn tail is seeded
  by writing real partial-JSON bytes with no trailing newline, the exact
  residue of a crash between the payload write and the newline write.
- The headline AC-1 plus AC-3 is driven through the COMPILED
  `log-query-api` binary launched as a child process with a real HTTP
  query, because the operator-visible behaviour is the binary recovering,
  binding and serving the intact prefix (the verifier D04 black-box
  path). The structured WARN is asserted by draining the child's stderr
  and parsing each line as JSON.
- `crates/wal-recovery`: 100% mutation kill (ADR-0005 Gate 5; CLAUDE.md
  per-feature 100%), via the new `gate-5-mutants-wal-recovery` CI job
  that landed in the SAME commit that created the crate (DEVOPS A1
  atomicity precedent, matching `gate-5-mutants-query-http-common`).
- Per-pillar call-site mutation: each pillar's existing
  `gate-5-mutants-<pillar>` `--in-diff` job mutates its own changed
  call-site lines automatically; those thin-call-site mutants are killed
  by the acceptance tests. The shared guard logic is mutation-killed once
  at the wal-recovery site (the ADR-0054 single-site benefit).
- pulse full-file mutation was deferred under the p95 probe; the CI
  `--in-diff` job covers the diff. (See the p95 wall-clock flake note in
  project memory; the diff-scoped mutation covers the feature's pulse
  change.)
- Gate 1 (cargo test --workspace) and Gate 4 (cargo deny, workspace-wide)
  auto-cover the new crate. Gate 2 (cargo public-api) and Gate 3 (cargo
  semver-checks) are opt-in package lists; wal-recovery is deliberately
  NOT enrolled (no surface-lock graduation requested). No trait
  signature changed; the four store traits stay byte-identical (AC-8).

## The honest finding

The recovery code was correct from the first commit (`0eb6227`). The
generic free function and its three guards passed their behavioural and
mutation tests as first written. The actual cost of the feature was three
test-harness defects in the lumen binary acceptance test
(`crates/log-query-api/tests/slice_08_torn_tail_recovery.rs`), fixed under
the lumen rewire commit (`87d9363`). The production recovery logic itself
needed no correction. This is recorded plainly because the value of the
evolution archive is its honesty about where the difficulty actually was:
in the binary-harness plumbing of the acceptance test, not in the routine
under test.

## Known follow-ups (open)

1. ADR-0059 Decision 8, layer b (the AST structural check). ADR-0059
   specifies a structural pre-commit check asserting that each in-scope
   pillar's `open` calls `wal_recovery::replay_wal_tolerating_torn_tail`
   and retains no inline `serde_json::from_str(&line) ... PersistenceFailed`
   replay loop. The ADR deliberately deferred the TOOL choice to DELIVER
   (`import-linter` was rejected as import-graph-only). The tool remains
   DEFERRED and the check remains UNWIRED. It is feedback, not a gate,
   consistent with the pure trunk-based, no-required-checks posture; when
   wired it belongs in the local pre-commit stage. Open.

2. Pre-existing cinder mutation gaps in unrelated methods. cinder carries
   surviving mutants in methods unrelated to this feature (`Debug`,
   `list_by_tier`, `evaluate_at`). These pre-date wal-torn-tail-recovery-v0
   and were neither introduced nor closed by it; this feature's `--in-diff`
   pulse and cinder jobs scope mutation to the diff, so these untouched
   methods were out of this feature's mutation scope. Recorded here as a
   pre-existing condition to address in a future cinder-scoped slice. Open.

Out of this slice and tracked elsewhere: sluice and strata SHARE-LATER
adoption of `wal-recovery` (sluice needs the fallible-`apply` variant).
