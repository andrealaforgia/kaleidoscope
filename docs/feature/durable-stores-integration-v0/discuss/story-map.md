# Story Map: durable-stores-integration-v0

## User: Priya Nair, platform reliability engineer (storage-plane trust owner)
## Goal: Trust that metrics, traces and profiles all survive a platform restart together under one tenant, with no cross-pillar or cross-tenant leakage.

## Backbone

| Wire the triad into the suite | Compose under one tenant + survive restart | Confirm cross-crate identity |
|-------------------------------|---------------------------------------------|------------------------------|
| Add ray + strata dev-deps     | Ingest metrics+traces+profiles for acme     | Thread one &TenantId through three pillars |
| Declare the `[[test]]` block  | Drop the process, reopen all three stores   | Read each back under the same tenant |
| Mirror first-triad helpers    | Assert acme recovers; globex does not leak  | Compile-time contract guard |

---

### Walking Skeleton (note: brownfield — already exists)

The `integration-suite` crate and its first-triad precedent already provide the
end-to-end skeleton (helpers, the `[[test]]` mechanism, the open/drop/reopen
shape). There is no skeleton to bootstrap. The thinnest end-to-end slice for
THIS feature is Slice 01: the second-triad compose-and-restart test itself,
which already touches all three backbone activities.

### Release 1 — Slice 01: Second-triad recovers under one tenant (US-01)

- Activities touched: all three (wire in, compose+restart, identity exercised within the same test).
- Target outcome KPI: KPI-1 (durability completeness of the composed triad — north star).
- Rationale: this is the whole point of the feature. One self-contained test
  proving metrics + traces + profiles compose under tenant `acme`, survive a
  drop-and-reopen, and that `globex` does not leak. Shippable end-to-end on its
  own; mirrors test 1 of the first-triad file.

### Release 2 — Slice 02: Cross-crate identity contract for signals (US-02)

- Activities touched: "Confirm cross-crate identity".
- Target outcome KPI: KPI-2 (identity-contract regression guard).
- Rationale: a tiny second test, mirroring test 2 of the first-triad file,
  documenting by exercised example that one `aegis::TenantId` crosses pulse,
  ray and strata with no conversion, and breaking the build if the shape drifts.
  Independently shippable; depends only on the dev-deps wired by Slice 01.

## Priority Rationale

| Priority | Slice | Target outcome | KPI | Value x Urgency / Effort | Rationale |
|----------|-------|----------------|-----|--------------------------|-----------|
| 1 | Slice 01 (US-01) | Composed triad recovers identically across restart, zero leakage | KPI-1 | 5 x 4 / 2 = 10 | Riskiest assumption and the north star. Without it the feature delivers nothing. Mirrors first-triad test 1. |
| 2 | Slice 02 (US-02) | One TenantId honoured by all three signal stores | KPI-2 | 3 x 2 / 1 = 6 | Cheap, high-leverage regression guard. Lower urgency because Slice 01 already exercises the shared identity in passing; this isolates it as a compile-time tripwire. |

Tie-break order applied: Walking Skeleton (pre-existing) > Riskiest Assumption
(Slice 01) > Highest Value (Slice 01 again) > remaining (Slice 02).

## Scope Assessment: PASS — 2 stories, 1 bounded context (integration-suite), estimated 0.5 day

Oversized signals checked (need 2+ to be oversized):

- User stories: 2 (<= 10). PASS.
- Bounded contexts / modules: 1 (the integration-suite crate; pulse/ray/strata/aegis are read-only collaborators, not modified). PASS.
- Walking-skeleton integration points: 0 new (skeleton pre-exists). PASS.
- Estimated effort: ~0.5 day (one test file, two tests, two dev-dep lines). PASS.
- Independent user outcomes shippable separately: 2, but both tiny and naturally co-located in one file mirroring the precedent. Not an oversizing signal.

Zero oversized signals. Right-sized. No split required.
</content>
