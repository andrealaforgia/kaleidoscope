# Peer Review — cinder-wal-error-surfacing-v0 (DISTILL)

Reviewer: acceptance-designer in review mode (Quinn), applying
`nw-ad-critique-dimensions` (Dimensions 1-9). No standalone
`nw-acceptance-designer-reviewer` agent is installed in this environment
(`~/.claude/agents/` holds only the `alf-*` reviewers), so the critique-dimensions
skill is applied directly as the review gate — the skill IS the reviewer's
rubric. Date: 2026-06-05.

## nWave-order reminder (honoured)

DISTILL runs BEFORE DELIVER. Production code implementing the fix does NOT exist
yet — `place`/`evaluate_at` still return `()`/`usize`, the WAL error is still
swallowed. Acceptance tests that are `#[ignore]`d and/or behaviourally-RED against
the present (buggy/old-signature) code are the EXPECTED and CORRECT state, NOT a
rejection reason. The review evaluates scenario QUALITY, business-language purity,
error coverage, the driving-adapter subprocess scenario, and ESPECIALLY
falsifiability — not the non-existence of the fix.

## Verdict iteration 1

```yaml
review_id: "accept_rev_20260605_cinder_wal_error_surfacing"
reviewer: "acceptance-designer (review mode, critique-dimensions rubric)"

strengths:
  - "Falsifiability is genuine and PROVEN BY RUNNING: each of the 5 failure
     scenarios FAILS on today's swallow bug with a concrete left/right diff
     (e.g. cinder overwrite: got Some(Cold), want Some(Hot)); the 4 healthy
     negative controls PASS. A swallow-bug test that could not fail is exactly
     what ADR-0060 §1 / ADR-0049 warned against — this design structurally avoids it."
  - "The grounded substrate correction (DWD-2): the designer READ append_wal and
     found that flush precedes fsync, so 'absent on reopen' is NOT a reliable
     discriminator for a failing-fsync substrate; the load-bearing assertion was
     correctly moved to memory-untouched on the LIVE handle. This is the kind of
     ground-in-code rigour the falsifiability mandate demands."
  - "Driving-adapter mandate satisfied with a REAL subprocess (Command::output) on
     the actual kaleidoscope-cli binary — happy path compiled+passing, failure path
     ignored — exercising exit code + stdout/stderr, not just the lib fn."
  - "No fake scaffold: a Result-shim was explicitly considered and REJECTED as
     Fixture-Theater-prone (DWD-5); the intended-Result specs are docs-only so the
     workspace build stays green (verified: cargo build --workspace --all-targets
     finished clean)."

issues_identified:
  happy_path_bias:
    - issue: "none — error-path ratio is 60% (6/10), exceeds the 40% mandate;
              every store op has both a failing and a healthy (negative-control)
              scenario."
      severity: "none"
  gwt_format:
    - issue: "Scenarios are expressed as Given/When/Then in the test doc-comments
              with single-When structure; the Rust fn bodies follow Given (seed) /
              When (the failing op) / Then (the invariant assertion). No multi-When."
      severity: "none"
  business_language:
    - issue: "grep for database/http/json/api/status-code in scenario Gherkin
              returned nothing; titles read as operator outcomes (a failed
              overwrite preserves the prior durable placement)."
      severity: "none"
  coverage_gaps:
    - issue: "US-01→R1-*/WS-*, US-02→WS-A/WS-B (D2), US-03→R2-*, US-04→R3-* — every
              story has >=1 scenario; D2/D3/D4 locked branches each encoded."
      severity: "none"
  walking_skeleton_centricity:
    - issue: "WS-A title 'Priya places a tier ... and reads it back durable' is a
              user goal, not a layer-connectivity description; Then asserts operator
              observations (exit 0, the placement line, the tier readable across a
              reopen), not internal side effects."
      severity: "none"
  observable_behavior:
    - issue: "Every Then asserts a return value (get_tier, depth, evaluate_at) or a
              real process outcome (exit code, stdout/stderr) — no private-field or
              mock-call assertions."
      severity: "none"
  traceability_coverage:
    - issue: "Story Check A passes (all 4 stories mapped). Environment Check B: the
              feature's environments are clean / with-pre-commit / ci (DEVOPS slim);
              all three run the same `cargo test` so the compiled RED + WS-A run in
              each. The real-read-only-WAL subprocess (WS-B) is #[ignore]d so it does
              not flake the hook (C-DEVOPS-3 determinism)."
      severity: "none"
  walking_skeleton_boundary:
    - issue: "WS strategy C (real-local-IO) declared in wave-decisions.md DWD-1; the
              WS implementation matches (real temp-dir WAL + real subprocess + failing
              FsyncBackend); litmus 'delete the real adapter, would WS still pass?' =
              NO (durability asserted across a real reopen). No @in-memory on any WS."
      severity: "none"

falsifiability_review:
  - check: "Each failure scenario asserts MORE than 'returns Err' (the trivial pass)."
    result: "PASS — the compiled RED tests assert the memory-untouched / queue-state-
             consistent invariant on the live handle, which the swallow bug violates
             (proven by the FAILED run output). The intended specs additionally assert
             Err(PersistenceFailed) — both halves are present."
  - check: "The failing substrate is grounded, not guessed."
    result: "PASS — confirmed NO failing FsyncBackend exists in wal-recovery today;
             a test-local FailingFsyncBackend (fsync_file -> io::Error) is defined
             against the public FsyncBackend trait; the brief's 'DELIVER may add a
             failing mode' is recorded as optional, not a blocker."
  - check: "Negative controls prove the surfacing change does not regress the green path."
    result: "PASS — 4 healthy scenarios compiled + PASS today."

priority_validation:
  - "Q1 (largest bottleneck addressed first): YES — the live cinder place path
     (US-01/US-02) is R1; the periodic sweep (US-03) R2; unwired sluice (US-04) R3,
     matching the story-map outcome priority. The walking skeleton is the live blast
     radius."
  - "Q2/Q3/Q4: scenario count is right-sized (10 compiled + 7 intended), no
     over-testing of combinations; the carpaccio cut-line (sluice = R3, separately
     taggable @uniformity) is preserved so cinder value is never gated on sluice."

mandate_compliance:
  CM-A: "Imports are crate-public surface only (cinder::, sluice::, kaleidoscope-cli
         binary via CARGO_BIN_EXE). Behaviour is driven through the TieringStore /
         Queue trait methods (driving ports) and the CLI binary (driving adapter);
         no internal-module imports (no cinder::file_backed::internal etc.)."
  CM-B: "Gherkin doc-comments use domain terms only (tenant, item, tier, persist,
         durable, dequeue, ack); grep for technical jargon in scenario text = empty."
  CM-C: "Scenarios validate complete operator journeys with business value: place +
         read-back-durable (WS-A), failed-overwrite-preserves-prior-value (R1-1),
         not isolated 'validator accepts input' shapes."

approval_status: "approved"

approval_rationale: >
  All 9 critique dimensions pass with zero blocker / high findings. Falsifiability —
  the explicit heart of this feature and the highest-risk dimension — is proven by
  running the suite (5 failure scenarios FAIL RED on the swallow bug with concrete
  diffs; 4 negative controls PASS). The driving-adapter subprocess mandate is met.
  RED-not-BROKEN is classified by RUNNING (the compiled tests fail behaviourally;
  the intended-Result specs are docs-only so the build stays green — verified). No
  production code was modified. Approved for handoff to DELIVER.
```

## Note on the WS-B substrate (recorded, not a finding)

WS-B uses a real read-only WAL file to make the binary's append fail with a genuine
`io::Error`. Depending on exactly where the append fails relative to store-open,
the binary may surface this as `CinderOpen` rather than `CinderPlace` — the OBSERVABLE
D2 contract the scenario asserts (non-zero exit, `persistence failed: io:` stderr,
nothing acked durable, not durable on a later read) holds either way. The exact
`cinder place:` stderr prefix is pinned additionally by the intended library-seam
spec (I-1), where the injected failing backend guarantees the failure lands in the
`place` append. DELIVER should confirm the prefix when un-ignoring WS-B; if the
read-only substrate proves to surface only at open, DELIVER may instead drive WS-B
through the `ingest` subcommand with the same read-only WAL, or assert the weaker
(still-correct) `persistence failed: io:` substring. This is a DELIVER refinement,
not a DISTILL blocker — the contract asserted is correct.
