# Slice 02 — Predicate filtering (US-RA-02)

## Goal

Lift Ray from "give me every span for this service in this
range" to "give me every span that matches this predicate
inside that scope".

## IN scope

- `Predicate` with `span_name`, `kind`, `status` filters
- `TraceStore::query_with` composing range + predicate
- Intersection semantics
- KPI 2

## OUT scope

- TraceQL operators (v1)
- Span events / link predicates (v1)
- Attribute-path predicates (v1)

## Learning hypothesis

Disproves "a linear scan with cheap predicate matching is fast
enough at 10k spans". If KPI 2 fails, the v1 substrate work
gets re-prioritised.

## Effort

≤1 day.
