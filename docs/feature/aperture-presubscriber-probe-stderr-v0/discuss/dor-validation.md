# Definition of Ready Validation — aperture-presubscriber-probe-stderr-v0

## Story: US-01 — An operator sees why the gateway refused to start

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1. Problem statement clear, domain language | PASS | "Priya Nair, SRE … aperture refuses SILENTLY: the refusal is emitted through tracing before the subscriber is installed, so the event is dropped and the process just exits 1." Domain language (Earned-Trust probe, fail-closed, downstream); no technical solution prescribed. |
| 2. User/persona with specific characteristics | PASS | Priya Nair, SRE running aperture as a systemd/k8s-supervised OTLP gateway; secondary persona = supervisor / Bea's A19/A20 black-box harness. |
| 3. 3+ domain examples with real data | PASS | (1) collector not up yet, endpoint `http://otelcol-sink:4318`, the verifier's exact scenario; (2) catalogued v0 substrate lie (200 OPTIONS / 503 POST); (3) negative control — healthy downstream + config-error (`tls.enabled=true`, ADR-0061). Real endpoints, real error shapes. |
| 4. UAT in Given/When/Then (3-7 scenarios) | PASS | 4 scenarios: probe-refusal emits a line; line names sink + error; fail-closed exit unchanged; healthy + config-error paths unchanged. Within 3-7. |
| 5. AC derived from UAT | PASS | 4 AC map 1:1 to the 4 scenarios with stable slugs (a-probe-refusal-emits-a-structured-stderr-line; the-line-names-the-sink-and-the-error; fail-closed-exit-is-unchanged; healthy-downstream-and-config-error-paths-unchanged). |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | 1 story, 4 scenarios, ~1 day; Scope Assessment in story-map.md trips 0 of 5 oversized signals. |
| 7. Technical notes: constraints/dependencies | PASS | Mechanism flagged to DESIGN (2 options); verified loci listed; double-probe nuance noted; constraints (fail-closed unchanged, no post-init regression, no config-error regression, ADR-0005, mutation 100%, never 1.0.0). |
| 8. Dependencies resolved or tracked | PASS | None blocking; builds on ADR-0007 probe + main.rs:63-82 precedent + gate-5-mutants-aperture; downstream consumer = Bea A19/A20. |
| 9. Outcome KPIs defined with measurable targets | PASS | KPI-1: 100% of probe-refusal starts emit a reason line (baseline 0%); KPI-2: 0 silent startup exits (baseline 1). Both with measurement method and guardrails. |

## Elevator Pitch Test (Dimension 0, BLOCKING — self-check)

| Invariant | Status | Evidence |
|-----------|--------|----------|
| Presence (Before / After / Decision) | PASS | All three lines present in US-01. |
| Real entry point | PASS | "After" references the running binary `aperture --config /etc/aperture/aperture.toml` started against a down downstream — an operator-invocable surface, not an internal function. |
| Concrete output | PASS | "sees" describes observable stderr: `event=health.startup.refused reason: sink probe failed: …` plus non-zero exit — not internal state. |
| Job connection | PASS | Decision: fix the DOWNSTREAM instead of guessing — a real operator decision. |
| Slice-level value | PASS | The single story is user-visible (operator-facing stderr), not @infrastructure. |

## DoR Status: PASSED

All 9 items PASS with evidence; Dimension 0 elevator-pitch invariants all
PASS. Eligible for peer-review gate.
