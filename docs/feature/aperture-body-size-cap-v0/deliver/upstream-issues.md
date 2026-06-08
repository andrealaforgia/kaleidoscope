# Upstream issue — DD2/C2 "unset = no cap = unbounded" rested on a wrong premise

**Feature:** `aperture-body-size-cap-v0`
**Raised at:** DELIVER (fix-forward, post-merge) — 2026-06-08
**Severity:** HIGH (a DoS regression in the DEFAULT posture of a DoS-guard feature)
**Status:** corrected in code + docs (this fix-forward)

## The DISCUSS/DESIGN assumption

DESIGN `design/wave-decisions.md > DD2 (resolves DISCUSS D1)` states (verbatim):

> **`None` (unset) = no cap = today's exact behaviour** (C2). A `0`, if
> reachable, is treated as "no cap" at the accessor/parse boundary, never a
> zero-byte reject-everything limit (US-03 scenario 3).

and DD1's HTTP arm (`design/wave-decisions.md:29-34`):

> The three handlers stop extracting the bare `body: Bytes` and read through a
> length-checked path consulting the cap on `HttpState`. **The bare
> `axum::extract::DefaultBodyLimit` layer is NOT used alone** because its silent
> 413 cannot carry the `body_too_large` event [...]

The body_size_cap.rs no-cap branch was implemented from that premise, with the
doc-comment: *"the no-cap path is byte-for-byte today's behaviour"* — an
UNBOUNDED `body.collect()`.

## Why the premise was wrong

"Today's behaviour" for the HTTP arm was NOT unbounded. BEFORE this feature
(commit `ad8436d`) the three handlers extracted `body: axum::body::Bytes`. In
axum 0.7.9 / axum-core 0.4.5 the `Bytes` extractor enforces a **2 MB
`DefaultBodyLimit`** (`DEFAULT_LIMIT = 2_097_152`) unless
`DefaultBodyLimit::disable()` is layered. `DefaultBodyLimit` is referenced ZERO
times in aperture, so the OLD default posture rejected any HTTP body over ~2 MB
with a 413 — even with no `max_recv_msg_size` configured.

The DESIGN note "the bare `DefaultBodyLimit` layer is NOT used alone" correctly
observed that `DefaultBodyLimit` cannot carry the custom event — but it MISSED
that switching the extractor from `Bytes` to a raw `Body` (to gain the
length-checked seam) silently REMOVES the 2 MB default the `Bytes` extractor was
giving us for free. So `None` = "today's behaviour" was implemented as
UNBOUNDED, when today's behaviour was 2 MB-bounded.

The net effect: the feature whose whole purpose is a DoS guard shipped a DEFAULT
posture (every existing deployment, none of which sets `max_recv_msg_size`) that
was *weaker* than before — an unbounded `body.collect()` on the HTTP arm. The
regression was masked because the slice-11 unset controls deliberately stay
UNDER 2 MB (`logs_body_large_under_axum_default`, ~419 KB), so no test drove a
body across the old 2 MB threshold.

## The correction

"Backward-compatible" for the unset path means **preserving** the prior 2 MB
framework default, NOT removing it. The fix:

- `DEFAULT_HTTP_BODY_LIMIT_BYTES = 2 * 1024 * 1024` — a named const documenting
  that it preserves axum 0.7's pre-existing `DefaultBodyLimit`.
- The unset (`None`/`0`) HTTP path now bounds its collect to that default and
  rejects an over-default body with a **plain 413** (NO `body_too_large` event)
  — matching the OLD `Bytes`-extractor behaviour, which had no such event. The
  custom event remains reserved for an explicitly CONFIGURED cap.
- A configured `Some(limit)` REPLACES the default outright (may be higher or
  lower) — unchanged.

C2 ("unset = no cap = today's exact behaviour") is now TRUE again, because
"today's behaviour" is correctly modelled as the 2 MB framework default rather
than as "unbounded".

## gRPC arm — checked, NOT regressed

The gRPC unset path was checked and is NOT affected. On the unset path
`GrpcBodyCapLayer` is a pass-through and `with_decoding_backstop` leaves the
service untouched, so tonic 0.12.3's native `DEFAULT_MAX_RECV_MESSAGE_SIZE =
4 * 1024 * 1024` (4 MB) default still applies. tonic's 4 MB default is the
gRPC-arm equivalent of axum's 2 MB default; it was never dropped, so the gRPC
unset posture remained backward-compatible. No gRPC change was needed.

## Falsifiable guard added

`tests/slice_11_body_size_cap.rs ::
unset_cap_body_over_axum_2mb_default_still_rejected_413_sink_untouched` drives a
~3 MB body at an UNSET cap through the real HTTP ingest endpoint and asserts 413
+ empty sink. It FAILED against the unbounded code (200, full body forwarded to
the sink) and passes against the fix. Three `body_size_cap` unit tests pin the
2 MB const value and the inclusive at-default / over-default boundary for
mutation coverage.

## Back-propagation owed upstream

If this feature is ever re-derived (or a similar extractor-swap is designed
elsewhere), DISCUSS/DESIGN must record: **when replacing a framework extractor
that carries an implicit default (axum `Bytes` -> raw `Body`, tonic typed-decode
-> raw frame), re-apply the implicit default explicitly, because "unset = no
change" only holds if the prior default is preserved, not silently dropped.**
