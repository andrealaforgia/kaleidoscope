# Wave Decisions — wal-torn-tail-recovery-v0 (DEVOPS)

British English. No em dashes in body.

## Wave: DEVOPS (Apex, nw-platform-architect)

## Autonomous run note

Autonomous overnight run. All interactive decisions were made by the
agent per the standing instruction; nothing was deferred to the user.
This is a deliberately SLIM wave: a library-internal recovery-contract
change plus one new leaf crate, with no deploy surface. The headline is
that the existing five ADR-0005 gates already cover this feature, and
the only delta is one new per-crate mutation job that lands with the
crate in DELIVER. DEVOPS only; does NOT proceed into DISTILL.

## Inherited DEVOPS decisions (resolved, almost all carried)

| # | Decision | Resolution |
|---|---|---|
| D1 | Deployment target | N/A. Library plus storage-internal change; nothing is deployed. Kaleidoscope ships no crate; operators consume them. |
| D2 | Container orchestration | N/A. No service, no container. |
| D3 | CI/CD platform | Existing GitHub Actions per ADR-0005. Extend, do not recreate. |
| D4 | Existing infrastructure | Yes. The five-gate workflow and the per-crate gate-5 fan-out already exist; this wave adds exactly one job and edits nothing else. |
| D5 | Observability / logging | The feature emits one structured tracing WARN (`event="wal.recovery.torn_tail_dropped"`, fields `pillar`, `line`, `dropped_bytes`). CONFIRMED: the existing JSON-to-stderr tracing posture (the subscriber that already captures `event="health.startup.refused"` and `event="listener_bound"`, per read-api-tracing-subscriber-v0 / ADR-0009) covers it. No new metric, dashboard, or alert at v0. |
| D6 | Deployment strategy | N/A. No rollout; no rollback procedure applies to a library change. The "rollback" for a bad recovery routine is a fix-forward commit on trunk, consistent with the project's fix-forward posture. |
| D7 | Continuous learning | N/A this slice. |
| D8 | Git branching | Trunk-based, already established. Pure trunk-based, no required status checks, no enforce_admins: CI is feedback, not a merge gate. |
| D9 | Mutation testing strategy | Per-feature, 100% kill rate (ADR-0005 Gate 5; already in CLAUDE.md). CLAUDE.md is NOT re-written. |

## Key Decisions

### A1. The new crate's mutation gate lands atomically with the crate in DELIVER (CI decision: option (a))

`crates/wal-recovery` does not exist yet; the crafter creates it in
DELIVER. The new `gate-5-mutants-wal-recovery` job is specified here in
full and DELIVER adds it in the SAME commit that creates the crate.

Justification:

- **It is the disciplined, precedent-matching choice.** Every prior
  new-crate job (query-http-common per ADR-0054, self-observe,
  aperture-storage-sink, the query-api family) landed its
  `gate-5-mutants-<crate>` job in the crate's own DELIVER, not ahead of
  it. Atomicity keeps the workflow consistent with the workspace at
  every commit on trunk.
- **The persistent-red worry does not actually arise either way.** The
  job is an `--in-diff` path-filtered job. It opens with the standard
  empty-diff short-circuit: `git diff "$BASELINE" HEAD --
  'crates/wal-recovery/**'`, and if that diff is empty it prints a skip
  line and `exit 0`. On any push that does not touch
  `crates/wal-recovery/**` (which is every push until the crate lands),
  the job exits zero-second green BEFORE `cargo mutants -p wal-recovery`
  is ever invoked. So even option (b) could not produce a persistently
  red job against a non-existent crate. Option (a) is chosen for
  atomicity and precedent, not because (b) is unsafe; the empty-diff
  guard makes both safe, and (a) is simply the cleaner shape.
- **CI is feedback, not a gate.** A red CI job on Kaleidoscope main
  blocks no merge (no required status checks, no enforce_admins), but it
  is still noise to be avoided. Landing the job with the crate produces
  zero noise.

The crafter copies the block in A3 verbatim into `.github/workflows/ci.yml`
in the DELIVER commit that creates `crates/wal-recovery`, placing it
alongside the other `gate-5-mutants-*` jobs (the natural neighbour is
`gate-5-mutants-query-http-common`, its closest analogue: both are
shared leaf crates extracted by the rule of three).

### A2. The existing workspace-wide gates auto-cover the new crate; the opt-in gates deliberately do not

- **Gate 4 (`cargo deny --all-features check`)**: workspace-wide; walks
  the whole dependency graph. Covers `crates/wal-recovery` and its
  `serde` / `serde_json` / `tracing` dependency closure AUTOMATICALLY
  the moment it is a workspace member. The `tracing = "0.1"` edge is
  already in `Cargo.lock` (aperture and the read tier), so zero
  resolution churn and nothing new for Gate 4 to flag.
- **Gate 1 (`cargo test --workspace --all-targets --locked`)**:
  workspace-wide. Picks up the new crate's unit tests and the
  behavioural gold-test, plus the four pillars' AC tests, AUTOMATICALLY.
  No edit to the Gate 1 invocation is needed.
- **Gate 2 (`cargo public-api`) and Gate 3 (`cargo semver-checks`)**:
  these are opt-in per-package (explicit `-p` / `--package` lists). They
  do NOT auto-cover a new crate. wal-recovery is deliberately NOT added
  to those lists: DESIGN did not request surface-lock graduation, and
  wal-recovery is a shared internal leaf consumed only by the pillars in
  this repo, not a downstream-consumer-facing library in the sense
  Spark / Sieve / Codex are. If a future slice wants its surface locked,
  that is a separate, deliberate graduation, not an omission to fix now.
  Note also that the store trait byte-identity AC-8 is enforced by Gate 2
  on the EXISTING graduated packages and by the unchanged trait
  signatures of lumen / ray / cinder / pulse, not by any wal-recovery
  graduation.
- **Gate 5 (`cargo mutants`)**: per-crate fan-out. This is the one place
  a new crate needs a new job. Hence A1 / A3.

Stated explicitly as requested: **yes, Gate 1 and Gate 4 cover the new
crate automatically once it exists; Gate 2 and Gate 3 do not (they are
opt-in and wal-recovery is intentionally not enrolled); Gate 5 needs the
one new job in A3.**

### A3. Exact `gate-5-mutants-wal-recovery` job block (mirrors gate-5-mutants-query-http-common)

DELIVER adds this verbatim to `.github/workflows/ci.yml`, alphabetically
or topically beside `gate-5-mutants-query-http-common`, in the commit
that creates `crates/wal-recovery`:

```yaml
  gate-5-mutants-wal-recovery:
    name: Gate 5 — cargo mutants (wal-recovery)
    runs-on: ubuntu-latest
    needs:
      - gate-2-public-api
      - gate-3-semver
    timeout-minutes: 30
    steps:
      - name: Check out repository
        uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2
        with:
          fetch-depth: 0

      - name: Install stable Rust toolchain
        uses: dtolnay/rust-toolchain@e97e2d8cc328f1b50210efc529dca0028893a2d9 # v1
        with:
          toolchain: stable

      - name: Cache Cargo registry, git index and target/ (wal-recovery)
        uses: actions/cache@27d5ce7f107fe9357f9df03efb73ab90386fccae # v5.0.5
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-mutants-wal-recovery-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-mutants-wal-recovery-
            ${{ runner.os }}-cargo-stable-

      - name: Install cargo-mutants (precompiled binary)
        uses: taiki-e/install-action@711e1c3275189d76dcc4d34ddea63bf96ac49090 # v2.76.0
        with:
          tool: cargo-mutants

      - name: cargo mutants (wal-recovery, in-diff)
        # Same --in-diff strategy and baseline cascade
        # (origin/main → HEAD~1 → full) as the query-http-common,
        # query-api, log-query-api, trace-query-api, lumen, ray,
        # cinder, pulse, harness, aperture, spark, sieve, and codex
        # jobs. An empty diff (commit does not touch
        # crates/wal-recovery/) short-circuits to a zero-second exit,
        # so this job is a green no-op on every push that does not
        # touch the crate (including, by construction, every push
        # before the crate is created — though by ADR-0059 DEVOPS A1
        # this job lands in the SAME commit that creates the crate).
        #
        # Per wal-torn-tail-recovery-v0 DEVOPS (ADR-0059), this single
        # job covers every src file in the new shared leaf crate via
        # path-filtered --in-diff. wal-recovery is, like
        # query-http-common, a shared leaf extracted by the rule of
        # three, so this job is modelled on
        # gate-5-mutants-query-http-common. Primary mutation targets
        # (ADR-0059 Verification): the three guard conditions
        # (is_last_line boundary ==/!=, ends_with_newline true/false,
        # the parse-failed arm), the tracing::warn! emission (must not
        # be deletable without a surviving test), and the line /
        # dropped_bytes field values (off-by-one on the line number,
        # wrong byte count must be killed). Target kill rate: 100%
        # (K4 / ADR-0005 Gate 5; CLAUDE.md per-feature 100%).
        run: |
          DIFF_FILE=$(mktemp)
          BASELINE=""
          if git rev-parse --verify origin/main >/dev/null 2>&1 && \
             [ "$(git rev-parse origin/main)" != "$(git rev-parse HEAD)" ]; then
            BASELINE="origin/main"
          elif git rev-parse --verify HEAD~1 >/dev/null 2>&1; then
            BASELINE="HEAD~1"
          fi

          if [ -n "$BASELINE" ]; then
            git diff "$BASELINE" HEAD -- 'crates/wal-recovery/**' > "$DIFF_FILE"
            if [ ! -s "$DIFF_FILE" ]; then
              echo "No wal-recovery-touching changes vs $BASELINE; skipping mutation testing."
              exit 0
            fi
            echo "--- wal-recovery diff vs $BASELINE (head) ---"
            head -40 "$DIFF_FILE"
            echo "--- (truncated) ---"
            cargo mutants \
              --package wal-recovery \
              --in-diff "$DIFF_FILE" \
              --no-shuffle \
              --jobs 2
          else
            echo "No baseline available; running full mutation suite."
            cargo mutants \
              --package wal-recovery \
              --no-shuffle \
              --jobs 2
          fi

      - name: Upload mutants.out artefact (wal-recovery)
        if: success() || failure()
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        with:
          name: mutants-out-wal-recovery
          path: mutants.out/
          retention-days: 30
```

Note on the four pillars' own gate-5 jobs: lumen, ray, cinder, and pulse
already have `gate-5-mutants-<pillar>` jobs with `--in-diff` path
filters. The call-site edits in each pillar's `file_backed.rs` are
inside `crates/<pillar>/**`, so each pillar's existing job mutates its
own changed call-site lines automatically when DELIVER lands. The shared
guard logic is mutation-killed once at the wal-recovery site (the
ADR-0054 single-site benefit), and each pillar's job kills its own
thin-call-site mutants. No edit to the four pillar jobs is required.

### A4. The AST structural pre-commit check (ADR-0059 Decision 8 layer b): tool deferred to DELIVER, correctly

ADR-0059's Verification section specifies a structural layer: an AST
pre-commit check asserting each in-scope pillar's `open` calls
`wal_recovery::replay_wal_tolerating_torn_tail` and retains no inline
`serde_json::from_str(&line) ... PersistenceFailed` replay loop. The ADR
DELIBERATELY defers the TOOL choice for that check to DELIVER (the ADR
rejected `import-linter` as import-graph-only and left the concrete AST
hook to the crafter). DEVOPS does NOT pre-empt that choice. This wave
records only that:

- the structural check, when the crafter wires it, is a LOCAL
  pre-commit-stage concern (it belongs in `scripts/hooks/pre-commit` or
  a hook it calls), consistent with the Local Quality Gates posture; and
- it is feedback, not a merge gate, in keeping with the pure
  trunk-based, no-required-checks posture. If the crafter prefers to
  also surface it in CI, that is a fast cargo-based or grep/AST step,
  not a new heavyweight gate.

DEVOPS leaves the tool selection to DELIVER as the ADR intends and adds
no infrastructure for it in this wave.

## Infrastructure Summary

| Item | Status |
|---|---|
| Deploy target | None (library / storage-internal change) |
| New container / orchestration | None |
| CI/CD platform | Existing GitHub Actions (ADR-0005), extended by exactly one job |
| Gate 4 (cargo deny) | Covers new crate automatically (workspace-wide) |
| Gate 1 (cargo test --workspace) | Covers new crate automatically (workspace-wide) |
| Gate 2 (cargo public-api) | Unchanged; wal-recovery deliberately not enrolled |
| Gate 3 (cargo semver-checks) | Unchanged; wal-recovery deliberately not enrolled |
| Gate 5 (cargo mutants) | One NEW job: `gate-5-mutants-wal-recovery` (A3), lands with the crate in DELIVER, 100% kill |
| Local pre-commit hook | No edit required (cargo test --workspace and clippy --all-targets already cover the new crate) |
| Local pre-push hook | No edit required (Gate 2/3 package list unchanged) |
| Observability | One structured WARN on the existing tracing stream; no new metric / dashboard / alert |
| AST structural check | Tool deferred to DELIVER per ADR-0059; local pre-commit-stage feedback when wired |

## Constraints Established

- **CI is feedback, not a merge gate** (pure trunk-based, no required
  status checks, no enforce_admins). The new job blocks no merge; it
  exists to give the 100% kill-rate signal.
- **The new gate-5 job MUST land in the same DELIVER commit that creates
  `crates/wal-recovery`** (A1). Adding it earlier is unnecessary (the
  empty-diff guard would make it a green no-op anyway) and adding it
  later would leave a window where the crate's guard logic is unmutated.
- **100% kill rate** on `crates/wal-recovery` per ADR-0005 Gate 5 and
  CLAUDE.md. Primary targets are the three guard conditions, the
  `tracing::warn!` emission, and the `line` / `dropped_bytes` field
  values (ADR-0059 Verification).
- **No new observability infrastructure.** The WARN rides the existing
  structured tracing-to-stderr stream. Any alerting on torn-tail
  frequency is operator policy, not a feature requirement
  (outcome-kpis.md).
- **wal-recovery is not graduated to Gate 2 / Gate 3** in this wave;
  surface-lock graduation is a separate deliberate decision if ever
  wanted.

## Upstream Changes

- One new CI job (`gate-5-mutants-wal-recovery`, A3) added to
  `.github/workflows/ci.yml` by DELIVER. This is the only workflow edit
  this feature requires.
- No edit to `gate-1-test`, `gate-4-deny`, `gate-2-public-api`,
  `gate-3-semver`, the four pillar gate-5 jobs, or either local hook.
- No CLAUDE.md change (the per-feature 100% mutation strategy is already
  recorded there).
- `crates/wal-recovery` becomes a new `Cargo.toml` workspace member
  (DELIVER), pulling `serde` / `serde_json` / `tracing`, all already in
  `Cargo.lock`. Gate 4 revalidates the graph automatically.

## Production readiness (slim, library framing)

Most of the standard production-readiness checklist is N/A for a
non-deployed library. The applicable items:

- [x] Acceptance criteria covered by tests (AC-1..AC-10 via Gate 1
      workspace tests and the behavioural gold-test; verified at DELIVER).
- [x] Mutation gate at 100% kill (A3, ADR-0005 Gate 5).
- [x] Static analysis (clippy --all-targets -D warnings) covers the new
      crate via the existing pre-commit hook and CI.
- [x] Logging structured and searchable (the WARN rides the existing
      structured tracing stream; D5 confirmed).
- [x] No trait-signature regression (Gate 2 on graduated packages plus
      unchanged pillar trait signatures; AC-8).
- [n/a] Deployment / rollback procedure (nothing is deployed; trunk
      fix-forward is the correction path).
- [n/a] On-call training, runbook, canary, smoke tests (no service).

## Handoff

- **To DELIVER (crafter)**: add the A3 job block verbatim to
  `.github/workflows/ci.yml` in the same commit that creates
  `crates/wal-recovery`; do not touch the other gates or the hooks; wire
  the ADR-0059 Decision 8 layer-b AST structural check with the tool of
  your choice as a local pre-commit-stage feedback step (ADR defers the
  tool to you); keep the pre-commit and pre-push hooks green.
- **Does NOT proceed into DISTILL.** This is a DEVOPS-only wave.

## Peer review

- Intended reviewer: `nw-platform-architect-reviewer`. See the outcome
  below.

### Peer review outcome

Apex's subagent session dropped (socket closed) before it could append
the review outcome, so `nw-platform-architect-reviewer` was run at the
top level by the orchestrator against this file, `environments.yaml`,
and the live `.github/workflows/ci.yml`.

Verdict: **APPROVED**. 0 blocking issues. The reviewer independently
verified against the real workflow that (a) the A3 job block faithfully
mirrors `gate-5-mutants-query-http-common`, (b) all five action-pinning
SHAs match the rest of ci.yml (checkout v6.0.2, rust-toolchain v1, cache
v5.0.5, install-action v2.76.0, upload-artifact v7.0.1), (c) the stable
1.85 / nightly-2026-04-15 pins are consistent, (d) Gate 1 and Gate 4 are
workspace-wide and auto-cover the new crate, and (e) Gate 2 and Gate 3
are opt-in package lists that correctly do NOT enrol wal-recovery.

Three forward-looking findings, all to be honoured at DELIVER (none
block DEVOPS, none are defects in this wave):

1. CRITICAL (handoff, not a defect): the `gate-5-mutants-wal-recovery`
   job MUST land in the SAME commit that creates `crates/wal-recovery`.
   This is exactly the A1 constraint; the DELIVER peer review must
   reject any split. The empty-diff guard makes a split non-dangerous
   (green no-op) but it would break the stated atomicity precedent.
2. MEDIUM: before DELIVER lands, confirm the local pre-commit hook runs
   `cargo test --workspace` without a package-exclusion or explicit
   `-p` list that would silently omit wal-recovery. If it names
   packages or uses `--exclude`, enrol wal-recovery.
3. LOW: optional spot-check that the self-observe and
   aperture-storage-sink gate-5 jobs historically landed with their
   crate, to keep the A1 precedent claim exact.

These three are carried into the DELIVER handoff above.
