# DEVOPS Decisions — pulse-series-identity-v0

British English. No em dashes.

This was a deliberately slim DEVOPS wave. The feature is a library-only,
in-crate data-model correction (see DESIGN `wave-decisions.md`, "DEVOPS
Handoff Annotation"), so the wave confirms that the existing CI contract
covers it without modification rather than designing new infrastructure.
The two artefacts produced are this file and `environments.yaml`. The
slim shape follows the `cinder-to-pulse-bridge-v0` precedent.

## Key Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | Deployment target: none (library). | Pulse is a library at v0/v1, no daemon, no network, no binary. There is nothing to deploy; the change is to in-crate keying logic. |
| D2 | CI/CD platform: GitHub Actions, existing, unchanged. | ADR-0005's five-gate contract already runs on every push to main. No new or amended gate is warranted. |
| D3 | No new CI gate. | The change touches three files of an already-gated crate. Gate 1 (workspace test) runs the new acceptance file; Gate 4 (deny) sees no new dependency; Gate 5 mutation is covered below. Gates 2 and 3 (public-api, semver) scope to harness/spark/sieve/codex and do not include pulse; the MetricStore trait signature is unchanged regardless. |
| D4 | Mutation testing: per-feature, 100% kill rate, on the existing `gate-5-mutants-pulse` job. | The job already exists in `ci.yml` and runs `cargo mutants --in-diff`, so it picks up the mutations in `store.rs`, `file_backed.rs`, and `metric.rs` automatically. No workflow edit, no new job. Mutation scope per DESIGN: those three files. |
| D5 | `SeriesKey` is crate-private (`pub(crate)`) at DELIVER. | The key is internal identity machinery, not public surface. Keeping it crate-private avoids enlarging the public API and keeps the door clean should pulse later graduate to Gate 2/3. |
| D6 | Snapshot format may change; no migration, shim, or version negotiation. | No production Pulse data exists (library-only). Stated so DELIVER does not invent a migration path. |
| D7 | Git branching: trunk-based, unchanged. | Push directly to main; CI is feedback, not a merge gate (per project convention). |
| D8 | No new observability instrumentation. | Pulse IS part of the platform's observability substrate, but this feature adds no instrumentation of its own. The acceptance tests are the empirical probe. |

## Infrastructure Summary

- Deployment: none (library-only, no artefact).
- CI/CD: GitHub Actions, ADR-0005 five gates, unchanged. `gate-5-mutants-pulse` already present.
- Observability: no new instrumentation.
- Mutation testing: per-feature, 100% kill rate on `crates/pulse/src/{store.rs, file_backed.rs, metric.rs}`.
- External integrations: none. No contract tests apply.

## Constraints Established

- No new CI gate, no contract amendment; ADR-0005 inherited unchanged.
- No new external dependency (Gate 4 clean; `SeriesKey` uses only `std` + derives).
- Snapshot format change is permitted; no migration because no production data.
- The new acceptance file MUST exercise a real `FileBackedMetricStore`
  (tempdir) including snapshot and WAL-only recovery, per
  `environments.yaml` (clean environment, durable substrate).

## Upstream Changes

None. The DESIGN handoff annotation anticipated every DEVOPS conclusion;
this wave confirmed it against `ci.yml` (Gate 2 scope verified to
exclude pulse; `gate-5-mutants-pulse` verified present). No DESIGN
assumption changed.

## Handoff to DISTILL

For `@nw-acceptance-designer`: write the acceptance tests in a new
`crates/pulse/tests/` file (e.g. `v1_slice_03_series_identity.rs`),
mirroring `v1_slice_02_snapshot.rs` (real `FileBackedMetricStore` in a
tempdir). Cover the eight acceptance criteria in
`slices/slice-01-series-identity-by-label-set.md`. The `clean`
environment with the durable substrate is the only environment to
parametrise over; there are no external services.
