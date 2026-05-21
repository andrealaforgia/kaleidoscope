# Slice 02: Cross-crate tenant-identity contract for signals `@infrastructure`

Story: US-02 (Cross-crate tenant-identity contract holds across the signal pillars)
KPI: KPI-2 (identity-contract regression guard)
Priority: P2
Status: Ready (pending DISCUSS reviewer approval)

## Honest labelling

`@infrastructure`. This slice enables no new operator decision on its own; it is
a compile-time regression guard. Its release value is realised at the slice/
feature level because it ships alongside Slice 01 (a user-visible story). No
Elevator Pitch is claimed — see review Dimension 0 slice-level rule.

## Outcome

A second test in the same target,
`tenant_id_is_the_cross_crate_identity_contract_for_signals`, documents by
exercised example that one `aegis::TenantId` crosses pulse, ray and strata with
no conversion. If `aegis::TenantId`'s shape drifts, the target fails to compile
— turning a would-be runtime isolation bug into an immediate build failure.

## Shippable end-to-end?

Yes. A tiny standalone test. Depends only on the dev-deps and `[[test]]` block
that Slice 01 introduces; once those exist, this test compiles and runs on its
own.

## Carpaccio taste tests

- **Demonstrable in one session**: yes — one assertion-bearing test.
- **Thin vertical**: yes — one identity threaded through three adapters.
- **Independently valuable**: yes — delivers KPI-2 (the regression tripwire).
- **Right-sized**: ~0.1 day; mirrors first-triad test 2.

## Work (DELIVER wave, authored by @nw-software-crafter)

1. Add `tenant_id_is_the_cross_crate_identity_contract_for_signals` to
   `crates/integration-suite/tests/v1_three_durable_stores_compose.rs`.
2. Hold one `let shared = tenant("shared");`. Open the three FileBacked stores
   at sub-paths of one temp root.
3. Pass `&shared` to `pulse.ingest`, `ray.ingest`, `strata.ingest` (one record
   each), then read each back under `&shared` and assert length 1.
4. `cleanup(root)`.

## Acceptance criteria (from US-02)

- [ ] One `aegis::TenantId` binding passed by reference to all three adapters, no conversion.
- [ ] Each store reads exactly one record back under that tenant.
- [ ] Target compiles only while `aegis::TenantId` is shared verbatim across the three crates.

## Note

Co-located with Slice 01 in one file. Sequencing is soft: Slice 01 must add the
dev-deps and `[[test]]` block first; both tests then live and run together,
mirroring the two-test structure of
`crates/integration-suite/tests/v1_three_adapters_compose_under_restart.rs`.
</content>
