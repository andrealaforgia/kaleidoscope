# DISTILL upstream issues — read-path-query-api-auth-v0

Scholar (DISTILL, 2026-06-08). One observation, NOT a blocker for this feature.

## OBS-1 — a pre-existing, NON-ignored RED scaffold in a SIBLING feature leaves `cargo test` red on trunk

**What.** While verifying my new auth suites keep trunk green, the full default
`cargo test -p log-query-api` run surfaced **2 failing tests** in
`crates/log-query-api/tests/slice_07_tracing_subscriber.rs`:

- `clean_startup_announces_log_query_api_starting_on_stderr` — FAILED
- `clean_startup_reports_bound_listener_address_on_stderr` — FAILED

(3 others in the same file pass.)

**Whose.** These belong to the separate, in-flight feature
**`read-api-tracing-subscriber-v0`**, NOT to `read-path-query-api-auth-v0`. The
`query_http_common::init_tracing` function is a DOCUMENTED NO-OP scaffold (see
its rustdoc: *"The body is a deliberate NO-OP ... This is the RED-not-BROKEN
posture (Mandate 7): the new subprocess acceptance test
(`crates/log-query-api/tests/slice_07_tracing_subscriber.rs`) asserts
`health.startup.refused` / `*_starting` reach stderr and is therefore RED
against this no-op"*). Those two scenarios assert the CLEAN-startup events reach
stderr, which the no-op cannot satisfy yet.

**Why it matters here.** Unlike my auth scenarios (all `#[ignore]`d so trunk
stays green), those two tracing-subscriber scenarios are NOT `#[ignore]`d, so a
plain `cargo test --workspace` is ALREADY red on trunk before this feature
touched anything. That sibling feature's DISTILL appears to have left its RED
scaffold un-ignored (a Mandate 7 slip: RED-not-BROKEN tests must be `#[ignore]`d
so the trunk pre-commit gate passes).

**Confirmed NOT caused by this feature.** `slice_07_tracing_subscriber.rs`, the
`log-query-api` `main.rs`, and `init_tracing` are NOT in this feature's
changed-file set (`git status`): I added only `router_with_auth` + the `auth`
field + the new `slice_09_read_auth.rs`. My change leaves the existing
`router()` signature byte-for-byte unchanged, so the existing slice tests are
unaffected.

**Recommendation (for the orchestrator / the tracing-subscriber feature's
DELIVER, NOT for this wave).** Either (a) `read-api-tracing-subscriber-v0`'s
DELIVER lands the real `init_tracing` body and turns those two green, or
(b) those two scenarios are `#[ignore]`d with a RED-reason until that DELIVER
runs. No action is required from `read-path-query-api-auth-v0`; flagged here for
visibility because it affects the shared "trunk stays green" invariant the task
asked me to preserve, and I have preserved it for THIS feature's tests.

## Otherwise: no contradiction

DESIGN's DD1-DD6, the additive model, the no-bearer-bypass precedence, the
audience fence, the redaction discipline, and the ephemeral-port / token-mint
test seam were all honoured without any upstream change. No
`distill/upstream-changes.md` is created.
