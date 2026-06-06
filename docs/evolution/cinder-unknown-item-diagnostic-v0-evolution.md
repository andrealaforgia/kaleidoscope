# Evolution archive — cinder-unknown-item-diagnostic-v0

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
`aegis-ingest-auth-v0-evolution.md` and
`beacon-slo-operator-path-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below. This entry is deliberately SHORT, proportionate to a
one-line fix, but keeps the same section spine.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: NONE. A `Display`-string fidelity fix is not architecturally
  significant (DESIGN Decision 4): no new component, port, dependency,
  quality-attribute trade-off, or public-API surface. The decision record
  lives in the feature wave-decisions, not a new ADR.
- Closes: the verifier's only open finding, Bea's UC-TIER coverage batch
  issue 011 (expectation K18, UC-TIER-008/009). It returns to GREEN once
  re-verified on origin.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| discuss | `28165b7` | name the id the operator typed, not the type |
| design | `6a54ae1` | narrowest correct fix, no ADR for a message |
| devops | `bb33b95` | existing CI covers a one-line fix, no semver |
| distill | `b058a34` | RED tests pinning the quoted-id contract |
| deliver | `ddbe982` | bare-id Display + cinder-local mutation kill |
| docs | `5713222` | narrative + slide closure |

The as-built facts below are read from the DELIVER commit `ddbe982`.

## The problem, in Earned-Trust framing

cinder's `MigrateError::UnknownItem` `Display` arm
(`crates/cinder/src/store.rs:55-58`) rendered the item via the `ItemId`
newtype's `Debug`, so an operator who typed `ghost` got back
`cannot migrate unknown item ItemId("ghost") for tenant <tenant>`. The
refusal itself was correct: the migrate fails closed and exits non-zero
on an unknown item, which is exactly right. But the refusal said the wrong
words. The CLI help (`main.rs:208` for migrate, `:245` for get-tier)
promised the bare quoted form `"ghost"`, and the code leaked the internal
type name instead. This is a small but pure instance of the lie the
project's Earned-Trust posture forbids: code disagreeing with its own
documented contract. A correct fail-closed refusal that names the wrong
token still erodes the operator's trust in what the tool is telling them,
and it leaks an implementation detail (`ItemId(`) into an operator-facing
diagnostic.

The finding originates from Bea Verifier's UC-TIER coverage batch, issue
011 (K18, UC-TIER-008/009), which read the diagnostic the operator
actually sees rather than the test the codebase shipped with.

## The decision lineage

### The narrowest fix: `{:?}` on `item.as_str()` in the single arm

DESIGN Decision 1: render the id placeholder as `Debug` of `item.as_str()`
(a `&str`) rather than `Debug` of the `ItemId` newtype. `Debug` of a
`&str` emits the value double-quoted (`"ghost"`), byte-equal to the
documented help shape, where a bare `{}` would emit unquoted `ghost` and
miss the contract. This mirrors the established in-codebase precedent at
`kaleidoscope-cli/src/lib.rs:107`, where `invalid tier {value:?}` renders
`invalid tier "warm"` via the same `{:?}`-on-a-string idiom, keeping the
diagnostic family self-consistent. The mechanism also escapes correctly at
the double-quote edge for free: an id containing a quote renders
`"gh\"ost"` under `{:?}`, where a hand-rolled literal-quote wrap would
break the visual delimiter.

### The `Display`-impl-on-`ItemId` alternative, rejected

DESIGN Decision 2 evaluated and rejected adding `impl fmt::Display for
ItemId` rendered via `{item}`. Two disqualifying reasons: a natural
newtype `Display` emits the inner value bare (no quotes), so the arm would
STILL need to add quotes and the impl would carry no part of the fix; and
it widens the blast radius on a re-exported public type
(`cinder::ItemId`), inviting future callers to print ids unquoted and
re-opening the very inconsistency this feature closes. The single-arm
change is strictly narrower, fully correct, and contract-faithful.

### One arm covers both subcommands; no new ADR; no semver

DESIGN Decision 3: `get-tier` and `migrate` construct the identical
`MigrateError::UnknownItem` and both render through the same
`Error::CinderMigrate` Display composition, so the one arm change fixes
both with zero CLI-side edits. The pre-existing `cinder migrate:` prefix
on a get-tier error is deliberately-consistent wording and stays out of
scope. Decision 4: no new ADR, for the reasons in Status. Decision 5: no
semver bump (the change is the body of a private `Display` impl; no type,
trait, or signature moves; cinder and kaleidoscope-cli are not in the Gate
2/3 semver-pinned set). cinder stays `0.2.0`, kaleidoscope-cli stays
`0.1.0`. NEVER 1.0.0.

## The as-built shape

One production line moved (`crates/cinder/src/store.rs`):

```text
- "cannot migrate unknown item {item:?} for tenant {tenant}"
+ "cannot migrate unknown item {:?} for tenant {tenant}", item.as_str()
```

The fail-closed exit-1 behaviour of `migrate` (and `get-tier`) on an
unknown item is UNCHANGED. Only the rendered id token changed:
`ItemId("ghost")` becomes `"ghost"`.

## The proof and its boundary

- 100% mutation kill on the changed cinder surface (ADR-0005 Gate 5;
  CLAUDE.md per-feature 100%), via `cargo mutants --package cinder
  --in-diff`: 1 mutant found, 1 caught, 0 missed.
- The operator contract is proven end-to-end through the
  `kaleidoscope-cli` subprocess tests
  (`crates/kaleidoscope-cli/tests/unknown_item_diagnostic.rs`, 4/4): the
  diagnostic shows the bare quoted id and never the `ItemId(` wrapper,
  across both `migrate` and `get-tier`. `migrate_subcommand.rs` (6/6)
  stays green under the new wording.
- The cross-crate coverage lesson is the load-bearing finding. The
  operator contract is tested at the CLI level, but `gate-5-mutants-cinder`
  runs `cargo test -p cinder`, which never executes the kaleidoscope-cli
  subprocess tests. Cinder therefore had NO crate-local coverage of
  `MigrateError`'s `Display`, and the `Display::fmt ->
  Ok(Default::default())` empty-message mutant survived green-and-hollow:
  the surface was correct and the mutation gate that guards it could not
  see the test that proved it. The fix was to add a cinder-LOCAL `Display`
  unit test (`crates/cinder/tests/migrate_error_display.rs`, 1/1) that
  asserts the rendered message directly inside the crate, killing the
  empty-Display mutant and guarding the revert-to-`{item:?}` mutant. The
  guard must sit with the thing it guards: a per-crate mutation gate only
  measures the per-crate test suite, so a contract proven only in a
  sibling crate's subprocess tests is, to that gate, unproven.
- No semver consequence: cinder `0.2.0`, kaleidoscope-cli `0.1.0`,
  unchanged.

## The lesson

A correct fail-closed refusal can still say the wrong words, and code that
disagrees with its own help is the Earned-Trust lie in miniature. The fix
was one line. The lesson was in the proof, not the fix: a per-crate
mutation gate measures only the per-crate test suite, so a contract proven
solely through a sibling crate's subprocess tests leaves an empty-`Display`
mutant alive in the crate that owns the message. The guard has to live
with the thing it guards. With the cinder-local `Display` test added, the
mutant dies and the verifier's only open finding (issue 011 / K18) returns
to GREEN once re-verified on origin.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted. The verifier's
issue 011 / K18 that this feature targets is CLOSED (pending re-verify on
origin) and is not listed below.

1. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap needs its own slice. Open.

2. sluice wiring. sluice remains UNWIRED: no gateway/server `src` path
   constructs or drives `FileBackedQueue`. The wiring itself is a separate,
   still-open slice. Open.

3. sluice torn-tail migration. sluice still carries the inline
   parse-or-die recovery loop; its migration to the shared
   `replay_wal_tolerating_torn_tail` routine is the tracked ADR-0059 §5
   follow-up. Open.

4. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
   doubles the store, because lumen has no idempotency key. The designed
   extraction (ADR-0064 DD-3): success-case dedup earns its own slice.
   Open.

5. ingest-bounded-memory. The buffer-all-then-flush design (ADR-0064)
   holds the whole input's records in RAM before commit. A future feature
   lifts it with a temp-WAL staging stage or a max-records streaming cap.
   Open.

6. ADR-0059 Decision 8 layer b, the AST structural check, remains
   UNWIRED. The structural pre-commit check; the tool choice was deferred
   and remains deferred. It is feedback, not a gate, consistent with the
   pure trunk-based, no-required-checks posture; when wired it belongs in
   the local pre-commit stage. Open.

7. aegis unwired. No path authenticates; the aegis surface exists but is
   not on any request path. Open.

8. OTLP partial_success never populated. The OTLP `partial_success`
   response field is never populated, so partial-accept signalling is not
   surfaced to clients. Open.

9. The two claims-honesty DOCUMENT items remain future features if
   wanted. The actual Prometheus-stepped grid for `query_range` (a
   query-api feature) and real gRPC-prefix honouring for `harness`
   (`Framing::GrpcProtobuf`) were documented as v0 reality rather than
   built; each would retire its respective pin. Open only if wanted.

10. aperture early-Ok tolerance. The unexpected-early-`Ok`-without-shutdown
    is treated as FATAL at v0 (surfaced, not tolerated), the honest
    default for a listener that stops unbidden. Open only if such a path
    ever appears.

11. beacon non-30d error budget periods. v0 supports ONLY a 30d error
    budget period. Other windows (7d, 90d) would each need their own
    `MWMBR_TABLE` row set and earn their own slice. Open only if wanted.
