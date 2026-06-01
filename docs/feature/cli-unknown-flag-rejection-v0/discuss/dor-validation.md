# Definition of Ready — cli-unknown-flag-rejection-v0

9-item hard gate. Each item validated with evidence. Status: PASS.

## 1. Problem statement clear, domain language

PASS. Each story states the operator pain in shell terms: an operator
mistypes a flag, option, or verb and the CLI either ignores it silently
(US-02, the real gap) or already rejects it but without an anchored
contract (US-01, US-03). No technical jargon in the problem framing.

## 2. User/persona with specific characteristics

PASS. Personas are concrete platform operators: Sofia Marino (top-level
flags, cron wrappers), Diego Herrera (subcommand flags from runbooks),
Marcus Bauer (relies on existing flags daily). Each has a context and a
motivation tied to trusting the exit code.

## 3. Three or more domain examples with real data

PASS. Every story carries 3 examples with real persona names and real
tokens (`--bogus`, `--observ-otlp`, `--sicne`, `stat`, `migrate-tier`,
`--observe-otlp /tmp/m.ndjson`, tenant `acme`). No `user123`-style
placeholders.

## 4. UAT in Given/When/Then (3 to 7 scenarios)

PASS. US-01 has 2, US-02 has 3, US-03 has 2, US-04 has 2 Gherkin
scenarios; 9 across the feature, none exceeding 7 per story. All scenario
titles describe operator outcomes ("Operator is told when ...", "asking
for help is not an error"), not implementation.

## 5. AC derived from UAT

PASS. Each story's Acceptance Criteria block maps directly to its
scenarios (exit code, stderr substring, no stdout records, byte-equivalent
behaviour). No AC introduces a requirement absent from a scenario.

## 6. Right-sized (1 to 3 days, 3 to 7 scenarios)

PASS. Scope assessment in `story-map.md` scores 0 of 5 oversized signals:
4 stories, 1 module, 0 integration points, about 1 day, 1 coherent
outcome. Only US-02 carries a code change and it is bounded to argv
validation in the subcommand wrappers.

## 7. Technical notes: constraints/dependencies

PASS. System Constraints section pins the hand-rolled parser (no clap), the
additive-fix requirement, and the observable-contract pins. Each story's
Technical Notes name the exact code path
(`main.rs:70`, `parse_observe_otlp`, `parse_flag_iso`).

## 8. Dependencies resolved or tracked

PASS. Three DESIGN decisions are explicitly flagged and tracked in
`wave-decisions.md` (Flags to DESIGN 1 to 4): the code-gap-vs-re-anchor
split (resolved by grounding), the exit-code pin, the stderr message-shape
pin, and the no-ADR recommendation. No unresolved external dependency. No
DISCOVER/DIVERGE artefacts exist; origin is the EDD verifier defect log,
recorded in `wave-decisions.md`.

## 9. Outcome KPIs defined with measurable targets

PASS. `outcome-kpis.md` defines K1 (100% non-zero exit), K2 (100% stderr
usage substring), K3 (0 regressions), K4 (K11 `held` -> satisfied on a
non-reverted anchor). Each has a target, a measurement method, and a stated
baseline.

## Verdict

All 9 items PASS. Ready for DESIGN handoff. The one load-bearing item DESIGN
must close before DELIVER is the exact exit code and stderr substring (flags
2 and 3), since the EDD verifier will assert on those literals.
