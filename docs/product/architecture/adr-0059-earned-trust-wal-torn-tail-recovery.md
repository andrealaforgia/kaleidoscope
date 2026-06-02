# ADR-0059 — Earned-Trust on the read-back: recover the intact acked prefix past a torn WAL tail

- **Status**: Accepted
- **Date**: 2026-06-02
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `wal-torn-tail-recovery-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0040 (the WAL + snapshot + replay recovery discipline
  whose `open` replay loop this ADR refines; the contract this feature
  hardens; cited, NOT modified). ADR-0049 (the immediately prior
  Earned-Trust sibling on the WRITE side: probe-must-honour-fsync; the
  per-record `sync_all` it added is precisely what makes the torn final
  line the EXACT residue this ADR recovers from; cited as the immediate
  precedent this ADR is the read-back mirror of, NOT modified). ADR-0050
  (Earned-Trust on the read side: per-request caps; the same principle
  applied to the query boundary; cited as lineage, NOT modified).
  ADR-0054 (the `query-http-common` extraction; the rule-of-three
  shared-crate precedent that governs FLAG 4 below; cited, NOT modified).
  ADR-0005 (the five CI gates, including Gate 5 100% mutation kill on
  modified files; Gate 2 `cargo public-api` byte identity on store trait
  signatures).

## Context

Six file-backed storage pillars recover their state on `open` by loading
an optional snapshot and then replaying an append-only NDJSON write-ahead
log: one `serde_json`-serialised record per line, newline-terminated. The
replay loop is **parse-or-die**. The FIRST line that fails to parse maps
to a `PersistenceFailed` variant and aborts the whole `open`. The shape
is verified IDENTICAL across all six:

- `crates/lumen/src/file_backed.rs:107-121` (`FileBackedLogStore::open`, `LogStoreError`)
- `crates/ray/src/file_backed.rs:120-135` (`FileBackedTraceStore::open`, `TraceStoreError`)
- `crates/cinder/src/file_backed.rs:135-163` (`FileBackedTieringStore::open`, `MigrateError`)
- `crates/pulse/src/file_backed.rs:164-187` (`FileBackedMetricStore::open`, `MetricStoreError`)
- `crates/sluice/src/file_backed.rs:167-180` (`EnqueueError`) — same shape, NOT in this slice
- `crates/strata/src/file_backed.rs:114-122` (`ProfileStoreError`) — same shape, NOT in this slice

Every loop is the same five lines: `reader.lines().enumerate()`, skip
`line.is_empty()`, `serde_json::from_str(&line)`, map the first parse
error to `PersistenceFailed { reason: "WAL parse error at line N: ..." }`,
apply the record. The pillars differ only in their error type and their
`WalRecord` enum.

ADR-0049 closed the write side: every WAL append now calls `sync_all`
after the buffered flush, so the bytes are crash-durable. The residue a
fsync-honest, append-only WAL leaves after a `kill -9`, an OOM kill, a
power loss, or an unclean container stop is a **torn final line**: a
partial record with no trailing newline, because the append path writes
the record bytes, then the newline, then fsyncs, and an interruption
between any of those steps leaves a tail that does not parse. That torn
tail is the EXPECTED post-crash shape that ADR-0040's "survives a
restart" durability promise exists to handle.

Today a single torn trailing line blocks recovery of the entire intact,
acked, durable prefix that precedes it. This is fail-closed and therefore
SAFE — the store never serves partial or corrupt data — but it
**contradicts the durability promise**: an operator whose collector was
killed cannot bring it back up, even though every acked record up to the
crash is intact on disk. It is an Earned-Trust gap in the same family
ADR-0040 / 0049 / 0050 address: the write side became crash-honest under
ADR-0049, but the matching READ-BACK refuses the very shape a
crash-honest write produces. **ADR-0049 made writes crash-honest; this
ADR makes the read-back of a crash-honest write recover the durable
prefix instead of refusing it.**

Worse, cinder's module doc at `crates/cinder/src/file_backed.rs:36-38`
and its `open` doc at lines 104-106 already CLAIM that a truncated last
WAL line "is detected and ignored" and that all other parse errors are
surfaced. The code does the OPPOSITE: it surfaces the truncated last line
as `PersistenceFailed` just like every other pillar. A false robustness
claim shipped in a project whose whole thesis is structural honesty
against vendor overstatement. This ADR makes the code true and corrects
the doc.

ADRs in this repository are immutable (superseded, never edited).
ADR-0040 / 0049 / 0050 / 0054 are Accepted and referenced as precedents;
they are NOT modified. ADR-0059 is the next free number (the highest
existing was 0058, verified by `ls docs/product/architecture/adr-*.md`).

## Decision

### 1. Refined recovery contract: tolerate the torn final line ONLY, recover the intact prefix, warn

The WAL replay loop on `open` gains exactly one new tolerated shape and
NO others. When `serde_json::from_str` fails on a line, the loop drops
that line and finishes recovery with the prefix already accumulated **iff
all three conditions hold**:

1. **is-last-line** — the failing line is the final line of the WAL (no
   further bytes follow it),
2. **no-trailing-newline** — the WAL byte stream does NOT end in `\n` (the
   record was torn mid-write, not a complete record that happens to be
   malformed), and
3. **parse-failed** — the line is the parse failure under consideration.

If all three hold, the loop DROPS that one torn record, finishes recovery
with the intact prefix, emits the structured WARN event of Decision 3,
and returns the recovered store. **In every other case the loop returns
the existing `PersistenceFailed` error exactly as today**: a parse failure
that is not the last line (mid-file corruption), OR a final line that DOES
end in a trailing newline (a complete-but-malformed record), stays
fail-closed. This is a **narrowing** of fail-closed, not an abandonment of
it. The value of the tolerance depends entirely on it being narrow: a
tolerance that swallowed mid-file corruption would be strictly worse than
today's behaviour.

The torn tail is **dropped, never repaired and never partially decoded**.
The recovered state is byte-equivalent to what would have been recovered
had the torn record never been appended.

No write path, no snapshot path, no trait signature changes.
`LogStore`, `TraceStore`, `TieringStore`, `MetricStore` signatures stay
byte-identical (Gate 2 `cargo public-api` enforces this; AC-8).

### 2. Detection mechanism: trailing-byte inspection + index-vs-last comparison (FLAG 2)

`BufRead::lines()` strips the newline and does NOT reveal whether the
final line had one, so the loop cannot distinguish a torn tail from a
newline-terminated malformed final line by iterating `lines()` alone. The
mechanism is the cheapest correct one that inspects the ACTUAL trailing
byte:

- Before the replay loop, read the final byte of the WAL file (or, if the
  bytes are already in hand, inspect the last byte of the read buffer) to
  compute a single boolean `ends_with_newline`. A WAL that ends in `\n`
  has a complete final record; a WAL that does NOT end in `\n` has a torn
  final record candidate.
- The loop already enumerates lines with their zero-based index. The
  total line count is knowable (either by reading the file into lines
  up-front, or by a one-line read-ahead that defers application of line
  *i* until line *i+1* is known to exist or not). The failing line is the
  last line iff its index is the final index.
- The torn-tail tolerance fires **only** when the parse failure is on the
  final line AND `ends_with_newline` is `false`. A parse failure on any
  earlier line refuses regardless of `ends_with_newline` (condition 1
  fails). A parse failure on the final line refuses when
  `ends_with_newline` is `true` (condition 2 fails: a complete record that
  is malformed is real corruption, not a tear).

The precise in-loop realisation (read the whole file then iterate a
counted slice, versus a one-line read-ahead buffer, versus a final-byte
`seek` + per-line index compare) is the **crafter's implementation
choice** in DELIVER, constrained to honour the three observable
conditions and the two negative ACs (AC-5, AC-6). The DESIGN wave pins
the OBSERVABLE behaviour and the `ends_with_newline` discriminator; it
does NOT pin the byte-reading micro-mechanism. The empty-line skip
(`if line.is_empty() { continue; }`) already present in all loops is
orthogonal and unchanged; an empty final line with no trailing newline is
not a parse failure and never reaches the tolerance arm.

**Why the trailing byte and not "the parser hit EOF".** The cinder code's
existing comment (lines 146-160) reasons that it "can't know last until
we have read ahead" and then refuses anyway. The honest discriminator is
the physical byte on disk: a crash-torn record provably lacks the closing
`\n` that the append path writes last; a complete-but-malformed record
provably has it. Reading the trailing byte makes the substrate PROVE the
tear rather than inferring it from parser state, in the same spirit as
ADR-0049's "make the substrate prove fsync" over "read its self-claims".

### 3. Structured warning: `event="wal.recovery.torn_tail_dropped"` (FLAG 3)

Dropping a torn tail emits exactly one `tracing` WARN event, riding the
existing structured `event = "..."` convention
(`event="health.startup.refused"`, `event="listener_bound"` at the read
tier and gateway; confirmed across the binaries). The event name and
field set are pinned here:

```
tracing::warn!(
    event = "wal.recovery.torn_tail_dropped",
    pillar = "lumen",        // "lumen" | "ray" | "cinder" | "pulse"
    line = 10001_u64,         // 1-based line number of the dropped tail
    dropped_bytes = 58_u64,   // byte length of the dropped torn line (excludes the absent newline)
);
```

- **event** — `"wal.recovery.torn_tail_dropped"`, dotted style mirroring
  `health.startup.refused`. Confirmed from FLAG 3's proposal; no change.
- **pillar** — a `&'static str` pillar identity, one of `lumen`, `ray`,
  `cinder`, `pulse`. NOT the crate's full name, NOT a struct path; the
  short operator-facing pillar word the rest of the platform uses.
- **line** — the 1-based line number of the dropped tail (matching the
  `idx + 1` convention already used in every `PersistenceFailed` reason
  text, so an operator reads the same numbering in both the warning and
  the refusal).
- **dropped_bytes** — the byte length of the dropped torn line as read
  from the WAL, excluding the absent trailing newline (there is none to
  count). This lets an operator confirm the drop was a small tail, not a
  large swathe.

The four facts AC-3 mandates (event name, pillar, line, dropped bytes)
are all present. No new metric, no new dashboard, no new event name
beyond this one WARN. The event is emitted at most once per `open` (a
clean crash leaves at most one torn tail; a second parse failure would be
mid-file and would refuse before reaching the tail). The WARN level is
correct: recovery SUCCEEDED, but the operator must be told something was
dropped so they can confirm it was the benign post-crash tear and not a
silent gap.

### 4. Shared recovery routine in a new `wal-recovery` crate, not per-pillar copy-paste (FLAG 4)

The torn-tail recovery logic is factored into **one shared free function
in a new leaf crate `crates/wal-recovery`**, generic over the per-pillar
record type and parameterised by a small closure seam, rather than
replicated into each pillar's `open`. The Reuse Analysis (below, and in
the feature `wave-decisions.md`) is decisive: there are SIX stores with
the identical loop, of which four are in this slice and two
(`sluice`, `strata`) carry the same shape and will inevitably want the
same fix. Copy-pasting the three guard conditions, the trailing-byte
inspection, and the warning emission into four `open` paths now and two
later guarantees the divergence ADR-0054 was extracted to end and that
ADR-0040 case B explicitly warned a recovery copy-paste produces (a
latent ordering bug shipped because the copy drifted).

The crafter owns the exact signature in DELIVER; the DESIGN-pinned SEAM is:

```text
// crate: wal-recovery  (leaf, depends only on serde_json + tracing)
//
// Replays the WAL lines, applying each parsed record via `apply`,
// tolerating ONLY a torn final line (no trailing newline) by dropping it
// and emitting the structured WARN. Any other parse failure is returned
// through `on_parse_error`, which each pillar maps to its own
// PersistenceFailed variant. Generic over the record type R and the
// caller's error type E; no dyn, monomorphised per pillar.
pub fn replay_wal_tolerating_torn_tail<R, E>(
    wal_bytes: &[u8],            // or a Read + Seek; crafter's choice
    pillar: &'static str,        // "lumen" | "ray" | "cinder" | "pulse"
    mut apply: impl FnMut(R),    // applies a parsed record to in-memory state
    on_parse_error: impl Fn(usize, &str, &serde_json::Error) -> E, // line, raw, err -> pillar error
) -> Result<(), E>
where
    R: serde::de::DeserializeOwned;
```

This is **Rust-idiomatic** per CLAUDE.md: a free function, generic over
`R: DeserializeOwned` and the caller's error `E`, monomorphised per
pillar with NO `dyn Trait` indirection; the two closures (`apply`,
`on_parse_error`) are the seam that absorbs the per-pillar differences
(distinct `WalRecord` enums, distinct error types, pulse's `tenant_counts`
side-table maintained inside `apply`). The pillars keep their own
`WalRecord` enum and their own error type; only the loop body — the part
that is genuinely identical and is the part that must NOT drift — is
shared. Each pillar's `open` shrinks to: read the WAL bytes, call
`replay_wal_tolerating_torn_tail` with a closure that applies its record
type, then continue with its existing post-replay work (lumen's re-sort,
ray's `sort_all`, pulse's `tenant_counts` reseed — all unchanged and
outside the shared routine).

Mutation testing gains the ADR-0054 benefit: the three guard conditions
(is-last-line, no-trailing-newline, parse-failed) and the warning
emission live at ONE site, so a single `cargo mutants` run on
`crates/wal-recovery` kills the guard mutants once and meaningfully,
instead of the kill signal being split across four (eventually six)
suites. `crates/wal-recovery` gets its own `gate-5-mutants-wal-recovery`
expectation (DEVOPS decides the CI wiring; the expectation is annotated).

This is consistent with ADR-0049's per-pillar `FsyncBackend` choice
rather than contradicting it: ADR-0049 kept the trait per-pillar at slice
01 because only ONE pillar was in scope and the shape was not yet proven;
it explicitly said "later pillars may need to share it; successor slices
will decide". Here four pillars are in scope at once, the shape is fully
proven (six identical loops), and the recovery loop body is pure
data-shuffling with no pillar-specific policy, so the share is justified
NOW under the same rule-of-three discipline ADR-0054 applied to the read
tier.

### 5. Scope: lumen, ray, cinder, AND pulse all in this slice (FLAG 1)

Pulse is **IN SCOPE for this slice**. Its WAL replay loop
(`crates/pulse/src/file_backed.rs:164-187`) is structurally identical to
the other three: `reader.lines().enumerate()`, skip empty,
`serde_json::from_str`, map to `MetricStoreError::PersistenceFailed`,
`apply_ingest`. The `tenant_counts` rebuild that DISCUSS flagged as a
possible complication does NOT interact with the torn-tail drop: the
counts are maintained inside `apply_ingest` during replay AND reseeded by
a single belt-and-braces post-loop pass over the rebuilt `series` map
(lines 189-205). Dropping the torn final line simply means `apply_ingest`
is called one fewer time; the post-loop reseed then reflects exactly the
recovered prefix's cardinality, because it reads the rebuilt map, not the
WAL. The drop is therefore transparent to pulse's cardinality watermark
(ADR-0051). Pulse's `apply_ingest` becomes the body of the `apply`
closure passed to the shared routine; everything else in pulse's `open`
(snapshot rehydrate, post-loop sort, post-loop `tenant_counts` reseed)
stays unchanged and outside the shared routine. The marginal cost of
including pulse is one more `apply` closure; the coverage is uniform
across all four pillars in this slice. No deferral.

`sluice` and `strata` are **explicitly OUT of this slice** (they are not
in the feature's stated scope), but because the recovery routine is now
shared (Decision 4), extending them later is a small `apply`-closure
addition per pillar with zero new GUARD logic — the divergence risk of
deferring them is eliminated by sharing. One follow-up nuance read from the
code: `sluice`'s replay applies records through a FALLIBLE `apply_record`
(`crates/sluice/src/file_backed.rs:176`, propagated with `?`), unlike the
four in-scope pillars whose `apply` is infallible. When sluice/strata are
extended, the shared seam will need a fallible-`apply` variant (a
`try`-closure returning `Result<(), E>`, or a second entry point); the
four in-scope pillars use the infallible `apply: FnMut(R)` seam unchanged.
Recorded as a tracked follow-up, not a blocker for this slice.

### 6. cinder doc correction (FLAG, AC-7)

The cinder module doc at `crates/cinder/src/file_backed.rs:36-38` and the
`open` doc comment at lines 104-106 are corrected to describe the ACTUAL
(newly correct) behaviour: a torn final line with no trailing newline is
dropped with a structured warning naming the pillar, line, and dropped
byte length; every other parse failure (mid-file, or a newline-terminated
malformed final line) is surfaced as
`MigrateError::PersistenceFailed`. The doc moves from a false claim the
code contradicted to a true claim the code now honours.

## Alternatives considered

### A. Tolerate ALL un-parseable lines (skip-and-continue on any parse failure). Rejected.

For: trivially recovers from any corruption. Against: this is strictly
WORSE than today's fail-closed behaviour. It would silently swallow
mid-file corruption — a gap in the middle of the acked history — and
serve a store that is missing records the operator believes are durable,
with no signal that anything was lost. The whole value of the durability
promise is that acked means recoverable; skipping arbitrary bad lines
breaks that. The narrow tolerance (final torn line only) is the entire
point. Rejected hard; AC-5 is the explicit guard that this was rejected.

### B. Checksum or length-prefix every WAL record so a torn tail is detectable structurally. Rejected for this slice.

For: a length-prefixed or CRC-tagged framing would let recovery detect a
short/torn final record without inspecting trailing bytes, and would also
catch bit-rot in the middle. Against: it is a WAL format change. It breaks
the on-disk compatibility of every existing pillar's WAL, requires a
format-version field and a migration, touches the write path (which this
feature explicitly does not), and is a far larger blast radius than the
problem warrants. The torn tail is the ONLY post-crash residue an
append-only fsync-honest WAL produces; trailing-byte inspection detects it
exactly without a format change. Checksums for mid-file bit-rot are a
genuine future concern but a separate feature under its own ADR. Rejected
for this slice; recorded as a possible successor.

### C. Length-prefixed framing (records as `LEN\n BYTES` blocks). Rejected.

A specific instance of B: prefix each record with its byte length so a
truncated final block is detectable by comparing the declared length to
the bytes available. Same rejection as B (WAL format change, write-path
change, migration), plus it loses the human-readable NDJSON property that
makes the WAL inspectable with `tail` and `jq` today. Rejected.

### D. Per-pillar replication of the recovery logic (no shared crate). Rejected (FLAG 4).

For: no new crate node; each pillar stays self-contained; matches
ADR-0049's slice-01 per-pillar `FsyncBackend` precedent. Against: there
are six identical loops; replicating the three guard conditions and the
trailing-byte inspection into four `open` paths now and two later is
exactly the copy-paste divergence ADR-0054 was extracted to end and
ADR-0040 case B warned a recovery copy-paste produces. The mutation
signal would be split across four (eventually six) suites instead of
killed once at a shared site. The recovery loop body is pure
data-shuffling with no pillar-specific policy, so it shares cleanly behind
a closure seam; the per-pillar `WalRecord` and error types are absorbed by
generics + two closures with no `dyn`. ADR-0049 kept its trait per-pillar
because only one pillar was in scope and the shape was unproven; here four
are in scope and the shape is fully proven. Rejected in favour of the
shared `wal-recovery` crate (Decision 4).

### E. Detect "last line" by catching the parser's EOF state rather than the trailing byte. Rejected.

For: no extra byte read; rely on `serde_json` reporting an unexpected-EOF
error class on the torn record. Against: `serde_json::from_str` on a
single NDJSON line does not reliably distinguish "truncated mid-token"
from "syntactically complete but semantically wrong" via error class
alone; a torn line can fail with a non-EOF error (e.g. truncated inside a
string value yields an unterminated-string error, not unexpected-EOF), and
a complete-but-malformed line can fail with various classes too. Inferring
the tear from parser error state is fragile and would mis-classify some
torn tails as non-tears (refusing a recoverable store) and risks
mis-classifying some complete-malformed lines as tears (the AC-6
violation). The physical trailing byte is the honest, unambiguous
discriminator. Rejected.

### F. Read-side repair (attempt to complete the torn record). Rejected.

For: theoretically the torn record's tenant/op prefix is intact and could
be salvaged. Against: a torn record was NEVER durably acked (the crash
interrupted its append before the closing newline and the post-record
fsync that constitutes the ack). Salvaging it would FABRICATE data the
caller never received an ack for, violating the "no data is ever
fabricated or partially served" constraint. The torn tail is dropped,
never repaired. Rejected on principle.

## Consequences

### Positive

- **The durability promise becomes whole**. ADR-0049 made the write side
  crash-honest; this ADR makes the read-back recover the crash-honest
  write's durable prefix instead of refusing it. The two together close
  the loop: acked-then-crashed now means recoverable across all four
  in-scope pillars. K1 (the north-star) moves from 0% to 100% of
  crashed-with-torn-tail stores recovering their prefix.
- **The tolerance is provably narrow**. Mid-file corruption and
  newline-terminated malformed final lines stay fail-closed (AC-5, AC-6);
  the guardrail K4 is preserved co-equal with the positive path. A
  recovery that silently swallowed mid-file corruption would be strictly
  worse than today; this ADR explicitly does not do that.
- **One honest doc**. The cinder false robustness claim is corrected to
  match the code (AC-7, K5). The project's structural-honesty thesis no
  longer ships a doc claiming a robustness the code lacked.
- **No drift, one mutation site**. The recovery loop body lives once in
  `crates/wal-recovery`; the three guard conditions and the warning are
  mutation-killed at a single site, and extending sluice/strata later is a
  one-line closure addition with zero new logic.
- **No trait change, no blast radius**. `LogStore`, `TraceStore`,
  `TieringStore`, `MetricStore` signatures are byte-identical to the prior
  tag; Gate 2 (`cargo public-api`) catches any regression (AC-8).
- **Operator confidence**. The single structured WARN naming pillar,
  line, and dropped bytes lets an operator confirm exactly one torn tail
  was dropped (K3) before trusting the recovered store.

### Negative

- **One more crate node**. `crates/wal-recovery` is a new workspace
  member and one more compilation unit (the same modest cost ADR-0054
  accepted for `query-http-common`). The cost is compile-time only; the
  function monomorphises per pillar and is small enough to inline. Judged
  worth it against the six-way copy-paste divergence it prevents.
- **Two library crates gain a `tracing` dependency**. lumen and pulse (and
  ray; cinder already transitively) do not currently depend on `tracing`;
  the shared `wal-recovery` crate depends on `tracing = "0.1"`, already in
  `Cargo.lock` via aperture and the read tier (per the
  `read-api-tracing-subscriber-v0` precedent), so zero resolution churn.
  The pillars get the warning emission for free through the shared crate;
  they do not each add `tracing` directly.
- **sluice and strata stay un-recovered until a follow-up**. Two of the
  six stores keep the parse-or-die behaviour this slice does not touch.
  The follow-up is a one-line closure addition each (Decision 5); recorded
  as tracked, not blocking. The risk of their drifting from the in-scope
  four is eliminated by the shared routine.
- **The trailing-byte read is one extra small I/O on open**. The recovery
  routine reads the final byte (or the whole WAL) to compute
  `ends_with_newline`. On open only, once, bounded; negligible against the
  full WAL replay it accompanies.

### Trade-off summary

The refinement is intentionally narrow: it adds exactly one tolerated
shape (the torn final line with no trailing newline) to the WAL replay
loop, recovers the intact prefix, warns, and keeps every other parse
failure fail-closed. The trade-off is "absolute fail-closed simplicity"
against "an honest durability promise that recovers the residue a
crash-honest WAL actually leaves". v0/v1 takes the latter and guards the
narrowness with two negative acceptance criteria and 100% mutation kill on
the three guard conditions.

## Verification

- **Earned-Trust enforcement (three orthogonal layers, per the methodology
  and the ADR-0049 precedent)**: (a) **subtype/type check** — the shared
  `replay_wal_tolerating_torn_tail` is generic over `R: DeserializeOwned`
  and the caller's error `E`; each pillar's `open` consumes it with its
  own `WalRecord` and error type, so a signature drift fails the build at
  the call site (`mypy`-equivalent: `cargo check` + the generic bound). (b)
  **structural check** — an AST pre-commit check asserts each in-scope
  pillar's `open` calls `wal_recovery::replay_wal_tolerating_torn_tail` and
  no in-scope pillar retains an inline `serde_json::from_str(&line)...
  PersistenceFailed` replay loop (the thing that must not be
  copy-pasted back in). `import-linter` was investigated and rejected (its
  contracts are import-graph only, with no API for method/call-presence
  enforcement); the AST hook covers the structural layer. (c)
  **behavioural gold-test** — a CI gold-test in `crates/wal-recovery`
  exercises the catalogued substrate lies: a torn final line (recovers +
  warns), a mid-file parse failure (refuses, no warn), a
  newline-terminated malformed final line (refuses, no warn), a
  snapshot-plus-single-torn-line (recovers to snapshot state), and an
  empty/no-WAL case. A single-layer bypass is caught by at least one of the
  other two.
- **Self-application of Earned-Trust**: the gold-test layer is itself the
  probe that verifies the recovery routine actually drops the tail and
  warns (not merely claims to); the AST layer is the probe that verifies
  the pillars actually call the shared routine (not merely that it exists).
- **Mutation testing**: `cargo mutants` scoped to the modified files
  (`crates/wal-recovery/src/lib.rs` and the call-site changes in each
  pillar's `file_backed.rs`) at the 100% kill-rate gate (ADR-0005 Gate 5;
  CLAUDE.md). Primary targets: the three guard conditions
  (`is_last_line` boundary `==`/`!=`, `ends_with_newline` true/false, the
  parse-failed arm), the warning emission (the `tracing::warn!` call must
  not be deletable without a surviving test), and the `line`/`dropped_bytes`
  field values (off-by-one on the line number, wrong byte count must be
  killed). Covered by each in-scope pillar's existing `gate-5-mutants-*`
  job via `--in-diff` plus a new `gate-5-mutants-wal-recovery` expectation
  (DEVOPS wires the CI; the expectation is annotated, not assumed).
- **Gate 2 (`cargo public-api`)** confirms `LogStore`, `TraceStore`,
  `TieringStore`, `MetricStore` trait signatures are byte-identical to the
  prior tag (AC-8).
- **AC traceability**: AC-1 (intact-prefix recovery, lumen end-to-end via
  `GET /api/v1/logs`, verifier D04), AC-2 (dropped not repaired), AC-3
  (the structured WARN fields of Decision 3), AC-4 (snapshot-plus-torn-tail
  on ray), AC-5 (mid-file fail-closed), AC-6 (newline-terminated malformed
  final line fail-closed), AC-7 (cinder doc), AC-9 (scope = lumen, ray,
  cinder, pulse; Decision 5), AC-10 (mutation kill).

## External-integration handoff

None. The recovery routine reads the in-process filesystem under
`pillar_root`, not a network service. No consumer-driven contract test
recommendation. The structured WARN event rides the existing structured
`tracing` stream captured by the same subscriber that captures
`event="health.startup.refused"` and `event="listener_bound"`
(`read-api-tracing-subscriber-v0`, ADR-0009 posture); no new metric, no new
dashboard at v0.
