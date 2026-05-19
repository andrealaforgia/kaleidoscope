# DISCUSS — cli-evaluate-policy-subcommand-v0

Author: orchestrator-direct (agent quota exhausted)
Date: 2026-05-19

## D1 — Feature type
Backend / CLI subcommand.

## D2 — Walking skeleton
No. The CLI exists; this is a new subcommand that exposes an
existing Cinder API method (`TieringStore::evaluate_at`).

## D3 — UX research depth
Lightweight. Same operator persona (Priya).

## D4 — JTBD
No. The job is obvious: trigger Cinder's auto-tiering policy
manually for testing, debugging, or backfill-after-outage.

## D5 — Tenant-less subcommand (FIRST in the CLI)

`TieringStore::evaluate_at(now, policy) -> usize` is cross-tenant
by design. It walks every `(TenantId, ItemId)` entry in the store
and migrates whichever entries match the policy thresholds.
Result is the TOTAL count migrated across all tenants.

This is the first kaleidoscope-cli subcommand that does NOT take
a tenant id as a positional argument. Every prior subcommand
(`ingest`, `read`, `stats`, `migrate`, `place`, `list-items`,
`get-tier`) takes `<tenant_id>` first. `evaluate-policy` cannot,
because the API does not. Two options were considered and one
rejected:

  - Option A (chosen): subcommand shape is
    `evaluate-policy <data_dir> <hot_to_warm_secs> <warm_to_cold_secs>`.
    Faithful to the underlying API. Stdout reports
    `evaluated migrated=<total>\n` with NO tenant qualifier.
  - Option B (rejected): require a `<tenant>` positional arg
    and filter the result post-hoc by examining each migration
    via the recorder. Rejected because (a) Cinder's bulk
    `evaluate_at` returns a count, not a list, so filtering
    post-hoc requires either pre-state-snapshot diffing (heavy)
    or recorder-introspection plumbing (new code). Honest
    mapping of the underlying API wins.

This deviation from the tenant-first convention is the principal
new design surface this feature introduces. Documented here so
reviewers and future operators are not surprised.

## D6 — Duration argument units

Two positional args after data_dir, both unsigned integer
seconds. `hot_to_warm_secs warm_to_cold_secs`. Constructed via
`std::time::Duration::from_secs(value)`. No floating point, no
unit suffixes, no time-string parsing. Operators typing 86400
will read it as "one day" with no ambiguity.

## D7 — `--observe-otlp` interaction

The recorder slot on FileBackedTieringStore is the same slot
that migrate uses. If `--observe-otlp <path>` is set, the bulk
evaluate_at will emit one `cinder.migrate.count` line per
internal migration, just like a sequence of manual migrate calls
would. The audit trail composes naturally.

## D-Out-of-scope

- Dry-run (would require Cinder API extension).
- Per-tenant filter (rejected per D5).
- Custom policy shapes other than age-based.
- Floating-point duration arguments.

## Reference class

Bigger than the prior CLI features. The TierPolicy construction
and the two-Duration parsing are new surface; the rest is
inheritance. Estimated half a day.

## Outcome KPIs

- OK1 success: `evaluated migrated=<N>\n` to stdout matches the
  count returned by `evaluate_at`; exit 0.
- OK2 invalid-secs fail-fast: non-integer args (negative,
  non-numeric, overflow) → non-zero exit, stderr names the bad
  value.
- OK3 idempotent under stable (now, policy): two consecutive
  evaluate-policy calls with the same args produce the same N
  on the first call and 0 on the second (everything that should
  have migrated has migrated).
- OK4 --observe-otlp emits N cinder.migrate.count lines.
