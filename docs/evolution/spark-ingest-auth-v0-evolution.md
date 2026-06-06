# Evolution archive — spark-ingest-auth-v0

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
`beacon-slo-operator-path-v0-evolution.md` and
`aegis-ingest-auth-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0069
  (`docs/product/architecture/adr-0069-spark-ingest-auth.md`), which gives
  the Spark SDK the client-side KEY to the door ADR-0068 locked, and which
  carries a load-bearing `## Amendment (DISTILL back-propagation)` that
  SHRANK the feature mid-flight (see the back-propagation section below).
- Closes: the symmetric consequence of `aegis-ingest-auth-v0`. ADR-0068
  made aperture mandate a bearer on every ingest request, fail-closed; this
  feature is the client-side counterpart that lets the Spark SDK present
  one. Without it the integrator who upgraded to a secured gateway could
  not reach it at all: verifier scenarios E01-E04 (the Spark to aperture
  round-trip, traces AND logs) were GREEN pre-auth and went BLOCKED the
  instant the door was locked.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| deliver | `742536b` | real programmatic bearer knob via `with_metadata` on all three exporters |
| docs | `b1c2b62` | narrative + slide closure |

The DISCUSS, DESIGN, DEVOPS and DISTILL artefacts landed on `main` ahead
of DELIVER, each from its own wave agent; the as-built facts below are
read from the DELIVER commit `742536b`.

## The problem, in Earned-Trust framing

This is the client-side echo of aegis-ingest-auth. aperture now rejects
any ingest request that arrives without `authorization: Bearer <jwt>`
(`UNAUTHENTICATED` / `401`, `reason=missing_claim`, nothing stored). The
Spark SDK had NO programmatic way to present that token. Confirmed at
DESIGN by reading the source: `SparkConfig` had no auth knob (`with_endpoint`
was the only transport knob); none of the three exporters
(`SpanExporter` / `LogExporter` / `MetricExporter`) attached auth metadata
or installed an interceptor; and the `init.rs` `use` imported
`WithExportConfig` but not the `WithTonicConfig` trait that carries
`with_metadata`. So an integrator who upgraded to a secured gateway had a
correct, secured collector in front of them and no key in hand. The
telemetry simply did not arrive: E01-E04 were blocked, not failing
loudly, just denied at a door the SDK could not knock on.

## The DISTILL back-propagation that shrank the feature

This is the load-bearing story of the feature, and it is an Earned-Trust
parable. The original DD4 took it on faith that Spark must PARSE
`OTEL_EXPORTER_OTLP_HEADERS` itself: a spark-owned comma-list parse, a
case-insensitive `authorization` extraction, a percent-decode, and a
fail-fast on a malformed value. An entire CREATE row in the Reuse
Analysis was budgeted for that parser.

DISTILL (Scholar) falsified the assumption by READING THE LOCKED
DEPENDENCY rather than trusting the brief. `opentelemetry-otlp =0.27`
ALREADY honours `OTEL_EXPORTER_OTLP_HEADERS` natively, unconditionally, on
the exact construction path Spark uses. Traced end to end through the
locked source: `.with_tonic()...build()` reaches `build_channel`, which
calls `parse_headers_from_env` UNCONDITIONALLY, before and independent of
whether `.with_metadata` was ever called, with no `from_env` gate; the
env value is percent-decoded upstream (`Bearer%20<jwt>` becomes
`Bearer <jwt>`); and the same path fires for all three signals. The
env-honouring half of this feature already worked CODE-FREE. The
spark-owned parser was redundant, and it was DELETED FROM THE DESIGN
BEFORE A LINE OF IT WAS WRITTEN.

The deeper finding was a mutual test-don't-trust. The verifier's msg-038
reported, black-box, that "spark doesn't honour `OTEL_EXPORTER_OTLP_HEADERS`",
and my own msg-028 repeated it. Both claims were UNTESTED. Reading the
dependency and then RUNNING the env-before-init probe against the real
aperture showed the env path was a VALID KEY THE WHOLE TIME: the gateway
accepted an env-set bearer with no spark change. msg-038 was therefore
environmental (the var was most plausibly not inherited by the process
that ran `spark::init`, or set after exporter build), not a code gap. The
report and its echo had each trusted the other rather than the substrate.

A second substrate truth fell out of the same reading and decided
precedence (below): if Spark attached a programmatic `authorization` AND
the env also carried one, BOTH reach `build_channel`, and the env value
WINS, because the merge is `HeaderMap::extend`, which OVERWRITES a present
key rather than appending. The original DD2's "programmatic wins" would
have been silently violated by the library. The amendment reframed
precedence to match reality: env-as-override on key collision, documented
not coded.

## The decision lineage

### ADR-0069 plus its immutable Amendment

ADR-0069 was accepted with a full DD1-DD5 plan including the spark-owned
env parser. The `## Amendment (DISTILL back-propagation)` section was then
APPENDED, not a rewrite: the original Decision stays on the record as the
as-accepted text, and the amendment GOVERNS DELIVER where the two
disagree. It WITHDREW the `OTEL_EXPORTER_OTLP_HEADERS`-parser CREATE row,
REVISED DD2 precedence to env-as-override, and DROPPED the spark-owned
malformed-header AC (env parsing is upstream's concern now, and upstream's
behaviour for a malformed env value is a silent drop, `.ok()?`, not
fail-fast; Spark cannot impose fail-fast without re-adding the parser it
just removed). What STOOD unchanged was the genuinely spark-owned core:
DD1 (the `with_metadata` attachment via one helper across all three
exporters), the DD2 surface (one additive `with_bearer_token`), DD3 (the
redacting newtype), DD5 (the byte-unchanged no-token path), and the Gate
2/3 public-API consequence.

### It reuses aegis's opaque-Debug redaction precedent

The never-log posture is not invented here. It mirrors aegis's
hand-written `Debug` that renders the HS256 signing key as `<opaque>`. The
`BearerToken` newtype renders `<redacted>` on the same principle: the
secret-ness travels with the value, localised to one tiny type, so a
future `#[non_exhaustive]` field addition cannot accidentally un-redact
the token.

### Env-wins precedence is upstream's `HeaderMap::extend`, documented not coded

The precedence rule is not Spark's code. It is the locked upstream's
`extend`-overwrites behaviour, surfaced honestly in the
`with_bearer_token` rustdoc: the programmatic knob is the supported API,
and a concurrently-set `OTEL_EXPORTER_OTLP_HEADERS=authorization=...` is an
operator override that upstream applies last on key collision. Spark
writes zero env-handling code; it documents the substrate's truth rather
than pretending to a precedence the library would silently break.

## The as-built shape

### The programmatic knob, one helper, the apply shim across all three exporters

`SparkConfig::with_bearer_token(impl Into<String>)` is the one new public
method, additive on the `#[non_exhaustive]` struct, recording the token in
a private `bearer_token: Option<BearerToken>` field defaulted `None`. At
`build_pipeline`, one `pub(crate)` free function
`build_auth_metadata(&SparkConfig) -> Result<Option<MetadataMap>>` does a
knob-only resolution: when the token is set it returns `Some` carrying
exactly one entry, `authorization = "Bearer <token>"`, built ONCE; `None`
otherwise. A single generic free-fn shim `apply_auth<B: WithTonicConfig>`
is cloned into all three `.with_tonic()` exporter builders
(span / log / metric), so no signal can be left un-authenticated by
omission, the all-three anti-omission property. `WithTonicConfig` was
added to the `opentelemetry_otlp` `use`, and `tonic` was named directly
for `MetadataMap` / `MetadataValue` (already in `Cargo.lock` via the
`opentelemetry_otlp` to `tonic` chain, also a direct dep of aperture; NO
new external crate, so Gate 4 is unaffected).

### The BearerToken redacting newtype

The token lives inside `BearerToken`, whose `Debug` renders
`<redacted>` and which has no value-`Display`. `SparkConfig`'s derived
`Debug` recurses into it, so `dbg!` / `panic!("{config:?}")` never renders
the JWT. The raw value is reached only via a single `pub(crate)`
`expose()` accessor whose ONLY caller is `build_auth_metadata`, which
writes it into the `MetadataMap` (the wire), never into a `tracing` macro.
A token whose bytes are not a valid HTTP header value surfaces as
`SparkError::ExporterInitFailed { reason }` at metadata-build time, the
reason naming the kind and NEVER echoing the token bytes.

### Zero env code; the no-token path byte-unchanged

There is NO spark `OTEL_EXPORTER_OTLP_HEADERS` parser, NO percent-decode,
NO spark-owned malformed-env fail-fast; the env half is upstream's
entirely. When no token is resolved, `build_auth_metadata` returns `None`,
the apply shim leaves all three builders untouched, and the exporters are
built byte-identically to the pre-auth code. The local-collector
no-auth workflow is preserved; `slice_01..slice_07` stay green.

## The proof and its boundary

- 100% mutation kill on the MODIFIED spark surface (ADR-0005 Gate 5;
  CLAUDE.md per-feature 100%), via `cargo mutants --in-diff` on the
  modified files (`config.rs`, `init.rs`): 4 mutants, 2 CAUGHT, 2
  UNVIABLE, 0 MISSED, 0 timeout. The existing `gate-5-mutants-spark` job
  carries it; no new CI job was needed.
- The never-log guard is a hard gate: a configured, recognisable token is
  greped across every Spark log surface, the `SparkConfig` `Debug`, and
  the error paths, asserting ZERO occurrences and the redacted placeholder
  where the field renders.
- The env-happy-path test is GREEN WITH NO SPARK CHANGE: an
  `OTEL_EXPORTER_OTLP_HEADERS=authorization=Bearer%20<jwt>` set before
  `spark::init`, exported to the real aegis-authenticated aperture, is
  ACCEPTED. This is the empirical disambiguation probe (principle 12) that
  reconciles msg-038 environmental rather than a code gap, and it is kept
  un-ignored as a non-regression guard against any future double-attach.
- The programmatic-knob accept is proven end to end through a real
  aegis-authenticated aperture with a `RecordingSink` and an in-suite
  HS256 mint (the ADR-0068 F5 seam, reused verbatim): a valid bearer
  configured via `with_bearer_token` is accepted; the no-token control is
  denied `missing_claim`, sink empty. A metric-only accept is the
  falsifiable all-three witness (a partial wire that authenticated only
  traces and logs would deny the metric export and fail), complemented by
  the `build_auth_metadata` unit assertion in DELIVER's inner loop that
  kills the drop-`.with_metadata`-on-one-builder mutant. The env-wins
  precedence test asserts the documented reality (both set, env value on
  the wire). No leaked aperture processes.

## The Gate 2/3 consequence: spark IS public-API tracked

This is the real difference from the recent cinder/aperture features. Unlike
those crates, `spark` IS in the Gate 2/3 public-API set (Gate 2
`cargo public-api -p spark`, Gate 3 `cargo semver-checks --package spark`).
The new `with_bearer_token` is a PUBLIC method (the `BearerToken` newtype
and the field stay `pub(crate)`/private and do not enter the public API),
so Gate 3 classifies it as an additive MINOR change on a
`#[non_exhaustive]` struct. DELIVER bumped `crates/spark/Cargo.toml` from
`0.1.0` to `0.2.0`, additive MINOR, pre-1.0, NEVER 1.0.0 (a public
stability promise, Andrea's call alone). There is no public-api snapshot
file for spark; the baseline is the `origin/main` git checkout, and the
added method is accepted on merge.

## The verifier's self-correction

Recorded in the same spirit as the prior archives' honest-finding
sections. The verifier did not stand on her "blocked" call. In msg-052 she
TESTED the env path herself and E01-E04 went GREEN, correcting her own
msg-038 "spark doesn't honour it". The chain that began as a mutual
test-don't-trust (her untested report, my untested echo) closed with the
person who raised the doubt running the probe and retracting it against
real evidence. That is the loop the whole project exists to make routine:
the claim is settled by the substrate, not by who said it.

## Note for the operator: how to present the bearer

This feature adds no deployment precondition of its own (the gateway-side
refuse-to-start belongs to aegis-ingest-auth). It adds an SDK capability.
An integrator presents the credential in TWO supported ways: in code via
`SparkConfig::with_bearer_token(<jwt>)` (the primary, newly-shipped API),
or via the conventional `OTEL_EXPORTER_OTLP_HEADERS=authorization=Bearer%20<jwt>`
which `opentelemetry-otlp` honours code-free on Spark's path. If both are
set, the env value is the final writer on key collision (upstream
`HeaderMap::extend`), documented on the knob's rustdoc. The token is never
logged. Exporting to an authenticated gateway with no token yields
gateway-side `missing_claim` denials; exporting to an unauthenticated
collector with no token still works unchanged.

## The lesson

A feature is not measured by how much you add. The most honest code in
this feature is the parser that was DELETED: an entire spark-owned
`OTEL_EXPORTER_OTLP_HEADERS` parse, extract, percent-decode and fail-fast,
budgeted in the Reuse Analysis, withdrawn before a line of it was written,
because reading the locked dependency showed the env path was a valid key
the whole time. Test the claim and read the dependency rather than
trusting the report: a black-box "spark doesn't honour it" (msg-038) and
its repetition (msg-028) were BOTH untested, and a single run against the
real aperture falsified both. What remained was the genuinely missing
thing, the programmatic knob no upstream API provided, attached uniformly
to all three exporters, never logged, and shipped as the additive MINOR a
public-API-tracked crate demands. The smaller, truer feature is the
better one.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted. The aegis ingest
door is now reachable from the Spark SDK; read-path auth and ingest
role-gating remain the next aegis slices.

1. read-path auth (the next aegis wire). The query / log-query /
   trace-query read APIs are still unauthenticated; aperture-storage-sink
   reaches through `.inner` and read-path tenant authority is deferred.
   Wiring aegis onto the read path, with the full role matrix in view, is
   the next aegis slice. Open.

2. ingest role-gating. ingest auth is authentication-only: any valid
   catalogued token may ingest. Rejecting a valid `viewer` on the write
   path is the deferred authorization decision; the `TenantContext.role`
   is already threaded, so the follow-up is one
   `if ctx.role != Operator { reject }` gate with no re-plumbing. Open.

3. aegis "JWKS"-vs-HS256 doc-fix. `aegis/src/lib.rs` overstates "JWKS";
   the validator is HS256 pre-shared-key only. Disposition: a `docs:`
   fix-forward or a trivial micro-wave, correct text "validates against a
   configured issuer and audience using a pre-shared HS256 key (RS256/JWKS
   is v1)". Open.

4. spark general `with_auth_header(name, value)`. v0 ships only
   `with_bearer_token` (the exact need). A general header map is a clean
   non-breaking widening later on the same `#[non_exhaustive]` struct (the
   field stays `Option<BearerToken>`; a future `headers` vector is an
   independent addition). Open only if a non-`authorization` header is ever
   wanted.

5. spark per-request dynamic auth (the interceptor migration). v0 sets the
   token once at `init` via `.with_metadata` (rotation is deployment-managed
   via env/restart). If rotating in-process credentials ever land,
   `WithTonicConfig::with_interceptor` is the clean future migration. Open
   only if wanted.

6. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap needs its own slice. Open.

7. sluice wiring. sluice remains UNWIRED: no gateway/server `src` path
   constructs or drives `FileBackedQueue`. Its `Queue` surface was made
   fail-loud before it is wired (zero live blast radius); the wiring itself
   is a separate, still-open slice. Open.

8. sluice torn-tail migration. sluice still carries the inline
   parse-or-die recovery loop; its migration to the shared
   `replay_wal_tolerating_torn_tail` routine is the tracked ADR-0059 §5
   follow-up. Open.

9. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
   doubles the store, because lumen has no idempotency key. The designed
   extraction (ADR-0064 DD-3): success-case dedup earns its own slice.
   Open.

10. ingest-bounded-memory. The buffer-all-then-flush design (ADR-0064)
    holds the whole input's records in RAM before commit. A future feature
    lifts it with a temp-WAL staging stage or a max-records streaming cap.
    Open.

11. ADR-0059 Decision 8 layer b, the AST structural check, remains
    UNWIRED. The structural pre-commit check asserting in-scope stores
    delegate to the shared wal-recovery routine and carry no `let _ =`
    swallow; the tool choice was deferred and remains deferred. It is
    feedback, not a gate, consistent with the pure trunk-based,
    no-required-checks posture; when wired it belongs in the local
    pre-commit stage. Open.

12. OTLP partial_success never populated. The OTLP `partial_success`
    response field is never populated, so partial-accept signalling is not
    surfaced to clients. Open.

13. The two claims-honesty DOCUMENT items remain future features if
    wanted. The actual Prometheus-stepped grid for `query_range` (a
    query-api feature) and real gRPC-prefix honouring for `harness`
    (`Framing::GrpcProtobuf`) were documented as v0 reality rather than
    built; each would retire its respective pin. Open only if wanted.

14. beacon non-30d error budget periods. v0 supports ONLY a 30d error
    budget period. Other windows (7d, 90d) would each need their own
    `MWMBR_TABLE` row set and earn their own slice. Open only if wanted.
