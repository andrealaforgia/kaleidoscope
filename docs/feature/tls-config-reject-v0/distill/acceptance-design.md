# Acceptance Design — tls-config-reject-v0 (DISTILL)

- **Feature ID**: tls-config-reject-v0
- **Wave**: DISTILL (nWave)
- **Designer**: Scholar (nw-acceptance-designer)
- **Date**: 2026-06-04
- **Story**: US-TLS-01 (7 ACs, 6 BDD scenarios)
- **Driving ADR**: ADR-0061 (supersedes ADR-0008 runtime reaction; schema preserved)
- **Test file**: `crates/aperture/tests/slice_09_tls_config_reject.rs` (slice 09 — next free; 08 = graceful_shutdown)

## Behaviour under test (not re-decided — fixed by ADR-0061)

Aperture **refuses to start** when `tls.enabled = true` OR `auth.spiffe.enabled = true`
(v0 implements neither transport encryption nor SPIFFE auth). The refusal lands in
`RawConfig::into_config` as `Err(ConfigError)`; `main.rs` catches it, emits a structured
stderr line carrying `event=config_validation_failed` naming the requested knob(s), and
exits with code **2**; **no listener binds**. Both knobs false/absent → start and bind
exactly as today.

## Driving port (from brief.md "For Acceptance Designer")

Two black-box surfaces, both operator-facing:

1. **In-process seam** — `Config::from_toml_str` → `RawConfig::into_config`. The same
   entry point slice 07 uses. Refusal observable: `Err(ConfigError)` whose message names
   the knob. This is the **strongest AC-4 guarantee**: because `Config` is never
   constructed, the bind path (`compose::spawn_grpc`/`spawn_http`) is *structurally
   unreachable* — no ordering discipline required (ADR-0061 §"Refusal point").
2. **Binary subprocess** (`@real-io`) — `aperture --config <file>` via
   `CARGO_BIN_EXE_aperture`. Observable: exit code 2 + a structured stderr line with
   `event=config_validation_failed` naming the knob + connection-refused on the OTLP
   ports. This is the operator-visible surface the user story actually describes (Priya
   runs `aperture --config aperture.toml`).

No JSON-on-stdout API exists for this path; the observable surface is **exit code +
structured stderr events + presence/absence of a bound listener**.

## Scenario inventory (11 tests, 6 ignored / 5 green)

| # | Test | AC | Surface | Ignored? | Category |
|---|------|----|---------|----------|----------|
| 1 | `ac1_tls_enabled_true_refuses_config_construction` | 1, 4 | seam | yes (RED) | error/refusal |
| 2 | `ac1_tls_enabled_true_binary_exits_two_naming_tls_and_binds_nothing` | 1, 4 | binary `@real-io` | yes (RED) | error/refusal |
| 3 | `ac2_spiffe_enabled_true_refuses_config_construction` | 2, 4 | seam | yes (RED) | error/refusal |
| 4 | `ac2_spiffe_enabled_true_binary_exits_two_naming_spiffe_and_binds_nothing` | 2, 4 | binary `@real-io` | yes (RED) | error/refusal |
| 5 | `ac3_both_knobs_true_refuses_naming_both` | 3, 4 | seam | yes (RED) | error/refusal |
| 6 | `ac3_both_knobs_true_binary_exits_two_naming_both_and_binds_nothing` | 3, 4 | binary `@real-io` | yes (RED) | error/refusal |
| 7 | `ac5_both_knobs_false_into_config_succeeds` | 5 | seam | **no** | negative control |
| 8 | `ac5_both_knobs_false_starts_binds_and_emits_no_refusal_event` | 5 | seam (spawn) | **no** | negative control |
| 9 | `ac6_security_tables_absent_into_config_succeeds` | 6 | seam | **no** | negative control |
| 10 | `ac6_security_tables_absent_starts_binds_and_emits_no_refusal_event` | 6 | seam (spawn) | **no** | negative control |
| 11 | `ac7_comment_correction_is_a_deliver_verified_criterion` | 7 | doc | **no** | DELIVER-verified marker |

**Error/refusal ratio**: 6 of 11 = **55%** (≥ 40% mandate). The negative controls are
first-class (they guard the fleet-wide non-regression — any embedder like `gateway`
must keep starting), not afterthoughts.

## Walking skeleton

This feature is a **single config-validation invariant** on an already-built binary, not
a new vertical slice. The thinnest demo-able user journey is the binary refusal itself:
*"Priya sets `tls.enabled=true`, runs `aperture --config aperture.toml`, and the collector
refuses to start (exit 2) naming the knob, binding no plaintext listener."* That is test #2
(`ac1_..._binary_exits_two_naming_tls_and_binds_nothing`), tagged `@real-io` — it drives
the real binary end-to-end through the operator's actual entry point and observes the
operator-visible outcome (refusal, named knob, no cleartext port). It is demo-able to the
security/compliance stakeholder verbatim. The negative-control binary positive-bind path is
deliberately NOT added (DEVOPS D4: fixed-port 4317/4318 collision); the positive path is
proven on the ephemeral in-process seam (tests #8, #10).

## RED-not-BROKEN posture

The refusal CODE does not exist yet (`into_config` has no reject branch; DELIVER adds it).
The 6 refusal tests are written against the **existing public API** (`from_toml_str`,
`into_config`, the binary), so they **compile** today but **fail behaviourally** (today
`into_config` returns `Ok(Config{ tls_enabled: true })`; the binary binds). Proven:
running `ac1_tls_enabled_true_refuses_config_construction --ignored` fails with
*"into_config returns Err(ConfigError): Config { … tls_enabled: true … }"* — a business
failure, not a setup/compile error. Every refusal test carries
`#[ignore = "RED until DELIVER: tls-config-reject-v0"]` so `cargo test --workspace` stays
green at the DISTILL commit. DELIVER removes the ignores.

## Mandate 4 — pure-function note

No new business logic is extractable beyond the boolean predicate "is either knob true?".
The refusal decision is a two-field read on the already-deserialised `RawConfig`
(`aperture.security.tls.enabled` / `aperture.security.auth.spiffe.enabled`) co-located with
the existing identical-bind-address validation in `into_config`. There is no impure
sub-operation to isolate behind a new adapter; the existing config-load adapter (figment)
is unchanged. Fixture parametrization is the two-knob truth table itself (4 input rows),
applied at the seam — not an environment matrix. See mandate-compliance.md (CM-D).
