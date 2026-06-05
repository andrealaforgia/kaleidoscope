# Story Map: beacon-sighup-reload-v0

British English throughout, no em dashes.

## User: Sofia Okonkwo, on-call platform operator for a payments cluster

## Goal: apply a rule-catalogue edit to a running beacon-server without a restart and without dropping the alerts already being evaluated, and be protected when the edit is malformed

## Backbone

The operator's end-to-end job, left to right in chronological order.

| Edit the catalogue | Signal the daemon | Daemon re-reads + validates | Daemon swaps or refuses | Operator observes outcome |
|--------------------|-------------------|-----------------------------|-------------------------|---------------------------|
| Add/remove/change a `.toml` rule file | `kill -HUP <pid>` | `load_rules` re-run; validate (>=1 rule) | Atomic swap on valid; keep-previous on invalid | Read structured reload / refusal event |
| (valid edit) | (same process) | New catalogue parses | Stop old tasks, start new tasks | `rules reloaded: rules_loaded=N added=A removed=R` |
| (malformed edit) | (same process) | Parse error / zero rules | Keep previous, no crash, no partial apply | `reload refused: <file>: <error>; previous catalogue retained` |
| (unchanged surviving rule) | (same process) | Rule present by name in both | Preserve in-flight Firing/since | No re-page; no spurious incident |

---

### Walking Skeleton

Not applicable as a delivery construct: this is a brownfield capability,
not a greenfield end-to-end build (Walking Skeleton = No, per
wave-decisions DEC-2). beacon-server, the loader, the per-rule
evaluation loop, the inhibition resolver, and the durable state store
all already exist. The thinnest honest end-to-end slice that delivers
operator value is the SINGLE slice below, because "reload" and
"refuse-bad-reload" are co-equal halves of one trustworthy behaviour.

### Release 1 (the single slice): "Operators apply rule edits live, safely"

Target outcome: SIGHUP-applied valid rule edits take effect within one
evaluation interval (100% of valid reloads), and 100% of malformed
reloads keep the daemon running on the previous catalogue with a
diagnostic and no re-page.

Stories:

- **US-01 Apply edited rules with SIGHUP, no restart** — installs the
  SIGHUP handler; re-reads and validates the catalogue; atomically swaps
  in a valid new catalogue; added rule fires, removed rule stops; emits
  the structured success event. This is the B03 headline (start with
  rule A firing, add rule B, SIGHUP, assert B fires).
- **US-02 Refuse a malformed reload and keep the previous catalogue** —
  the co-equal safety negative. Validate-before-swap; on invalid keep
  previous, no crash, no partial apply, emit refusal diagnostic; a
  surviving Firing rule keeps its `since` and does not re-page. Carries
  the FLAGGED-for-DESIGN in-flight-state-preservation AC.

Both stories ship together: US-01 without US-02 would be an unsafe
reload, and US-02 has nothing to refuse without US-01's swap path. They
are two stories, one slice, one release.

## Priority Rationale

Priority is by outcome impact and hard dependency, not feature grouping.

| Priority | Story | Target outcome | Rationale |
|----------|-------|----------------|-----------|
| 1 | US-01 | Valid edits apply live within one interval; restarts-to-apply fall to zero | The headline capability and the precondition for everything else. There is no swap to make safe until the swap path exists. B03 (the verifier's filed issue) is satisfied by US-01's happy path. |
| 2 | US-02 | Malformed edits never crash, never partial-apply, never re-page; diagnostic surfaced | Co-equal in value but strictly dependent on US-01's swap path. The safety negative is what makes the headline trustworthy: an operator will only adopt live reload if a fat-fingered edit is known-safe. This story carries the load-bearing safety AC ("previous catalogue stays active") and the in-flight-state AC flagged for DESIGN. |

Tie-break note: both are Must Have for the slice to have release value
(per the reviewer's Dimension 0 slice-level check, a slice needs at least
one user-visible story; here both are user-visible and neither is
`@infrastructure`). US-01 is sequenced first only because US-02 depends
on its swap path; they release as one unit.

## Scope Assessment: PASS — 2 stories, 1 bounded context (beacon-server composition root + reuse of beacon loader/store), estimated 1-3 days

Oversized signals checked (Elephant Carpaccio gate): >10 stories (no, 2)
| >3 bounded contexts (no, the change is confined to the beacon-server
composition root plus pure reuse of the existing `beacon` loader and
`RuleStateStore`) | walking skeleton needs >5 integration points (no) |
effort >2 weeks (no) | multiple independent shippable outcomes (no, the
two stories are one inseparable safe-reload behaviour). Right-sized; no
split. Carpaccio of "atomic swap preserving state" away from "basic
reload" was considered and rejected (DEC-4): it would create an
infrastructure-only sliver with no independently demonstrable operator
value, and the state-preservation negative is exactly what makes the
headline trustworthy.
