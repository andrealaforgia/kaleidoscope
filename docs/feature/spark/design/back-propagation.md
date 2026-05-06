# Back-propagation note — `spark` v0 DESIGN to DISCUSS

> **Wave**: DESIGN.
> **Author**: Morgan (`nw-solution-architect`).
> **Date**: 2026-05-06.
> **Recipient**: Bea (orchestrator) for forwarding to Luna
> (`nw-product-owner`) if a DISCUSS contract update is warranted.

DESIGN's job is to lock technology and internal structure without
changing the DISCUSS-locked contracts. This file captures the **one
case** where DESIGN's investigation surfaced a contract that DISCUSS
should consider revising — **and the proposed text for that revision**.

DESIGN has NOT modified any DISCUSS artefact. The decision whether to
revise the DISCUSS contracts is Bea's; Luna would make the change if
Bea agrees.

---

## Issue 1 — Drained/dropped counts on the shutdown / flush-deadline events

### The DISCUSS contract today

`docs/feature/spark/discuss/user-stories.md > US-SP-06 > Acceptance Criteria`:

> - On clean flush: a single `tracing::info!(target: "spark")` event
>   with message containing `"shutdown complete drained=N"` is emitted.
> - On deadline: a single `tracing::warn!(target: "spark")` event with
>   message containing `"flush deadline exceeded dropped=M"` and the
>   configured `flush_timeout_ms` is emitted.

`docs/feature/spark/discuss/journey-spark.yaml > step 5 tui_mockup`:

```
INFO spark: shutdown complete drained=7

[On timeout — spans/logs/metrics still in batch processor's buffer when deadline elapses]
WARN spark: flush deadline exceeded dropped=3 flush_timeout_ms=${flush_timeout_ms}
```

The illustrative `drained=7` and `dropped=3` values, and the
acceptance criterion "containing `drained=N`", imply Spark reports an
**integer count** of drained / dropped records.

### What DESIGN found

The OpenTelemetry Rust SDK at the family-pinned 0.27 (ADR-0013 §1)
does NOT expose drained/dropped record counts publicly:

- `SdkTracerProvider::force_flush_with_timeout` returns `OTelSdkResult`
  (a `Result<(), OTelSdkError>` alias). No count.
- `BatchSpanProcessor`'s internal counters are private.
- The same applies to `BatchLogProcessor` and `PeriodicReader`.

DESIGN therefore locks (per ADR-0014 §2) the v0 event shape as:

```
INFO  spark: shutdown complete drained=unknown
WARN  spark: flush deadline exceeded dropped=unknown flush_timeout_ms=500
```

### Two paths forward

**Path A — Update DISCUSS to accept `=unknown`** (Morgan's
recommendation):

The acceptance criteria in US-SP-06 + the slice-06 §"Acceptance
summary" + the journey-spark.yaml tui_mockup all read "drained=N" /
"dropped=M" with N and M as numeric placeholders. Updating them to
"drained=N (where N is the SDK-exposed count if available, or
'unknown' if not)" preserves the contract intent (the event is
emitted, the deadline is bounded, the outcome is observable) while
acknowledging the SDK's actual API surface at v0.

**Proposed edits**:

1. `user-stories.md > US-SP-06 > Acceptance Criteria`: change
   "containing `drained=N`" to "containing `drained=N` (N is the
   SDK-exposed drained count if available; v0 with `opentelemetry_sdk
   =0.27` reports `drained=unknown` because the SDK does not expose
   the counter)".
2. `journey-spark.yaml > step 5 tui_mockup`: change `drained=7` to
   `drained=unknown` (illustrative for v0); change `dropped=3` to
   `dropped=unknown`.
3. `slices/slice-06-flush-deadline.md > Known unknowns`: tighten
   "Whether the `tracing` event field `dropped=N` is reliable on the
   OTel SDK 0.27 (or whatever version DESIGN pins) is the
   load-bearing uncertainty. If the SDK does not expose the count at
   all, the WARN event reads `'flush deadline exceeded (dropped count
   unavailable)'` with a documented limitation. DESIGN-wave (Morgan)
   decides." into a settled answer: "DESIGN ADR-0014 §2 confirms the
   OTel SDK at v0.27 does NOT expose the count. v0 emits
   `drained=unknown` / `dropped=unknown`. A future SDK release that
   exposes the counts can switch to the integer without breaking the
   v0 vocabulary contract."

This path keeps the v0 contract honest: the event is emitted, the
deadline is bounded, the outcome is observable; the count is
informational and best-effort.

**Path B — Ship a Spark-internal counter** (rejected, recorded for
posterity):

Spark could wrap each provider in a Spark-side processor that counts
records as they pass through. This would let Spark report integer
counts even when the SDK does not.

**Rejected** because:

- It duplicates state the SDK already tracks internally (just not
  publicly).
- Wrapping the processor changes the OTel SDK pipeline shape; future
  SDK upgrades would need to verify the wrapper still composes
  correctly.
- The v0 user value is the **bounded flush** + the **observable
  outcome event**, NOT the integer count. The count is informational.
- A future SDK release will likely expose the count; designing around
  the absence is throwaway code.

### Bea's call

Recommend Path A. The semantic intent of the DISCUSS contract is
preserved (event emitted, deadline bounded, outcome observable);
only the integer-vs-`unknown` literal changes.

If Bea forwards Path A to Luna, the changes are mechanical and
non-invasive. The DESIGN ADRs are already aligned with Path A; the
DISCUSS edits are documentation tightening, not contract change.

---

## Issue 2 — None.

There is no other DESIGN finding that requires a DISCUSS contract
revision. Every other DISCUSS contract is implementable verbatim by
the technology choices and ADRs DESIGN has produced.

DESIGN has answered the five "Suggestions for Morgan" Sentinel listed
in `peer-review.md`:

| Sentinel suggestion | DESIGN answer |
|---|---|
| 1. OTel semconv version verification | ADR-0013 §2: `service.name` uses `opentelemetry-semantic-conventions::resource::SERVICE_NAME`; `tenant.id`, `feature_flag.*`, `experiment.id` are Kaleidoscope-house attributes (not OTel-semconv at 0.27); migration path documented. |
| 2. OTel SDK version pin | ADR-0013 §1: exact-minor pin `=0.27` for the OTel family; mirrors harness ADR-0003 in style; migration path to `=0.28` and v1 documented. |
| 3. Flush-timeout mechanism | ADR-0014 §1: sequential flush with shared remaining-time budget; ADR-0014 §2: `drained=unknown` / `dropped=unknown` at v0 (this is Issue 1 above). |
| 4. GlobalAlreadyInitialised test mechanism | ADR-0015 §2: per-binary test isolation via `[[test]]` declarations; `tests/invariant_single_init.rs` is its own binary with a single `#[test]` function. |
| 5. SparkGuard posture | ADR-0016: opaque + `#[must_use]` with directive message + Drop-only + no public methods + minimum trait derives. |

DESIGN is otherwise clean against DISCUSS. Issue 1 is the one note
that warrants Bea's attention before DISTILL begins (so the
acceptance designer Atlas does not hard-code substring assertions
against `drained=7` literally).
