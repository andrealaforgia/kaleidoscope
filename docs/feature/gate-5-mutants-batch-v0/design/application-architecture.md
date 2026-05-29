# Application Architecture — gate-5-mutants-batch-v0

British English. No em dashes in body. (Em dash appears only inside the
job `name:` display field, copied verbatim from the sibling.)

- **Wave**: DESIGN (application scope, trivial)
- **Author**: Morgan (`nw-solution-architect`)
- **Date**: 2026-05-29

Pure CI workflow extension. Eight new `gate-5-mutants-<crate>` job
blocks are added to `.github/workflows/ci.yml`, one per residual crate
from the `gate-5-mutants-lumen-v0` audit (commit a11910f residue). No
production source change. No new tooling. No new dependency.

## Changes Per File

| File | Change | Magnitude | Detail |
|------|--------|-----------|--------|
| `.github/workflows/ci.yml` | EXTEND | +~200 LOC (8 blocks x ~86 LOC plus blank separators) | Insert eight `gate-5-mutants-<crate>` job blocks at their alphabetical-neighbour slots (see `wave-decisions.md` DD1). Each block is a verbatim copy of the `gate-5-mutants-lumen` template (lines 1210 to 1295) with the thirteen token swaps of DD3. The seventeen existing job blocks are byte-identical pre vs post. |

No other file is touched. No `crates/*/src/**`, no `Cargo.toml`, no
`Cargo.lock`, no `deny.toml`, no `rust-toolchain.toml`, no ADR.

## The eight jobs

Package names verified from each `crates/<dir>/Cargo.toml` line 2; all
eight equal their directory name.

| Crate dir | Package name (verified) | Diff filter path | Placement (insert after) |
|-----------|-------------------------|------------------|--------------------------|
| `aegis` | `aegis` | `crates/aegis/**` | `gate-5-mutants-aperture` (line 503) |
| `augur` | `augur` | `crates/augur/**` | `gate-5-mutants-aperture-storage-sink` (line 949) |
| `sluice` | `sluice` | `crates/sluice/**` | `gate-5-mutants-sieve` (line 692) |
| `beacon-server` | `beacon-server` | `crates/beacon-server/**` | `gate-5-mutants-beacon` (line 1635) |
| `cinder` | `cinder` | `crates/cinder/**` | the new `gate-5-mutants-beacon-server` block |
| `loom` | `loom` | `crates/loom/**` | `gate-5-mutants-log-query-api` (line 1123, ends 1208) |
| `integration-suite` | `integration-suite` | `crates/integration-suite/**` | `gate-5-mutants-harness` (line 453) |
| `kaleidoscope-gateway` | `kaleidoscope-gateway` | `crates/kaleidoscope-gateway/**` | `gate-5-mutants-kaleidoscope-cli` (line 1723) |

Each job uses `needs: [gate-2-public-api, gate-3-semver]`, verbatim
from the sibling (lines 1213 to 1215), uniform across all seventeen
existing jobs.

Small-crate behaviour (`integration-suite` ~50 src LOC,
`kaleidoscope-gateway` ~486 src LOC): the job ships and is green in both
no-op cases. An empty diff short-circuits via the shell `exit 0`; a diff
with zero viable mutants causes `cargo mutants` itself to exit 0 (no
surviving mutants is a pass; a non-zero exit is reserved for survivors
and for build failures). The job activates automatically once the crate
gains mutable production code. See `wave-decisions.md` DD4.

## Verification

Post-DELIVER, the closing agent verifies:

1. **Count**: `grep -c "name: gate-5-mutants-" .github/workflows/ci.yml`
   returns `25` (was `17`, +8). Equivalently
   `grep -c "^  gate-5-mutants-" .github/workflows/ci.yml` returns `25`.
2. **Per-crate existence**: for each of the eight,
   `grep "gate-5-mutants-<crate>:" .github/workflows/ci.yml` returns
   exactly one line, with the matching `--package <crate>` and
   `crates/<crate-dir>/**` diff glob in its block.
3. **YAML validity**:
   `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`
   exits 0. (If `pyyaml` is absent from the system Python, the Ruby
   equivalent `ruby -ryaml -e "YAML.load_file('.github/workflows/ci.yml')"`
   is acceptable, per the `gate-5-mutants-lumen-v0` DEVOPS precedent.)
4. **Zero regression**: the seventeen pre-existing `gate-5-mutants-*`
   job blocks are byte-identical pre vs post; every non-gate-5 job is
   byte-identical. Verified by `git diff` restricted to lines outside
   the eight new blocks (K3, K6).

## No C4 / No ADR

No C4 diagram and no ADR are produced. This is an infrastructure YAML
batch: eight verbatim copies of one established job block with a
four-token-class swap per copy. There is no new component, no new
boundary, no new integration, and no new technology to model in C4. The
decision content is fully captured by DD1 to DD4 in `wave-decisions.md`.
Replicating an established pattern (ADR-0047, ADR-0048, ADR-0052; the
`gate-5-mutants-lumen-v0` and `query-http-common-v0` precedents) is
execution, not an architectural decision. The five-gate ADR-0005
contract is unchanged; this adds eight per-crate instances of Gate 5,
not a sixth gate. ADR immutability is preserved.
