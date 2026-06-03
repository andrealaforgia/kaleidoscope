# Definition of Ready — Validation & Peer Review — store-fsync-durability-v0

- **Wave**: DISCUSS
- **Analyst**: Luna (nw-product-owner)
- **Date**: 2026-06-03
- **Stories**: US-01..US-07 (`user-stories.md`)

## Dimension 0 — Elevator Pitch Test (BLOCKING, checked first)

Every story has an `### Elevator Pitch` with Before / After / Decision
enabled. Checked invariants:

| Story | 3 lines present | Real entry point (After) | Concrete output (sees) | Real decision (Decision enabled) | Verdict |
|-------|-----------------|--------------------------|------------------------|----------------------------------|---------|
| US-01 lumen | Yes | `GET /api/v1/logs` (curl) | response body contains the acked log record | trust 200 OK as durable; skip dead-letter replay | PASS |
| US-02 ray | Yes | `GET /api/v1/traces` (curl) | response contains the acked span | trust trace durable; investigate from collector data | PASS |
| US-03 strata | Yes | collector restart → strata `open()` + in-process query | recovered state contains the acked profile | do not quarantine strata as suspect after crash | PASS |
| US-04 cinder | Yes | collector restart → cinder `open()` + recovered ledger | recovered ledger contains the acked migration | trust tiering ledger intact; skip manual reconcile | PASS |
| US-05 sluice | Yes | collector restart → sluice `open()` + dequeue | recovered queue contains the acked enqueue | trust queue did not drop work; skip re-driving producers | PASS |
| US-06 beacon state | Yes | collector restart → store `open()` + recovered state | recovered state has the rule in its acked transition | trust rules resume in acked state; skip manual re-arm | PASS |
| US-07 pulse snapshot | Yes | `GET /api/v1/metrics` (curl) | response serves the acked series after mid-snapshot crash | trust metrics survive a snapshot-time crash | PASS |

**Internal-store entry-point note (US-03/05/06):** these stores have no
HTTP read path. Their user-invocable entry point is the **operator
restarting the collector** (a real, operator-initiated action) and the
store's `open()` succeeding with the acked state present — an
operator-observable outcome (the collector comes back up vs refuses to
start / loses data). This is NOT an internal service-function-only pitch:
the operator invokes the restart and observes the recovery outcome. The
"sees" clause names recovered state content, not "tests pass" or "data is
persisted" in the abstract. **No BLOCKING trip.**

**Slice-level value check:** every slice contains a user-visible story
(no slice is all-`@infrastructure`). The walking-skeleton slice (US-01)
is end-user-visible via `GET /api/v1/logs`. **PASS.**

Dimension 0 verdict: **PASS** (no BLOCKING issue).

---

## DoR 9-Item Checklist

### Story: US-01 (lumen, walking skeleton)

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1. Problem statement clear, domain language | PASS | "lumen acknowledges a write as durable after only `BufWriter::flush()` (`file_backed.rs:281`), leaving bytes in the page cache — lost on power failure." Operator-domain language (acked, durable, power loss, restart). |
| 2. User/persona with specific characteristics | PASS | Priya Nair, on-call SRE, self-hosted collector on bare-metal with local NVMe, paged at 03:00 by a PDU trip. |
| 3. 3+ domain examples with real data | PASS | 3 examples with real data: `connection pool exhausted` line, tenant `acme`, 10,001st-batch snapshot, torn-tail with `wal.recovery.torn_tail_dropped pillar="lumen"`. |
| 4. UAT in Given/When/Then (3-7 scenarios) | PASS | 5 scenarios (acked survives crash; opens after mid-snapshot crash; torn tail dropped; lying-substrate refusal; graceful-restart regression). |
| 5. AC derived from UAT | PASS | 7 AC each trace to a scenario incl. the kill-9 proving test (AC #6). |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | One store, 5 scenarios, demonstrable in one session. Walking skeleton is deliberately thin. |
| 7. Technical notes: constraints/dependencies | PASS | Reuses `FsyncBackend` (ADR-0049 §6/§8); atomic-snapshot gap; pairs with ADR-0059; probe wiring at gateway. |
| 8. Dependencies resolved or tracked | PASS | ADR-0049 (landed), ADR-0059 lumen recovery (landed). Both resolved. |
| 9. Outcome KPIs with measurable targets | PASS | K1/K2/K3 (0% → 100% provable), measured by the out-of-process kill-9 proving test. |

**US-01 DoR Status: PASSED**

### Story: US-02 (ray)

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1. Problem statement | PASS | "ray acknowledges durability after only `BufWriter::flush()` (`:392`)... snapshot with `File::create` (`:171`)." |
| 2. Persona | PASS | Priya (shared persona, trace-investigation context). |
| 3. 3+ examples | PASS | `trace_id=4bf92f` `POST /checkout`; mid-snapshot; 5-of-6 spans boundary. |
| 4. UAT (3-7) | PASS | 4 scenarios. |
| 5. AC from UAT | PASS | 6 AC incl. proving test. |
| 6. Right-sized | PASS | One store, 4 scenarios. |
| 7. Technical notes | PASS | Reuses US-01 seam; ADR-0059 ray recovery landed. |
| 8. Dependencies | PASS | US-01, ADR-0059 (landed). |
| 9. KPIs | PASS | K1/K2/K3, kill-9 proving test on `GET /api/v1/traces`. |

**US-02 DoR Status: PASSED**

### Story: US-03 (strata)

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1. Problem statement | PASS | `wal.flush()`-only (`:333`), non-atomic snapshot (`:170`). |
| 2. Persona | PASS | Priya, profile-pillar recovery context. |
| 3. 3+ examples | PASS | `payment-svc` profile; mid-snapshot; empty-store-pre-write boundary. |
| 4. UAT (3-7) | PASS | 4 scenarios. |
| 5. AC from UAT | PASS | 6 AC incl. proving test. |
| 6. Right-sized | PASS | One store, 4 scenarios. |
| 7. Technical notes | PASS | strata not in ADR-0059 slice — flagged for DESIGN. |
| 8. Dependencies | PASS (tracked) | US-01; strata torn-tail recovery flagged as a DESIGN reconciliation item, tracked not blocking. |
| 9. KPIs | PASS | K1/K2/K3. |

**US-03 DoR Status: PASSED**

### Story: US-04 (cinder)

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1. Problem statement | PASS | `wal.flush()`-only (`:383`), non-atomic snapshot (`:207`); over-claimed doc corrected under ADR-0059 §6. |
| 2. Persona | PASS | Priya, tiering-ledger context. |
| 3. 3+ examples | PASS | `blk-7781` hot→warm; mid-snapshot; torn tail. |
| 4. UAT (3-7) | PASS | 4 scenarios. |
| 5. AC from UAT | PASS | 6 AC incl. proving test. |
| 6. Right-sized | PASS | One store, 4 scenarios. |
| 7. Technical notes | PASS | ADR-0059 cinder recovery landed. |
| 8. Dependencies | PASS | US-01, ADR-0059 (landed). |
| 9. KPIs | PASS | K1/K2/K3. |

**US-04 DoR Status: PASSED**

### Story: US-05 (sluice)

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1. Problem statement | PASS | `wal.flush()`-only (`:391`), non-atomic snapshot (`:243`); fallible `apply_record` noted. |
| 2. Persona | PASS | Priya, durable-queue context. |
| 3. 3+ examples | PASS | `job-5521` enqueue; mid-snapshot; in-flight item boundary. |
| 4. UAT (3-7) | PASS | 4 scenarios. |
| 5. AC from UAT | PASS | 6 AC incl. proving test. |
| 6. Right-sized | PASS | One store, 4 scenarios. |
| 7. Technical notes | PASS | Fallible-apply seam (ADR-0059 §5) flagged for DESIGN. |
| 8. Dependencies | PASS (tracked) | US-01; fallible-apply seam tracked. |
| 9. KPIs | PASS | K1/K2/K3. |

**US-05 DoR Status: PASSED**

### Story: US-06 (beacon state_store)

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1. Problem statement | PASS | `wal.flush()`-only (`:334`), non-atomic snapshot (`:259`). |
| 2. Persona | PASS | Priya, alerting-rule-state context. |
| 3. 3+ examples | PASS | `r-payment-latency`→`firing`; mid-snapshot; torn tail. |
| 4. UAT (3-7) | PASS | 4 scenarios. |
| 5. AC from UAT | PASS | 6 AC incl. proving test. |
| 6. Right-sized | PASS | One store, 4 scenarios. |
| 7. Technical notes | PASS | ADR-0040 seam; not in ADR-0059 slice — flagged for DESIGN. |
| 8. Dependencies | PASS (tracked) | US-01; ADR-0040; torn-tail extension tracked. |
| 9. KPIs | PASS | K1/K2/K3. |

**US-06 DoR Status: PASSED**

### Story: US-07 (pulse snapshot atomicity)

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1. Problem statement | PASS | pulse WAL durable (ADR-0049) but snapshot non-atomic (`File::create` at `:257`); mid-snapshot crash = total loss. |
| 2. Persona | PASS | Priya, metrics-pillar context. |
| 3. 3+ examples | PASS | 50,000-point mid-snapshot; temp-never-canonical; crash-at-rename boundary. |
| 4. UAT (3-7) | PASS | 4 scenarios (incl. post-snapshot WAL-durability regression). |
| 5. AC from UAT | PASS | 6 AC incl. proving test. |
| 6. Right-sized | PASS | Snapshot-only, 4 scenarios, smallest slice. |
| 7. Technical notes | PASS | Reuses pulse `FsyncBackend.fsync_file`/`fsync_dir` (landed); WAL unchanged. |
| 8. Dependencies | PASS | US-01, ADR-0049 (landed). |
| 9. KPIs | PASS | K1/K3 (mid-snapshot opens 0% → 100%). |

**US-07 DoR Status: PASSED**

### DoR Summary

**All 7 stories: PASSED (9/9 each).** Two stories carry tracked-not-blocking
DESIGN reconciliation items (US-03 strata, US-06 beacon — torn-tail
recovery extension; US-05 sluice — fallible-apply seam). These are
dependencies correctly TRACKED per DoR item 8, not unresolved blockers,
because the fsync + atomic-snapshot work in each slice is independent of
the torn-tail recovery extension.

---

## Peer Review (nw-product-owner-reviewer mode)

Persona shift: independent requirements reviewer. Dimensions applied per
`nw-po-review-dimensions`. Two iterations max.

### Iteration 1

```yaml
review_id: "req_rev_20260603_storefsync_01"
reviewer: "product-owner (review mode)"
artifact: "docs/feature/store-fsync-durability-v0/discuss/user-stories.md"
iteration: 1

strengths:
  - "Dimension 0 PASS across all 7 stories: every Elevator Pitch names a real operator entry point (HTTP read path for lumen/ray/pulse; collector-restart+open for internal stores) and a concrete observable output."
  - "Problem statements are grounded in verified code locations (file:line), not assumed — the durability gap was confirmed by archaeology on 2026-06-03."
  - "Strong lineage: every story traces to ADR-0049 §8's named successor scope and interlocks with ADR-0059's torn-tail recovery; the kill-9 test is correctly framed as the FIRST genuine producer of the torn tail ADR-0059 recovers."
  - "The kill-9 proving test is a first-class AC in every story, not an afterthought — directly remediating the 'false-confidence' finding that 1194 in-process tests overstate durability."
  - "Carpaccio slicing is genuine: one store per slice, walking skeleton is exactly one store end-to-end, each slice independently shippable and verifiable."

issues_identified:
  confirmation_bias:
    happy_path_bias:
      - issue: "Risk that stories over-focus on the acked-survives path and under-specify the sad paths."
        severity: "high"
        location: "all stories"
        resolution: "MITIGATED in the artifact: every story includes (a) a mid-snapshot-crash scenario, (b) a torn-tail/never-acked-absent scenario or in-flight/empty boundary, and (c) a lying-substrate refusal scenario. The negative space (torn tail dropped not repaired; never-acked absent; mid-file corruption stays fail-closed via G1 guardrail) is explicitly covered. No action."
    technology_bias:
      - issue: "Does the requirement prescribe sync_all / temp-rename (an implementation)?"
        severity: "medium"
        location: "user-stories Solution sections"
        resolution: "ACCEPTED-AS-CORRECT: the OBSERVABLE criteria (C2 durable-on-disk, C3 atomic-whole-or-absent) are solution-neutral; the sync_all/rename mechanism is named in Solution/Technical-Notes as the PROVEN ADR-0049 pattern being generalised, which is appropriate lineage citation for a hardening feature, not premature technology choice. The AC themselves assert observable outcomes (acked write present, store opens), not the mechanism. No action."

  completeness_gaps:
    missing_error_scenarios:
      - issue: "Is the lying-substrate refusal path covered for all stores, not just pulse?"
        severity: "critical"
        location: "K4 / per-store refusal scenarios"
        resolution: "COVERED: K4 targets 1/7 → 7/7 for the refusal path; each story has a 'refuses to start on a substrate that lies about fsync' scenario. No action."
    missing_nfrs:
      - issue: "Throughput cost of per-record sync_all is an NFR; is it acknowledged?"
        severity: "medium"
        location: "wave-decisions risk register"
        resolution: "ACKNOWLEDGED: per-record sync_all cost recorded in wave-decisions risk register and KPI guardrails; batched fsync deferred to a successor ADR per ADR-0049 §4. Correctness-over-capacity is the explicit v0 posture. No action."

  clarity_issues:
    - issue: "'Survives a restart' could be read as graceful-only (the very ambiguity that hid the bug)."
      severity: "high"
      location: "C2"
      resolution: "RESOLVED by C2, which pins 'durable = on stable storage, survives a power loss / kill-9 mid-write, NOT survives a graceful in-process reopen'. The ambiguity is named and closed. No action."

  testability_concerns:
    - issue: "Is 'opens cleanly' testable, and is the proving test deterministic (not a p95 flake)?"
      severity: "critical"
      location: "K3 / C6 / G3"
      resolution: "TESTABLE: 'open() succeeds without a parse error' is a binary assertion; C6/G3 mandate a deterministic invariant, not a wall-clock threshold, explicitly avoiding the overnight-p95-flake class. No action."

  priority_validation:
    q1_largest_bottleneck: "YES"   # the four-quadrants #1 defect, verified in code
    q2_simple_alternatives: "ADEQUATE"  # ADR-0049/0059 alternatives (batched fsync, format change, fork-in-tokio) considered and rejected with reasons
    q3_constraint_prioritization: "CORRECT"  # correctness-over-capacity at v0; riskiest-assumption (deterministic out-of-process crash test) validated first in the walking skeleton
    q4_data_justified: "JUSTIFIED"  # defect verified by code archaeology (file:line counts), ranked #1 by the four-quadrants assessment
    verdict: "PASS"

approval_status: "approved"
critical_issues_count: 0
high_issues_count: 0
```

### Review Outcome

All Dimension-0 checks PASS (no BLOCKING). All confirmation-bias,
completeness, clarity, and testability concerns were either MITIGATED in
the artifact as written or ACCEPTED-AS-CORRECT with rationale. Priority
validation PASS on all four questions. Zero critical, zero high
unresolved issues. **Approved in iteration 1** — no second iteration
required.

---

## Final Gate Status

- Dimension 0 (Elevator Pitch, BLOCKING): **PASS**
- DoR 9-item checklist, all 7 stories: **PASSED**
- Peer review: **APPROVED (iteration 1)**
- Handoff to DESIGN: **NOT performed** (brief: "Do NOT proceed into
  DESIGN"). Artifacts are DESIGN-ready and parked for the next wave.
