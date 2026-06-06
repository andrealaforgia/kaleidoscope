# DEVOPS Self-Review — aegis-ingest-auth-v0 (SLIM)

- **Reviewer**: Apex (`nw-platform-architect`), structured self-review against
  the `nw-platform-architect-reviewer` dimensions.
- **Date**: 2026-06-06
- **Why self-review**: the `nw-platform-architect-reviewer` Agent is not
  invocable as a nested subagent from within this subagent context (the same
  constraint recorded for the prior slim-DEVOPS features, e.g.
  `aperture-serve-loop-error-surfacing-v0/devops/wave-decisions.md` §"Peer
  Review"). Per the established slim-DEVOPS precedent on this project, a
  structured self-review against the reviewer's exact dimensions is conducted and
  the verdict is marked **APPROVED_PENDING_INDEPENDENT_REVIEW**.
- **Artefacts under review**: `devops/environments.yaml`,
  `devops/wave-decisions.md`.

## nWave-Order Reminder (carried into the review)

In nWave, DEVOPS runs BEFORE DISTILL and DELIVER. At DEVOPS time NO production
code, NO tests, and NO CI-config changes exist yet for this feature. That
absence is EXPECTED and CORRECT — it is NOT a finding and NOT a rejection
reason. The review judges whether this wave (a) CONFIRMS the existing ADR-0005
CI contract covers the feature, (b) DOCUMENTS the new auth-config deployment
precondition, and (c) produces the two artefacts — NOT the non-existence of code
or new pipeline files.

## Dimension 1 — External validity (claims grounded in verifiable evidence)

| Claim | Evidence | Verdict |
|---|---|---|
| All modified files live under `crates/aperture/` (so the `crates/aperture/**` glob covers them) | `test -f` confirmed `src/config/mod.rs`, `src/transport.rs`, `src/app.rs`, `src/ports/mod.rs`, `src/compose.rs`, `src/sinks.rs` all present | ✓ verified |
| `gate-5-mutants-aperture` exists, `--in-diff` on `crates/aperture/**` | ci.yml:505-602; diff glob ci.yml:577 | ✓ verified |
| `gate-5-mutants-aegis` exists, `--in-diff` on `crates/aegis/**`, short-circuits on empty diff | ci.yml:2000-2081; glob :2054; short-circuit :2055-2057 | ✓ verified |
| aegis is a workspace member at 0.1.0, AGPL-3.0-or-later | Cargo.toml:12 (`"crates/aegis"`); crates/aegis/Cargo.toml:3,8 | ✓ verified |
| jsonwebtoken already in Cargo.lock (9.3.1), single entry | `grep -c` = 1; version 9.3.1 | ✓ verified |
| aperture has NO aegis dep today (correct pre-DELIVER state) | `grep aegis crates/aperture/Cargo.toml` = NONE | ✓ verified |
| Gate 2/Gate 3 enrol only otlp-conformance-harness, spark, sieve, codex | pre-push lines 54, 77 | ✓ verified |
| Gate 4 = `cargo deny --all-features check` on every push | ci.yml:83-114 | ✓ verified |
| Gate 1 = `cargo test --workspace --all-targets --locked` | ci.yml:136-184 | ✓ verified |
| Refuse-to-start / exit-2 / config_validation_failed posture | ADR-0068 DD4; design/wave-decisions.md DD4 + fail-closed table | ✓ sourced |

**No unsupported claim.** Every load-bearing assertion is tied to a file:line or
a shell-verified fact.

## Dimension 2 — Evidence-based findings (severity-driven)

- **CRITICAL**: none.
- **HIGH**: none.
- **MEDIUM**: none.
- **LOW**: none.

Candidate findings considered and dismissed:

1. *"No new gate for the auth boundary?"* — Dismissed. The auth boundary is an
   IN-PROCESS HS256 validation (no IdP, no JWKS, no network at validation time);
   it is exercised by the token-matrix acceptance suite under Gate 1 and
   mutation-covered under the existing Gate 5 aperture job. No external
   integration -> no contract test, no new gate (brief DEVOPS handoff agrees).
   Adding a gate speculatively would violate "no new component without 'no
   existing alternative'".
2. *"The new aegis dependency needs a deny-config / advisory review?"* —
   Dismissed. aegis is in-workspace (already a member, 0.1.0); jsonwebtoken
   9.3.1 is already in the lockfile and already passed the existing Gate 4.
   No new external/license/advisory surface. Confirmed, not a finding.
3. *"DD7 aegis doc-fix should be a mutation target?"* — Dismissed. It is a
   non-behavioural `docs:` change, explicitly ADJACENT (not folded), kept out of
   scope precisely so `gate-5-mutants-aegis` stays a short-circuit. Correct call.
4. *"Rollback = git revert is too glib for a security feature?"* — Addressed,
   not dismissed: the artefacts explicitly flag that a revert RE-OPENS the
   gateway (re-admits tokenless writes) and is therefore a deliberate SECURITY
   decision, prefer fix-forward. This is the correct, honest rollback posture for
   a fail-closed auth feature.

## Dimension 3 — Rollback-first (every plan starts with rollback)

`environments.yaml > rollback` and wave-decisions D6 + Infrastructure Summary
state the rollback before anything else operational: `git revert`, stateless
service, happy-path wire/response contracts unchanged, WITH the explicit caveat
that a revert re-opens the gateway (a security regression) so fix-forward is
preferred. Rollback is present, tested-in-spirit (revert of a stateless internal
change), and honestly caveated. ✓

## Dimension 4 — SLO / observability completeness

No new SLO is introduced (no deploy, no traffic SLO change). Observability is
the aegis one-event-per-validate audit line on aperture's existing stderr stream
+ the one aperture pre-validate `missing_claim` line + the
`config_validation_failed`/exit-2 refuse-to-start signal — all existing
conventions, no new stack. The four outcome KPIs are mapped to concrete
correlations over the existing audit↔sink event streams, with the two CRITICAL
alert thresholds (KPI-1 < 100%; any secret-bytes-in-logs) carried from
outcome-kpis.md. Baselines are 0% by construction. ✓ complete for a SLIM,
no-deploy, audit-event-only change.

## Dimension 5 — Shift-left security / secret handling

The ONE real operational concern (the HS256 secret) is handled correctly and
thoroughly:
- Secret supplied BY FILE REFERENCE, never inline (clig.dev / OWASP).
- Config stores `PathBuf`, never the bytes; aegis opaque-Debugs the key; errors
  name the file by path only; audit/deny lines carry no token/secret —
  structural per DD1, not discipline.
- Operator MUST file-permission-restrict the secret file and MUST never commit
  it; rotation is operational (replace file + restart).
- Any secret-bytes-in-logs occurrence is a CRITICAL guardrail.
- Fail-closed: no auth config -> refuse to start (exit 2), no opt-out flag (the
  ADR-0061 silent-downgrade trap is closed).

This is the load-bearing security posture of the wave and it is captured in both
artefacts. ✓

## Dimension 6 — DORA / delivery posture

Trunk-based, CI-as-feedback (no required checks, no enforce_admins — project
memory). The change is a small batch (one crate, REUSE + EXTEND), inherits the
existing five-gate pipeline unchanged, and adds no lead-time or change-failure
surface beyond the (intended) behaviour change of rejecting tokenless callers.
No DORA regression; the feature improves the gateway's failure posture
(refuse-to-start beats silently-open). ✓

## Dimension 7 — Handoff completeness (DISTILL / DELIVER)

Nine explicit Constraints (C-DEVOPS-1..9) hand DISTILL/DELIVER: no new CI job;
the refuse-to-start deployment precondition + secret handling; the
no-new-external-dep / cargo-deny confirmation; the no-public-API-break /
no-semver-bump confirmation; deterministic auth tests in both hooks + CI;
falsifiability + non-regression; guardrails-green; no CLAUDE.md change; the DD7
adjacency. The KPI instrumentation section hands the measurement plan. The
"What this wave does NOT do" list fences scope. ✓ complete.

## Dimension 8 — Simplest-solution / no speculative components

No new infrastructure, no new crate, no new CI job, no new observability stack,
no new external dependency. Every "no new X" is justified by "an existing
alternative covers it" (the existing aperture gate-5 job; the existing Gate 4
deny pass; the existing stderr audit stream; the in-workspace aegis crate). The
ONE addition (the auth-config deployment precondition) is a documented
operational requirement, not a built component. ✓ passes the simplest-solution
check.

## Verdict

**APPROVED_PENDING_INDEPENDENT_REVIEW** — 0 blocking issues (0 CRITICAL, 0 HIGH,
0 MEDIUM, 0 LOW).

The two artefacts correctly (a) confirm the existing ADR-0005 five-gate CI
contract covers the feature with NO new gate (gate-5-mutants-aperture covers all
modified files; gate-5-mutants-aegis short-circuits), (b) confirm cargo-deny
(Gate 4) is satisfied with NO new external/license/advisory surface (in-workspace
aegis path dep; jsonwebtoken already vetted), (c) confirm NO public-API break and
NO semver bump (neither crate enrolled in Gate 2/3, no break to flag, stay
pre-1.0, NEVER 1.0.0), and (d) document the NEW refuse-to-start deployment
precondition with full secret-handling guidance. An independent top-level
`nw-platform-architect-reviewer` run is recommended before DISTILL.
