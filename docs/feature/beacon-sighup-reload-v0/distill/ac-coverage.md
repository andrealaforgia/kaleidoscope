# AC Coverage — beacon-sighup-reload-v0 (DISTILL)

British English throughout, no em dashes.

Each acceptance criterion from `discuss/user-stories.md` is mapped to its
operator-observable signal and to the test in
`crates/beacon-server/tests/sighup_reload.rs` that pins it. The observable
is always one of: a firing incident at the webhook sink, a named structured
event on the child's stderr, the child process liveness, or the incident
`started_at` (the `since` proxy). No internal state is asserted.

## US-01 — apply edited rules with SIGHUP, no restart

| AC | Observable (operator-visible) | Test fn |
|----|-------------------------------|---------|
| AC-1: SIGHUP triggers a reload, not termination and not a silent no-op | `beacon.reload.succeeded` appears on stderr after `kill -HUP`; child still alive | `added_rule_begins_firing_after_sighup_without_restart`; `successful_reload_emits_structured_event_naming_what_changed` |
| AC-2: a valid reload replaces the live set without restart; added rule begins, removed rule stops | newly-added rule's Firing incident reaches the sink; child pid unchanged across the signal | `added_rule_begins_firing_after_sighup_without_restart` (added + same-pid); `removed_rule_stops_evaluating_after_sighup` (removed) |
| AC-3: a removed rule issues no further incidents; its durable state is dropped and logged | removed rule's Firing count does not increase after the swap settles | `removed_rule_stops_evaluating_after_sighup` |
| AC-4: a no-change SIGHUP produces no spurious Firing and no spurious Resolved | surviving rule's Firing count stays exactly 1; zero Resolved incidents for it | `no_change_sighup_swaps_cleanly_with_no_spurious_emissions` |
| AC-5: a successful reload emits one structured INFO event carrying `rules_loaded` + added/removed counts | stderr contains `beacon.reload.succeeded`, `rules_loaded`, `added` | `successful_reload_emits_structured_event_naming_what_changed` |

## US-02 — refuse a malformed reload, keep the previous catalogue

| AC | Observable (operator-visible) | Test fn |
|----|-------------------------------|---------|
| AC-1: a non-validating reload (zero rules) keeps the previous catalogue, does not crash, does not partially apply | `beacon.reload.refused` on stderr; child still alive; surviving rule still firing | `malformed_reload_keeps_previous_catalogue_and_does_not_crash`; `reload_to_empty_catalogue_is_refused_daemon_keeps_alerting` |
| AC-2: a refused reload emits one structured refusal event naming the file + parse error (+ "did you mean") + "previous catalogue retained" | stderr `beacon.reload.refused` contains `payments.toml`, `for_duration`, and `previous_catalogue_retained` | `malformed_reload_keeps_previous_catalogue_and_does_not_crash` |
| AC-3: a rule Firing before a refused reload keeps firing with its original `since`; no second Firing, no Resolved; no re-page | surviving rule has exactly ONE Firing incident across the refused reload; same `started_at` | `surviving_firing_rule_does_not_repage_across_refused_reload` |
| AC-4 (FLAGGED for DESIGN, decided in ADR-0063): a name-matched rule in both catalogues preserves in-flight state + `since` across a **valid** swap; no spurious second Firing; no reset to Inactive | surviving rule has exactly ONE Firing incident across the **successful** reload; same `started_at` | `surviving_firing_rule_keeps_state_and_does_not_repage_on_successful_reload` |
| AC-5: a partly-broken catalogue (>=1 valid rule + a malformed file) applies the valid rules AND surfaces the per-file diagnostic | `beacon.reload.succeeded` + valid new rule fires + stderr names the skipped file (`inventory.toml`) | `partly_broken_catalogue_applies_valid_rules_and_surfaces_diagnostic` |

## Coverage summary

- **US-01**: 5 AC -> 4 tests (AC-1/AC-2/AC-5 share the two success tests).
- **US-02**: 5 AC -> 5 tests (one per AC, with the two carryover ACs split
  across the success path and the refused path per reviewer condition 2).
- **Total**: 10 AC, all covered; 9 tests; 0 AC unmapped.

## DISCUSS UAT scenario -> test crosswalk

Every Gherkin UAT scenario embedded in `user-stories.md` has a test:

| DISCUSS UAT scenario | Test fn |
|---|---|
| US-01: newly-added rule begins firing after SIGHUP without a restart | `added_rule_begins_firing_after_sighup_without_restart` |
| US-01: a removed rule stops being evaluated after SIGHUP | `removed_rule_stops_evaluating_after_sighup` |
| US-01: SIGHUP with no on-disk change swaps cleanly | `no_change_sighup_swaps_cleanly_with_no_spurious_emissions` |
| US-01: a successful reload emits a structured event naming what changed | `successful_reload_emits_structured_event_naming_what_changed` |
| US-02: a malformed reload keeps the previous catalogue active | `malformed_reload_keeps_previous_catalogue_and_does_not_crash` |
| US-02: a malformed reload does not re-page on-call | `surviving_firing_rule_does_not_repage_across_refused_reload` |
| US-02: a refused reload emits a diagnostic naming the problem | `malformed_reload_keeps_previous_catalogue_and_does_not_crash` |
| US-02: a reload that yields zero rules is refused, daemon keeps alerting | `reload_to_empty_catalogue_is_refused_daemon_keeps_alerting` |
| US-02: a rule unchanged across a valid reload keeps its in-flight state | `surviving_firing_rule_keeps_state_and_does_not_repage_on_successful_reload` |
| US-02 domain ex.3: partly-broken catalogue applies the good rules, names the bad file | `partly_broken_catalogue_applies_valid_rules_and_surfaces_diagnostic` |

No DISCUSS UAT scenario is left without a proving test.
