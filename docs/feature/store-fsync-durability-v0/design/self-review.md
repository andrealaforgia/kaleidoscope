# DESIGN self-review — store-fsync-durability-v0

- **Reviewer**: Morgan (`nw-solution-architect`), structured self-review
  against `nw-sa-critique-dimensions`.
- **Date**: 2026-06-04
- **Why self-review**: the `nw-solution-architect-reviewer` Agent tool is
  not invocable from this subagent context (only Read/Write/Edit/Glob/Grep
  are available). Per the brief's fallback, a rigorous structured self-review
  against the SA dimensions is recorded here and an **independent top-level
  `nw-solution-architect-reviewer` run is recommended before DISTILL**.

```yaml
review_id: "arch_rev_store-fsync-durability-v0_selfreview_2026-06-04"
reviewer: "nw-solution-architect (self-review; independent run recommended)"
artifact: "docs/product/architecture/brief.md (store-fsync-durability-v0 section), docs/product/architecture/adr-0060-earned-trust-store-fsync-durability.md"
iteration: 1

strengths:
  - "The load-bearing correction (a SIGKILL cannot prove WAL fsync because the page cache survives the process death) is made explicit in ADR-0060 Decision 1, the brief, the For-Acceptance-Designer note, and upstream-changes.md — four reinforcing places, so DISTILL cannot miss it."
  - "Reuse Analysis is a 4-option table (copy-paste / consume-from-pulse / new-crate / extract-into-wal-recovery) with a decisive architectural fact: every pillar already depends on wal-recovery INWARD and not on pulse (verified at crates/lumen/Cargo.toml:25). EXTRACT is evidence-driven, not preference."
  - "No trait change (C1) is honoured by injecting FsyncBackend through an inherent open_with_fsync_backend constructor (mirrors pulse's existing one), not a trait member; Gate 2 cargo public-api enforces byte-identity."
  - "The two proving mechanisms map cleanly to KPIs: (a) process-kill -> K3 snapshot atomicity; (b) lying substrate -> K2/K4 wal fsync. Each AC is proven by the mechanism that can actually falsify it."
  - "atomic_write_snapshot procedure is POSIX-precise: same-dir temp (so rename is intra-filesystem atomic), fsync-tmp, rename, fsync-parent-dir. Closes the gap ADR-0049 §5 left open even in pulse."
  - "Earned-Trust three-layer enforcement (subtype/structural/behavioural) carried from ADR-0049/0059 with self-application; import-linter explicitly investigated and rejected with the standing reason."

issues_identified:
  architectural_bias:
    - issue: "Resume-driven / over-engineering check: does the feature add unjustified complexity (a new crate, microservices, trendy tech)?"
      severity: "n/a"
      location: "ADR-0060 Decision 4, alternatives C"
      recommendation: "PASS. No new crate is introduced — the existing wal-recovery leaf crate is broadened. New-crate option C was explicitly rejected. No services, no frameworks; the simplest factoring (reuse the crate the pillars already import) is chosen. Default modular-monolith / leaf-crate shape preserved."
    - issue: "Technology preference bias: is sync_all/per-record fsync chosen by preference?"
      severity: "low"
      location: "ADR-0060 Decision 3, fsync strategy alternative"
      recommendation: "PASS. sync_all over sync_data and per-record over batched are both inherited from ADR-0049 §4 with the same recorded rationale (WAL length metadata is part of the durability promise; correctness over capacity at v0). Not a fresh preference; a consistent lineage decision."
  decision_quality:
    - issue: "Does ADR-0060 have >=2 alternatives with rejection rationale for each load-bearing decision?"
      severity: "low"
      location: "ADR-0060 Alternatives considered"
      recommendation: "PASS. Shared-helper has 3 rejected alternatives (copy-paste, consume-from-pulse, new-crate); proving has 3 (single-SIGKILL, real-power-cut, fork-in-tokio); fsync strategy reuses ADR-0049's rejections. Each names a concrete For/Against."
    - issue: "Context completeness: business problem, constraints, quality attributes present?"
      severity: "none"
      location: "ADR-0060 Context"
      recommendation: "PASS. The two verified defects + the false-confidence root cause + the per-store code-location table + the C1..C8 constraints are all present and traced to DISCUSS verification."
  completeness_gaps:
    - issue: "Reliability/recoverability quality attribute (the whole point) addressed with a strategy, not just asserted?"
      severity: "none"
      location: "ADR-0060 Decisions 1-3; brief C4 + For-Acceptance-Designer"
      recommendation: "PASS. Recoverability is addressed by per-record fsync + atomic snapshot + torn-tail recovery (ADR-0059), each with an explicit proving mechanism and KPI."
    - issue: "Observability: are the failure/recovery signals specified?"
      severity: "none"
      location: "ADR-0060 Decision 3 + Consequences"
      recommendation: "PASS. Reuses event=health.startup.refused (substrate=<descriptor>) and event=wal.recovery.torn_tail_dropped verbatim; no new event/metric/dashboard (C4)."
    - issue: "Performance: per-record sync_all throughput cost acknowledged with a strategy?"
      severity: "low"
      location: "ADR-0060 Consequences (Negative)"
      recommendation: "PASS (acknowledged). The cost is recorded; batched fsync is a documented ADR-0049 §4 alt-B successor. Acceptable at v0 (correctness over capacity, no production load). Not a blocker; flagged for a successor."
  implementation_feasibility:
    - issue: "Testability: can each AC be tested in isolation through a port/seam?"
      severity: "none"
      location: "ADR-0060 Decision 3 + brief For-Acceptance-Designer"
      recommendation: "PASS. Mechanism (b) is in-suite and deterministic via the open_with_fsync_backend seam + LyingFsyncBackend; mechanism (a) is a child-process kill via std::process::Command. Both avoid fork-in-tokio (C5) and timing assertions (C6)."
    - issue: "Team capability / paradigm match: does the design fit the Rust-idiomatic CLAUDE.md paradigm?"
      severity: "none"
      location: "ADR-0060 Decision 4 seam"
      recommendation: "PASS. Free function atomic_write_snapshot + a trait only where polymorphism is genuinely needed (FsyncBackend, to inject the lying double); no class hierarchies, no dyn where monomorphisation suffices. Matches the wal-recovery precedent."
    - issue: "FsyncBackend crate move risk to the gateway import path."
      severity: "medium"
      location: "ADR-0060 Decision 4; wave-decisions risk register"
      recommendation: "MITIGATED. pulse re-exports the moved family so pulse::{fsync_probe, FsyncBackend, ...} resolves unchanged; Gate 2 + cargo check catch any drift. Crafter must keep the re-export shim; flagged in the ADR and risk register."
  priority_validation:
    q1_largest_bottleneck:
      evidence: "The #1 four-quadrants implementer-backlog defect; two defects verified in code per-store by Luna (wave-decisions.md DISCUSS). North star K1 0/7 -> 7/7."
      assessment: "YES"
    q2_simple_alternatives:
      assessment: "ADEQUATE — copy-paste (simplest, rejected for sevenfold drift), single-SIGKILL test (simplest proving, rejected as proving nothing), consume-from-pulse (no code move, rejected for layering inversion). Three simpler options considered and rejected with rationale."
    q3_constraint_prioritization:
      assessment: "CORRECT — the wal-fsync gap (every acked write) is the larger silent-loss surface and lands on six stores; the snapshot gap (mid-snapshot window) is narrower but total-loss and lands on all seven including pulse. Both addressed; rollout riskiest-assumption-first."
    q4_data_justified:
      assessment: "JUSTIFIED — per-store code locations verified (DISCUSS table); the page-cache reasoning is verified black-box-verifier reasoning; the dependency fact (lumen depends on wal-recovery not pulse) verified at Cargo.toml:25."

approval_status: "approved (self-review); independent reviewer run recommended"
critical_issues_count: 0
high_issues_count: 0
medium_issues_count: 1   # FsyncBackend crate-move import risk — mitigated by re-export shim
low_issues_count: 4      # all PASS-with-note, no action required
```

## Residual recommendation

One MEDIUM item (the FsyncBackend crate move) is mitigated by the pulse
re-export shim and caught by Gate 2 + `cargo check`; no design change
needed, but the crafter MUST preserve the re-export. No critical or high
issues. An independent top-level `nw-solution-architect-reviewer` run is
recommended before DISTILL to discharge the self-review bias the methodology
exists to reduce; this DESIGN does NOT proceed into DEVOPS/DISTILL.
