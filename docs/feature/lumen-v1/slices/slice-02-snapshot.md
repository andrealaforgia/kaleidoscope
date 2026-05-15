# Slice 02 — Snapshot compaction (US-LV1-02)

Bounded recovery time via explicit `snapshot()`.

## IN scope

- `snapshot()` method writes state + truncates WAL
- Recovery loads snapshot then replays remaining WAL
- KPI 2

## OUT scope

- Atomic rename, auto-trigger — v2.

## Learning hypothesis

If KPI 2 fails on Lumen at 10 000 records, the
per-batch NDJSON shape was a wrong choice for the v1
WAL and v2 substrate needs to land sooner.

## Effort

≤1 day.
