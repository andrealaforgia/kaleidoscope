# Peer Review — Iteration 1 — `aperture` v0 (DISCUSS)

> **Wave**: DISCUSS — Phase 4 (peer review gate).
> **Reviewer mode**: `nw-product-owner` in independent-reviewer persona, applying the `nw-po-review-dimensions` skill (Dimensions 0–5).
> **Date**: 2026-05-04.
> **Iteration**: 1 of max 2.
> **Artefacts under review**:
>
> - `journey-aperture.yaml`
> - `journey-aperture.feature`
> - `journey-aperture-visual.md`
> - `shared-artifacts-registry.md`
> - `story-map.md`
> - `prioritization.md`
> - `slices/slice-01-walking-skeleton.md` through `slices/slice-08-graceful-shutdown.md`
> - `user-stories.md`
> - `outcome-kpis.md`
> - `dor-validation.md`
> - `wave-decisions.md`

Persona shift: from author to independent reviewer. Mindset: assume nothing, challenge assumptions, verify the package is fit for DESIGN handoff.

---

```yaml
review_id: "req_rev_20260504_113000"
reviewer: "product-owner (review mode, applying nw-po-review-dimensions)"
artifact: "docs/feature/aperture/discuss/* (full DISCUSS package)"
iteration: 1

strengths:
  - "Every elevator-pitch After line names a real network entry point (gRPC :4317, POST http://localhost:4318/v1/logs, /healthz, /readyz, SIGTERM) and concrete observable output (gRPC status codes, HTTP response bodies, structured stderr JSON lines with field examples). Dimension 0 satisfied."
  - "The locked scope (Andrea's six Q&A items + Slice 01 shape) is recorded verbatim in wave-decisions.md, preventing DESIGN-wave re-derivation. The naming-clarification note (Framing::Grpc vs Framing::GrpcProtobuf) shows the author resolved the only ambiguity in Andrea's text without inventing a new decision."
  - "Shared-artifacts-registry distinguishes HIGH/MEDIUM/LOW integration risk and names CI invariants (no_telemetry_on_telemetry, single_validator_per_signal) that reference downstream wave owners. The contract is unambiguous and machine-checkable."
  - "Eight thin end-to-end slices, none of which are single-column. The walking skeleton (Slice 01) uses the real harness (not a stub), honouring Andrea's explicit choice to land integration risk early. The six taste tests are all enumerated in story-map.md and pass."
  - "KPIs are framed around what consumers can measure (gRPC status ratios, downstream-acceptance ratios, listener uptime, observed concurrency saturation events) rather than internal Aperture state. KPI 1 is binary (walking-skeleton tripwire) — appropriate for an Empathy-stage greenfield component per Maurya's OMTM."
  - "@property scenario in journey-aperture.feature defends the refusal-not-drop invariant — exactly the right shape for an ongoing quality criterion (Maurya/Doerr smell test passes)."
  - "Reject-path message identity (Aperture passes the harness's OtlpViolation::Display output verbatim through gRPC grpc-message and HTTP body) is named as a DISCUSS contract (D6 in wave-decisions.md). This is the right level — the reject-text is part of the wire contract, not an internal implementation detail."
  - "DoR validation file checks all 9 items per story with concrete evidence. No remediation needed."

issues_identified:

  dimension_0_elevator_pitch_test:
    presence_check: PASS
    real_entry_point_check: PASS
    concrete_output_check: PASS
    job_connection_check: PASS
    slice_level_check: PASS_WITH_NOTE
    notes:
      - issue: "Slice 07 (TLS/SPIFFE schema knob) carries no user story; it is the only @infrastructure slice. Author has self-flagged this and justified it at slice level (skipping costs nothing now, breaks the schema in Phase 2 when Aegis ships). The slice ships ALONGSIDE Slices 06 and 08 (both user-facing), satisfying the slice-level invariant that no slice in isolation is purely @infrastructure when sequenced into a release. Verdict: ACCEPTABLE. The justification is load-bearing (Aegis schema break is real), not retrofitted, and is recorded in three places (System Constraint 7, slice-07-tls-schema-knob.md, dor-validation.md Summary)."
        severity: "low"
        location: "slices/slice-07-tls-schema-knob.md; user-stories.md > Out-of-scope; dor-validation.md > Summary"
        recommendation: "No change required. The author's framing — 'this slice rides alongside user-facing Slices 06 and 08' — meets the spirit of Dimension 0's slice-level test."

  confirmation_bias:
    - issue: "Concurrency-cap default value (1024 per transport) appears in story-map.md, prioritization.md, slice-05-backpressure.md, user-stories.md, shared-artifacts-registry.md, and wave-decisions.md without any data-driven justification. The author acknowledges this is a placeholder and DESIGN may revisit it, but a placeholder repeated six times begins to look like a number with authority."
      severity: "medium"
      location: "Six files mention 1024 as default without sourcing"
      recommendation: "Add one sentence to wave-decisions.md > D7: 'Default 1024 chosen as a placeholder large enough to absorb realistic burst traffic from a 50-pod application cluster (50 pods × 16 concurrent exporters / 1 Aperture replica = 800 — round up to 1024). DESIGN may calibrate against measured production traffic.' This converts the placeholder from authority to first-pass estimate with a reasoning trail."
    - issue: "Drain-deadline default (30000 ms) is similarly an unjustified number, repeated in multiple places."
      severity: "low"
      location: "slice-08-graceful-shutdown.md, user-stories.md > US-AP-09, wave-decisions.md > D8"
      recommendation: "Add a one-sentence justification: '30 s default chosen to match Kubernetes' default terminationGracePeriodSeconds (30 s); k8s sends SIGKILL after that period regardless, so deadlines longer than 30 s have no effect under k8s anyway.' This grounds the number in the operator's expected orchestrator."
    - issue: "Default downstream timeout for ForwardingSink (5 s, US-AP-08 technical notes) is a third unjustified number."
      severity: "low"
      location: "slice-06-forwarding-sink.md, user-stories.md > US-AP-08, wave-decisions.md > implicit"
      recommendation: "Add a brief rationale: '5 s default chosen because OTel SDK default exporter timeout is 10 s; Aperture's ForwardingSink should fail before the SDK times out so the SDK's retry budget is not consumed by a hung downstream.' Cites the OTel SDK convention as the anchor."
    - issue: "No explicit 'rejected alternatives' for the OtlpSink trait shape. The author specifies a contract (Send + Sync, async accept, Result<(), SinkError>) and defers signature specifics to DESIGN, but does not enumerate the alternatives that were considered (e.g. a sync sink, a channel-based sink, a callback-based sink) and why they were rejected."
      severity: "medium"
      location: "user-stories.md > System Constraints item 5; wave-decisions.md > D2"
      recommendation: "Add a 'rejected alternatives' subsection under D2 in wave-decisions.md naming: (a) sync trait — rejected because ForwardingSink must do network I/O, (b) channel-based sink — rejected because backpressure semantics get fuzzy across the channel boundary, (c) callback-based sink — rejected because the trait shape is what Sieve will plug into and a trait is the standard Rust shape. Three alternatives + reason rejected, matching the harness's wave-decisions style."

  completeness_gaps:
    - issue: "No explicit failure scenario for Aperture's stderr backpressure. The journey YAML names it as a step-5 failure mode (disk full, broken pipe to systemd-journal) but no UAT scenario exercises it. If stderr writes block, does Aperture block on traffic? If they fail, does Aperture stay up?"
      severity: "high"
      location: "journey-aperture.yaml step 5 failure_modes; missing UAT in journey-aperture.feature"
      recommendation: "Add a UAT scenario to journey-aperture.feature step 5: 'Aperture continues serving traffic if stderr writes fail' — given a process whose stderr is closed (or set to /dev/null with permission errors), when traffic arrives, then the harness validation and sink hand-off still complete and the SDK still receives gRPC OK. This converts a noted failure mode into a tested behaviour."
    - issue: "No NFR or scenario for memory bounds under sustained overload. The concurrency cap bounds in-flight count but not memory; a single in-flight request could carry a multi-MiB OTLP body. With cap=1024 and max body size 4 MiB, the worst-case memory footprint is 4 GiB just for inbound buffers."
      severity: "high"
      location: "Missing NFR; max_recv_msg_size and max_concurrent_requests both exist independently"
      recommendation: "Add a derived NFR to wave-decisions.md (or a new System Constraint to user-stories.md): 'Worst-case in-flight memory footprint for inbound buffers is bounded by max_concurrent_requests × max_recv_msg_size × number_of_transports. With v0 defaults (1024 × 4 MiB × 2) this is 8 GiB; operators are expected to lower one or both bounds to fit their pod memory limit. This is a documentation contract, not a runtime check.' Surfaces the relationship explicitly so an operator does not get OOM-killed silently."
    - issue: "Concurrent-shutdown-and-incoming-request edge case not specified. What happens if a request begins arriving (TCP SYN accepted, body partially read) at the exact moment SIGTERM fires? Does it count as 'in-flight' for drain purposes? The journey YAML and US-AP-09 are silent on this boundary."
      severity: "medium"
      location: "user-stories.md > US-AP-09, journey-aperture.yaml step 6"
      recommendation: "Add an AC bullet to US-AP-09: 'A request whose body is being read at the moment of SIGTERM either (a) completes if the body is fully received before the listener-close grace window, or (b) is reset at the TCP level if the listener has already closed. Either way, the SDK observes a deterministic outcome (gRPC UNAVAILABLE / TCP reset), never a half-acknowledged response.' Closes a real edge case that bites every graceful-shutdown implementation."
    - issue: "Operator-misconfiguration of bind addresses to overlap (grpc=0.0.0.0:4318 AND http=0.0.0.0:4318) not handled. With Andrea's locked decision that both listeners share the HTTP port for /healthz/readyz, what if the operator sets identical bind addresses for the two transports? Listener bind would fail on the second one — but is the failure message helpful?"
      severity: "low"
      location: "user-stories.md > US-AP-01"
      recommendation: "Add a UAT scenario in US-AP-01: 'Configuration with identical bind addresses for grpc and http produces a clear error' — given config with grpc.bind_addr = http.bind_addr, when Aperture starts, then exit code is non-zero, stderr names the conflict explicitly (event=config_validation_failed reason='grpc and http bind addresses must differ'). Catches a real misconfiguration before any listener tries to bind."
    - issue: "Stakeholder coverage check: third-party-engineer persona is named in three stories (US-AP-01, US-AP-04, US-AP-08) but never gets a domain example that highlights what they uniquely need vs the operator persona. The Sieve persona is mentioned in System Constraint 5 but never appears in a UAT or domain example in user-stories.md."
      severity: "medium"
      location: "user-stories.md > all stories"
      recommendation: "Add one Sieve-specific UAT scenario to US-AP-03 or US-AP-08: 'A custom impl OtlpSink (representing Sieve's future shape) plugs into Aperture without crate-level changes' — given a test sink that mimics Sieve's expected shape, when Aperture is configured to use that sink, then valid records flow through it and gRPC OK is returned upstream. Defends D2's 'Sieve will plug into this trait' claim with a real test."

  clarity_issues:
    - issue: "max_recv_msg_size is described as default '4 MiB per transport' in shared-artifacts-registry.md but is never named in user-stories.md AC. The body_too_large UAT in journey-aperture.feature uses 1 MiB in the example. Inconsistency between the registry default and the UAT example value."
      severity: "high"
      location: "shared-artifacts-registry.md > max_recv_msg_size; journey-aperture.feature > Receive payload section"
      recommendation: "Either (a) make the journey UAT use 4 MiB to match the registered default, or (b) explicitly note that the UAT scenario configures a smaller bound for testing reasons. Option (b) is more honest: 'Aperture's HTTP listener is configured with max_recv_msg_size=1048576 (1 MiB, lower than the v0 default of 4 MiB to make the test scenario quick to drive)'. Removes the apparent inconsistency."
    - issue: "Story IDs use 'US-AP-01' through 'US-AP-09' but the prioritization table also uses 'P1' through 'P8' as priority labels. Two parallel numbering schemes is mildly confusing. The story IDs are stable; the priority labels are derived. The prioritization.md table should make this explicit."
      severity: "low"
      location: "prioritization.md > Backlog suggestions"
      recommendation: "Add a one-line note above the table: 'Story ID is stable; priority label (Pn) is derived from the slice the story lands in and may shift if slice ordering changes during DESIGN.' Resolves any reader confusion about which is canonical."
    - issue: "The 'OTLP Profiles signal' is named as out-of-scope in wave-decisions.md ('not stable in OTel spec at the harness's pinned version') but the rationale for why this matters to Aperture (vs being a harness-only concern) is implicit. A reader unfamiliar with OTLP could read 'profiles' and wonder whether Aperture's path-routing should reject /v1/profiles or 404 it."
      severity: "low"
      location: "user-stories.md > US-AP-02 (404 example uses /v1/profile); wave-decisions.md > Out-of-scope"
      recommendation: "Already partially addressed: US-AP-02's 404 UAT uses /v1/profile as an example. Consider tightening the example to '/v1/profiles' (the actual OTel-canonical path Aperture rejects in v0) so the rejection has documentary value. One-character clarification."

  testability_concerns:
    - issue: "KPI 3 ('100% of pilot operators report /readyz is what they use') depends on a post-Phase-1 survey that does not exist yet and whose mechanism is not specified. This is a soft KPI dressed as a hard one."
      severity: "high"
      location: "outcome-kpis.md > KPI 3"
      recommendation: "Either (a) downgrade KPI 3 to a leading-secondary indicator and rephrase as 'CI integration test asserts /readyz returns 200 only when both listeners are bound, in 100% of test runs' (a structural KPI rather than a survey-based one), or (b) keep the survey-based KPI but specify the survey instrument (Google Form? structured 30-min call? what questions?) and the threshold for success (3-of-3? majority?). Option (a) is cleaner because it is testable today; option (b) is fine if the survey is actually planned."
    - issue: "KPI 1 ('100% of the documented Slice-01 demo command sequence completes without manual intervention') is binary, but the AC bullets across US-AP-01 through US-AP-03 are not enough on their own to cover the full demo. The demo runs cargo build, starts a process, runs another cargo example, greps stderr, asserts gRPC OK — none of those steps is in any story's AC."
      severity: "medium"
      location: "outcome-kpis.md > KPI 1; slice-01-walking-skeleton.md > Demo command"
      recommendation: "Add one AC to US-AP-03: 'The Slice-01 demo command sequence in slice-01-walking-skeleton.md runs end-to-end in CI without manual intervention' — and include the demo as an integration test fixture in the DISTILL or DELIVER wave. This converts KPI 1 from a CI-aspiration to a test-defended invariant."
    - issue: "KPI 8 ('1000-restart load test') is testable but not yet scheduled. CI cadence: every commit? Every release? The KPI says 'every release' in the Measurement Plan; 1000 restarts in CI per release is a non-trivial time budget. Worth flagging."
      severity: "low"
      location: "outcome-kpis.md > KPI 8 Measurement Plan"
      recommendation: "Add a note: 'The 1000-restart scenario is expected to run in approximately N minutes given Aperture's startup time of ~50 ms; total wall-clock budget for the scenario is the chief calibration knob for DEVOPS'. Surfaces the time cost so DEVOPS can plan."

  priority_validation:
    q1_largest_bottleneck: "YES"
    q1_evidence: |
      The story map's Priority Rationale explicitly identifies the riskiest assumption (load behaviour, addressed in Slice 05) and orders it after the three-signal contract is complete. Slice 01 lands integration risk first per Andrea's locked walking-skeleton decision. The ordering matches the structure 'walking skeleton → highest-value next → riskiest unvalidated → production-readiness'.
    q2_simple_alternatives: "ADEQUATE"
    q2_evidence: |
      story-map.md > Priority Rationale enumerates the rationale for each slice's position with reference to alternatives implicitly considered (e.g. 'positioned as the first slice that defends an unproven load assumption, immediately after the three-signal contract is complete'). The wave-decisions.md > Out-of-scope table enumerates rejected alternatives at the FEATURE level (internal queue, blocking, silent drop, multi-tenancy, adaptive caps, etc.) with whose-job-when justifications. Adequate; could be stronger if the OtlpSink trait alternatives were enumerated explicitly (see confirmation_bias finding above).
    q3_constraint_prioritization: "CORRECT"
    q3_evidence: |
      User-mentioned constraints are quantified by impact in wave-decisions.md: Q1 transport coverage drives Slice 01+02 ordering; Q4 backpressure shape drives Slice 05 contract; Q5 TLS schema drives Slice 07; Q6 observability drives the closed event vocabulary. No minority constraint dominates; the locked scope shape drives slice ordering.
    q4_data_justified: "JUSTIFIED"
    q4_evidence: |
      The key decisions are not performance-optimisation decisions (where 'no data' would FAIL); they are integration-shape decisions, justified by the OTel SDK contract (Q4 'no block' rationale), the architecture document (Q1 transport choice), and the harness's locked DESIGN decisions (validate_* signatures, type-path identity contract). Where placeholder values appear (concurrency cap default, drain deadline, downstream timeout), they are flagged as placeholders for DESIGN to revisit.
    verdict: "PASS"

approval_status: "approved_with_revisions"
critical_issues_count: 0
high_issues_count: 4
medium_issues_count: 5
low_issues_count: 4

verdict_summary: |
  No CRITICAL issues. Four HIGH issues, all addressable by adding 5–10 lines of content to existing files (no structural rework). Five MEDIUM issues, mostly clarification or rationale-completion. Four LOW issues, cosmetic.

  The DISCUSS package is FUNDAMENTALLY SOUND. Andrea's locked scope is honoured; the harness boundary is correctly framed as the load-bearing dependency; the walking skeleton lands integration risk early; the slicing is end-to-end and demonstrably right-sized; the KPIs are consumer-measurable and the @property invariant defends the refusal-not-drop guarantee.

  The revisions below should be applied before DESIGN handoff. None of them is a structural blocker; all are quality-improvements that would land in a DESIGN-wave back-propagation if not addressed now (more expensive than addressing them now).

required_revisions_iteration_2:
  - "Add concurrency-cap default rationale to wave-decisions.md > D7 (50-pod cluster math)."
  - "Add drain-deadline default rationale to wave-decisions.md > D8 (k8s terminationGracePeriodSeconds anchor)."
  - "Add downstream-timeout default rationale to user-stories.md > US-AP-08 Technical Notes (OTel SDK 10 s convention anchor)."
  - "Add OtlpSink trait rejected-alternatives subsection to wave-decisions.md > D2 (sync, channel, callback alternatives + reasons)."
  - "Add stderr-backpressure UAT scenario to journey-aperture.feature step 5 (Aperture continues serving traffic when stderr writes fail)."
  - "Add memory-bound NFR to wave-decisions.md (max_concurrent_requests × max_recv_msg_size × num_transports = worst-case footprint)."
  - "Add SIGTERM-mid-receive boundary AC to US-AP-09 (deterministic TCP-reset or completion, never half-acknowledged response)."
  - "Add overlapping-bind-address UAT to US-AP-01 (config validation refuses identical grpc/http bind addresses)."
  - "Add Sieve-shaped sink UAT to US-AP-03 (custom impl OtlpSink plugs in without crate changes)."
  - "Reconcile max_recv_msg_size default (4 MiB) with UAT example value (1 MiB) — explicit note about test-time vs default."
  - "Replace KPI 3 (survey-based) with structural KPI on /readyz behaviour OR specify the survey instrument and pass threshold."
  - "Add Slice-01-demo-runs-in-CI AC to US-AP-03 (KPI 1 becomes test-defended)."
  - "Add CI time-budget note to KPI 8 measurement plan (1000 restarts × 50 ms startup = wall-clock estimate)."
  - "Tighten the /v1/profile 404 example in US-AP-02 to /v1/profiles (the actual OTel path Aperture rejects)."
  - "Add story-ID-vs-priority-label clarification line to prioritization.md > Backlog suggestions."
```

---

## Review-loop posture

Iteration 1 verdict: **approved with revisions**. The 14 required revisions are all additive content changes; none requires re-thinking the journey shape, the slice plan, or the KPI framework. They are the kind of polish that a second-iteration reviewer would otherwise flag, and addressing them now keeps the DESIGN handoff clean.

Iteration 2 will re-check the 14 specific revisions and will not re-litigate the dimensions that PASSED in iteration 1.
