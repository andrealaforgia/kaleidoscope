# Self-Review — store-fsync-durability-v0 (DISTILL)

Structured self-review against the acceptance-designer critique dimensions
(1–9). The `nw-acceptance-designer-reviewer` (Sentinel) Agent is NOT invocable
from this subagent context; this rigorous self-review stands in, and an
independent top-level reviewer run is RECOMMENDED before DELIVER.

```yaml
review_id: "accept_self_rev_store-fsync-durability-v0"
reviewer: "acceptance-designer (self-review mode)"

strengths:
  - "Two-mechanism split honoured exactly: AC-snapshot-atomicity proven by a
     real out-of-process SIGKILL (mechanism a), AC-wal-fsync proven by an
     in-suite LyingFsyncBackend (mechanism b). A SIGKILL is never the sole
     proof of a wal-fsync AC — the load-bearing prohibition is respected."
  - "67.5% negative/edge coverage (27/40): substrate-refusal, lying-substrate
     discard/truncate, the @property any-point invariant, plus empty-store and
     in-flight boundaries."
  - "Strategy C with real child processes and real files; zero InMemory in the
     durability path. WS litmus 'delete the real adapter' fails — it reads real
     on-disk WAL/snapshot back."
  - "Whole suite stays GREEN under the exact pre-commit command; RED-not-BROKEN
     via panicking __SCAFFOLD__ seams; no --no-verify needed."
  - "The brief's prohibition is honoured: zero direct entries through
     wal_recovery::atomic_write_snapshot / fsync_probe; all entry is through
     store driving ports + the crash-target process."

issues_identified:
  happy_path_bias:
    - issue: "none — negative/edge ratio is 67.5%, far above 40%."
      severity: "none"
  gwt_format:
    - issue: "Each scenario is one trigger → one event → one observable
       outcome; single When per scenario (the kill, or the ingest-then-reopen).
       Rust test functions, not .feature files (the repo convention by
       example — lumen v1_slice_03, pulse v1_slice_03); GWT lives in the
       module/inline docs and the persona-framed names."
      severity: "none"
  business_language:
    - issue: "Durability vocabulary (SIGKILL, fsync, WAL, snapshot) is the
       ubiquitous language of this domain, carried verbatim from the DISCUSS
       stories and ADR-0060 — not leaked jargon. No HTTP codes / JSON-shape /
       DB terms in scenario intent."
      severity: "none"
  coverage_gaps:
    - issue: "All seven stories US-01..US-07 have ≥1 scenario tagged @US-0N;
       every named AC (snapshot-atomicity, wal-fsync, substrate-refusal,
       recovery-regression) maps to ≥1 test. See ac-coverage.md."
      severity: "none"
  walking_skeleton_centricity:
    - issue: "lumen WS title is a user goal ('acked log survives a mid-snapshot
       crash and is queryable after restart'), Then is a user observation (the
       record is in the query result), Priya can confirm it."
      severity: "none"
  observable_behavior:
    - issue: "Every Then asserts a driving-port return value (query/get_trace/
       get_tier/dequeue/load_all) or an observable process outcome (stderr
       event, non-zero exit). No private-field / call-count assertions. The one
       file-path mention is a no-op comment, not an assertion."
      severity: "none"
  traceability_coverage:
    - check_A_story_to_scenario: "PASS — US-01..US-07 each covered."
    - check_B_environment_to_scenario: "clean + ci (environments.yaml). Both
       run the identical `cargo test --workspace --all-targets --locked`; the
       WS Given establishes a real tmp pillar root + a real child process,
       which is what both environments exercise. No deploy/staging env exists
       (slim wave)."
      severity: "none"
  walking_skeleton_boundary:
    - issue: "Strategy C declared in wave-decisions.md (DWD-1). WS uses real
       adapters for all mechanisms; no @in-memory on any WS scenario. Every
       store's real file-backed adapter has a @real-io @adapter-integration
       scenario."
      severity: "none"

residual_risks_for_DELIVER:
  - "Mechanism (a) timing: DELIVER must implement the crash-target so the kill
     can land mid-snapshot; the any-point invariant assertion is robust to
     this, but the child must actually be IN the snapshot loop (print
     CRASH_TARGET_READY only after the first ack + before looping snapshots)."
  - "pulse re-export move: DELIVER re-points pulse's FsyncBackend re-export to
     wal_recovery without a Gate-2 public-API diff (the symbols stay
     byte-identical). If Gate 2 flags a diff, it is a real defect (DEVOPS)."
  - "strata/sluice stay on parse-or-die recovery until the ADR-0059 §5
     follow-up; their AC-recovery-regression is framed as the acked-prefix
     outcome (no torn_tail_dropped event asserted for those two)."

approval_status: "conditionally_approved (self-review); independent top-level
  nw-acceptance-designer-reviewer run RECOMMENDED before DELIVER"
```

## Definition of Done (DISTILL→DELIVER gate)

1. [x] All acceptance scenarios written; step logic delegates to store
       driving ports / the crash-target process (no business logic in tests).
2. [x] Test pyramid: acceptance suites authored; unit/integration locations
       for the shared seam noted (the wal-recovery gold-test is DELIVER's).
3. [~] Peer review: rigorous self-review done; independent reviewer run
       RECOMMENDED (Sentinel not invocable from this subagent context).
4. [x] Tests run in CI/CD: they run under `cargo test --workspace
       --all-targets --locked` in gate-1-test and the local hook (all ignored,
       suite GREEN).
5. [x] Story demonstrable: the lumen WS is demo-able to a stakeholder (Priya
       restarts; her acked log is present; the store opened cleanly).
```
