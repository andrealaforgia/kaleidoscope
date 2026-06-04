# Slice 04 — Harness validation-depth honesty

- **Story**: US-04
- **Priority**: P4
- **Type**: Pure prose (lib.rs + README + Cargo.toml)
- **Independently shippable**: yes
- **DESIGN weight**: light (prose); pairs with Slice 06

## Value

The conformance harness stops claiming semantic OTLP-wire-specification
validation when it performs only structural decode-level validation.

## Exact loci (verified)

| File:line | False claim | Truth source |
|-----------|-------------|--------------|
| `otlp-conformance-harness/src/lib.rs:1-7` | "validates byte sequences against the OpenTelemetry OTLP **wire specification**" | `validate.rs:15-43` + `decode.rs:117-135`: non-empty, first tag = resource field #1, prost-decodes as asserted type, signal-mismatch fallback. NO semantic checks. |
| `otlp-conformance-harness/README.md:3-4` | same | same |
| `otlp-conformance-harness/Cargo.toml:11` | "Validates byte sequences against the OpenTelemetry OTLP wire specification." | same |
| `otlp-conformance-harness/README.md:8-16` | "Status: … implementation intentionally absent … every `validate_*` returns `unimplemented!()`" | `lib.rs:17-22` "three validators are implemented and green" |

## Corrected claim (canonical)

"Validates that a byte sequence **decodes structurally** as the asserted OTLP
signal type: non-empty, first wire tag references the resource field, and prost-
decodes as the asserted `Export*ServiceRequest`, with a signal-mismatch fallback.
This is **decode-level, not semantic** — it does NOT check trace_id/span_id
length, timestamps, attributes, or semantic conventions."

## Acceptance shape (for DISTILL)

- Guard: "wire specification" semantic overclaim ABSENT in the 3 depth loci;
  "structural decode-level" + named-absent-semantic-checks PRESENT; README status
  describes green validators (no "unimplemented").
- One acceptance test: a structurally-valid `ExportTraceServiceRequest` with a
  4-byte `trace_id` is ACCEPTED by `validate_traces` — pinning the documented
  semantic boundary.

## Guardrails

- Pure prose; does NOT change validation behaviour. The `GrpcProtobuf` framing
  decision is Slice 06, not here.
