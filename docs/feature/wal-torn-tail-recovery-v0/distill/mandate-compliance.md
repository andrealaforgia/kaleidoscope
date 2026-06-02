# Mandate Compliance Evidence — wal-torn-tail-recovery-v0 (DISTILL)

British English. No em dashes in body.

Evidence that all four acceptance-test design mandates pass, prepared for
the DELIVER handoff.

## CM-A: Hexagonal boundary enforcement (driving ports only)

Every test file imports ONLY crate-public driving ports plus std and
serde_json. Zero internal-component imports; zero reference to the
not-yet-existing internal `crates/wal-recovery` routine.

| File | Driving-port imports |
|---|---|
| `log-query-api/tests/slice_08_torn_tail_recovery.rs` | the compiled binary via `env!("CARGO_BIN_EXE_log-query-api")`; `lumen::{FileBackedLogStore, ..}` (for seeding the WAL the binary reads); `aegis::TenantId`; `serde_json::Value` (stderr/HTTP parsing) |
| `lumen/tests/v1_slice_03_torn_tail_recovery.rs` | `lumen::{FileBackedLogStore, LogStore, LogStoreError, ..}` |
| `ray/tests/v1_slice_03_torn_tail_recovery.rs` | `ray::{FileBackedTraceStore, TraceStore, TraceStoreError, ..}` |
| `cinder/tests/v1_slice_03_torn_tail_recovery.rs` | `cinder::{FileBackedTieringStore, TieringStore, MigrateError, ..}` |
| `pulse/tests/v1_slice_05_torn_tail_recovery.rs` | `pulse::{FileBackedMetricStore, MetricStore, MetricStoreError, ..}` |

Grep evidence:

```
$ grep -rn "wal_recovery\|replay_wal_tolerating" crates/*/tests/*torn_tail*
ZERO (good)
```

No test invokes the internal shared `wal-recovery` function. Each pillar's
recovery is exercised indirectly through its store `open` plus a trait
read; lumen's is additionally exercised through the compiled read-API
binary. This is the correct driving-port boundary (brief "For Acceptance
Designer": do NOT enter through the shared function directly).

## CM-B: Business language abstraction

Scenario names and step prose use operator/domain language: "operator
restart serves the intact acked prefix", "recovery emits one structured
warning", "mid-file corruption stays fail-closed", "snapshot plus single
torn tail recovers exactly the snapshot state". The terms used (acked
record, torn tail, recover, fail-closed, snapshot, pillar) are the
ubiquitous language of US-01 and ADR-0059, not implementation jargon.

Technical mechanics (NDJSON bytes, `serde_json`, BufReader, the
trailing-byte discriminator) appear ONLY inside helper functions
(`append_torn_tail`, `seed_acked_prefix`) and on-disk WAL manipulation,
which is the Layer-3 "business service handles technical implementation"
boundary: the scenario bodies read as operator journeys. HTTP and stderr
mechanics are confined to helpers (`http_get_body`, `spawn_until_settled`,
`stderr_event_value`); the assertions speak observable outcomes (records
served, the WARN naming pillar/line/dropped_bytes, the store refusing to
open).

## CM-C: User journey completeness + walking skeleton

- **Walking skeleton (1, user-centric, demo-able)**:
  `operator_restart_serves_the_intact_acked_prefix_after_a_torn_tail` is a
  complete operator journey: trigger (restart the binary against a crashed
  pillar_root), business logic (the store drops the torn tail and recovers
  the prefix on open), observable outcome (the listener binds and the read
  API serves all N acked records), business value (the collector is back
  up serving the durable history without WAL surgery). A non-technical
  stakeholder confirms "yes, that is what an operator needs after a
  crash". Tagged `@walking_skeleton @real-io @driving_port`.
- **Focused scenarios (14)**: the store-reopen positives, the AC-4
  snapshot case, the pulse cardinality property, and the seven
  fail-closed negatives. Each is a complete journey (crash residue on disk
  -> restart/reopen -> observable recovered-or-refused outcome), not an
  isolated technical operation. Ratio 1 skeleton : 14 focused, appropriate
  for a single-behaviour brownfield hardening slice.

## CM-D: Pure function extraction before fixtures

Not applicable as a fixture-parametrisation concern: there is no
cross-environment fixture matrix (both DEVOPS environments, `clean` and
`ci`, run the identical `cargo test --workspace` with empty
preconditions). The impure operation (filesystem WAL I/O, child process,
TCP) is already isolated behind the pillars' `FileBacked*Store::open`
adapters and the `log-query-api` binary; the tests exercise those real
adapters directly. The pure business logic of the feature (the three guard
conditions: is-last-line, no-trailing-newline, parse-failed) is extracted
by DESIGN into the shared `wal-recovery` free function (ADR-0059 Decision
4), whose unit-level gold-test the crafter owns in DELIVER. The acceptance
suite correctly tests the real adapters end to end; no fixture is
parametrised across environments, so CM-D's "extract before parametrise"
rule is satisfied vacuously and correctly.

## Verification summary

- `cargo test --workspace --all-targets --locked`: exit 0, zero live
  failures, all 15 new scenarios `#[ignore]`d (the pre-commit hook stays
  green; `--no-verify` never used).
- `cargo clippy` on lumen, ray, cinder, pulse, log-query-api with
  `--all-targets`: clean (no warnings, no errors).
- No scaffold (`__SCAFFOLD__` / `// SCAFFOLD: true`) anywhere: all tests
  compile against today's public APIs.
- Negative/edge ratio: 8/15 (53%), exceeds the 40% target.
