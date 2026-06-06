# Evolution archive — aegis-ingest-auth-v0

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
`aperture-serve-loop-error-surfacing-v0-evolution.md` and
`beacon-slo-operator-path-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0068
  (`docs/product/architecture/adr-0068-aegis-ingest-auth.md`), which WIRES
  (does NOT re-engineer) the correct-but-unwired `aegis::Validator` onto
  the live aperture OTLP ingest path, fail-closed, reusing the validation
  core verbatim and the ADR-0061 refuse-to-start pattern verbatim.
- Closes: the four-quadrants Q3 "Tested But Unwired" gap headed on every
  recent archive's follow-up list as "aegis unwired (no path
  authenticates; the surface exists but is on no request path)". This is
  the dominant Q3 item: the unreachable thing was the front-door lock
  itself. The aegis-v0 D10 deferral ("Aperture/Beacon/Prism keep auth-free
  at v0; integrating aegis into each component is its own slice in v1") is
  the deferral this feature redeems for the ingest path.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| deliver | `7f72db8` | fail-closed ingest auth at the door, tenant ripple through the pipeline |
| docs | `9817ec9` | narrative + slide closure |

The DISCUSS, DESIGN, DEVOPS and DISTILL artefacts landed on `main` ahead
of DELIVER, each from its own wave agent; the as-built facts below are
read from the DELIVER commit `7f72db8`.

## The problem, in Earned-Trust framing

aegis was a CORRECT LOCK WITH NO DOOR FITTED. `aegis::Validator::validate`
(`crates/aegis/src/validator.rs:174-209`) is real HS256 over
`jsonwebtoken`: alg-confusion-safe (`Algorithm::HS256` pinned),
fail-closed on `exp`, exact issuer and audience equality, unknown-tenant
rejected against a TOML catalogue, eight typed `ValidationError` variants
each with a stable `reason()` audit string, emitting exactly one
structured `tracing` event per call. The validator was tested and
conservative. And `Validator::new` was called by NOBODY but aegis's own
tests. Across the whole repo the 15+ crates depending on aegis imported
only the `TenantId` newtype, never the `Validator`.

The platform-wide consequence, verified at DESIGN, was the purest unwired
lie: tenant identity flowed through the entire system as a typed newtype
WHOSE PROVENANCE WAS UNAUTHENTICATED AT THE BOUNDARY. aperture's gRPC
handlers (`transport.rs:638,715,781`) and HTTP handlers
(`transport.rs:344,436,523`) never read the gRPC `authorization` metadata
nor the HTTP `Authorization` header. Each ingest path
(`app::ingest_logs/traces/metrics`) ran `(bytes, transport, sink) ->
validate (OTLP-conformance, NOT auth) -> sink.accept(SinkRecord)`, and the
`SinkRecord` carried NO tenant at all. So anyone could POST OTLP with any
claimed `tenant_id` and aperture would accept it and store telemetry under
that tenant. Mallory could write under Diego's tenant by simply asserting
it. The careful lock sat in a drawer while the gateway's front door stood
open.

This is the acked-but-actually-broken posture the project's Earned-Trust
stance forbids, in its highest-stakes form: the unreachable, tested,
correct component was the AUTHENTICATION itself, and the door it was meant
to guard was wide open to anyone with a tenant string.

## The decision lineage

### ADR-0068 WIRES, it does not re-engineer

ADR-0068's whole posture is restraint. The validator is correct; the
crypto is correct; the eight-reason taxonomy is correct. The ADR adds NO
crypto, NO new validator, NO duplicated validation. The Reuse Analysis
records the entire aegis core as REUSE-verbatim: `validate` /
`validate_with_subject`, `TenantContext` / `TenantId` / `Role`, the
`ValidationError` taxonomy and `reason()`, `Validator::new` /
`ValidatorConfig`, `load_catalogue` / `TenantCatalogue`, the
one-audit-event-per-call field contract, and the opaque-Debug for the
signing key. The net-new surface is the thin auth-extraction boundary
(bearer extraction plus reject mapping per transport) and four config
fields. No new crate, no new always-running task, no second validator.

### It reuses the ADR-0061 refuse-to-start pattern verbatim

A missing or unreadable auth config is refused at the same
`RawConfig::into_config` seam that ADR-0061 (`tls-config-reject-v0`) uses
to refuse a `tls.enabled` / `auth.spiffe.enabled` knob it cannot honour.
The refusal returns `ConfigError`, hits the existing `main.rs` exit-2 arm,
emits `config_validation_failed`, and binds no listener. There is no new
exit code (exit 2 is config error, distinct from ADR-0066's exit 3
serve-failure) and no new refusal machinery: auth-config absence becomes
one more invariant in the validator that already rejects identical bind
addresses and the ADR-0061 security knobs. The SPIFFE/TLS refusals remain
independent invariants in the same validator; aegis v0 is HS256, SPIFFE is
aegis v1.

### Authentication-only for v0; role-gating, SPIFFE, RS256/JWKS, OPA all deferred

DD6 resolves the role question explicitly: v0 is authentication-only. Any
valid token for a catalogued tenant (`viewer` OR `operator`) may ingest;
aegis still rejects `unknown_role` for free. v0 does NOT reject a valid
`viewer` on the write path. The minimum fail-closed property the audit
demands is "no unauthenticated or forged write", which is authentication
plus tenant tagging; role-gating is a separable authorization concern best
answered with the read-path role matrix in view, and deferring it keeps
the live-gateway blast radius minimal (the change moves "who can write"
from "anyone" to "any authenticated catalogued tenant", the smaller safer
step). The `TenantContext.role` is already threaded to the handler, so the
follow-up is one `if ctx.role != Operator { reject }` gate with no
re-plumbing. SPIFFE / RS256 / JWKS / OPA and read-path auth are aegis v1,
out of scope.

## The as-built shape

### DD1 — the `[aperture.security.auth.jwt]` table, secret by FILE PATH only

A new TOML sub-table `[aperture.security.auth.jwt]` (sibling to the
reserved `[aperture.security.auth.spiffe]`), `#[serde(deny_unknown_fields)]`
like every other aperture config struct, with four required fields:
`issuer`, `audience`, `secret_file` and `catalogue_path`. The HS256 secret
is supplied by a FILE PATH, NEVER inline: an inline TOML string is
loggable, lands in config dumps, in `Debug`, in shell history. The config
struct stores only `secret_file: PathBuf`. Aperture reads the bytes once
at composition, moves them straight into `aegis::ValidatorConfig`, and the
bytes live only inside the `Arc<aegis::Validator>` (whose hand-written
`Debug` already renders the key as `"<opaque>"`). There is nothing secret
on `Config` to leak through its derived `Debug`. aperture gains a
non-wildcard `aegis = { path = "../aegis" }` dependency and constructs the
validator once at composition (`load_catalogue` + `Validator::new`).

### DD2 — per-transport bearer extraction and the exact reject mapping

The shared `authenticate()` step runs AFTER the ADR-0010 concurrency
permit (so a flood of tokenless requests is still bounded by the cap) and
BEFORE any body or content-type work (fail-closed means an unauthenticated
caller learns nothing about the body it sent). gRPC reads
`request.metadata().get("authorization")` as `Bearer <jwt>`; a reject maps
to `Status::unauthenticated(<aegis reason()>)`. HTTP reads the
`Authorization` header; a reject maps to `401` plus
`WWW-Authenticate: Bearer` (RFC 6750) naming the aegis reason, with the
reason string as the body. A missing or empty bearer is the one
aperture-owned `missing_claim` deny line, decided at the extraction
boundary so the reason is stable and the cheap path stays cheap; a present
token reaches aegis. The status and body carry the aegis `reason()`
taxonomy verbatim, never the secret and never the token.

### DD3 — the tenant ripple via `TenantScoped<T>` through the whole pipeline

The validated `TenantContext.tenant_id` flows handler -> `ingest_*` ->
`SinkRecord`. `SinkRecord` variants now carry
`TenantScoped<T> { tenant, inner }`, so "an accepted record is tagged with
the tenant that authenticated it" is a TYPE-LEVEL guarantee: there is no
way to build a `SinkRecord` without a tenant. The tenant flows from the
validated `aegis::TenantContext` (`ctx.tenant_id`) through
`ingest_logs/traces/metrics` into the `SinkRecord`, never a default and
never a hardcoded value. `OtlpSink::accept(record)` keeps its signature
(the tenant rides inside the record, so no sink-implementor breaks). The
ripple ran through aperture, aperture-storage-sink (which reaches through
`.inner`, read-path tenant authority deferred to a later feature), sieve
and spark. sieve's `SamplingSink` preserves the tenant: it filters spans
then re-tags the kept-traces envelope with the SAME authenticated tenant,
so sampling never re-attributes telemetry. The single-validator-per-signal
invariant is kept: the auth check is `aegis::Validator::validate_with_subject`,
a DIFFERENT symbol in the transport handler, not a harness `validate_*`
call site, so the harness call-site count stays one per signal.

### DD4 — refuse-to-start, no opt-out

Absent, incomplete or unreadable `[aperture.security.auth.jwt]` (a missing
field, an unreadable `secret_file` or `catalogue_path`, an unparseable
catalogue) refuses to start at `into_config`: exit 2,
`event=config_validation_failed` naming the offending field or path by
reference (never bytes), and NO listener binds (structural: `Config` is
never constructed, so the bind path is never entered). There is NO opt-out
flag. A flag defaulting OFF would turn "forgot to configure auth" into
"silently shipped an open gateway", the exact ADR-0061 silent-downgrade
trap. The secure path is the only path. Local and dev runs supply a dev
auth block with a throwaway secret file and a one-tenant catalogue, one
extra config block, not a security-relaxing flag.

### DD5 — exactly one decision event per request; aegis owns the validated-request audit

Aperture does NOT emit its own deny event for validated requests. It calls
`validate_with_subject(_, _, "ingest_<signal>")` and aegis's single
`info!`(allow) / `warn!`(deny) event is the one source of truth, fields
`tenant_id` / `role` / `decision` / `subject` / `reason`. The one
pre-validate case (no, empty or malformed bearer, decided before
`validate`) is the only aperture-owned authz line, in the same field shape
with an added `transport=` axis, firing only on that path. So "exactly one
decision event per request" holds across all paths: validate-reached
requests get aegis's event, pre-validate rejects get aperture's single
event, never both, never neither. The secret and the token never appear in
any field, on any path.

### DD6 — authentication-only for v0

Recorded above in the decision lineage: any valid catalogued token may
ingest; `unknown_role` is rejected for free; a valid `viewer` is not
rejected on the write path. Role-gating is the clean follow-up that needs
no re-plumbing.

### DD7 — the aegis "JWKS" doc-fix kept adjacent

`aegis/src/lib.rs` says "JWKS"; the validator is HS256 pre-shared-key only.
The decision is to flag it adjacent, NOT fold it: it touches aegis (outside
this feature's modified-file set) and would pull aegis back into the 100%
mutation scope for a non-behavioural change. Disposition: a `docs:`
fix-forward on the closed wave or a trivial micro-wave. Correct text:
"validates against a configured issuer and audience using a pre-shared
HS256 key (RS256/JWKS is v1)". Carried forward below.

## The proof and its boundary

- 100% mutation kill on every VIABLE mutant across the modified surface
  (ADR-0005 Gate 5; CLAUDE.md per-feature 100%), via `cargo mutants
  --in-diff`: aperture 28 caught / 31 unviable / 0 missed; sieve 1 caught
  / 4 unviable / 0 missed; aperture-storage-sink 4 unviable / 0 viable / 0
  missed. The existing `gate-5-mutants-aperture --in-diff` job picked up
  the diff; no new CI job was needed (DEVOPS C-DEVOPS-1).
- The four load-bearing security ACs were all confirmed HONEST by the
  completing crafter: secret-never-logged (an end-to-end test asserts the
  configured secret never appears across boot, deny and accept);
  refuse-to-start (exit 2 plus a named field, no listener bound);
  tenant-from-the-validated-token-not-hardcoded (the tenant rides from
  `ctx.tenant_id`, and the `ANONYMOUS_TENANT` sentinel is unreachable from
  the binary); and reject-stores-nothing (a reject returns before
  `ingest_*` is ever called, so the body is never re-encoded and never
  reaches the sink).
- The driving-adapter subprocess proof: the config-reject suite runs the
  real `aperture --config <file>` binary, reads a real exit code 2, scrapes
  the `config_validation_failed` stderr line, and probes connect-refused on
  the default OTLP ports (the black-box "no listener bound" observable).
  The accept/reject suite drives a real aperture instance over real TCP
  with a real `tonic` gRPC client and a real `reqwest` HTTP client.
- The in-suite HS256 token-minting seam: `jsonwebtoken::encode` signs each
  token with the SAME secret bytes the `secret_file` holds, for the
  catalogued test tenant, with `iss`/`aud` matching the test config and a
  future `exp`. aegis exposes no public token-minting helper (its
  `Validator` only validates), so the suite mints with `jsonwebtoken`
  directly, mirroring aegis's own `make_jwt` test helper. Each negative
  control perturbs exactly one axis across the eight-reason matrix; the
  happy-path accept assertions were STRENGTHENED to "200/OK AND exactly one
  tenant-tagged allow line" so they remain falsifiable against today's
  no-auth code.
- 24 slice_10 auth tests green (un-ignored): the gRPC and HTTP reject
  matrix (missing / expired / wrong-issuer / wrong-audience /
  invalid-signature / unknown-tenant / unknown-role),
  valid-token-tagged-with-its-tenant, nothing-stored-on-reject,
  one-decision-line, refuse-to-start (exit 2 plus named field, no listener
  bound), and secret-never-logged. sieve gained a tenant-preservation unit
  test; spark and aperture-storage-sink tests were threaded onto the
  `TenantScoped` shape without weakening any assertion. The
  `invariant_single_validator` test stayed green. No leaked aperture
  processes.
- Semver held pre-1.0: `aperture` and `aegis` are not in the Gate 2/3
  public-API set; the `ingest_*` / `SinkRecord` change is breaking to
  in-crate callers only and additive-in-spirit (every record gains a
  guaranteed tenant). aegis is unchanged. NEVER 1.0.0: that is a public
  stability promise, Andrea's call alone, and premature while these APIs
  churn.

## The delivery-resilience note

Recorded in the same spirit as the prior archives' honest-finding
sections. The FIRST DELIVER run dropped on a transient 529 after roughly
95% of the work was done, with the tenant ripple INCOMPLETE and
UNCOMMITTED. A fresh crafter finished it from the working tree: it fixed
the sieve decorator's four `TenantScoped` sites, FOUND AND FIXED a sibling
consumer the first run had missed, threaded spark, and ran mutation BEFORE
the single commit so there was no amend. The result is the one clean
DELIVER commit `7f72db8` with the ripple complete end to end.

The lesson is that a wave interrupted mid-ripple is finished by re-deriving
the full modified set from the design's tenant-ripple map and the compiler,
not by trusting the partial diff: the missed sibling consumer surfaced
precisely because the completing crafter walked the ripple map rather than
the half-applied edit, and mutation-before-commit kept the proof and the
commit atomic.

## The lesson

A lock is not security until it is fitted to a door, the door is not
secure until it fails closed, and an identity is worthless unless it is
carried unforgeable all the way to where the data rests. aegis was a
careful, well-tested HS256 validator that ENFORCED NOTHING: `Validator::new`
was called only by aegis's own tests, every dependent imported only the
`TenantId` newtype, and the gateway accepted telemetry from anyone under
any claimed tenant. The value was not in the validator, which already
worked. It was in the door (the per-transport bearer extraction and the
fail-closed refuse-to-start), in the refusal to ship an open window (no
opt-out flag, exit 2 without auth config), and in carrying the
authenticated tenant unforgeable through gateway, storage sink and the
filtering decorator as a type-level guarantee. This closes the dominant
four-quadrants Q3 item: the front-door lock is finally reachable through
the surface the verifier probes rather than the suite that proved it
correct in private.

## Note for the operator: a new deployment precondition

This feature adds a REFUSE-TO-START deployment precondition. An HS256
`secret_file`, a tenant `catalogue_path`, and the `issuer` and `audience`
are now REQUIRED for aperture to bind its ingest listeners. A gateway
deployed without a complete, readable `[aperture.security.auth.jwt]` block
exits 2 with `config_validation_failed` and binds nothing. Any caller
currently ingesting WITHOUT a token will be rejected (`UNAUTHENTICATED` /
`401`); this is the intended security change and belongs in the release
notes. The dev path supplies a throwaway secret file and a one-tenant
catalogue, not a relaxing flag.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted. The "aegis unwired"
item that headed prior archives' lists is CLOSED for the ingest path by
this feature and recast below as the read-path follow-up.

1. read-path auth (the next aegis wire). The query / log-query /
   trace-query read APIs are still unauthenticated; aperture-storage-sink
   reaches through `.inner` and read-path tenant authority is deferred.
   Wiring aegis onto the read path, with the full role matrix in view, is
   the next aegis slice. Open.

2. ingest role-gating. v0 is authentication-only (DD6): any valid
   catalogued token may ingest. Rejecting a valid `viewer` on the write
   path is the deferred authorization decision; the `TenantContext.role`
   is already threaded, so the follow-up is one `if ctx.role != Operator
   { reject }` gate with no re-plumbing. Open.

3. aegis "JWKS"-vs-HS256 doc-fix (DD7). `aegis/src/lib.rs` overstates
   "JWKS"; the validator is HS256 pre-shared-key only. Disposition: a
   `docs:` fix-forward or a trivial micro-wave, correct text "validates
   against a configured issuer and audience using a pre-shared HS256 key
   (RS256/JWKS is v1)". Open.

4. US-AUTH-04 traces/metrics parity slice. This slice scoped the
   falsifiable boundary to the logs spine; traces and metrics reuse the
   same auth spine and are a follow-on slice. Open.

5. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap needs its own slice. Open.

6. sluice wiring. sluice remains UNWIRED: no gateway/server `src` path
   constructs or drives `FileBackedQueue`. Its `Queue` surface was made
   fail-loud before it is wired (zero live blast radius); the wiring
   itself is a separate, still-open slice. Open.

7. sluice torn-tail migration. sluice still carries the inline
   parse-or-die recovery loop; its migration to the shared
   `replay_wal_tolerating_torn_tail` routine is the tracked ADR-0059 §5
   follow-up. Open.

8. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
   doubles the store, because lumen has no idempotency key. The designed
   extraction (ADR-0064 DD-3): success-case dedup earns its own slice.
   Open.

9. ingest-bounded-memory. The buffer-all-then-flush design (ADR-0064)
   holds the whole input's records in RAM before commit. A future feature
   lifts it with a temp-WAL staging stage or a max-records streaming cap.
   Open.

10. ADR-0059 Decision 8 layer b, the AST structural check, remains
    UNWIRED. The structural pre-commit check asserting in-scope stores
    delegate to the shared wal-recovery routine and carry no `let _ =`
    swallow; the tool choice was deferred and remains deferred. It is
    feedback, not a gate, consistent with the pure trunk-based,
    no-required-checks posture; when wired it belongs in the local
    pre-commit stage. Open.

11. OTLP partial_success never populated. The OTLP `partial_success`
    response field is never populated, so partial-accept signalling is not
    surfaced to clients. Open.

12. The two claims-honesty DOCUMENT items remain future features if
    wanted. The actual Prometheus-stepped grid for `query_range` (a
    query-api feature) and real gRPC-prefix honouring for `harness`
    (`Framing::GrpcProtobuf`) were documented as v0 reality rather than
    built; each would retire its respective pin. Open only if wanted.

13. aperture early-Ok tolerance. The unexpected-early-`Ok`-without-shutdown
    is treated as FATAL at v0 (surfaced, not tolerated), the honest
    default for a listener that stops unbidden. If a future transport
    legitimately self-stops `Ok` without a shutdown request, that
    distinction would earn its own slice. Open only if such a path ever
    appears.

14. beacon non-30d error budget periods. v0 supports ONLY a 30d error
    budget period. Other windows (7d, 90d) would each need their own
    `MWMBR_TABLE` row set and earn their own slice. Open only if wanted.
