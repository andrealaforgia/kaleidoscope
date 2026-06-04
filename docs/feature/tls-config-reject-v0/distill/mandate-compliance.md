# Mandate Compliance — tls-config-reject-v0 (DISTILL)

Evidence for the four acceptance-test design mandates. Test file:
`crates/aperture/tests/slice_09_tls_config_reject.rs`.

## CM-A — Hexagonal boundary (driving ports only)

All tests enter through one of the two **driving ports** named in brief.md's
"For Acceptance Designer" note, never an internal component:

- **In-process seam**: `Config::from_toml_str` (the public config-load entry point;
  `crates/aperture/src/config/mod.rs:103`) → `RawConfig::into_config`. Refusal observed as
  the returned `Result`. Followed, on the positive path, by `aperture::spawn` (the public
  composition entry point).
- **Binary subprocess**: `aperture --config <file>` via `CARGO_BIN_EXE_aperture` — the
  operator's actual entry point.

Imports in the test file (driving-port surface only):

```
use aperture::config::Config;        // public config entry point (from_toml_str)
use aperture::ports::OtlpSink;       // public port trait, for the spawn positive control
use aperture::testing::RecordingSink;// public test seam (same one slice_07 uses)
```

No internal validator, parser, or `RawConfig` private function is imported or called. The
refusal predicate inside `into_config` is exercised *indirectly* through the public
`from_toml_str` result and through the binary's exit code — never reached into directly.
**CM-A: PASS.**

## CM-B — Business language abstraction

This is a systems/CLI feature; the "business language" is the operator's domain:
configuration knobs, refusal, exit codes as an established operator contract, and
listeners. The user-story vocabulary (`tls.enabled`, `auth.spiffe.enabled`,
"refuse to start", "no plaintext listener", "exit code 2") IS the ubiquitous language of
US-TLS-01 and ADR-0061 — these are operator-facing config keys and the documented
exit-code contract, not leaked implementation internals.

Test names read as operator outcomes: `tls_enabled_true_refuses_config_construction`,
`both_knobs_true_binary_exits_two_naming_both_and_binds_nothing`,
`both_knobs_false_starts_binds_and_emits_no_refusal_event`. Assertions check the
operator-observable surface (exit code, stderr event naming the knob, connect-refused, a
bound port), per Dimension 7. No assertion reaches a private field or a method-call count.
**CM-B: PASS** (operator-domain language; `config_validation_failed`/`tls.enabled` are the
contract vocabulary, not jargon to be abstracted away).

## CM-C — User-journey completeness / walking skeleton

The walking skeleton is the binary refusal (test #2, `@real-io`): Priya runs
`aperture --config aperture.toml` with `tls.enabled=true`; the collector refuses to start
(exit 2), names the knob on stderr, binds no plaintext port. Complete journey: operator
trigger (run with a security-knob config) → system processes the rule (config validation
refuses) → observable outcome (exit 2 + named-knob stderr line) → business value (no
cleartext telemetry leaves the host; the operator learns immediately). Demo-able verbatim
to the security/compliance stakeholder. The negative controls prove the dual journey: with
knobs off, the operator gets a started, bound, telemetry-accepting collector (unchanged).
**CM-C: PASS.**

## CM-D — Pure-function extraction before fixtures

The only business logic this feature adds is the predicate **"is either security knob
true?"** — a pure two-field boolean read on the already-deserialised `RawConfig`
(`aperture.security.tls.enabled` / `aperture.security.auth.spiffe.enabled`), co-located in
`into_config` with the existing identical-bind-address validation. There is no impure
sub-operation to isolate behind a NEW adapter: the config-load adapter (figment) is
unchanged and pre-existing; the predicate touches no filesystem, network, or environment.

Fixture parametrization here is the **two-knob truth table** (4 input rows: tls-only,
spiffe-only, both, neither/absent) applied directly at the seam as distinct TOML inputs —
NOT an environment matrix. No cross-environment fixture parametrization is introduced; the
two proving environments (clean, ci; DEVOPS environments.yaml) run the *same* deterministic
checks. **CM-D: PASS** — minimal extraction, no environment-fixture explosion.

## Summary

| Mandate | Status | Evidence |
|---------|--------|----------|
| CM-A — driving ports only | PASS | imports = `config::Config`, `ports::OtlpSink`, `testing::RecordingSink`, binary subprocess; zero internal-component imports |
| CM-B — business/operator language | PASS | test names = operator outcomes; assertions = observable surface; vocab = config-key + exit-code contract |
| CM-C — journey completeness / WS | PASS | binary refusal `@real-io` skeleton = full operator journey, demo-able; dual negative-control journey |
| CM-D — pure extraction before fixtures | PASS | logic = pure two-bool predicate; fixtures = two-knob truth table at the seam, no env matrix |
