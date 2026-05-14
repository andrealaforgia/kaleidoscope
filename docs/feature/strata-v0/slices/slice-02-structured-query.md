# Slice 02 — Predicate filtering (US-ST-02)

## Goal

Lift Strata from "give me every profile for this service in
this range" to "give me only the profiles of this type".

## IN scope

- `Predicate` with `profile_type` filter
- `ProfileStore::query_with(tenant, service, range,
  &Predicate)`
- KPI 2

## OUT scope

- Predicates on samples / locations / functions (v1; requires
  the columnar substrate to be efficient)
- Symbolisation predicates (v1)
- Flame graph diff (Prism v1)

## Learning hypothesis

Disproves "a linear scan with cheap `profile_type` equality
matching is fast enough at 1 000 profiles". If KPI 2 fails,
the v1 substrate work gets re-prioritised.

## Effort

≤1 day.
