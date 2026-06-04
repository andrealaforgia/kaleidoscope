# Definition of Ready Validation — tls-config-reject-v0

## Story: US-TLS-01 — Aperture refuses to start when an unimplemented security knob is requested

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear, domain language | PASS | Priya Nadkarni (SRE) sets `tls.enabled=true`, collector comes up green, ships plaintext; only signal is a sampled-away warn line. Domain language: transport encryption, plaintext, collector, telemetry-in-transit. No technical solution prescribed. |
| 2 | User/persona with specific characteristics | PASS | Primary: platform/SRE engineer deploying aperture in a regulated fleet via `aperture --config <path>`, motivated to fail loudly not silently downgrade. Secondary: security/compliance reviewer auditing exit codes + startup events. |
| 3 | 3+ domain examples with real data | PASS | Four examples with real names + real config: Priya (tls on), Marcus Bell (spiffe on), Priya (both on, Phase-2 port-back), Wei Tanaka (negative control, both off / absent). Real knobs (`[aperture.security.tls] enabled`), real ports (`:4317`/`:4318`). |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | 6 scenarios: tls-refuse, spiffe-refuse, both-refuse, negative-control-off, negative-control-absent, comment-correction. Within the 3-7 band. Titles are business outcomes, no implementation. |
| 5 | AC derived from UAT | PASS | 7 AC checkboxes, each traced to a scenario number. Cover refusal (per knob + both), no-plaintext-bind assertion, both negative controls, and the comment correction. |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 1 story, 6 scenarios, 1 bounded context (aperture startup), one reject branch + comment fix + tests, < 1 day. Scope Assessment in `story-map.md`: PASS. |
| 7 | Technical notes: constraints/dependencies | PASS | ADR-0008 supersession (lines 19/36/164/166) recorded; refuse-before-bind sequencing; exit code 2; event-constant candidates from closed vocabulary; test-fallout flag; mutation-100% note. All dependencies exist and ship. |
| 8 | Dependencies resolved or tracked | PASS | Config loader (ADR-0008, shipped), compose/spawn path, observability event module — all present. No external dependency. No blocker. ADR-0008-supersession tracked as an upstream change for DESIGN in `wave-decisions.md`. |
| 9 | Outcome KPIs defined with measurable targets | PASS | `outcome-kpis.md`: North-star = fraction of security-knob-set startups that refuse with no plaintext bind (target 1.0, baseline 0.0); guardrail = knob-off startups still bind (1.0); secondary = 0 false comments. Each has who/does-what/by-how-much/baseline/measured-by. |

## DoR Status: PASSED

All 9 items pass with evidence. No remediation required.

## Solution-neutrality check

The story asserts observables only — non-zero exit (code 2 is the *existing* contract,
not a new choice), a structured refusal event naming the knob, and no plaintext bind.
It does NOT prescribe the event constant, the validation function shape, or the
control-flow location of the check; those are explicitly deferred to DESIGN. PASS.

## Anti-pattern scan

- Implement-X: none — story starts from operator pain (silent plaintext downgrade).
- Generic data: none — Priya Nadkarni, Marcus Bell, Wei Tanaka; real knobs and ports.
- Technical AC: none — AC describe exit code, refusal event, and absence of binds, all
  operator-observable. Exit code 2 is an existing user-facing contract, not an
  implementation choice.
- Technical scenario titles: none — titles describe operator outcomes ("Collector
  refuses to start when transport encryption is requested but unimplemented").
- Oversized: no — 6 scenarios, single outcome.
- Abstract requirements: none — every rule has concrete examples.
