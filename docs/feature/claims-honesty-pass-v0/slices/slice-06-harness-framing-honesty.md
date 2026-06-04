# Slice 06 — Harness `GrpcProtobuf` framing honesty (DESIGN flag #2)

- **Story**: US-06
- **Priority**: P6 (lowest reader-reach)
- **Type**: DOCUMENT-vs-IMPLEMENT (DESIGN decides)
- **Independently shippable**: yes (may share a PR with Slice 04)
- **DESIGN weight**: MEDIUM — a real decision

## Value

The harness's `GrpcProtobuf` framing claim matches its behaviour — an evaluator
feeding gRPC-framed OTLP bytes knows whether to strip the length prefix first.

## Exact loci (verified)

| File:line | State | Note |
|-----------|-------|------|
| `otlp-conformance-harness/src/framing.rs:16-18` | ALREADY ADMITS | enum doc: "the gRPC length prefix is the caller's responsibility to strip" |
| `otlp-conformance-harness/src/lib.rs` + `README.md` | DOES NOT FLAG | present `GrpcProtobuf` as a supported framing without flagging it is inert |

`framing` is never branched on in `validate.rs`/`decode.rs`; it is only echoed
into `OtlpViolation`. `GrpcProtobuf` is accepted but never acted on.

## The decision (DESIGN owns it)

- **Option A — document (DISCUSS recommends)**: state at `lib.rs`/README level
  that `Framing` is echoed into violations for diagnostics and does NOT change
  validation; for `GrpcProtobuf` the caller strips the gRPC length prefix. Pure
  prose.
- **Option B — honour**: strip the gRPC length prefix when `GrpcProtobuf` is
  asserted, so a length-prefixed body validates directly. Real capability; code
  touch + per-feature mutation obligation; belongs in its own feature.

## Acceptance shape (for DISTILL)

- Doc guard on the framing description (claim == behaviour).
- Acceptance test: prefix-stripped bytes validate identically under both framings
  (inert framing, under A); a length-prefixed body fails-requiring-strip (A) or is
  accepted via harness stripping (B), matching the corrected doc.

## Guardrails

- DISCUSS does NOT decide A vs B. The harness validation-depth correction (Slice
  04) is pure prose regardless; this slice is only the framing decision. If B,
  mutation obligation + behaviour change apply.
