# Slice 02 — FileBackedRuleStateStore + recovery wired into startup

Value slice. Carries the Elevator Pitch.

## Summary

Add `FileBackedRuleStateStore` (WAL append on each transition,
`snapshot()` truncates WAL, `open()` recovers snapshot then replays
WAL) mirroring `crates/lumen/src/file_backed.rs`. Wire `open()` into
beacon-server startup; seed each per-rule loop from recovered state;
emit a recovery-count log line; drop states for rules no longer in
config. Alert state survives restart.

## Stories

- US-02 (firing alerts survive restart without re-paging) — carries the
  Elevator Pitch
- US-03 (pending dwell clocks preserved across restart)

## Elevator Pitch (US-02)

- **Before**: restarting beacon-server during a live incident re-pages
  the rotation because every firing rule resets to Inactive and
  re-fires.
- **After**: `beacon-server --rules /etc/beacon/rules --backend ...`
  logs `recovered alert state rules_recovered=42 firing=3 pending=5`,
  the still-active firing rule stays Firing silently, nobody is
  re-paged.
- **Decision enabled**: the operator can restart during a live incident
  without weighing it against a re-page storm.

## Carpaccio taste tests

| Test | Result |
|------|--------|
| Operator-visible behaviour change on its own? | Yes — firing alerts survive restart; recovery counts visible in the startup log. |
| Thin and end to end? | Yes — one durable adapter, one wiring point (`run_rule` seed + startup `open()`). |
| Real entry point in the Elevator Pitch? | Yes — the existing `beacon-server` invocation; no invented surface. |
| Concrete observable output? | Yes — the `recovered alert state ...` log line and the absence of a re-page. |
| Enables a user decision? | Yes — restart-during-incident becomes safe. |

## Constraints carried

- ADR-0037: `transition` untouched; durability lives in the store port.
- No new CLI/HTTP surface; wiring is the existing `run_rule` loop and
  startup.
- WAL/snapshot path derivation mirrors lumen's `file_backed.rs` helpers.
- Latency budgets pinned to GitHub Actions ubuntu-latest: persist p95
  <= 2 ms, recover p95 <= 1.5 s.

## DESIGN-wave flags

- `RuleState` needs `Serialize`/`Deserialize` derives.
- Recover only states whose rule still exists in config (drop + log
  stale entries).
- Decide the store base-path config key and where `snapshot()` is
  triggered (startup, periodic, or shutdown).
- Clock-skew handling on recovered `since` (existing
  `unwrap_or_default` dwell maths already tolerates future-dated
  instants).

## Dependencies

Slice 01 (US-01) must land first.
