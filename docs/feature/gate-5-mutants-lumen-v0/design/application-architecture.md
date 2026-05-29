# Application Architecture — gate-5-mutants-lumen-v0

British English. No em dashes in body.

- **Wave**: DESIGN (application scope, trivial)
- **Author**: Morgan (`nw-solution-architect`)
- **Date**: 2026-05-29

This feature is a CI workflow extension. There is no application-code
delta. The only architectural surface touched is the GitHub Actions
workflow file. This document records the file-level change set, the
post-DELIVER verification recipe, and an explicit justification for
omitting a C4 diagram.

## Changes Per File

| File | Kind | Summary | ~LOC |
|------|------|---------|------|
| `.github/workflows/ci.yml` | EXTEND | Insert one new job block `gate-5-mutants-lumen:` at line 1209, immediately after `gate-5-mutants-log-query-api` (ends at line 1208) and immediately before `gate-5-mutants-trace-query-api` (starts at line 1210). Verbatim copy of the sibling block with thirteen textual token swaps (see `wave-decisions.md` DD3). | ~87 |

No other file is touched. No Rust source, no `Cargo.toml`,
`Cargo.lock`, `deny.toml`, `rust-toolchain.toml`, ADR, or schema
file is modified.

## Verification

Post-DELIVER, the following three checks confirm the gate is shipped
and functioning. They map to K1, K3, and K2 from `outcome-kpis.md`
respectively.

1. **Existence (K1)**: at the feature-close commit,

    ```sh
    grep -c "gate-5-mutants-lumen" .github/workflows/ci.yml
    ```

    returns at least 2 (the job key on its own line plus at least one
    other occurrence among cache keys, step names, artefact name,
    diff filter path, and the inline comment). For tighter K1, the
    job key alone is verified by

    ```sh
    grep -c "^  gate-5-mutants-lumen:$" .github/workflows/ci.yml
    ```

    which returns exactly 1.

2. **Workflow registration**:

    ```sh
    gh workflow view ci.yml | grep gate-5-mutants-lumen
    ```

    lists the job under the workflow's job index, confirming GitHub
    Actions has parsed and scheduled it.

3. **Synthetic mutation test (K2)**: introduce a deliberate mutation
   in `crates/lumen/src/predicate.rs` (for example, flip
   `!record.body().contains(needle)` to `record.body().contains(needle)`
   in a `Predicate::matches` arm), open a PR, and observe that the
   new `gate-5-mutants-lumen` job reports a surviving mutation and a
   red check. Revert the mutation and observe the job turns green.

    The negative half of K2 (the diff filter excludes non-lumen
    changes) is verified by a separate synthetic PR that touches
    only a non-lumen file, e.g. `crates/log-query-api/src/lib.rs`;
    the new job emits
    `No lumen-touching changes vs origin/main; skipping mutation testing.`
    and exits 0 in under a minute.

K3 (zero regression on sixteen pre-existing jobs) and K4 (zero net
new dependency) are verified by `git diff` over the pre-feature and
post-feature commits at the workflow file and at `Cargo.toml`,
`Cargo.lock`, `deny.toml`. Neither requires a runtime observation.

## No C4 Diagram

This feature does not change any application-level component
boundary, port, adapter, or integration. The CI workflow is build-time
infrastructure, not part of the runtime system topology. A C4
diagram (System Context, Container, or Component) would carry no
information that the change-set row above does not already carry
more precisely, and would constitute gold plating against the
"simplest solution first" architectural principle. The runtime
topology of Kaleidoscope is unchanged; the `lumen` crate's runtime
interface and dependency graph are byte-identical pre vs post.

The post-feature CI workflow gains one job; the runtime system gains
nothing. The DEVOPS wave (Apex) executes the single workflow edit
specified in `wave-decisions.md`.
