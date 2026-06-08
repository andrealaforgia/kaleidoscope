# Evolution archive — aperture-body-size-cap-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md`,
`store-fsync-durability-v0-evolution.md`,
`tls-config-reject-v0-evolution.md`,
`claims-honesty-pass-v0-evolution.md`,
`beacon-sighup-reload-v0-evolution.md`,
`cli-ingest-atomic-v0-evolution.md`,
`cinder-wal-error-surfacing-v0-evolution.md`,
`aperture-serve-loop-error-surfacing-v0-evolution.md`,
`beacon-slo-operator-path-v0-evolution.md`,
`aegis-ingest-auth-v0-evolution.md`,
`spark-ingest-auth-v0-evolution.md`,
`perf-kpi-ci-non-gating-v0-evolution.md`,
`aperture-presubscriber-probe-stderr-v0-evolution.md`,
`speed-up-local-precommit-v0-evolution.md` and
`claims-honesty-pass-2-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below. This is a PRODUCTION-LOGIC feature (a new aperture
transport-boundary guard module, handler rewiring, config wiring, plus a
post-deliver regression fix), so the record is proportionate to that
scope and carries the load-bearing regression lesson in full.

## Status

- State: DELIVERED and pushed on `main`, then a post-deliver regression
  fix-forwarded and pushed on `main`. The whole story is below.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent. The upstream wave artefacts
  (discuss / design / devops / distill, plus ADR-0073) landed bundled
  into the DELIVER commit `7313f0b` rather than as separate per-wave
  commits; the as-built facts below are read from that commit and from
  the fix-forward commit `88ef2aa`.
- ADR: ADR-0073
  (`docs/product/architecture/adr-0073-aperture-body-size-cap.md`),
  which records the transport-boundary enforcement site, the honest
  protection-strength envelope, and decisions D1 through D5. It cites
  ADR-0010 (per-transport concurrency cap) as the refusal-shape and
  event-shape precedent, ADR-0008 (the `max_recv_msg_size` config field
  it finally wires) and ADR-0009 (the `body_too_large` event constant it
  finally emits). Supersedes nothing.
- Closes: the four-quadrants Q3 disclosed-omission item. `max_recv_msg_size`
  was parsed per transport arm for forward-compatibility
  (`#[allow(dead_code)]`, "unused at v0") but never reached `Config` and
  was never enforced; a `BODY_TOO_LARGE` event constant existed in
  `observability.rs` with no emitter. An operator who set the knob
  expecting OOM protection got none. An OTLP ingest gateway with no
  body-size limit is an OOM / DoS vector.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| deliver (incl. discuss/design/devops/distill artefacts + ADR-0073) | `7313f0b` | enforce `max_recv_msg_size` at the transport boundary; HTTP 413 / gRPC `RESOURCE_EXHAUSTED`; `body_too_large` event; app.rs secondary; 100% mutation kill on modified files |
| docs | `cd567e0` | narrative + slide for the slice (a guard placed after the cost is paid is not a guard) |
| deliver fix-forward | `88ef2aa` | preserve the 2 MB default body limit when `max_recv_msg_size` is unset; the `Bytes`->`Body` switch had dropped axum's `DefaultBodyLimit`, leaving the default posture unbounded |
| docs | `c942307` | coda for the regression and its fix (the guard that removed the old guard) |

## The problem, in Earned-Trust framing

This is the swallowed-resource-exhaustion sibling of ADR-0066
(serve-loop) and the cinder / sluice WAL fixes, turned on a DISCLOSED but
unwired knob. The four-quadrants Q3 report flagged two emitter-less /
enforcer-less artefacts in the live OTLP ingest gateway (aperture,
`tonic` gRPC on `:4317`, `axum` HTTP/protobuf on `:4318`):

- `max_recv_msg_size: Option<u32>` was parsed per transport arm
  (`config/mod.rs`, `#[allow(dead_code)]`, "unused at v0") but never
  reached `Config`, had no accessor, and was never enforced anywhere. An
  operator setting it, expecting the OOM protection the OTLP collector
  convention implies, got nothing.
- `BODY_TOO_LARGE` was a constant in the closed event vocabulary
  (`observability.rs`) with NO emitter. The vocabulary advertised a
  rejection event the code could never fire.

Today an oversized OTLP body was accepted and decoded into memory with no
cap. On the HTTP arm axum extracted the ENTIRE request body as
`body: Bytes` (already buffered into memory) before the handler ran; on
the gRPC arm tonic DECODED the protobuf frame into a typed request before
the handler ran. A single oversized payload was an OOM / DoS vector on
both arms. The load-bearing honesty requirement, the reason this is an
Earned-Trust feature and not a feature-add, is that the chosen
enforcement site MUST deliver the protection the operator assumes
(reject BEFORE the oversized body is buffered or decoded into memory),
and the acceptance criteria MUST be worded to the protection strength
actually achieved, never overstated.

## The design decision (ADR-0073)

The decision is: primary protection at the TRANSPORT BOUNDARY; the event
survives via a thin custom rejection seam at each boundary; an app.rs
`&[u8]` early-return is a DISCLOSED defence-in-depth secondary only.

### D2, the crux: enforce at the boundary, not the app core

The simplest seam is an `&[u8]` length check at the top of
`app::ingest_logs` / `ingest_traces`. It is the WEAKER guard: it runs
AFTER axum has buffered the full `Bytes` and AFTER tonic has decoded the
frame, so the oversized allocation has ALREADY happened. A guard that
fires only once the 200 MB body is fully in memory does not prevent the
OOM the operator set the cap to prevent. It guards the harness decode and
validate, not the allocation. Adopting it as the primary guard would
force the AC to overstate protection, which is exactly the
disclosed-omission pattern this feature exists to close.

The stronger, honest guard rejects before the body is buffered or
decoded, at the framework boundary:

- HTTP: the handlers stop extracting `body: Bytes` directly and read the
  body through a length-checked path that consults the configured cap.
  The cap is enforced against `Content-Length` FIRST (reject before
  reading a single byte when the declared length exceeds the cap) and
  against the actual streamed length as a backstop (so a lying or absent
  `Content-Length` cannot bypass it, the read aborting once the cap is
  exceeded, at most ~one cap of bytes buffered, NOT the full oversized
  body).
- gRPC: refuse the over-cap frame by inspecting the length-prefix before
  tonic decodes it, with `max_decoding_message_size(cap)` pinned as the
  deepest backstop. The typed request is never allocated.
- app.rs `&[u8]` early-return: kept as DISCLOSED defence-in-depth, NOT
  the primary guard. Its value is a belt-and-braces backstop if a future
  transport change ever lets a body past the boundary, and a single
  transport-agnostic place to build the event field shape once. It MUST
  NOT be sold as preventing the allocation.

### The honest protection-strength envelope, stated not overclaimed

ADR-0073 locks a per-arm protection-strength table the AC must use:
HTTP with `Content-Length` present is rejected before ANY body byte is
read (full OOM protection); HTTP with absent or lying `Content-Length` is
bounded to ~one cap of bytes before the abort (bounded, NOT unbounded,
and the AC says "before the full body is buffered", NOT "before any
byte"); gRPC is refused in the codec before decode. The locked claim is
"rejected before the harness decodes/validates it AND before the full
oversized body is buffered/decoded into memory", with the stronger
"before any byte" reserved for the `Content-Length`-present HTTP case
only. The claim equals the placement.

### D1, D3, D4, D5

- D1: a single collapsed `max_recv_msg_size` cap on `Config` shared by
  both transports (mirroring the concurrency precedent ADR-0010), not a
  genuine per-arm cap (deferred as surface for no v0 operator need). The
  `Option` is load-bearing: `None` (unset), and a `0` value, mean "no
  cap" decided at the accessor boundary, never a zero-byte
  reject-everything limit. The additive `pub` builder setter is the only
  new public surface.
- D3: `limit` is always the exact configured cap; `size` is the value
  the rejection surface TRUTHFULLY observed (declared `Content-Length`,
  the streamed-abort byte count, or the gRPC frame length tonic refused
  at). Never fabricate a precise `size` the placement cannot observe.
- D4: the cap covers logs, traces AND metrics in this slice. Excluding
  metrics would re-create the exact disclosed-omission pattern the
  feature closes.
- D5: HTTP `413 Payload Too Large` (the precise semantic for an entity
  exceeding the size limit, not 400); gRPC `RESOURCE_EXHAUSTED` (tonic's
  native status for the `max_decoding_message_size` rejection, matching
  ADR-0010's resource-protection framing, not `INVALID_ARGUMENT`).

No new crate, no new dependency. The `Config` field and accessor are
`pub(crate)` (mirroring `max_concurrent_requests`); the builder setter is
additive. aperture is not in the Gate 2 / Gate 3 public-API set, so the
ripple is internal and aperture stays at 0.1.0 (CLAUDE.md;
`semver_one_zero_is_andreas_call`).

## The as-built shape (deliver `7313f0b`)

- New `crates/aperture/src/body_size_cap.rs` module: `read_http_body_within_cap`
  (the length-checked HTTP read seam), `GrpcBodyCapLayer` (the gRPC
  length-prefix-inspecting layer wrapping each generated service), and
  `emit_body_too_large` (the single shared event constructor, `warn`
  level, carrying `transport` / `signal` / `limit` / `size`).
- `transport.rs`: the three HTTP handlers moved from the `Bytes`
  extractor to a raw `Body` so they can read through the length-checked
  path; each generated gRPC service gained the `GrpcBodyCapLayer` plus the
  `max_decoding_message_size` backstop.
- `app.rs`: the secondary defence-in-depth `&[u8]` size early-return on
  all three `ingest_*`, before `validate_*`, mapped to 413 /
  `RESOURCE_EXHAUSTED`.
- `config/mod.rs` and `compose.rs`: the single collapsed cap wired from
  the config arm to `Config` and onto the HTTP state and gRPC services.
- Acceptance: `tests/slice_11_body_size_cap.rs`, 16 tests green and 0
  ignored, covering the 19 distilled scenarios (input variations
  parametrised). The DWD-5 trap is the load-bearing test-design choice:
  the framework's own 2 MB `DefaultBodyLimit` would have given a FALSE
  GREEN (a 413 the framework produces, not the wired cap), so the tests
  use a tiny 16-byte cap against ~100-byte bodies, so that ONLY the WIRED
  cap can cause the 413. Binds use ephemeral `127.0.0.1:0`, avoiding the
  fixed-port 4317 / 4318 flake (`project_aperture_fixed_port_4317_flake`).
- Mutation (ADR-0005 Gate 5): `cargo mutants --in-diff`, 89 mutants, 55
  caught / 34 unviable / 0 missed, i.e. 100% kill on the modified files.
  The killable ceiling-arithmetic survivors in `collect_grpc_body_within_cap`
  and the overrun boundary were killed by two new at-ceiling unit tests;
  the `transport.rs` backstop boundary was deduplicated through a single
  mutation-covered `active_cap` source; one genuine equivalent (the
  `poll_ready` delegate to tonic's always-ready service) is marked
  `#[cfg_attr(test, mutants::skip)]` and justified in
  `deliver/mutation-equivalent-mutants.md`.

## The regression and its fix (`88ef2aa`), the load-bearing evolution lesson

The `Bytes` -> raw `Body` extractor switch, needed to gain the
length-checked event-emitting read seam, SILENTLY DROPPED a guard the old
code carried for free. In axum 0.7.9 / axum-core 0.4.5 the `Bytes`
extractor enforces a 2 MB `DefaultBodyLimit` unless explicitly disabled.
`DefaultBodyLimit` is referenced zero times in aperture, so the OLD
default posture already rejected any HTTP body over ~2 MB with a 413, even
with no `max_recv_msg_size` configured. Switching the extractor to a raw
`Body` removed that 2 MB default. So with the cap UNSET, which is the
default every existing deployment runs (none set the knob at v0), the HTTP
path became UNBOUNDED: a `body.collect()` with no ceiling. A DoS
REGRESSION in the DEFAULT posture of a DoS-guard feature, the default case
left WEAKER than before the feature shipped.

The regression was masked because the slice-11 unset controls
deliberately stay UNDER 2 MB, so no acceptance test had driven a body
across the old 2 MB threshold. It was caught by the Implementer's OWN
diligence on a hook handed to the Verifier, before the Verifier or anyone
outside re-verified.

Verified test-first: a new acceptance test drove a ~3 MB body at an UNSET
cap through the real HTTP endpoint and observed HTTP 200 with the full
3,288,955-byte body forwarded to the sink (75,000 records accepted),
confirming the regression RED. The fix: a named
`DEFAULT_HTTP_BODY_LIMIT_BYTES = 2 * 1024 * 1024` const; the unset
(`None` / `0`) HTTP path now bounds its collect to that default and
returns a PLAIN 413 (no `body_too_large` event, matching the old
`Bytes`-extractor behaviour, which fired no such event) on an
over-default body. A configured `Some(limit)` still REPLACES the default
outright. C2 ("unset = no cap = today's exact behaviour") is TRUE again,
because "today's behaviour" is now correctly modelled as the 2 MB
framework default rather than as "unbounded".

The doc-comment claiming the unset path was "byte-for-byte today's
behaviour" was ITSELF false (it described an unbounded collect as
backward-compatible) and was corrected in `body_size_cap.rs` and
`transport.rs` to describe "unset" as the PRESERVED framework default,
not "unbounded / no cap". The DD2 / C2 design assumption (unset = no cap =
today's behaviour, premised on today's behaviour being unbounded) was
back-propagated upstream in `deliver/upstream-issues.md`: when replacing a
framework extractor that carries an implicit default (axum `Bytes` ->
raw `Body`, tonic typed-decode -> raw frame), re-apply the implicit
default explicitly, because "unset = no change" only holds if the prior
default is PRESERVED, not silently dropped.

The gRPC arm was checked and NOT regressed. On the unset path
`GrpcBodyCapLayer` is a pass-through and the decoding backstop is left
untouched, so tonic 0.12.3's native 4 MB `max_decoding_message_size`
default (the gRPC-arm equivalent of axum's 2 MB) still applies. No gRPC
change was needed. The fix touched `body_size_cap.rs`, doc-comments in
`transport.rs`, one acceptance test plus helper, and three unit tests
pinning the 2 MB const and the inclusive at-default / over-default
boundary for mutation coverage. Additive / internal; aperture stays
0.1.0.

## The proof and its boundary

- Acceptance: `tests/slice_11_body_size_cap.rs`, 16 tests green and 0
  ignored at the DELIVER commit, plus the fix-forward acceptance test
  `unset_cap_body_over_axum_2mb_default_still_rejected_413_sink_untouched`
  (RED against the unbounded code, green against the fix). The tiny
  16-byte cap design ensures the WIRED cap, not the framework default, is
  what the green tests exercise.
- Mutation (ADR-0005 Gate 5): 89 mutants, 0 missed (100% kill on the
  modified files), one justified equivalent on the tonic `poll_ready`
  always-ready delegate. The fix-forward's three new unit tests extend
  that coverage to the restored default const and its boundary.
- The honest protection-strength envelope is the proof boundary: HTTP
  with `Content-Length` present rejects before any byte; HTTP with
  absent or lying length is bounded to ~one cap, NOT zero; gRPC refuses
  in the codec before decode. These are stated, not overstated, and the
  AC is worded to exactly them.
- SemVer (Gate 2 / Gate 3): none; aperture stays 0.1.0; never 1.0.0
  (CLAUDE.md; `semver_one_zero_is_andreas_call`).

## Note for the operator

This feature changes runtime behaviour on the live ingest gateway. With
`max_recv_msg_size` SET, an over-cap OTLP body is now rejected at the
transport boundary before it is buffered or decoded into memory: HTTP
returns 413, gRPC returns `RESOURCE_EXHAUSTED`, and exactly one
`warn`-level `body_too_large` event is emitted naming the transport,
signal, limit and observed size, in the same JSON shape as
`concurrency_cap_hit`, so the existing scrape catches it. With
`max_recv_msg_size` UNSET (the default), the HTTP arm preserves the prior
2 MB framework default (plain 413, no event, byte-for-byte the old
posture) and the gRPC arm preserves tonic's native 4 MB default; the
unset fleet is genuinely untouched relative to before the feature. The
cap is the per-request memory ceiling on the ingest path; with ADR-0010's
`cap x body x transport` product, this `max_recv_msg_size` is the `body`
term operators multiply for pod sizing.

## The lesson

When you take over a job the framework was quietly doing for you, here the
axum `Bytes` extractor's 2 MB `DefaultBodyLimit`, you inherit the WHOLE of
it, including the part you did not know was there. The switch to a raw
`Body` was made to gain the length-checked event seam, and it silently
discarded a ceiling nobody had written down, leaving the default posture
weaker than before. And the default / unset configuration is the one MOST
deployments run, so a guard that only protects when explicitly configured
can leave the common case weaker than before it shipped: the regression
lived precisely in the posture every operator was actually in. Two
operational rules fall out and are now in the code and the ADR. Place a
guard BEFORE the cost is paid, at the boundary, not in the core where the
memory is already spent, otherwise it guards the decode, not the
allocation. And word the claim to the protection ACTUALLY given: the
honest envelope ("before the full body is buffered", with "before any
byte" only where the declared length lets it) is the claim that survives
an adversary reading the code, and the doc-comment that overstated "unset
is byte-for-byte today's behaviour" was itself a small lie that the fix
had to correct.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted. The four-quadrants
Q3 `max_recv_msg_size` disclosed-omission item (and the emitter-less
`body_too_large` constant) is CLOSED by this feature.

1. body-size-cap rejection counter. ADR-0073 D4 scoped the
   `body_too_large` event IN and explicitly DEFERRED a rejection counter
   metric. A future slice could add a counter alongside the event so an
   operator can rate-track rejections without log scraping. Open only if
   wanted.

2. genuine per-arm body cap (ADR-0073 D1 / Option D). v0 ships a single
   collapsed cap shared by both transports. If operators report needing
   divergent HTTP and gRPC caps (the arms buffer differently), this is the
   revisit gate. Open only if wanted.

3. gRPC exact-size fidelity (ADR-0073 D3). The gRPC `size` field reports
   the length tonic observed at refusal rather than an exact full-body
   byte count, because the strong guard refuses before full decode. If
   that proves operationally insufficient, the revisit is whether a
   bounded pre-decode length read can recover a more precise frame size
   without re-introducing the OOM. Open only if wanted.

4. faster-test-fsync-backend-v0. The fsync-bound durability bins remain
   I/O-bound in CI, paying the honest per-record `sync_all` of
   ADR-0049 / 0060. A future feature could speed them with a faster
   test-fsync backend or a batched-fsync test mode behind an env guard.
   Open.

5. read-path auth (the next aegis wire). The query / log-query /
   trace-query read APIs are still unauthenticated; aperture-storage-sink
   reaches through `.inner` and read-path tenant authority is deferred.
   Open.

6. ingest role-gating. ingest auth is authentication-only: any valid
   catalogued token may ingest. Rejecting a valid `viewer` on the write
   path is the deferred authorization decision; the `TenantContext.role`
   is already threaded, so the follow-up is one
   `if ctx.role != Operator { reject }` gate with no re-plumbing. Open.

7. aegis "JWKS"-vs-HS256 doc-fix. `aegis/src/lib.rs` overstates "JWKS";
   the validator is HS256 pre-shared-key only. Disposition: a `docs:`
   fix-forward or a trivial micro-wave. Open.

8. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap needs its own slice. Open.

9. sluice wiring. sluice remains UNWIRED: no gateway/server `src` path
   constructs or drives `FileBackedQueue`. The wiring is a separate,
   still-open slice. Open.

10. sluice torn-tail migration. sluice still carries the inline
    parse-or-die recovery loop; its migration to the shared
    `replay_wal_tolerating_torn_tail` routine is the tracked ADR-0059 §5
    follow-up. Open.

11. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
    doubles the store, because lumen has no idempotency key. The designed
    extraction (ADR-0064 DD-3): success-case dedup earns its own slice.
    Open.

12. ingest-bounded-memory. The buffer-all-then-flush design (ADR-0064)
    holds the whole input's records in RAM before commit. A future
    feature lifts it with a temp-WAL staging stage or a max-records
    streaming cap. Open.

13. ADR-0059 Decision 8 layer b, the AST structural check, remains
    UNWIRED. The structural pre-commit check asserting in-scope stores
    delegate to the shared wal-recovery routine and carry no `let _ =`
    swallow; the tool choice was deferred and remains deferred. It is
    feedback, not a gate, consistent with the pure trunk-based,
    no-required-checks posture; when wired it belongs in the local
    pre-commit stage (now the fast `--lib` stage). Open.

14. OTLP partial_success never populated. The OTLP `partial_success`
    response field is never populated, so partial-accept signalling is
    not surfaced to clients. Open.

15. beacon non-30d error budget periods. v0 supports ONLY a 30d error
    budget period. Other windows (7d, 90d) would each need their own
    `MWMBR_TABLE` row set and earn their own slice. Open only if wanted.
