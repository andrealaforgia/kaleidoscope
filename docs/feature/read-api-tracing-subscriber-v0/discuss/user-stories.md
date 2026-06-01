<!-- markdownlint-disable MD024 -->

# User Stories — read-api-tracing-subscriber-v0

## System Constraints

- No HTTP contract change: request/response bodies and status codes of all
  three read APIs are untouched. This work is stderr-only.
- No new crate. Modify `crates/{query-api,log-query-api,trace-query-api}/src/main.rs`
  and the matching `Cargo.toml` files. An optional shared init helper in
  `query-http-common` is a DESIGN choice, not a new crate.
- Subscriber configuration must match aperture exactly (JSON layer to
  stderr, EnvFilter / RUST_LOG-aware). No bare `fmt().init()`.
- The subscriber is installed as the first action in `main`, before the
  first `tracing::` call. Pre-init failures use `eprintln!`, as aperture
  does.
- Events must reach process stderr as greppable structured lines so a
  black-box harness can assert them.
- `#[mutants::skip]` remains on each `main`; gate-5 100% kill rate is
  preserved.

> Persona: **Priya Nair**, a platform on-call operator at a tenant running
> Kaleidoscope's read tier in containers. She reads container stderr (via
> `kubectl logs` / `docker logs`) when a service will not come up or when
> she needs to confirm a deploy is healthy. Today the three read binaries
> give her nothing to read.

---

## US-01: log-query-api startup lifecycle visible on stderr

### Problem

Priya Nair deploys `log-query-api` and tails the container's stderr to
confirm the service is up. Today she sees nothing: the binary emits
`log_query_api_starting` and `listener_bound` tracing events, but no
subscriber is installed, so every event is discarded. She cannot tell
whether the process is alive, stuck, or already serving until she probes
the port from outside.

### Who

- Platform on-call operator | reads container stderr after a deploy |
  wants to confirm the service is up and listening before moving on.

### Solution

Install a tracing subscriber (matching aperture's posture) as the first
action in `log-query-api`'s `main`, so the `log_query_api_starting` and
`listener_bound` events render to stderr.

### Elevator Pitch

Before: the operator starts `log-query-api` and sees an empty stderr until
something probes the port.
After: run `log-query-api` -> stderr shows a structured
`event=log_query_api_starting` line and then
`event=listener_bound transport=http addr=0.0.0.0:9091`.
Decision enabled: the operator confirms the service is up and listening,
and moves on without port-probing.

### Domain Examples

#### 1: Happy path — clean startup

Priya runs `log-query-api` with `KALEIDOSCOPE_LOG_QUERY_TENANT=acme` and a
readable Lumen store. stderr shows
`{"event":"log_query_api_starting","tenant_resolved":true,...}` followed by
`{"event":"listener_bound","transport":"http","addr":"0.0.0.0:9091"}`.

#### 2: Edge case — non-default address

Priya sets `KALEIDOSCOPE_LOG_QUERY_ADDR=0.0.0.0:19091`. The
`listener_bound` event reports `addr=0.0.0.0:19091`, so she confirms the
override took effect from stderr alone.

#### 3: Boundary — RUST_LOG raises threshold

Priya sets `RUST_LOG=warn`. The `log_query_api_starting` and
`listener_bound` info events are filtered out (EnvFilter honoured), so she
learns the filter behaves exactly as it does for aperture.

### UAT Scenarios (BDD)

#### Scenario: Operator sees the service announce itself at startup

Given Priya runs `log-query-api` with tenant `acme` and a readable store
When the process starts
Then its stderr contains a structured `log_query_api_starting` event

#### Scenario: Operator sees the bound listener address

Given Priya runs `log-query-api` with tenant `acme` and a readable store
When the listener binds
Then its stderr contains a `listener_bound` event naming the bound address

#### Scenario: Startup chatter respects the log level filter

Given Priya sets `RUST_LOG=warn` before starting `log-query-api`
When the process starts cleanly
Then the info-level startup events are absent from stderr

### Acceptance Criteria

- [ ] Running `log-query-api` cleanly prints a `log_query_api_starting`
  event to stderr.
- [ ] A `listener_bound` event with the bound address appears on stderr.
- [ ] `RUST_LOG=warn` suppresses the info-level startup events
  (EnvFilter honoured, matching aperture).

### Outcome KPIs

- Who: read-tier operators of `log-query-api`.
- Does what: confirm the service is up from stderr without port-probing.
- By how much: startup lifecycle events visible on 100% of clean starts.
- Measured by: black-box harness captures stderr and greps both events.
- Baseline: 0% (stderr empty today).

---

## US-02: log-query-api fail-closed refusal is visible before exit

### Problem

When `log-query-api` refuses to start (substrate not probe-able, tenant
unset), Priya sees only the bare `Err` Rust prints from `main`. The
binary emits a structured `health.startup.refused` event naming the
reason, but no subscriber is installed so it is discarded. She cannot tell
WHY the service refused to start.

### Who

- Platform on-call operator | triaging a crash-looping read service |
  needs the refusal reason to decide what to fix.

### Solution

With the subscriber installed, the existing
`tracing::error!(event = "health.startup.refused", reason = ...)` renders
to stderr BEFORE the non-zero exit, giving Priya the reason.

### Elevator Pitch

Before: a fail-closed start prints only the bare `Err` Rust emits from
`main`.
After: run `log-query-api` with an unprobeable substrate -> stderr shows
`event=health.startup.refused reason="..."` then the process exits
non-zero.
Decision enabled: the operator reads the reason and decides whether to
fix the tenant config, the store path, or the substrate.

### Domain Examples

#### 1: Happy path of the failure — tenant unset

Priya forgets `KALEIDOSCOPE_LOG_QUERY_TENANT`. stderr shows
`{"event":"health.startup.refused","reason":"..."}` naming the unresolved
tenant, then the process exits non-zero.

#### 2: Edge case — store not readable

Priya points `KALEIDOSCOPE_PILLAR_ROOT` at a path whose Lumen store cannot
be probed. The `health.startup.refused` event names the store reason.

#### 3: Boundary — refusal at RUST_LOG=error

Priya runs with `RUST_LOG=error`. The startup info events are filtered,
but `health.startup.refused` (error level) still appears, so the refusal
is never hidden by a stricter filter.

### UAT Scenarios (BDD)

#### Scenario: Operator learns why the service refused to start

Given Priya starts `log-query-api` with the tenant unset
When the startup probe refuses the service
Then stderr contains a `health.startup.refused` event naming the reason

#### Scenario: The refusal event precedes the non-zero exit

Given a fail-closed startup of `log-query-api`
When the process exits
Then the `health.startup.refused` event was written to stderr before exit
And the exit status is non-zero

#### Scenario: Refusal survives a strict log filter

Given Priya sets `RUST_LOG=error` and starts with the tenant unset
When the startup probe refuses the service
Then the `health.startup.refused` event is still present on stderr

### Acceptance Criteria

- [ ] A fail-closed startup writes `health.startup.refused` with the
  reason to stderr.
- [ ] The event appears before the process exits non-zero.
- [ ] The refusal event is present even at `RUST_LOG=error`.

### Outcome KPIs

- Who: operators triaging a refusing `log-query-api`.
- Does what: read the refusal reason from stderr.
- By how much: refusal reason visible on 100% of fail-closed starts.
- Measured by: harness captures stderr on a fail-closed start, greps the
  event, asserts non-zero exit.
- Baseline: 0% (only the bare `Err` today).

---

## US-03: query-api startup lifecycle and refusal visible on stderr

### Problem

`query-api` (metrics read) has the same gap as `log-query-api`: it emits
`query_api_starting`, `listener_bound`, and `health.startup.refused`, but
no subscriber renders them. Priya gets an empty stderr on both healthy and
failing starts.

### Who

- Platform on-call operator | deploying or triaging the metrics read API |
  wants the same stderr visibility as for the logs read API.

### Solution

Install the aperture-posture subscriber in `query-api`'s `main` so its
startup and refusal events render to stderr.

### Elevator Pitch

Before: the operator starts `query-api` and sees an empty stderr on both
healthy and failing starts.
After: run `query-api` -> stderr shows `event=query_api_starting`,
`event=listener_bound addr=0.0.0.0:9090`, and on refusal
`event=health.startup.refused reason="..."`.
Decision enabled: the operator confirms the metrics read API is up, or
reads why it refused.

### Domain Examples

#### 1: Happy path — clean start

Priya runs `query-api` with tenant `acme`. stderr shows
`query_api_starting` then `listener_bound` at `0.0.0.0:9090`.

#### 2: Edge case — fail-closed on unprobeable store

Priya runs against a corrupt Pulse store. stderr shows
`health.startup.refused` naming the store reason, then non-zero exit.

#### 3: Boundary — RUST_LOG=warn

The startup info events are filtered; `health.startup.refused` (error)
would still appear if a refusal occurred.

### UAT Scenarios (BDD)

#### Scenario: Operator sees query-api announce startup and binding

Given Priya runs `query-api` with tenant `acme` and a readable store
When the process starts and binds the listener
Then stderr contains a `query_api_starting` event and a `listener_bound`
event with the bound address

#### Scenario: Operator sees why query-api refused to start

Given Priya starts `query-api` against an unprobeable Pulse store
When the startup probe refuses the service
Then stderr contains `health.startup.refused` with the reason before a
non-zero exit

#### Scenario: query-api honours the log-level filter

Given Priya sets `RUST_LOG=warn` and starts `query-api` cleanly
When the process starts
Then the info-level startup events are absent from stderr

### Acceptance Criteria

- [ ] Clean start of `query-api` prints `query_api_starting` and
  `listener_bound` to stderr.
- [ ] A fail-closed start prints `health.startup.refused` with the reason
  before a non-zero exit.
- [ ] `RUST_LOG=warn` suppresses the info startup events.

### Outcome KPIs

- Who: operators of `query-api`.
- Does what: confirm startup or read the refusal reason from stderr.
- By how much: 100% of starts produce the relevant stderr events.
- Measured by: harness captures stderr on clean and fail-closed starts.
- Baseline: 0% (stderr empty today).

---

## US-04: trace-query-api startup lifecycle and refusal visible on stderr

### Problem

`trace-query-api` (traces read) has the gap the EDD-verifier confirmed
directly (TQ01: empty stderr). It emits `trace_query_api_starting`,
`listener_bound`, and `health.startup.refused`, all discarded for want of
a subscriber.

### Who

- Platform on-call operator | deploying or triaging the traces read API |
  wants the same stderr visibility as the other two read APIs.

### Solution

Install the aperture-posture subscriber in `trace-query-api`'s `main` so
its startup and refusal events render to stderr.

### Elevator Pitch

Before: the operator starts `trace-query-api` and sees an empty stderr
(the verifier's TQ01 case).
After: run `trace-query-api` -> stderr shows `event=trace_query_api_starting`,
`event=listener_bound addr=0.0.0.0:9092`, and on refusal
`event=health.startup.refused reason="..."`.
Decision enabled: the operator confirms the traces read API is up, or
reads why it refused.

### Domain Examples

#### 1: Happy path — clean start

Priya runs `trace-query-api` with tenant `acme`. stderr shows
`trace_query_api_starting` then `listener_bound` at `0.0.0.0:9092`.

#### 2: Edge case — fail-closed on unprobeable ray store

Priya runs against an unreadable ray store. stderr shows
`health.startup.refused` naming the reason, then non-zero exit.

#### 3: Boundary — RUST_LOG=warn

The startup info events are filtered; the refusal error event would still
appear on a fail-closed start.

### UAT Scenarios (BDD)

#### Scenario: Operator sees trace-query-api announce startup and binding

Given Priya runs `trace-query-api` with tenant `acme` and a readable store
When the process starts and binds the listener
Then stderr contains a `trace_query_api_starting` event and a
`listener_bound` event with the bound address

#### Scenario: Operator sees why trace-query-api refused to start

Given Priya starts `trace-query-api` against an unprobeable ray store
When the startup probe refuses the service
Then stderr contains `health.startup.refused` with the reason before a
non-zero exit

#### Scenario: trace-query-api honours the log-level filter

Given Priya sets `RUST_LOG=warn` and starts `trace-query-api` cleanly
When the process starts
Then the info-level startup events are absent from stderr

### Acceptance Criteria

- [ ] Clean start of `trace-query-api` prints `trace_query_api_starting`
  and `listener_bound` to stderr.
- [ ] A fail-closed start prints `health.startup.refused` with the reason
  before a non-zero exit.
- [ ] `RUST_LOG=warn` suppresses the info startup events.

### Outcome KPIs

- Who: operators of `trace-query-api`.
- Does what: confirm startup or read the refusal reason from stderr.
- By how much: 100% of starts produce the relevant stderr events.
- Measured by: harness captures stderr on clean and fail-closed starts.
- Baseline: 0% (verifier confirmed empty stderr at TQ01).

---

## US-05: the three read binaries match aperture's subscriber posture

### Problem

aperture installs a JSON-to-stderr, RUST_LOG-aware subscriber (ADR-0009).
If the three read binaries each installed a subscriber differently (one
`fmt().init()`, one JSON, one with a different filter), operators would
face inconsistent log formats and filtering across the read tier, and the
black-box harness would need per-binary assertions. The goal is one
pattern across all four binaries.

### Who

- Platform operator AND the EDD-verifier | reading or asserting against
  read-tier stderr | want a single, predictable format and filter across
  aperture, query-api, log-query-api, and trace-query-api.

### Solution

Install the SAME subscriber configuration in all three read binaries as
aperture uses: a JSON layer to stderr with an EnvFilter
(`["fmt", "json", "env-filter", "registry"]`). DESIGN pins the exact
builder expression and decides whether it lives inline per `main` or in a
shared `query-http-common` helper.

### Elevator Pitch

Before: aperture emits JSON/RUST_LOG-aware stderr; the three read binaries
emit nothing, and any ad-hoc fix risks three different formats.
After: all four binaries emit the same JSON, RUST_LOG-aware stderr format;
one `RUST_LOG` value and one parser cover the whole read tier.
Decision enabled: operators apply one mental model and one filter across
the read tier; the verifier writes one assertion shape for all.

### Domain Examples

#### 1: Happy path — identical format across binaries

Priya greps `event=listener_bound` across aperture, query-api,
log-query-api, and trace-query-api stderr and gets the same JSON shape
from each.

#### 2: Edge case — one RUST_LOG value governs all

Priya sets `RUST_LOG=info` once in the deployment template; all four
binaries honour it identically.

#### 3: Boundary — verifier reuses one assertion helper

The EDD-verifier's stderr-capture-and-parse helper (modelled on
aperture's `stderr_capture`) parses each line as JSON for all four
binaries without per-binary special-casing.

### UAT Scenarios (BDD)

#### Scenario: Read-tier stderr format matches aperture

Given the subscriber is installed in all three read binaries
When each binary emits `listener_bound`
Then the event renders in the same JSON, RUST_LOG-aware format aperture
uses

#### Scenario: One log-level filter governs the whole read tier

Given Priya sets a single `RUST_LOG` value
When she starts any of the four read-tier binaries
Then each applies the filter identically

### Acceptance Criteria

- [ ] All three read binaries install a subscriber with the same
  configuration as aperture (JSON layer to stderr, EnvFilter).
- [ ] The rendered event format is consistent across all four binaries.
- [ ] A single `RUST_LOG` value governs all four binaries identically.

### Outcome KPIs

- Who: operators and the EDD-verifier.
- Does what: apply one format and one filter across the read tier.
- By how much: 4 of 4 read-tier binaries share one subscriber pattern.
- Measured by: code review confirms identical init expression; harness
  parses all four with one JSON helper.
- Baseline: 1 of 4 (only aperture today).

---

## US-06 (optional): pre-init failures are visible on stderr via eprintln

### Problem

Some failures in the read binaries happen BEFORE the first `tracing::`
call and thus before any subscriber could render them: `create_dir_all`
on the pillar root, `FileBacked*Store::open`, and `resolve_addr` all run
ahead of the first event. If these fail, the structured logging path
cannot help. aperture handles its equivalent pre-init window with
`eprintln!`. Without it, the read binaries would still drop the earliest
failures.

### Who

- Platform operator | hitting a failure in the earliest startup steps
  (bad pillar path, unopenable store, malformed addr) | needs SOME stderr
  signal even before the subscriber exists.

### Solution

For failures occurring before the subscriber is installed, emit a direct
`eprintln!("{binary}: ...: {e}")` line, exactly as aperture does for its
pre-init config window, before returning the error / exiting non-zero.

### Elevator Pitch

Before: a failure in the earliest startup steps yields only the bare
`Err`.
After: run `log-query-api` with a malformed `KALEIDOSCOPE_LOG_QUERY_ADDR`
-> stderr shows `log-query-api: addr error: ...` before a non-zero exit.
Decision enabled: the operator fixes the earliest-stage misconfiguration
even though the subscriber never got a chance to start.

### Domain Examples

#### 1: Happy path of the failure — malformed address

Priya sets `KALEIDOSCOPE_LOG_QUERY_ADDR=not-an-addr`. stderr shows
`log-query-api: addr error: ...` then a non-zero exit.

#### 2: Edge case — unopenable store

The pillar root exists but the Lumen store cannot be opened; stderr shows
an `eprintln!` line naming the open failure.

#### 3: Boundary — unwritable pillar root

`create_dir_all` fails on a read-only mount; stderr shows the directory
error via `eprintln!`.

### UAT Scenarios (BDD)

#### Scenario: Earliest-stage failures still reach stderr

Given Priya sets a malformed listener address for a read binary
When the address fails to parse before the subscriber is installed
Then stderr contains a direct `eprintln!` line naming the failure before a
non-zero exit

#### Scenario: Pre-init store failure is reported

Given the durable store cannot be opened at startup
When the failure occurs before the subscriber is installed
Then stderr contains a direct line naming the open failure

### Acceptance Criteria

- [ ] A pre-init failure (addr parse, store open, dir create) prints an
  `eprintln!` line to stderr naming the failure.
- [ ] The line appears before a non-zero exit.
- [ ] The behaviour matches aperture's pre-init `eprintln!` convention.

### Outcome KPIs

- Who: operators hitting earliest-stage startup failures.
- Does what: read the failure cause from stderr even pre-subscriber.
- By how much: pre-init failures visible on 100% of such failures.
- Measured by: harness forces a pre-init failure and greps stderr.
- Baseline: 0% (bare `Err` today).
