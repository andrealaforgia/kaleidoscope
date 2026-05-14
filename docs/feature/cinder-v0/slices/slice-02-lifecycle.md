# Slice 02 — Age-based lifecycle policy (US-CI-02)

## Goal

Lift Cinder from "place and migrate manually" to "advance
items through tiers based on age".

## IN scope

- `TierPolicy::age_based(hot_to_warm, warm_to_cold)`
- `TieringStore::evaluate_at(now, &TierPolicy)` returning
  the migration count
- Forward-only automatic migration; idempotence under
  repeated `evaluate_at(now, ...)`
- KPI 2

## OUT scope

- Size-based / query-rate-based policies (v1)
- Background-thread timer (v1; the operator binary owns
  the timer)
- Automatic rehydrate from Cold (v1)

## Learning hypothesis

Disproves "a linear pass over the per-tenant tier table is
fast enough as the periodic evaluator". If KPI 2 fails, the
v1 substrate needs an age-index.

## Effort

≤1 day.
