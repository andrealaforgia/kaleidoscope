# Slice 02 — Predicate filtering (US-LU-02)

## Goal

Lift Lumen from "give me logs in a time range" to "give me logs
in a time range that match this predicate" without changing the
trait's tenant or time-range surface.

## IN scope

- `Predicate` value type with `service` + `min_severity`
  filters
- `LogStore::query_with(tenant, range, &Predicate)` method
- Composition semantics (intersection)
- KPI 2 acceptance test: 200 queries over 10k records, assert
  p95 ≤ 10 ms

## OUT scope

- Body / attribute-path predicates (v1)
- Full-text search (v1, behind Tantivy)
- Predicate compilation / planning (v1, behind DataFusion)
- Aperture retrofit (v1)

## Learning hypothesis

Disproves "a linear scan with cheap predicate matching is fast
enough at 10k records to ship as the v0 query ceiling". If KPI
2 fails on the linear scan, we either tighten the ceiling
(unlikely; the roadmap accepts a loose v0 ceiling) or split out
a substrate-first slice. If it passes, the trait shape carries
forward to v1 without breaking changes.

## Effort

≤1 day. Predicate type + linear-scan filter is mechanical; the
acceptance test seeds a realistic 10k-record corpus and pins
the KPI 2 ceiling.
