# Story Map — tls-config-reject-v0

## User: Priya Nadkarni — platform/SRE engineer deploying aperture in a regulated telemetry fleet

## Goal: never ship telemetry in plaintext when I asked for transport encryption (or SPIFFE auth) — get a loud refusal instead of a silent downgrade

## Backbone

| Author config | Start collector | Observe outcome |
|---|---|---|
| Set `tls.enabled = true` / `auth.spiffe.enabled = true` | Run `aperture --config <path>` | Read exit code + stderr events; confirm no listener bound |
| (or leave both off / absent) | Run `aperture --config <path>` | Confirm normal start-and-bind (negative control) |

This is a brownfield defect-fix. All three backbone activities already exist in the
product (config authoring per ADR-0008, the `aperture` binary, the structured-event
stream). The feature changes exactly one reaction inside "Observe outcome": a
requested-but-unimplemented security knob moves from *warn-and-bind-plaintext* to
*refuse-to-start*.

---

### Walking Skeleton

Not applicable as a new end-to-end skeleton — the end-to-end flow (author → start →
observe) already ships. The thinnest slice that delivers the behaviour change is the
single story below, which is itself end-to-end (it spans all three backbone
activities: it reads the config knob, intercepts at start, and changes the observable
outcome).

### Release 1 (single slice): US-TLS-01 — Refuse to start on a requested-but-unimplemented security knob

- **Tasks**: refuse-to-start when `tls.enabled` or `auth.spiffe.enabled` is true
  (exit 2 + structured refusal event naming the knob + no listener binds); preserve
  normal start-and-bind when both are off/absent; correct the false `sinks.rs:94-95`
  comment.
- **Outcome KPI targeted**: 100% of security-knob-set startups refuse (non-zero exit +
  refusal event + zero plaintext binds); negative controls unchanged. See
  `outcome-kpis.md`.
- **Rationale**: this is the whole feature. It is one coherent, demonstrable behaviour
  change (6 UAT scenarios, well within the 3-7 band), observable in a single session by
  starting aperture with each config variant and reading exit code + stderr.

## Priority Rationale

Single slice, single priority. There is no slicing decision to make: the feature is
one sharp, atomic behaviour change to one reaction in one service. Splitting it (e.g.
"TLS knob" then "SPIFFE knob" as separate slices) would be a technical-layer split, not
an outcome split — both knobs serve the *same* operator outcome (no silent security
downgrade) and share one refusal behaviour, so they ship together as the coherent unit.
The two-knob truth table is required in one slice to satisfy the fail-closed outcome
and the mutation-100% gate.

Within the slice, the *riskiest assumption* validated first is the negative control:
that refusing on `=true` does not regress the overwhelmingly-common `=false`/absent
path (which would take down a collector fleet). The negative-control scenarios guard
that. The refusal scenarios then deliver the actual value.

## Scope Assessment: PASS — 1 story, 1 bounded context (aperture startup/config), estimated < 1 day

Oversized signals checked (none triggered):

- User stories: **1** (threshold > 10).
- Bounded contexts / modules: **1** — aperture `config` + `compose` + `observability`,
  one crate, one startup path (threshold > 3).
- Integration points: the slice touches the config loader, the compose/spawn sequence,
  and the event vocabulary — all within `crates/aperture` (threshold > 5).
- Independent shippable outcomes: **1** (no silent security downgrade). The TLS and
  SPIFFE knobs are not independent outcomes; they are one outcome with two triggers.
- Effort: a single reject branch + comment correction + tests. Well under 2 weeks.

Right-sized. No split. Do not pad.
