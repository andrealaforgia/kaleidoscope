# DELIVER wave decisions — aperture-body-size-cap-v0

## Fix-forward 2026-06-08 — restore the 2 MB default body limit on the unset HTTP path

**Trigger.** Post-merge defect review of the just-delivered feature. The
`Bytes` -> raw `Body` extractor switch (needed to gain the length-checked +
event-emitting read seam, DD1) silently dropped axum 0.7's pre-existing 2 MB
`DefaultBodyLimit` on the HTTP arm. The unset (default) path therefore ran an
UNBOUNDED `body.collect()` — a DoS regression in the DEFAULT posture of a
DoS-guard feature, weaker than before the feature for every deployment that does
not set `max_recv_msg_size` (i.e. all of them at v0).

**Verification (test-first).** A new acceptance test drove a ~3 MB body at an
UNSET cap through the real HTTP endpoint and observed HTTP 200 with the full
3,288,955-byte body forwarded to the sink (`event=request_received bytes=3288955`,
`event=sink_accepted record_count=75000`) — regression CONFIRMED RED.

**Fix.** `DEFAULT_HTTP_BODY_LIMIT_BYTES = 2 * 1024 * 1024`; the unset HTTP path
bounds its collect to it and returns a plain 413 (no `body_too_large` event,
matching the OLD `Bytes`-extractor 413 which had no event) on an over-default
body. A configured `Some(limit)` still REPLACES the default. gRPC was checked
and NOT regressed — tonic's native 4 MB `max_decoding_message_size` default
still applies on the unset path (pass-through layer + untouched backstop), so it
was left unchanged.

**Truth correction.** The DD2/C2 "unset = no cap = today's exact behaviour"
premise assumed today's behaviour was unbounded; it was 2 MB-bounded via axum's
`Bytes` extractor. Doc-comments in `body_size_cap.rs` and `transport.rs`
corrected to describe "unset" as "the preserved framework default", not
"unbounded / no cap". Full record:
`docs/feature/aperture-body-size-cap-v0/deliver/upstream-issues.md`.

**Semver.** Additive/internal; aperture stays `0.1.0`.

**Scope of change.** `crates/aperture/src/body_size_cap.rs`,
`crates/aperture/src/transport.rs` (doc-comments only),
`crates/aperture/tests/slice_11_body_size_cap.rs` (one acceptance test + helper;
three unit tests in the cap module).
