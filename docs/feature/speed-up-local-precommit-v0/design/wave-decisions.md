# Wave Decisions — speed-up-local-precommit-v0 (DESIGN)

> **Author**: `nw-solution-architect` (Morgan), DESIGN wave, 2026-06-07.
> Mode: **PROPOSE** (autonomous). Feature type: **Infrastructure**.
> **Decision record**: ADR-0072
> (`docs/product/architecture/adr-0072-fast-local-precommit-deep-tests-in-ci.md`).
> Brief section: `## Application Architecture — speed-up-local-precommit-v0`.

## Resolution of the DISCUSS-flagged decisions (D1-D6)

| # | Decision | RESOLVED | Basis |
|---|----------|----------|-------|
| **D1** | Exact fast local test subset | **`cargo test --workspace --lib --locked`** | The simplest honest cut. `--lib` runs only in-`src/` `#[cfg(test)]` unit tests across all 26 crates and **none** of the 165 `tests/*.rs` integration binaries — deterministically excluding all 26 fsync-bound durability bins and every subprocess bin, by construction, with zero deny-list to maintain. Satisfies US-02 (unit-test break still reds the hook). DISCUSS recommendation (a) adopted. |
| **D2** | clippy scope locally | **Keep `cargo clippy --all-targets --locked -- -D warnings`**; trim to `--lib` ONLY if Apex measures the fast-hook total over 5 min | Clippy is compile-bound, not fsync-bound — it type-checks test code but does not *run* the durability suites, so it carries no `sync_all` cost. High-value cheap gate, kept. The single sanctioned trim (`clippy --workspace --lib`) is conditioned on a real DELIVER measurement. DISCUSS recommendation adopted. |
| **D3** | CI-results-watching mechanism + cadence | **`scripts/ci-watch.sh`** (thin `gh run list --branch main --limit N` + `gh run view <id> --log-failed` wrapper) **+ documented cadence in CLAUDE.md `## CI watch`** | The safety net for the deep tests now off the local blocking path. Prints latest `main` conclusion + URL; surfaces **gate-1 and gate-5-mutants** reds clearly; degrades honestly when `gh` is absent/unauth (never a false green). Cadence: after every push + periodic poll while an agent works (target: deep-only regression detected within one cadence interval / < 1 hr). DISCUSS recommendation (b)+(c) adopted. |
| **D4** | Keep the fast gates | **Keep toolchain check + fmt + deny + clippy local, unchanged** | All fast / high-value. Only Step 4 scope changes (D1); Step 2 conditionally trimmable (D2). DISCUSS recommendation adopted. |
| **D5** | Honesty-trade documentation | **Documented explicitly in ADR-0072 §5 + the brief** | A local commit CAN reach main with a deep-only regression the fast hook did not run; caught by CI gate-1 (+ gate-5) + the D3 cadence, then fix-forwarded. Acceptable under trunk-based "CI is feedback, not a gate" PROVIDED the cadence is real (D3 makes it concrete). Mirrors ADR-0070's framing. DISCUSS recommendation adopted. |
| **D6** | Slow durability tests themselves | **Out of scope; flagged as future `faster-test-fsync-backend-v0`** | This feature speeds the LOCAL gate, not the durability tests. They stay I/O-bound IN CI (honest per-record `sync_all` of ADR-0049/0060 — that cost is the durability, not a defect). DISCUSS recommendation adopted. |

## Measured numbers (this wave)

### Structural inventory — MEASURED (filesystem, deterministic)

| Quantity | Value | How |
|---|---|---|
| Workspace crates | **26** | `Cargo.toml` members |
| Integration test binaries (`crates/**/tests/*.rs`) | **165** | each a separate `--all-targets` compile+run unit |
| of which fsync-bound durability bins | **26** | match `*wal_durability*` / `*snapshot*` / `*torn_tail*` / `*crash_durability*` / `*fsync_probe*` / `*snapshot_atomicity*` / `*filebacked_durable_recovery*` — cinder x4, pulse x6, lumen x4, ray x4, strata x3, sluice x3, beacon x1, log-query-api x1 |
| plus subprocess bins | aperture `slice_10_ingest_auth`, `serve_loop_error_surfacing`, `cli_smoke`, `probe_gold_runner`, `probe_refusal_visibility`; kaleidoscope-cli `*_roundtrip`; beacon-server smoke/reload | spawn real binaries |
| Bins `--lib` runs | **0 of 165** | `--lib` = in-`src/` unit tests only; no `tests/*.rs`, no doctests |

This is the load-bearing measurement: the 10-20 min is dominated by exactly
the 26 fsync bins + subprocess bins, and `--lib` excludes 100% of them by
construction.

### Wall-clock seconds — NOT MEASURED this wave (honest gap)

The DESIGN agent in this harness has **no shell-execution tool**
(Read/Write/Edit/Glob/Grep only); `cargo` / `time` could not be run from
DESIGN. The fmt / clippy / deny / `--lib` seconds and the fast-hook total
were therefore **not measured here and are NOT fabricated** (fabricating
seconds would violate test-don't-assume / Earned Trust — worse than declaring
the gap). The decision rests on the deterministic structural measurement
above.

| Item | Value | Source |
|---|---|---|
| Baseline full `--all-targets --workspace` local | **10-20 min** | DISCUSS / verified hook `:92-93` |
| Slimmed hook prior observation | "roughly 3-4 min even with clippy `--all-targets` under heavy parallel load" | DISCUSS US-01 example 3 (observation, not a Morgan-session measurement) |
| fmt seconds | **DELIVER-measured (Apex)** | — |
| clippy `--all-targets` seconds | **DELIVER-measured (Apex)** | — |
| deny seconds | **DELIVER-measured (Apex)** | — |
| `cargo test --workspace --lib --locked` seconds | **DELIVER-measured (Apex)** | — |
| **fast-hook total <= 5 min** | **DELIVER-confirmed (Apex), gated by US-01 timing AC** | — |

**DELIVER obligation on Apex**: sweep leaked procs
(`pkill -9 -f 'target/debug/aperture'`; `pkill -9 -f cargo-mutants`), ensure
a warm build, `time` the slimmed hook, record the seconds + total in
`docs/feature/speed-up-local-precommit-v0/deliver/wave-decisions.md`, confirm
<= 5 min. If over, apply the D2 clippy trim and re-measure.

## Reuse Analysis (MANDATORY)

| Capability needed | Existing asset | Verdict | Justification |
|---|---|---|---|
| Exclude every slow integration/durability/subprocess bin from the local run | `cargo test --lib` selector | **REUSE (cargo primitive)** | Runs only in-`src/` unit tests; never any of the 165 `tests/*.rs` bins. One-flag change; no deny-list to maintain. |
| Still catch unit-test breaks locally | `--lib` runs `#[cfg(test)]` unit tests | **REUSE** | Same selector satisfies US-02 (broken unit reds the hook). |
| Keep the deep gate enforced | CI `gate-1-test` (`--all-targets --workspace --locked`, ci.yml:182) | **REUSE (unchanged)** | Deep suite already runs in CI; local Step 4 was a slow duplicate. No CI edit (US-03). |
| Inspect CI run status from terminal | `gh` CLI (`gh run list`, `gh run view --log-failed`) | **REUSE (existing tool)** | `ci-watch.sh` is a thin wrapper, not a new mechanism. |
| Fast cheap gates | hook Steps 0/1/3 (toolchain/fmt/deny) | **REUSE (unchanged)** | Stay exactly as today (D4). |
| Compile-bound lint | hook Step 2 clippy `--all-targets` | **REUSE (unchanged; conditionally trimmable)** | Compile-bound not fsync-bound; trim to `--lib` only if measured over budget (D2). |
| Hook install/wiring | `scripts/hooks/install.sh` | **REUSE (unchanged)** | Slimmed hook rides existing `core.hooksPath` install. |
| Honesty-trade + non-gating precedent | ADR-0070 | **REUSE (precedent)** | Local-side sibling ADR-0070 left undone (its §7). |
| Watcher + cadence | none today | **CREATE (`scripts/ci-watch.sh` + CLAUDE.md `## CI watch`)** | No watcher / cadence exists; the local wait was the de-facto watch. Composed entirely of reused `gh` primitives. |
| Local-fast/CI-deep ADR + trade home | none today | **CREATE (ADR-0072)** | Citable home for the local-side posture + the deep-regression honesty trade. |

**Reuse verdict**: **REUSE** the `cargo --lib` primitive (whole test-scope
change is one existing flag), CI `gate-1-test` (unchanged), `gh`, the fast
hook steps, `install.sh`; **EXTEND** `scripts/hooks/pre-commit` (Step 4) +
docs; **CREATE** only `scripts/ci-watch.sh` and ADR-0072. No code, no test
body, no crate version touched. Both new assets are shell/docs → **Apex
writes them in DELIVER, NOT the crafter.**

## CI-watch design (D3 contract)

- **Mechanism**: `scripts/ci-watch.sh`, a thin wrapper over `gh`:
  - `gh run list --branch main --limit N` → latest `main` runs;
  - `gh run view <id> --log-failed` → on failure, the failing job/step tail.
- **Prints**: latest `main` run **conclusion** (`success`/`failure`/
  `in_progress`), **run URL**, and on failure the failing job + log tail.
  MUST surface **gate-1** (deep tests) and **gate-5-mutants** reds clearly —
  the two gates a slimmed local hook no longer pre-runs.
- **Honest degradation (Earned Trust)**: if `gh` is absent/unauthenticated or
  the network is down, print a clear remediation message and exit non-zero —
  **never** report green on an un-probed substrate. This is a probe
  responsibility on the DELIVER script (flagged to Apex).
- **Invoked, not auto-run**: a courtesy command, consistent with the hook
  being a courtesy.
- **Cadence (CLAUDE.md `## CI watch` + brief)**: after every push to main +
  periodic poll while an agent works a multi-slice task. Target:
  deep-only regression detected within one cadence interval (same working
  session / < 1 hr), not days (US-04 KPI).

## Honesty trade (D5) — stated plainly

With the deep tests off the local blocking path, a local commit CAN reach
`main` carrying a deep-only regression (durability / snapshot / torn-tail /
crash / subprocess / integration) the fast hook did not run. It is caught by
**CI gate-1 (+ gate-5-mutants) plus the `ci-watch.sh` cadence**, then
fix-forwarded — not stopped at commit. **Acceptable under trunk-based "CI is
feedback, not a gate" (`project_kaleidoscope_pure_trunk_based`) PROVIDED the
cadence is real** — D3 makes it real (a one-command script + a written
cadence). Same trade ADR-0070 accepted for the perf signal.

## Constraints carried into DELIVER

- Do NOT weaken CI: `gate-1-test` stays `cargo test --workspace --all-targets
  --locked`; `ci.yml` is NOT touched by this feature.
- Do NOT delete any test (the 165-bin count is unchanged).
- The fast subset MUST still catch cheap/common mistakes (compile errors,
  unit-test breaks via `--lib`, fmt, clippy).
- The CI-watch cadence is the mitigation and MUST be concrete (script + doc).
- **Routing**: both deliverables are SHELL scripts → **Apex (platform-
  architect) writes them in DELIVER, NOT the crafter** (CLAUDE.md: crafter
  writes only `crates/<name>/src/`; this feature touches no crate source).
- NO crate change, NO `Cargo.toml`/`Cargo.lock` change, NO new dependency, NO
  version bump; NEVER 1.0.0.
- Public-API / SemVer impact: **none** (Gates 2/3 see no surface change).

## Upstream changes (back-propagation to DISCUSS)

- **None required.** The DISCUSS user-stories, KPIs, and D1-D6 recommendations
  all hold unchanged; DESIGN adopted every DISCUSS recommendation. The only
  refinement DESIGN adds is the **structural measurement** (165 bins / 26
  fsync) that elevates D1 from "recommended" to "deterministically correct",
  and the explicit **measurement-honesty** note that the wall-clock <= 5 min
  bar is a DELIVER-confirmed measurement (Apex), not a DESIGN-asserted number.

## Out of scope (flagged, D6)

- Speeding the durability tests themselves in CI — future
  `faster-test-fsync-backend-v0` (e.g. an env-guarded fast-fsync test mode
  mirroring the ADR-0058 guard pattern). Flagged, not fixed here.
