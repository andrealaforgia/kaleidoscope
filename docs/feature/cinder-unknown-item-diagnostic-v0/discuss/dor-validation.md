# Definition of Ready Validation: cinder-unknown-item-diagnostic-v0

## Story: US-01 — The unknown-item diagnostic names the id I typed, quoted

| # | DoR Item | Status | Evidence / Issue |
|---|----------|--------|------------------|
| 1 | Problem statement clear, domain language | PASS | "Priya reads `ItemId("ghost")` and thinks the tool broke rather than 'that id isn't there'." Domain language (operator, item id, not-found, tenant); no implementation in the problem. |
| 2 | User/persona with specific characteristics | PASS | Priya Raman, Platform SRE, runs `kaleidoscope-cli migrate`/`get-tier` against Cinder, reads CLI `--help` as the message contract. Reused from existing cinder CLI tests. |
| 3 | 3+ domain examples with real data | PASS | Three: migrate unknown `ghost`/`acme`; get-tier composite `acme/batch-00042`/`globex` (wrong tenant); negative control known `blk-7781`/`acme`. Real ids, real tenants. |
| 4 | UAT in Given/When/Then (3-7) | PASS | Four scenarios: migrate-names-quoted-id; get-tier-names-quoted-id; matches-CLI-help-contract; known-item-and-exit-1-unchanged. Business-outcome titles, no implementation. |
| 5 | AC derived from UAT | PASS | Four AC, one per scenario, named (`unknown-item-migrate-names-the-bare-quoted-id`, etc.), each observable on stderr/stdout/exit-code. |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 1 story, 4 scenarios, single Display-arm change, < 1 day. Scope assessment PASS (0/5 oversized signals). |
| 7 | Technical notes: constraints/dependencies | PASS | Verified loci (store.rs:57; lib.rs:471/509; main.rs:208/245), shared-arm finding, render-site audit, `as_str()` accessor, existing-test gap, narrowest-fix guidance. |
| 8 | Dependencies resolved or tracked | PASS | None external. Self-contained. DESIGN decisions flagged in wave-decisions.md (fix locus; get-tier sharing; no-regression). |
| 9 | Outcome KPIs defined with measurable targets | PASS | Two KPIs with targets (leaks 1->0; doc-vs-code mismatch 1->0), baselines, measurement methods, guardrails (exit code, known-path, no other diagnostic, 5 gates, 100% mutation). |

## Elevator Pitch Test (Dimension 0 — blocking, checked first)

| Invariant | Status | Evidence |
|---|---|---|
| Presence (Before / After / Decision enabled) | PASS | All three lines present in US-01. |
| Real entry point | PASS | `kaleidoscope-cli migrate` / `kaleidoscope-cli get-tier` — operator-invocable binary subcommands. |
| Concrete output | PASS | Verbatim stderr sample: `cannot migrate unknown item "ghost" for tenant acme` (before/after contrasted). |
| Job connection | PASS | Decision: operator recognises not-found and re-runs with corrected id instead of escalating an internal fault. |
| Slice-level value | PASS | The one story is user-visible (operator-facing diagnostic); not infrastructure. |

## DoR Status: PASSED

All 9 items PASS with evidence; Elevator Pitch test PASS. Ready for peer
review and (after approval) DESIGN handoff. Do NOT proceed into DESIGN in
this wave.
