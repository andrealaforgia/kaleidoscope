# Upstream Changes (DESIGN -> DISCUSS back-propagation): aperture-body-size-cap-v0

Author: Morgan (nw-solution-architect). Wave: DESIGN. Date: 2026-06-07.
British English. No em dashes.

DESIGN's DD1/DD1a placement decision (transport-boundary guard with a
disclosed bounded residual, and a gRPC `size` field that reports the value
the rejection surface observed rather than an exact fully-read byte count)
refines some unqualified wording in the DISCUSS artifacts. The DISCUSS
REQUIREMENT is unchanged and fully met (reject an over-limit body before the
harness decodes/validates it; emit one named event; accept at/under limit;
unset = no cap; cover logs+traces+metrics). What changes is the **wording of
the protection claim** in two places, so the AC the acceptance-designer locks
do not overstate what the placement delivers. This is the Earned-Trust
requirement DISCUSS itself flagged for DESIGN (D2: "DESIGN states the
strength achieved and words the AC honestly").

Nothing here weakens the feature. The boundary guard DESIGN chose is
**stronger** than the simplest app.rs seam DISCUSS allowed as a fallback; the
refinement is purely to make the claim precise, not to lower it.

## Change 1 — qualify "before it is decoded into memory" to the honest strength

### Original (DISCUSS `user-stories.md`, US-01 Elevator Pitch, "After")

> with `max_recv_msg_size` set, a body exceeding the cap is REJECTED before
> the harness decodes it

and the JTBD framing (`user-stories.md` line 23-28, `wave-decisions.md` Operator Job):

> cap the accepted body size so a single oversized payload is rejected
> loudly **before it is decoded into memory**

### Original (DISCUSS `user-stories.md`, US-01 UAT scenario "oversized logs body")

> ```gherkin
> Then aperture rejects the body before it is validated or forwarded to the sink
> ```

### Refinement for DISTILL (honest strength, DD1a)

The AC the acceptance-designer locks should read, at the honest protection
strength DESIGN achieves:

> rejected **before the harness validates/decodes it AND before the full
> oversized body is buffered/decoded into memory**

with the per-arm precision:

- **HTTP with `Content-Length` present**: MAY assert the stronger "rejected
  **before any body byte is read**" (the boundary rejects on the declared
  length).
- **HTTP with absent/lying `Content-Length`**: assert "rejected before the
  **full** body is buffered" (a bounded `<= ~one cap` of bytes may be read
  before the abort — NOT the full oversized body).
- **gRPC**: assert "the frame is refused **in the codec before decode**; the
  typed request is never allocated".

**Why**: the JTBD phrase "before it is decoded into memory" is true at the
strong guard for the dominant cases (HTTP `Content-Length`-present; gRPC
codec refusal), but an unqualified blanket claim would overstate the
absent/lying-`Content-Length` HTTP case, where the honest strength is "before
the full body is buffered", bounded to ~one cap of bytes. DISCUSS itself
anticipated this (the D2 flag and the risk "The check lands AFTER the bytes
are already in memory ... DESIGN MUST state the protection strength achieved
and word the AC honestly"). This back-propagation supplies that wording.

## Change 2 — `size` is the value the rejection surface observed (not always an exact byte count)

### Original (DISCUSS `user-stories.md`, US-01 Domain Example 2, and AC)

> `event=body_too_large transport=http_protobuf signal=logs limit=4194304
> size=209715200`

and US-01 AC:

> exactly one `body_too_large` event names the signal, the limit, and the
> **actual size**

and US-02 AC:

> a `body_too_large` event reporting `limit=N size=N+1`

### Refinement for DISTILL (honest size shape, DD3)

`limit` is always the configured cap and is exact. `size` is the value the
rejection surface **truthfully observed** at the point of rejection:

- **HTTP `Content-Length` present**: the declared `Content-Length` (the
  example `size=209715200` is exactly this case and stands).
- **HTTP streamed backstop**: the byte count at which the read aborted
  (`>= limit`).
- **gRPC**: the frame length tonic's decoder refused at (the observed length
  exceeding the cap).

So the US-02 boundary AC (`size=N+1`) holds where the surface observes the
exact size (e.g. an exact `Content-Length`); the acceptance-designer should
construct the boundary edges so the surface observes the size faithfully
(exact `Content-Length`), and word the general `size` assertion as "the size
aperture observed at the point of rejection", not "the exact fully-read byte
count" for the streamed/gRPC cases.

**Why**: the strong guard refuses before the body is fully read/decoded, so
aperture cannot always know an exact fully-read byte count without
re-introducing the OOM it is preventing. Reporting the observed value (and
naming the field honestly) is the Earned-Trust-correct choice; fabricating a
precise `size` the placement cannot observe would be the overstatement the
feature exists to remove.

## What is NOT changed

- The requirement that an over-limit body is **rejected** (not truncated, not
  dropped, not accepted) and **named in exactly one warn event** — unchanged
  and fully met.
- Inclusive-limit boundary semantics (`size <= limit` accepted, `size >
  limit` rejected) — unchanged (US-02).
- Unset = no cap = today's behaviour — unchanged (US-03, C2).
- The reject codes are now LOCKED by DESIGN (DD5): HTTP **413**, gRPC
  **`RESOURCE_EXHAUSTED`** (DISCUSS left these as D5 options "e.g. 413" /
  "e.g. RESOURCE_EXHAUSTED"; DESIGN confirms exactly those).
- Metrics is now LOCKED IN-SCOPE (DD4): the cap covers logs, traces, AND
  metrics (DISCUSS scoped logs+traces and named metrics as the D4 question;
  DESIGN includes it).

## Disposition

These are **wording refinements to keep the locked AC honest at the achieved
protection strength**, not requirement changes. No DISCUSS artifact needs
rewriting; the acceptance-designer reads this file alongside `user-stories.md`
and words the AC per the refinements above. Recorded per the project's
back-propagation discipline.
