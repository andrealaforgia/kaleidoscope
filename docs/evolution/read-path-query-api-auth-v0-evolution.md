# Evolution archive — read-path-query-api-auth-v0

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
`speed-up-local-precommit-v0-evolution.md`,
`claims-honesty-pass-2-v0-evolution.md` and
`aperture-body-size-cap-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below. This is a PRODUCTION-LOGIC feature (a new per-request
read-auth capability in `query-http-common`, wiring across three read
binaries, plus composition-root config validation), so the record is
proportionate to that scope and carries both the load-bearing
no-bearer-bypass lesson and a real delivery-saga lesson in full.

## Status

- State: DELIVERED and pushed on `main`. Delivered across five DELIVER
  slices plus a docs commit; the whole story, including the
  network-instability saga that reshaped the slicing, is below.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0074
  (`docs/product/architecture/adr-0074-read-path-query-api-auth.md`),
  which records the additive auth model, the no-bearer-bypass precedence,
  the cross-surface audience fence, the verbatim `aegis::Validator` reuse,
  and decisions DD1 through DD6. It mirrors ADR-0068 (`aegis-ingest-auth-v0`)
  on every axis except the audience, cites ADR-0054 (the `query-http-common`
  shared seam that earned the rule-of-three), ADR-0061 (fail-closed
  refuse-to-start), and ADR-0053 (trace lookup-by-id, which must also be
  isolated). Supersedes nothing.
- Closes: the highest-value remaining four-quadrants-era security item,
  the read-path-auth follow-up explicitly carved out by ADR-0068 DD6 and
  carried forward as an open item in the two preceding evolution archives
  (`aperture-body-size-cap-v0` follow-up 5, `claims-honesty-pass-2-v0`
  follow-up 2). The read APIs resolved tenant per-instance, not
  per-caller, while the ingest door was authenticated per-request. This
  feature closes that asymmetry.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| deliver slice 1 | `cdccb51` | the shared `query-http-common` capability (`resolve_request_tenant_or_refuse`) + per-request bearer auth and tenant isolation on the metrics `query-api` |
| deliver slice 2 | `2552981` | per-request bearer auth on the logs + traces query APIs (incl. trace lookup-by-id) over the same shared capability; 21 scenarios un-ignored; 100% mutation kill |
| deliver slice 3a | `c389a23` | `query-api` composition-root auth-config refuse-to-start (`config_validation_failed`, partial / unreadable secret) + store-readability startup probe |
| deliver slice 3b | `d6a2094` | `log-query-api` composition-root auth-config refuse-to-start, mirroring 3a |
| deliver slice 3c | `6fb7f9a` | `trace-query-api` composition-root auth-config refuse-to-start, mirroring 3a |
| docs | `8b7c359` | narrative + slide for the feature (a system that bolts one door is not locked) |

The DISCUSS, DESIGN, DEVOPS and DISTILL artefacts (plus ADR-0074) landed
on `main` ahead of DELIVER, each from its own wave agent; the as-built
facts below are read from the five DELIVER commits.

## The problem, in Earned-Trust framing

After `aegis-ingest-auth-v0` (ADR-0068) closed the ingest door with a
per-request HS256 bearer, the read path was still authenticated
per-DEPLOYMENT, not per-CALLER. The three live read query APIs,
`query-api` (metrics / Pulse, `:9090`, `GET /api/v1/query_range`),
`log-query-api` (logs / Lumen, `:9091`, `GET /api/v1/logs`), and
`trace-query-api` (traces / Ray, `:9092`, `GET /api/v1/traces` plus the
trace lookup-by-id path), each resolved tenant from a single process-wide
env var (`KALEIDOSCOPE_<API>_QUERY_TENANT`). One tenant, fixed at
startup, for every request the process served. A platform-security
operator could not certify per-tenant read isolation at the request
boundary, because there was no request-boundary authentication to certify.

This is an asymmetric security posture: the writes authenticated, the
reads not. A system that authenticates one door and leaves the other
open has not been authenticated. It was the highest-value remaining item
on the four-quadrants security backlog once the ingest door was closed,
and it was carried as an explicit open follow-up in the two preceding
evolution archives.

## The design decision (ADR-0074)

The decision is the ADDITIVE model: PRESERVE today's per-instance
env-tenant default, and ADD an OPTIONAL per-request bearer path that,
when an auth config is present and complete, scopes the query to the
validated token's tenant. The shared capability lands ONCE in
`query-http-common` (`resolve_request_tenant_or_refuse`), the per-request
analogue of the existing `resolve_tenant_or_refuse` seam, reusing
`aegis::Validator` VERBATIM: real HS256 (jsonwebtoken), alg-confusion-safe,
fail-closed on `exp`, exact issuer and exact audience, unknown-tenant
rejected against a TOML catalogue, eight typed `ValidationError` reasons,
opaque-Debugged signing key, and exactly one structured audit event per
`validate_with_subject` call. No crypto was rewritten; no JWT validation
was reimplemented.

### The no-bearer-bypass precedence (the load-bearing security property)

`resolve_request_tenant_or_refuse` resolves a per-request tenant through
three arms, and the security of the whole feature rests on arm 2:

1. auth configured AND a valid bearer: tenant is the token's
   `TenantContext.tenant_id`; the existing tenant-scoped store query
   scopes to it (isolation for free, because the store already scopes by
   `&TenantId`).
2. auth configured AND a missing / malformed / invalid bearer:
   fail-closed 401 with the aegis reason, BEFORE the store, and the env
   tenant is NOT consulted. Once auth is configured the bearer is the
   sole authority; omitting or forging the header can never silently
   downgrade to the env tenant. The function returns the 401 directly
   from the validation-failure branch with no `else env_tenant`
   fall-through. This is the no-bearer-bypass, and it is the line the
   whole feature is for.
3. auth NOT configured: today's per-instance env-tenant seam, byte-for-byte,
   with the `Authorization` header ignored (backward compatibility).

### The audience fence and the additive trade

The cross-surface fence is the audience: read tokens carry
`aud=kaleidoscope-query`, against ingest's `aud=kaleidoscope-ingest`. The
same exact-audience check in the same validator, configured with a
different value, stops an ingest token from reading and a read token from
writing. No new code, a config value.

The stricter per-request-only alternative (mandatory bearer, no env
fallback) was the explicit MODEL FORK flagged for Andrea's veto and
carried verbatim across DISCUSS, the ADR and the DESIGN brief. Both Luna
and Morgan judged the model choice Andrea's to make, and proceeded on the
low-regret additive superset per decide-don't-ask: it delivers the
identical fail-closed and isolation properties (arms 1 and 2 are
unchanged under a veto), leaves every legacy env-tenant deployment
byte-for-byte intact, and forecloses nothing (a future flip to mandatory
is localised to the config-validation rule, with no bearer work wasted).
Per-API auth logic, a tower middleware layer, and an `enabled=false` flag
were each considered and rejected (the first triplicates the auth logic
ADR-0054 earned out; the second splits resolution across a layer and the
handler; the third re-creates the half-configured silent-downgrade trap).

No new crate, no new dependency edge: `aegis` and `jsonwebtoken` are
already in the graph, and `query-http-common` already depends on `aegis`
for `TenantId`. The stores (pulse / lumen / ray) and the env-tenant path
are untouched. All four crates stay at 0.1.0 (additive surface; none in
the Gate 2 / Gate 3 public-API set; never 1.0.0, Andrea's call;
CLAUDE.md / MEMORY).

## The as-built shape (the five slices)

- Slice 1 (`cdccb51`): the shared capability landed ONCE in
  `query-http-common` (`resolve_request_tenant_or_refuse`, the bearer
  extraction, and the pre-validate `missing_claim` decision event), wired
  into `query-api` (metrics). The router gained `Option<Arc<Validator>>`
  in its `ApiState` via an additive `router_with_auth`; the handler
  swapped its `resolve_tenant_or_refuse` call for the new one. Eight
  `query-api` scenarios plus thirteen `query-http-common` shared
  scenarios un-ignored (the eight-reason matrix, reasons-distinct,
  one-event-per-request, redaction), two backward-compat guardrails green.
  100% mutation kill on the modified surface (`query-http-common` 7/7
  viable, `query-api` 3/3).
- Slice 2 (`2552981`): logs and traces wired over the SAME shared
  capability in one thin slice (DESIGN collapsed the parity work, since
  both handlers already routed tenant resolution through
  `query-http-common`). `slice_09_read_auth` (logs, 9 scenarios) and
  `slice_05_read_auth` (traces incl. lookup-by-id, 12 scenarios). Each
  handler swapped its call; `router()` stayed byte-for-byte unchanged so
  the sibling slice tests stayed green. Reject scenarios seed env tenant
  `acme-prod` and its data so a 401 there proves no env fall-through.
  Mutation: log-query-api 3/3, trace-query-api 4/4 caught.
- Slices 3a / 3b / 3c (`c389a23`, `d6a2094`, `6fb7f9a`): the
  composition-root config validation, one slice per binary. A wholly
  absent auth config means env-tenant mode; a PARTIAL config or an
  unreadable `secret_file` is a refuse-to-start error,
  `event=config_validation_failed` naming the missing key or the PATH
  (never the secret bytes), exit code 2, no listener bound. The
  `startup_probe_tenant` uses a synthetic sentinel tenant so the existing
  store-readability probe can bind under auth with the env tenant unset
  (arm 1), while the env tenant is preserved otherwise. Each slice:
  four subprocess config-reject scenarios plus five composition-seam unit
  tests; 16 mutants, 13 caught, 3 unviable, 0 survived (100% kill).

Across the three binaries the read-auth behavioural acceptance suites are
`slice_08`/`slice_09`/`slice_05` plus the three config-reject subprocess
suites (`slice_08`/`slice_10`/`slice_06`, four scenarios each), all green,
all on ephemeral `127.0.0.1:0` binds (avoiding the fixed-port flake), with
100% mutation kill on each slice's modified surface.

## The delivery saga (a real evolution lesson)

This feature's DELIVER did not run clean, and the way it recovered is
worth recording. The original DELIVER run hit the weekly account usage
limit mid-slice-2. Two subsequent completion crafters were then dropped
on `ECONNRESET` while working broad three-binary briefs under a flaky
network: a single run touching all three composition roots was a long
brief, and its long-running mutation step gave the unstable connection a
wide window in which to fail, losing the whole run's uncommitted work.

The fix was to SCOPE each crafter run to a SINGLE crate. Slices 3a, 3b
and 3c (`c389a23`, `d6a2094`, `6fb7f9a`) are the same mechanical
composition-root work, split one-crate-per-run. A single-crate brief
keeps the per-run mutation step short, so the run completes inside the
network's stable windows, and per-crate commits mean a drop loses at most
one crate's work rather than three. The lesson: under network
instability, tight single-crate runs beat broad multi-crate ones, and
per-crate commits bound the blast radius of a dropped connection to one
crate.

## The proof and its boundary

- Acceptance, driven end-to-end through the three real read-API binaries
  with an `Authorization: Bearer <jwt>` header: the valid-token-reads-its-own-tenant
  positive control; the tenant-isolation positive-and-negative control
  (an `acme-prod` token sees `acme-prod` data, a `globex-staging` token
  sees it ABSENT) across metrics, logs, traces AND trace lookup-by-id;
  the no-bearer-bypass negative control (auth-on, env tenant also set and
  seeded, no bearer, expect a 401 and NOT the env tenant's data); the
  eight-reason reject matrix including the `wrong_audience` cross-surface
  fence; one-audit-event-per-request including the pre-validate
  `missing_claim` case; and the redaction guardrail (no secret bytes and
  no raw token in any 401 body, error, log line, or audit event).
- Mutation (ADR-0005 Gate 5): 100% kill on each slice's modified surface,
  scoped to the modified files per the per-feature strategy (CLAUDE.md).
- The no-bearer-bypass property is pinned by a test that SETS and SEEDS
  an env tenant and FAILS if a bad-token request returns one of its rows.
  That is the falsifiable hook: the reject ACs fail against an env-tenant
  fall-through, and the reason matrix fails against a non-validating impl.
- SemVer (Gate 2 / Gate 3): none; `query-http-common` and the three read
  APIs stay 0.1.0; `aegis` unchanged; never 1.0.0.

## The honest limit (deferred, recorded, not silently omitted)

ADR-0074 DD4 specified an auth-on startup NEGATIVE probe: a deliberately
known-bad token (for example a wrong-signature token) rejected by the
freshly-built `Validator` BEFORE the listener binds, proving the
configured lock actually rejects rather than merely that it was
constructed. This was DEFERRED. The config-reject acceptance tests assert
the store-readability startup probe (under the synthetic sentinel tenant)
and the refuse-to-start config validation, but they do not assert the
known-bad-token rejection at startup, so only the store-readability
startup probe was built. The validator IS exercised against bad tokens
exhaustively at REQUEST time (the eight-reason matrix), so the runtime
property is fully proven; what is not built is the additional
build-time-before-bind assertion of it. This is carried forward below as
an open follow-up, not a silent gap.

## The lesson

A system that authenticates one door and not the other has not been
authenticated. An asymmetric security posture is a false sense of
security: the ingest door was locked per-request while the read door
opened to whichever single tenant the process was started with, and a
reader could not tell from the outside that the read side was weaker. And
an auth check that silently falls back to a default on a bad token is
worse than none, because it looks like protection while giving none: a
401 you can bypass by simply omitting the header, downgrading to the
env tenant, is a door with a lock that opens when you do not knock. So
the no-fallback precedence (arm 2 returns the 401 directly, with no
`else env_tenant` after it) is the load-bearing line of the whole
feature, and it is pinned by a test that sets and seeds an env tenant and
reddens the moment a bad-token request returns a row. The proof of the
property is the absence of the fall-through, and the test exists to make
that absence loud.

## Note for the operator

This feature is additive. With NO read-auth config set, the three read
APIs behave byte-for-byte as before: the per-instance
`KALEIDOSCOPE_<API>_QUERY_TENANT` env tenant scopes every request and the
`Authorization` header is ignored. The existing fleet is untouched.

With a COMPLETE read-auth config set (`KALEIDOSCOPE_<API>_QUERY_AUTH_ISSUER`,
`_AUDIENCE`, `_SECRET_FILE`, `_CATALOGUE`), each request must carry a
valid `Bearer <jwt>` with `aud=kaleidoscope-query` for a catalogued
tenant; the query is then scoped to that token's tenant, a missing or
invalid bearer returns 401 with `WWW-Authenticate: Bearer` before the
store is touched, and the env tenant is never a fallback. The secret is
supplied by file path, never inline, and is never logged. A PARTIAL
config (some but not all four fields) or an unreadable secret file is a
refuse-to-start error (exit 2, `event=config_validation_failed` naming
the key or the path, no listener bound), so half-configured auth fails
closed rather than running open. v0 read auth is authentication and
tenant-scoping only; any valid token for a catalogued tenant (viewer or
operator) may read, role-gating is a recorded deferral.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted. The read-path-auth
item carried as open in the two preceding evolution archives is CLOSED by
this feature.

1. read-auth DD4 startup negative probe. ADR-0074 DD4 specified an
   auth-on startup probe that rejects a known-bad token before the
   listener binds; the config-reject tests asserted only the
   store-readability startup probe, so the negative-token startup
   assertion was deferred. The runtime reject path is fully proven by the
   eight-reason matrix; the open item is the build-time-before-bind
   assertion of it. Open.

2. read role-gating. v0 read auth is authentication and tenant-scoping
   only; any valid catalogued token (viewer or operator) may read. A
   future role gate is one `if ctx.role != … { reject }` with no
   re-plumbing (`TenantContext.role` is already available to the handler).
   The sibling of the ingest role-gating follow-up. Open.

3. faster-test-fsync-backend-v0. The fsync-bound durability bins remain
   I/O-bound in CI, paying the honest per-record `sync_all` of
   ADR-0049 / 0060. A future feature could speed them with a faster
   test-fsync backend or a batched-fsync test mode behind an env guard.
   Open.

4. ingest role-gating. ingest auth is authentication-only: any valid
   catalogued token may ingest. Rejecting a valid `viewer` on the write
   path is the deferred authorization decision; the `TenantContext.role`
   is already threaded, so the follow-up is one
   `if ctx.role != Operator { reject }` gate with no re-plumbing. Open.

5. aegis "JWKS"-vs-HS256 doc-fix. `aegis/src/lib.rs` overstates "JWKS";
   the validator is HS256 pre-shared-key only. Disposition: a `docs:`
   fix-forward or a trivial micro-wave. Open.

6. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap needs its own slice. Open.

7. sluice wiring. sluice remains UNWIRED: no gateway/server `src` path
   constructs or drives `FileBackedQueue`. The wiring is a separate,
   still-open slice. Open.

8. sluice torn-tail migration. sluice still carries the inline
   parse-or-die recovery loop; its migration to the shared
   `replay_wal_tolerating_torn_tail` routine is the tracked ADR-0059 §5
   follow-up. Open.

9. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
   doubles the store, because lumen has no idempotency key. The designed
   extraction (ADR-0064 DD-3): success-case dedup earns its own slice.
   Open.

10. ingest-bounded-memory. The buffer-all-then-flush design (ADR-0064)
    holds the whole input's records in RAM before commit. A future
    feature lifts it with a temp-WAL staging stage or a max-records
    streaming cap. Open.

11. ADR-0059 Decision 8 layer b, the AST structural check, remains
    UNWIRED. The structural pre-commit check asserting in-scope stores
    delegate to the shared wal-recovery routine and carry no `let _ =`
    swallow; the tool choice was deferred and remains deferred. It is
    feedback, not a gate, consistent with the pure trunk-based,
    no-required-checks posture; when wired it belongs in the local
    pre-commit stage (now the fast `--lib` stage). Open.

12. OTLP partial_success never populated. The OTLP `partial_success`
    response field is never populated, so partial-accept signalling is
    not surfaced to clients. Open.

13. prism dashboarding and the prism browser-matrix e2e. The Prism row is
    its single-metric reality and the Gate 7 e2e is MARKed scaffold with
    its digest SSOT and re-add roadmap preserved. Building the
    unified-visualisation dashboarding and standing up the browser-matrix
    e2e specs are each future features. Open only if wanted.

14. pulse columnar adapter. The Arrow / Parquet / DataFusion / TSDB
    columnar story was reframed FUTURE-tense rather than built. A future
    feature that ships it behind the existing `MetricStore` trait would
    move the columnar claim from future to present. Open only if wanted.

15. body-size-cap rejection counter and genuine per-arm body cap.
    ADR-0073 D4 scoped the `body_too_large` event in and deferred a
    rejection-counter metric; D1 ships a single collapsed cap shared by
    both transports. Each is a future slice if operators report the need.
    Open only if wanted.

16. beacon non-30d error budget periods. v0 supports ONLY a 30d error
    budget period. Other windows (7d, 90d) would each need their own
    `MWMBR_TABLE` row set and earn their own slice. Open only if wanted.
