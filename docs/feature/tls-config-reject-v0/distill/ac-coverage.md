# AC Coverage — tls-config-reject-v0 (DISTILL)

Each of US-TLS-01's 7 acceptance criteria → its observable → the test(s) that assert it.
Test file: `crates/aperture/tests/slice_09_tls_config_reject.rs`.

| AC | Criterion (US-TLS-01) | Observable (brief.md) | Test(s) | Status |
|----|----------------------|-----------------------|---------|--------|
| **AC-1** | `tls.enabled=true` → exit 2 + refusal event naming `tls.enabled` | seam: `Err(ConfigError)` names `tls.enabled`; binary: exit 2 + stderr `event=config_validation_failed` names `tls.enabled` | `ac1_tls_enabled_true_refuses_config_construction` (seam); `ac1_tls_enabled_true_binary_exits_two_naming_tls_and_binds_nothing` (binary `@real-io`) | RED (ignored) |
| **AC-2** | `auth.spiffe.enabled=true` (tls off) → exit 2 + event naming `auth.spiffe.enabled` | seam: `Err` names `auth.spiffe.enabled`, NOT `tls.enabled`; binary: exit 2 + stderr names `auth.spiffe.enabled` | `ac2_spiffe_enabled_true_refuses_config_construction` (seam); `ac2_spiffe_enabled_true_binary_exits_two_naming_spiffe_and_binds_nothing` (binary `@real-io`) | RED (ignored) |
| **AC-3** | both true → exit 2 + event names the requested knob(s); no silent proceed | seam: `Err` names BOTH; binary: exit 2 + stderr names BOTH | `ac3_both_knobs_true_refuses_naming_both` (seam); `ac3_both_knobs_true_binary_exits_two_naming_both_and_binds_nothing` (binary `@real-io`) | RED (ignored) |
| **AC-4** | on ANY refusal: no plaintext listener binds on `:4317`/`:4318`; no telemetry accepted | seam: no `Config` ⇒ bind path unreachable (strongest, refactor-proof); binary: connect-refused on both default OTLP ports after exit | structural via all 3 `*_refuses_config_construction` seam tests; black-box via `connect_refused_on_default_otlp_ports()` in all 3 `*_binary_*` tests | RED (ignored) |
| **AC-5** | both false → starts, `event=startup`, binds both listeners, NO refusal event | seam: `into_config` `Ok`; spawn: both ports bound, `startup` present, `config_validation_failed` absent | `ac5_both_knobs_false_into_config_succeeds`; `ac5_both_knobs_false_starts_binds_and_emits_no_refusal_event` | **GREEN** (passes today; non-regression guard) |
| **AC-6** | `[security]` tables absent ≡ both-false | seam: `into_config` `Ok`; spawn: identical to AC-5 | `ac6_security_tables_absent_into_config_succeeds`; `ac6_security_tables_absent_starts_binds_and_emits_no_refusal_event` | **GREEN** (passes today; non-regression guard) |
| **AC-7** | `sinks.rs:94-95` comment corrected to describe the real refusal | source-comment text (code-review/lint observable, **not runtime** — brief.md classifies it so) | `ac7_comment_correction_is_a_deliver_verified_criterion` (documents the decision; the actual correction is a **DELIVER code task**) | DELIVER-verified |

## AC-4 — the no-plaintext-bind guarantee, three ways

AC-4 is the security crux (no cleartext on refusal). It is asserted redundantly:

1. **Structural (strongest)** — the 3 `*_refuses_config_construction` seam tests assert
   `into_config` returns `Err`. Per ADR-0061, `run → wire_sink → spawn → spawn_grpc/http`
   is the *only* bind path, and it is entered only after a `Config` is successfully
   constructed. No `Config` ⇒ the bind path is never reached. This survives refactors of
   `spawn` ordering — it is not "the check sits before the bind", it is "the bind type
   never exists".
2. **Black-box connect-refused** — the 3 `*_binary_*` tests, after asserting exit 2, call
   `connect_refused_on_default_otlp_ports()`: a TCP connect to `127.0.0.1:4317` AND
   `:4318` must be refused. A refused connect is the operator-observable "no listener".
3. **No silent proceed (AC-3)** — both-true names BOTH knobs and refuses; it does not pick
   one and bind plaintext.

## AC-7 — why it is DELIVER-verified, not a runtime test

The brief's For-Acceptance-Designer note states AC-7 is *"a code-review/lint observable,
not a runtime one"*. Asserting the literal text of a source comment from an integration
test couples the suite to a source-line detail and breaks on any rewording. The marker
test `ac7_comment_correction_is_a_deliver_verified_criterion` records this decision in the
suite. **DELIVER task**: update `sinks.rs:94-95` from the false *"the config validator
rejects it ahead of this sink"* (a rejection that did not exist) to the now-true statement
that `tls.enabled=true` / `auth.spiffe.enabled=true` cause config validation to refuse
startup (ADR-0061) before the sink is ever constructed (ADR-0061 §"Comment correction").

## Coverage completeness

- All 7 ACs mapped. ✔
- Every refusal row of the two-knob truth table (3 rows) has BOTH a seam test and a
  binary `@real-io` test. ✔
- Both negative-control rows (knobs-off, `[security]`-absent) have a seam test and a
  spawn-and-bind test, asserting `startup` present + refusal-event absent. ✔
- Error/refusal ratio 6/11 = 55% (≥ 40%). ✔
