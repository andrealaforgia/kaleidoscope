# Slice 07 — TLS / SPIFFE schema knob (forward-compat insurance)

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: none — this slice is `@infrastructure` (config-schema only).
> **Depends on**: Slice 06.

## Outcome added

The v0 configuration schema **already carries** TLS and SPIFFE keys, defaulting off. v0 behaviour is plaintext-only; the schema is forward-compatible with Phase 2's Aegis arrival. If an operator sets `tls.enabled = true` on v0, Aperture emits a single warn-level stderr line at startup and continues plaintext.

This slice is **the only `@infrastructure` slice** in the v0 plan. It is justified at slice level because skipping it costs nothing now but breaks the schema in Phase 2 (Aegis), which is more expensive than adding it now.

## Why this slice is `@infrastructure` and ships anyway

The Elevator-Pitch test (PO review Dimension 0) flags `@infrastructure` slices as candidates for re-slicing because they have no user-visible value. Two facts justify Slice 07's existence in spite of this:

1. **The cost of not doing it is in another wave**: at v0 the absence of these keys is invisible; at Phase 2 their absence forces a config-schema break, which propagates to operator runbooks, k8s manifests, and CI fixtures. Andrea's locked Q5 decision says "schema present, default off" precisely to avoid that future break.
2. **It rides alongside slices that DO carry user-facing stories**: Slices 06 and 08 each ship demonstrable user value, and Slice 07's work is a config-schema test plus a startup warn-line — small enough that pulling it forward into either of those slices would have the same outcome with extra coupling. Keeping it as its own slice makes the forward-compat insurance traceable in the changelog.

## What it lights up

| Activity | Slice 07 coverage |
|---|---|
| Bind listeners | New stderr event `tls_not_supported_in_v0` when `tls.enabled = true`; listeners still bind in plaintext mode. |
| Receive payload | (Reuse — plaintext.) |
| Validate via harness | (Reuse.) |
| Hand off to sink | (Reuse.) |
| Observe self | (Reuse.) |
| Shut down gracefully | (Reuse.) |

## Demo command

```bash
# Run with the TLS knob set true on a v0 build.
cat > /tmp/aperture-tls-on.toml <<EOF
[aperture.security.tls]
enabled = true
cert_path = "/nowhere/cert.pem"
key_path  = "/nowhere/key.pem"
EOF

cargo run -p aperture -- --config /tmp/aperture-tls-on.toml

# Expected stderr: ONE warn-level line:
#   event=tls_not_supported_in_v0 reason="aperture v0 ships plaintext only; ignoring tls.enabled=true"
# Expected: listeners still bind in plaintext.
# Expected: no other behaviour change.
```

## Acceptance summary

- The config schema accepts the keys `aperture.security.tls.enabled`, `aperture.security.tls.cert_path`, `aperture.security.tls.key_path`, `aperture.security.spiffe.enabled`, `aperture.security.spiffe.trust_domain` without parse errors.
- All five keys default to off / empty when omitted from the config file.
- Setting `tls.enabled = true` produces exactly one `event=tls_not_supported_in_v0` stderr line at startup; behaviour is unchanged.
- Setting `spiffe.enabled = true` produces exactly one analogous warn line; behaviour is unchanged.
- A config-schema test (DESIGN-owned mechanism, conceptually a unit test on the parser) asserts that a config file containing all five keys parses successfully on a v0 build.

## Complexity drivers

- The smallest slice in the plan. Complexity is almost entirely in *not* doing more than necessary — the keys must exist in the schema, but the TLS code path itself does not.

## Known unknowns

- Whether the warn line should be `level=warn` or `level=info`. DISCUSS picks `warn` because misconfiguration (operator thinks they have TLS but does not) is more dangerous than verbosity.

## Out of scope

- Actual TLS termination (Aegis, Phase 2).
- Actual SPIFFE / SVID validation (Aegis, Phase 2).
- Mutual TLS (Aegis, Phase 2).
