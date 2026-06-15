<!-- markdownlint-disable MD024 -->

# User Stories - `experimentable-stack-v0`

> Persona note: the consumer of this feature is the **newcomer evaluating Kaleidoscope** - someone
> who just cloned the repo (or pulled an image) and wants to see the platform actually work before
> investing further; plus Andrea running it locally to experiment, and a contributor verifying a
> change. This feature is built by AI agents; the personas below are *users* of the run story, the
> generator, and the docs, not their builders. House style: British English, no human-effort
> estimation, no em-dashes. Scenario titles describe what the user achieves, never how the system
> implements it.
>
> This feature delivers the rest of Milestone 1 of the consolidation roadmap (items C2 + C3 + C4).
> C1 (`consolidated-runtime-v0`, the `kaleidoscope` binary) is DONE and CI-green: one process runs
> OTLP ingest + the pulse/lumen/ray stores + the three query routers over shared live state, so a
> metric sent at time T is queryable at T with no restart. C2/C3/C4 wrap that binary into the
> "one command, send, see" loop a newcomer can run in minutes.

---

## System Constraints

These apply to every story below and are not repeated in each:

1. **Builds on the consolidated runtime, not the old five binaries** (W1). The run story brings up
   the C1 `kaleidoscope` binary (crate `kaleidoscope-runtime`), one process, ports ingest gRPC
   4317 / HTTP 4318 and query metrics 9090 / logs 9091 / traces 9092, over one shared pillar root.
   It does NOT compose the old separate `kaleidoscope-gateway` + three query-API binaries.
2. **Additive** (W2). C2/C3/C4 add a compose file, a thin make/just wrapper, a telemetry
   generator, and a getting-started doc. The existing `Dockerfile`, `Dockerfile.gateway`,
   `Dockerfile.query-api`, the four standalone binaries, and the current CLI quick start are NOT
   removed or broken. US-07 is the explicit guardrail.
3. **Minimal-friction local posture** (W3). One tenant (`KALEIDOSCOPE_TENANT=acme`), auth OFF
   everywhere, no tokens, no TLS, one shared pillar volume. Experimenting needs no secrets.
4. **Solution-neutral**. Stories state observable outcomes (the stack is reachable; Prism paints a
   metric; the query returns the pushed rows). The exact wrapper (Makefile vs justfile), the
   generator implementation (extend the CLI vs a new tool vs an external OTel generator), whether a
   new `Dockerfile.runtime` is built, the compose topology, and whether Prism is served same-origin
   from the runtime's 9090 router or as a separate service are DESIGN/DEVOPS decisions. See
   `wave-decisions.md` flags.
5. **Honest verification limit** (W6). A docker-compose-up plus a real browser rendering Prism is
   verified by bringing it up and looking. CI (ubuntu, headless, trunk-based feedback-not-gate) can
   curl the query endpoints and assert the generator-then-query loop, but the browser render of
   Prism's chart is NOT browser-driven in CI today (project memory: prism ECharts needs a
   CI-browser). Claims about "Prism paints a metric in the browser" are honestly manual /
   smoke-script verified, not CI-gated. This is stated in the docs (US-06) and tracked in
   `wave-decisions.md`.
6. **Default ports and env** (illustrative, from C1 / `consolidated-runtime-v0/devops/environments.yaml`):
   ingest gRPC 4317 / HTTP 4318; metrics query 9090 (`/api/v1/query_range`); logs query 9091
   (`/api/v1/logs`); traces query 9092 (`/api/v1/traces`, `/api/v1/traces/by_id`).
   `KALEIDOSCOPE_TENANT` drives the ingest default tenant and all three query tenants;
   `KALEIDOSCOPE_PILLAR_ROOT` (sub-dirs `pulse`/`lumen`/`ray`) is the one shared store path; Prism's
   `config.json` `backend.url` is the relative `/api/v1`, so serving Prism same-origin from the
   metrics router (`KALEIDOSCOPE_QUERY_STATIC_DIR` to `apps/prism/dist`) needs no Prism config
   change and no CORS.
7. **One writer** (C1 constraint). The consolidated runtime is the sole writer of its pillar root;
   compose must not co-run it against a separate gateway on the same volume.

---

## US-01 - Bring the whole stack up with one command and reach it in a browser

### Elevator Pitch

- **Before**: A newcomer who clones Kaleidoscope finds no compose file, no Makefile, no run script.
  The README quick start covers only the CLI NDJSON demo. To see the real platform they would have
  to launch the consolidated runtime by hand and separately serve Prism. There is no one-command
  way in.
- **After**: Sam runs `make up` (a thin wrapper over `docker compose up`) and within minutes the
  consolidated runtime and Prism are running over a shared pillar volume with the single local
  tenant `acme` and auth off. Sam opens the documented local URL (the metrics router on
  `http://localhost:9090`, which serves Prism same-origin) and Prism's query panel renders in the
  browser; `curl http://localhost:9090/api/v1/query_range?query=request_count&start=..&end=..`
  returns `{"status":"success","data":{"resultType":"matrix","result":[]}}`.
- **Decision enabled**: Sam decides Kaleidoscope is something they can actually try, having got a
  running stack and a loaded UI from a single command, and proceeds to put telemetry in (US-04).

### Problem

The consolidation state assessment (`docs/analysis/consolidation-state-2026-06.md` section 3)
records the gap bluntly: no `docker-compose.yml`, no `Makefile`/`justfile`, no run script anywhere
in the tree, and a README quick start that documents only the CLI path. C1 made the runtime exist;
nothing yet brings it and Prism up together with one command. Until that exists, a newcomer cannot
get to "a running stack in my browser" without plumbing.

### Who

- **Sam Okonkwo, newcomer/evaluator**: a platform engineer assessing Kaleidoscope; wants a running
  stack and a visible UI from one command before deciding to invest more time.
- **Andrea, local experimenter**: wants to bring the stack up repeatedly without re-deriving the
  command each time.
- **A contributor**: brings the stack up to eyeball a change end to end.

### Solution

A `docker-compose` file that brings up the consolidated `kaleidoscope` runtime (ports 4317/4318 +
9090/9091/9092) with a shared named volume for the pillar root, the single `KALEIDOSCOPE_TENANT`,
auth off, and Prism served so the browser reaches it (recommended: same-origin from the metrics
router via `KALEIDOSCOPE_QUERY_STATIC_DIR`). A thin `make up` (or `just up`) target wraps the
compose command so the entry point is one short command.

### Domain Examples

#### 1: Happy Path - Sam brings the stack up and opens Prism

Sam clones the repo and runs `make up`. Compose builds and starts the consolidated runtime and
serves Prism. Sam opens `http://localhost:9090` and Prism's query panel renders. The stack is up
from one command.

#### 2: Happy Path - the query endpoints answer immediately after bring-up

Right after `make up`, before sending any telemetry, Sam runs
`curl 'http://localhost:9090/api/v1/query_range?query=request_count&start=0&end=9999999999'` and
gets `status:success` with an empty matrix (HTTP 200). The logs (9091) and traces (9092) endpoints
likewise answer with empty successes. The stack is genuinely up, not half-up.

#### 3: Integration - runtime and Prism share one pillar volume and one tenant

The runtime writes its pulse/lumen/ray pillars under the shared volume, and the metrics router that
backs Prism reads the same volume; both use tenant `acme`. A metric later ingested for `acme`
is therefore visible to the query the browser issues, with no second store and no path mismatch.

### UAT Scenarios (BDD)

#### Scenario: One command brings the stack up and Prism loads in the browser

```gherkin
Given a fresh clone of Kaleidoscope on Sam's machine
When Sam runs the one bring-up command
Then the consolidated runtime and Prism are running
And opening the documented local URL renders Prism's query panel in the browser
```

#### Scenario: The query endpoints answer right after bring-up

```gherkin
Given the stack has just been brought up with one command and no telemetry has been sent
When Sam queries the metrics, logs, and traces endpoints in turn
Then each returns a success with an empty result over HTTP 200
And none returns an error or a connection refusal
```

#### Scenario: The runtime and Prism share the pillar volume and the local tenant

```gherkin
Given the stack is up with tenant "acme" over a shared pillar volume
When a metric for "acme" is later ingested through the runtime
Then a query issued by Prism over the same volume and tenant can return that metric
And no separate store or mismatched path is involved
```

### Acceptance Criteria

- [ ] A single documented command brings up the consolidated runtime and Prism together.
- [ ] After bring-up, opening the documented local URL renders Prism in a browser (verified
  manually / by smoke per W6).
- [ ] After bring-up, the metrics, logs, and traces query endpoints each return an empty success
  (HTTP 200), not an error, before any telemetry is sent.
- [ ] The runtime and the metrics router that backs Prism operate over the same shared pillar volume
  and the same single tenant.

### Outcome KPIs

- **Who**: the newcomer / experimenter bringing the stack up.
- **Does what**: gets a running stack and a loaded Prism UI from one command.
- **By how much**: one command (down from "no run story exists"); 100% of bring-ups reach a
  loaded UI and answering endpoints.
- **Measured by**: a timed manual walkthrough plus a smoke script that curls the three query
  endpoints after `make up`.
- **Baseline**: impossible today - no compose/Makefile/run script exists.

### Technical Notes

- DESIGN/DEVOPS: compose topology (one runtime service + Prism same-origin vs a separate Prism
  service), whether a new `Dockerfile.runtime` is added (the three existing Dockerfiles build
  cli/gateway/query-api, none builds `kaleidoscope-runtime`), and Makefile vs justfile. See
  `wave-decisions.md` flags F1-F5.
- Prism's `config.json` `backend.url` is relative (`/api/v1`); same-origin serving from the 9090
  router needs no Prism config change (W6/registry).

### Dependencies

C1 `consolidated-runtime-v0` (DONE) provides the `kaleidoscope` binary this composes. This is the
feature walking skeleton; US-02..US-07 build on the running stack it provides.

---

## US-02 - Prism is pointed at the consolidated runtime and shows an honest empty state

### Elevator Pitch

- **Before**: Prism today is configured for a single metrics backend and is metrics-only; nothing
  has wired it to the consolidated runtime's query port, and a newcomer opening it on an empty stack
  could not tell "no data yet" from "broken".
- **After**: Sam opens Prism after `make up`; it queries the consolidated runtime's metrics router,
  and before any telemetry it shows a clear, honest empty state ("no data for this query yet"),
  not an error toast and not an endless spinner. When the runtime is unreachable, Prism shows a
  clear "cannot reach the backend" message instead of a blank screen.
- **Decision enabled**: Sam trusts what they see - an empty stack looks empty, a broken connection
  looks broken - so they confidently move on to sending telemetry (US-04) rather than debugging a
  non-problem.

### Problem

An evaluator's first look is usually at an empty stack. If Prism cannot distinguish "no telemetry
yet" from "backend down" from "request failed", the very first impression is confusion. Prism must
be pointed at the consolidated runtime and must render honest empty and error states. (Emotional
design: the first-encounter empty state is an onboarding opportunity, not a dead end.)

### Who

- **Sam, newcomer/evaluator**: first contact is an empty stack; needs to read its state correctly.
- **Andrea / a contributor**: opens Prism between experiments and needs unambiguous feedback.

### Solution

Configure Prism (compose-time) to query the consolidated runtime's metrics router. Recommended:
serve Prism same-origin from the 9090 router so its relative `/api/v1` backend just works with no
config edit; if served separately, set `config.json` `backend.url` to the runtime's metrics URL.
Ensure the empty-result and unreachable-backend states render as clear messages.

### Domain Examples

#### 1: Integration - Prism queries the consolidated runtime

Sam opens Prism, types `request_count`, and runs the query. The request reaches the consolidated
runtime's `/api/v1/query_range`. On an empty stack the response is an empty success and Prism shows
its empty state.

#### 2: Edge - honest empty state before any telemetry

Before sending anything, Sam runs a query in Prism. Instead of an error or a spinner that never
resolves, Prism shows a calm "no data for this query yet" message that invites Sam to send
telemetry.

#### 3: Error - backend unreachable shows a clear message

Sam stops the runtime (or queries before it is ready) and runs a Prism query. Prism shows a clear
"could not reach the backend" message naming what to check, not a blank panel.

### UAT Scenarios (BDD)

#### Scenario: Prism queries the consolidated runtime's metrics endpoint

```gherkin
Given the stack is up and Prism is open in the browser
When Sam runs a metrics query in Prism
Then the query is served by the consolidated runtime's metrics router
And the result is rendered in the query panel
```

#### Scenario: An empty stack shows an honest empty state, not an error

```gherkin
Given the stack is up and no telemetry has been sent
When Sam runs a query in Prism
Then Prism shows a clear empty state indicating there is no data yet
And it does not show an error and does not spin indefinitely
```

#### Scenario: An unreachable backend shows a clear message

```gherkin
Given the consolidated runtime is not reachable
When Sam runs a query in Prism
Then Prism shows a clear message that it could not reach the backend
And the message indicates what to check rather than leaving a blank panel
```

### Acceptance Criteria

- [ ] Prism's metrics queries are served by the consolidated runtime's metrics router after
  bring-up (no separate or stale backend).
- [ ] On an empty stack, Prism renders an explicit empty state, not an error and not an indefinite
  spinner.
- [ ] When the backend is unreachable, Prism renders a clear, actionable "cannot reach backend"
  message.

### Outcome KPIs

- **Who**: the newcomer reading the first-look UI state.
- **Does what**: correctly distinguishes empty / data / unreachable in Prism.
- **By how much**: 3 of 3 states render an unambiguous, correct message.
- **Measured by**: manual walkthrough of the three states; the empty and unreachable states are the
  browser-verified parts noted under W6.
- **Baseline**: Prism is metrics-only and not wired to the consolidated runtime; empty-vs-broken is
  not currently distinguishable to a newcomer.

### Technical Notes

- Whether Prism is same-origin (no config change) or a separate service (set `backend.url`) is
  DESIGN flag F4. Same-origin is recommended (CORS-free, no config edit).
- Logs/traces panels in Prism are out of scope here (roadmap C5, Milestone 2); this story keeps
  Prism metrics-only and honest.

### Dependencies

US-01 (the running stack and Prism serving).

---

## US-03 - The stack comes up clean and idempotent, or fails clearly

### Elevator Pitch

- **Before**: With no run story there is also no defined behaviour for the un-happy bring-ups: a
  fresh checkout with no state, a second `up` over an already-running stack, a teardown and
  re-up, or a port already taken by something else. Each is a chance for a confusing half-up stack.
- **After**: On a fresh checkout `make up` comes up clean with an empty pillar volume; a second
  `make up` is idempotent (it does not duplicate or corrupt anything); `make down` then `make up`
  returns to a working stack; and if a required port (say 9090) is already in use, bring-up fails
  with a clear error naming the port and what to do, rather than leaving half the stack running.
- **Decision enabled**: Sam trusts the run story enough to use it repeatedly - re-running, tearing
  down, and recovering - because its failure modes are predictable and legible.

### Problem

Reliability is the second layer of the needs hierarchy: a one-command stack that only works on a
pristine machine and fails opaquely otherwise will not survive real use. The brief calls out the
specific edges: fresh checkout clean (no stale state), second `up` idempotent, generator/stack
recoverable, and ports-already-in-use producing a clear error not a silent half-up.

### Who

- **Sam / Andrea / a contributor**: brings the stack up more than once, tears it down, and
  occasionally hits a taken port; needs each case to behave predictably.

### Solution

Compose plus the make/just wrapper handle: a fresh volume created empty on first `up`; an
idempotent re-up (compose reconciles to the desired state); a clean `down` and re-`up`; and a
clear, early failure when a required host port is unavailable (compose surfaces the bind conflict;
the wrapper presents it legibly). DESIGN/DEVOPS decide the exact mechanics.

### Domain Examples

#### 1: Edge - fresh checkout comes up clean

On a machine that has never run Kaleidoscope, `make up` creates an empty pillar volume and brings
the stack up; the query endpoints return empty successes. No stale data, no leftover state.

#### 2: Edge - a second up is idempotent

With the stack already running, Sam runs `make up` again. The stack remains healthy; nothing is
duplicated and the pillar volume is not corrupted.

#### 3: Edge - down then up returns to a working stack

Sam runs `make down`, then `make up`. The stack comes back; previously ingested telemetry on the
named volume is still present (durability is preserved), or, with a clean target, starts empty as
documented.

#### 4: Error - a port already in use fails clearly

Something else on Sam's machine is already bound to 9090. `make up` fails with a clear error naming
the conflicting port and suggesting the fix (free the port or override it), and does not leave a
half-up stack running.

### UAT Scenarios (BDD)

#### Scenario: A fresh checkout comes up clean with no stale state

```gherkin
Given a machine that has never run Kaleidoscope
When Sam runs the bring-up command
Then the stack starts with an empty pillar volume
And the query endpoints return empty successes
```

#### Scenario: A second bring-up is idempotent

```gherkin
Given the stack is already running
When Sam runs the bring-up command again
Then the stack remains healthy
And no telemetry is duplicated and the pillar volume is not corrupted
```

#### Scenario: Teardown then bring-up returns to a working stack

```gherkin
Given the stack has been torn down with the teardown command
When Sam runs the bring-up command again
Then the stack returns to a working state
And the query endpoints answer
```

#### Scenario: A port already in use fails clearly, not half-up

```gherkin
Given another process is already bound to a port the stack needs
When Sam runs the bring-up command
Then bring-up fails with a clear error naming the conflicting port and a suggested fix
And the stack is not left half-up
```

### Acceptance Criteria

- [ ] A fresh checkout brings up a clean stack with an empty pillar volume and answering endpoints.
- [ ] A second bring-up over a running stack is idempotent (no duplication, no corruption).
- [ ] Teardown then bring-up returns to a working stack.
- [ ] A required host port already in use causes a clear, named error and no half-up stack.

### Outcome KPIs

- **Who**: the repeat user of the run story.
- **Does what**: re-runs, tears down, recovers, and hits port conflicts predictably.
- **By how much**: 4 of 4 reliability/error cases behave as specified; 0 silent half-up states.
- **Measured by**: a scripted run-through of the four cases.
- **Baseline**: undefined today (no run story).

### Technical Notes

- Exact idempotency and clean/teardown semantics, and whether `down` preserves or clears the volume
  (a `make clean` vs `make down` split), are DESIGN/DEVOPS decisions (flag F2).
- Fixed-port flake discipline (project memory `aperture_fixed_port_4317_flake`) applies to any
  CI-side exercise of bring-up: prefer not binding fixed ports in automated tests.

### Dependencies

US-01.

---

## US-04 - Push sample telemetry with one command so the stack is not empty

### Elevator Pitch

- **Before**: After bring-up the stack is up but empty; a newcomer staring at an empty Prism has
  nothing to look at and no obvious way to put telemetry in. The "see" half of "one command, send,
  see" has no on-ramp.
- **After**: Sam runs one command (for example `make demo` / `make seed`) that pushes sample OTLP
  metrics, logs, and traces - including a `request_count` metric for `acme`, a log
  `"checkout failed: card declined"`, and a coherent `POST /api/v1/checkout` span (carrying that
  checkout failure as an Error status) under trace id
  `4bf92f3577b34da6a3ce929d0e0e4736` - to the running runtime. Refreshing Prism paints the
  `request_count` series; `curl http://localhost:9091/api/v1/logs?..` returns the log row and
  `curl http://localhost:9092/api/v1/traces?..` returns the span.
- **Decision enabled**: Sam decides the platform genuinely works end to end - they sent telemetry
  and saw it across all three signals - which is the moment that justifies a deeper look.

### Problem

The state assessment names a telemetry generator / sample data as a must-have (section 7, item 4):
a scripted way to push at least one metric, one log, and one trace over OTLP so the user has
something to query. Without it, the run story brings up an empty stack and the experiment stalls
at "now what?".

### Who

- **Sam, newcomer/evaluator**: wants a one-command way to fill the stack and see all three signals.
- **Andrea / a contributor**: regenerates sample telemetry while experimenting or testing a change.

### Solution

A telemetry generator, invoked by one command, that pushes sample OTLP metrics, logs, and traces to
the running runtime's ingest ports for tenant `acme`. DESIGN chooses the implementation (extend
`kaleidoscope-cli` with an OTLP-push path, a small new generator using `spark`, an external OTel
generator such as `telemetrygen` wired into compose, or a protobuf curl helper - see
`wave-decisions.md` flag F3). The observable contract is: after running it, all three signals are
queryable and Prism paints a metric.

### Domain Examples

#### 1: Happy Path - one command fills all three signals

Sam runs the generator command once. It pushes `request_count` (metrics), a
`"checkout failed: card declined"` log, and a coherent `POST /api/v1/checkout` span for `acme`. A metrics
query for `request_count`, a logs query, and a traces query each then return the sent telemetry.

#### 2: Happy Path - Prism paints the sample metric

After the generator runs, Sam refreshes Prism, queries `request_count`, and the series renders as a
line in the chart (browser-verified per W6).

#### 3: Error - generator against a not-yet-up stack fails clearly

Sam runs the generator before `make up` (or while the runtime is still starting). The generator
fails with a clear message that it could not reach the ingest endpoint and suggests bringing the
stack up first, rather than hanging or exiting silently.

#### 4: Edge - re-running the generator is safe

Sam runs the generator twice. The second run adds more sample telemetry without error; queries
still return successfully (the stack accumulates points rather than breaking).

### UAT Scenarios (BDD)

#### Scenario: One command pushes sample metrics, logs, and traces

```gherkin
Given the stack is up for tenant "acme"
When Sam runs the telemetry generator command once
Then a metrics query for "request_count" returns the sample metric
And a logs query returns the sample log "checkout failed: card declined"
And a traces query returns the sample span
```

#### Scenario: Prism paints the sample metric after the generator runs

```gherkin
Given Sam has run the telemetry generator
When Sam queries "request_count" in Prism
Then the sample metric is rendered as a series in the chart
```

#### Scenario: The generator against a down stack fails clearly

```gherkin
Given the stack is not yet up
When Sam runs the telemetry generator command
Then it fails with a clear message that the ingest endpoint could not be reached
And it suggests bringing the stack up first
And it does not hang or exit silently
```

#### Scenario: Re-running the generator is safe

```gherkin
Given the stack already holds sample telemetry from a previous generator run
When Sam runs the telemetry generator command again
Then it succeeds without error
And subsequent queries still return telemetry
```

### Acceptance Criteria

- [ ] One command pushes sample metrics, logs, and traces over OTLP to the running runtime for the
  local tenant.
- [ ] After it runs, a metrics query, a logs query, and a traces query each return the sample
  telemetry; Prism paints the sample metric (browser-verified per W6).
- [ ] Run against a stack that is not up, the generator fails with a clear, actionable message and
  does not hang or exit silently.
- [ ] Re-running the generator succeeds and leaves the stack queryable.

### Outcome KPIs

- **Who**: the newcomer filling the stack.
- **Does what**: pushes sample telemetry and sees all three signals come back.
- **By how much**: 3 of 3 signals queryable after one command; a metric painted in Prism.
- **Measured by**: a CI-runnable curl check of the three query endpoints after the generator (the
  HTTP loop is CI-testable; the Prism render is the manual part per W6).
- **Baseline**: no generator exists; the stack is empty after bring-up.

### Technical Notes

- Generator implementation is DESIGN flag F3. Options surfaced: (a) extend `kaleidoscope-cli` with
  a real OTLP-over-the-wire push subcommand (note: the CLI today writes NDJSON directly to
  lumen/cinder, not OTLP, so this is new client-side work); (b) a small new generator using `spark`
  (the manual-init OTel SDK wrapper) - best dogfoods the "built from scratch" ethos and covers all
  three signals over real OTLP; (c) wire an external OTel generator (`telemetrygen`) into compose as
  a one-shot service - lightest, but adds an external image and sits against the "built from
  scratch, not assembled" principle; (d) a curl-of-protobuf helper against 4318 - no toolchain but
  brittle to hand-encode. Recommended (b); DESIGN owns the call.
- Reuse C1's concrete sample data (`request_count`, the declined-checkout log, the trace id) so the
  generator, the docs, and any acceptance tests share one vocabulary (see registry).

### Dependencies

US-01 (running stack). Shares sample-data vocabulary with US-05 and US-06.

---

## US-05 - A freshly brought-up stack is not empty on the very first look

### Elevator Pitch

- **Before**: Even with a generator, the first thing a newcomer sees right after `make up` is an
  empty Prism, because the generator is a separate step they may not know to run yet. The first
  impression is "is it even working?".
- **After**: A small seed of sample telemetry is present on the first look - either auto-seeded on
  first bring-up or pushed by a one-shot generator step wired into the run story - so when Sam opens
  Prism immediately after `make up` there is already a `request_count` series to see, scoped to the
  one local tenant, without Sam having run anything extra.
- **Decision enabled**: Sam's first impression is "it works and there is data", which converts a
  newcomer into someone willing to experiment, rather than someone wondering whether bring-up
  succeeded.

### Problem

Key requirement (c): a way to push sample telemetry so the first look is not empty. An empty
first-run experience reads as broken to a newcomer even when everything is healthy. A tiny seed
turns the first look into a small win (first-success delight in the emotional arc).

### Who

- **Sam, newcomer/evaluator**: whose very first action after bring-up is to open Prism; needs
  something to see immediately.

### Solution

A small seed of sample telemetry present after bring-up, scoped to the single local tenant. DESIGN
decides whether this is an auto-seed inside the run story, a one-shot generator service in compose,
or simply documenting `make demo` as the immediate next step (US-04). It must run once (not
re-duplicate on every restart) and must be clearly sample data.

### Domain Examples

#### 1: Happy Path - the first look already shows a metric

Sam runs `make up` and immediately opens Prism. A `request_count` series for `acme` is already
present to query and chart, without Sam having run a separate generator command.

#### 2: Edge - the seed is scoped to the one local tenant

The seeded telemetry is attributed to `acme` (the single local tenant) and is queryable under it;
it does not appear under a different tenant.

#### 3: Edge - the seed runs once, not on every restart

Sam restarts the stack several times. The seed does not pile up duplicate copies of itself on each
restart; the first look stays a small, clean sample.

### UAT Scenarios (BDD)

#### Scenario: The first look after bring-up already shows sample telemetry

```gherkin
Given Sam has just brought the stack up
When Sam opens Prism for the first time without running any other command
Then a sample metric series is already present to query and chart
```

#### Scenario: The seed is scoped to the local tenant

```gherkin
Given the stack has been seeded with sample telemetry
When a query is made scoped to the local tenant "acme"
Then the seeded telemetry is returned
And it is not returned for a different tenant
```

#### Scenario: The seed does not duplicate on every restart

```gherkin
Given the stack has been seeded once
When Sam restarts the stack several times
Then the seeded sample does not accumulate duplicate copies on each restart
```

### Acceptance Criteria

- [ ] After bring-up, sample telemetry is present on the first look without the user running a
  separate command.
- [ ] The seeded telemetry is scoped to the single local tenant and is not visible to another
  tenant.
- [ ] The seed runs once and does not duplicate itself on every restart.

### Outcome KPIs

- **Who**: the newcomer on their first look.
- **Does what**: sees data immediately after bring-up with no extra step.
- **By how much**: first-look-not-empty in 100% of fresh bring-ups.
- **Measured by**: manual first-run walkthrough; a curl check that a metric exists right after
  bring-up.
- **Baseline**: empty first look today (and no run story at all).

### Technical Notes

- Seed mechanism is DESIGN flag F3 (shared with the generator): auto-seed vs one-shot compose
  service vs documented next step. Whichever is chosen must satisfy the once-only constraint.
- If DESIGN decides the seed is simply "run `make demo` as the documented first step", US-05 folds
  into US-04 + US-06; flag this in DESIGN rather than silently dropping the not-empty-first-look
  outcome.

### Dependencies

US-01, US-04 (shares the generator / sample data).

---

## US-06 - A newcomer can follow getting-started docs for the consolidated path

### Elevator Pitch

- **Before**: The README quick start documents only the `kaleidoscope-cli` NDJSON demo. A newcomer
  who wants the real "send OTLP, see it in Prism, query logs and traces" loop has no honest
  walkthrough for the consolidated path.
- **After**: A getting-started section (in the README and/or `docs/`) walks Sam through the
  consolidated path: one command up, one command to send sample telemetry, see the metric in Prism,
  query logs and traces, and the minimal config (`KALEIDOSCOPE_TENANT`, `KALEIDOSCOPE_PILLAR_ROOT`,
  auth off) and nothing more. It is honest about the verification limit (the browser experience is
  verified by bringing it up).
- **Decision enabled**: Sam completes the whole loop unaided from the docs and decides whether
  Kaleidoscope fits their needs, having actually experienced "one command, send, see".

### Problem

The state assessment names getting-started for the gateway/consolidated path as a must-have
(section 7, item 3): the README currently documents only the CLI. C4 replaces/extends it with an
honest walkthrough of the consolidated experiment loop with the minimal config and no more.

### Who

- **Sam, newcomer/evaluator**: reads the docs cold and follows them end to end with no prior
  Kaleidoscope knowledge.
- **Andrea / a contributor**: uses the docs as the canonical reference for the run story.

### Solution

A getting-started section documenting: bring the stack up (US-01), send sample telemetry (US-04) or
note the seed (US-05), see the metric in Prism (US-02), query logs and traces via the 9091/9092
endpoints, and the minimal config and nothing more. It states honestly that the compose/browser
experience is verified by bringing it up (W6), and points existing CLI users to the preserved CLI
demo (US-07).

### Domain Examples

#### 1: Happy Path - a newcomer completes the loop from the docs alone

Sam, who has never used Kaleidoscope, follows the getting-started section step by step: brings the
stack up, sends sample telemetry, sees `request_count` in Prism, and queries the sample log and
trace. Sam reaches "see" without external help.

#### 2: Happy Path - the docs state the minimal config and nothing more

The section lists exactly the config a local experiment needs (`KALEIDOSCOPE_TENANT`,
`KALEIDOSCOPE_PILLAR_ROOT`, auth off) and does not bury the newcomer in tokens, TLS, or production
concerns.

#### 3: Edge - the docs are honest about the verification limit

The section states plainly that the one-command browser experience is verified by bringing it up
and looking, and that CI exercises the query endpoints but not a browser render of Prism, so the
reader knows what is and is not automatically checked.

### UAT Scenarios (BDD)

#### Scenario: A newcomer completes the send-and-see loop from the docs alone

```gherkin
Given Sam has never used Kaleidoscope and is reading the getting-started section
When Sam follows the documented steps in order
Then Sam brings the stack up, sends sample telemetry, sees a metric in Prism, and queries a log and a trace
And Sam needs no help beyond the documented steps
```

#### Scenario: The docs state the minimal config and nothing more

```gherkin
Given Sam is reading the getting-started section
When Sam looks for the configuration needed for a local experiment
Then the minimal config is listed (one tenant, one pillar root, auth off)
And no tokens, TLS, or production setup is required for the local loop
```

#### Scenario: The docs are honest about the verification limit

```gherkin
Given Sam is reading the getting-started section
When Sam reaches the note on verification
Then it states that the one-command browser experience is verified by bringing it up
And it states that CI exercises the query endpoints but not a browser render of Prism
```

### Acceptance Criteria

- [ ] A getting-started section documents the consolidated path end to end: one command up, send
  sample telemetry, see a metric in Prism, query logs and traces.
- [ ] The section lists the minimal config (one tenant, one pillar root, auth off) and nothing more.
- [ ] The section honestly states the verification limit (browser/compose verified by bringing it
  up; CI covers the query endpoints, not a browser render).
- [ ] The section is the consolidated path, not the old CLI demo, and points CLI users to the
  preserved CLI demo (US-07).

### Outcome KPIs

- **Who**: the newcomer reading the docs.
- **Does what**: completes "one command, send, see" unaided from the getting-started section.
- **By how much**: a cold reader reaches "see a metric" following only the documented steps.
- **Measured by**: a dry-run of the docs by following them verbatim on a fresh checkout.
- **Baseline**: only the CLI NDJSON demo is documented; the consolidated path is undocumented.

### Technical Notes

- Where the section lives (README vs `docs/getting-started.md` linked from the README) is a DESIGN
  decision; the README quick start is the most discoverable home.
- The docs must reference the same sample-data vocabulary as the generator (US-04) and registry.

### Dependencies

US-01, US-02, US-04 (documents what they deliver); US-07 (the CLI demo it points to).

---

## US-07 - The existing separate-binary path and its Dockerfiles still work (additive guardrail)

### Elevator Pitch

- **Before**: There is a real risk that adding a consolidated run story quietly breaks or orphans
  the existing assets: the four standalone binaries, the three Dockerfiles
  (`Dockerfile`, `Dockerfile.gateway`, `Dockerfile.query-api`), and the current CLI quick start.
- **After**: Adding compose, the generator, and the getting-started docs is strictly additive. The
  standalone binaries still build and run, the three Dockerfiles still build, and the CLI quick
  start still works (preserved, or clearly marked as superseded by the consolidated path). Nothing
  a current user relies on regresses.
- **Decision enabled**: Andrea (and any current user) can adopt the new one-command story without
  fear that the existing separate-binary and Docker paths were sacrificed for it.

### Problem

Key requirement (e): the existing separate binaries and their Dockerfiles must not be broken; the
feature is additive. The C1 wave already established additivity for the runtime; C2/C3/C4 must
preserve it for the run-story assets and the documented CLI demo.

### Who

- **Andrea / an existing user**: may already use a standalone binary or a Dockerfile and must not be
  broken by the new run story.
- **A contributor / CI**: relies on the existing binaries and images continuing to build.

### Solution

Keep all four binaries and all three Dockerfiles intact; add (never replace) the compose file,
generator, and docs. If the README's CLI quick start is moved or relabelled, keep the CLI demo
working and discoverable rather than deleting it.

### Domain Examples

#### 1: Edge - the standalone binaries still build and run

After this feature lands, `cargo run -p kaleidoscope-gateway`, `-p query-api`, `-p log-query-api`,
and `-p trace-query-api` still build and run as before.

#### 2: Edge - the three existing Dockerfiles still build

`docker build -f Dockerfile.gateway .`, `-f Dockerfile.query-api .`, and the base `Dockerfile`
(cli) still build successfully; none is removed or broken by the new compose/runtime image.

#### 3: Edge - the CLI quick start is preserved or clearly superseded

The current README CLI NDJSON quick start still works if followed; if the consolidated path becomes
the primary quick start, the CLI demo is preserved and clearly linked, not silently deleted.

### UAT Scenarios (BDD)

#### Scenario: The existing standalone binaries still build and run

```gherkin
Given this feature has added the consolidated run story
When the four existing standalone binaries are built and run
Then each still builds and runs as it did before the feature
```

#### Scenario: The three existing Dockerfiles still build

```gherkin
Given this feature has added compose and the generator
When the existing Dockerfile, Dockerfile.gateway, and Dockerfile.query-api are built
Then each still builds successfully
And none has been removed or broken
```

#### Scenario: The CLI quick start is preserved or clearly superseded

```gherkin
Given the getting-started docs now cover the consolidated path
When a reader looks for the previous CLI quick start
Then the CLI demo still works if followed
And it is preserved and discoverable rather than silently deleted
```

### Acceptance Criteria

- [ ] The four standalone binaries (`kaleidoscope-gateway`, `query-api`, `log-query-api`,
  `trace-query-api`) still build and run after this feature.
- [ ] The three existing Dockerfiles still build and none is removed.
- [ ] The CLI NDJSON quick start still works and remains discoverable (preserved or clearly marked
  as superseded by the consolidated path).

### Outcome KPIs

- **Who**: existing users of the standalone binaries, Dockerfiles, and CLI demo.
- **Does what**: continues to build and run the pre-existing paths unchanged.
- **By how much**: 0 regressions across the four binaries, three Dockerfiles, and the CLI demo.
- **Measured by**: build/run checks of each pre-existing asset (CI gate-1 already compiles the
  workspace; Docker builds are the manual/smoke part).
- **Baseline**: all pre-existing assets build and run today.

### Technical Notes

- This is a cross-cutting guardrail spanning all three slices; it protects requirement (e).
- A new `Dockerfile.runtime` (if added per flag F1) is additive alongside the existing three, not a
  replacement.

### Dependencies

None upstream; this guards the additions made by US-01, US-04, and US-06.
