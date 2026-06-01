<!-- markdownlint-disable MD024 -->
# User Stories — gateway-tracing-subscriber-v0

Operability hardening for the fourth Kaleidoscope binary,
`kaleidoscope-gateway`. The three read APIs were closed by
`read-api-tracing-subscriber-v0`; this feature closes the gateway, which
is the WRITE/INGEST-side binary and therefore aligns to the **aperture**
posture, NOT to `query-http-common` (the read tier's scaffolding). Issue
005 (black-box verifier, operability) moves from `partial` to RESOLVED
when the gateway's startup lifecycle events render on stderr.

JTBD analysis: skipped (Decision 4 = No). This is a backend/operability
defect closure with a known operator persona and a fixed WIRE contract;
motivations are not in question.

## System Constraints

- **British English** throughout; no em dashes in body prose.
- **No new crate**; changes are confined to
  `crates/kaleidoscope-gateway/src/main.rs`, optionally
  `crates/kaleidoscope-gateway/src/composition.rs`, and
  `crates/kaleidoscope-gateway/Cargo.toml`.
- **Anti-coupling invariant (load-bearing):** the gateway is write-side
  and MUST NOT take a dependency on `query-http-common` (the read tier
  crate). It matches the aperture posture directly. The write side does
  not import the read tier's scaffolding.
- **Stable WIRE contract for the verifier (verifier-007):** the only
  thing the verifier asserts is the rendered event shape on stderr:
  `gateway_starting` (info), `listener_bound` (info, fields
  `transport`+`addr`), `health.startup.refused` (error, fields
  `substrate`+`reason`), JSON-to-stderr, same line shape as the read
  tier and aperture. The wiring/home of the subscriber is the
  implementer's choice and invisible to the verifier.
- **No technology selection here**; the subscriber builder and its home
  are a DESIGN decision (see flags). DISCUSS pins only the observable
  contract.

## Grounding (read, not assumed)

Findings from reading the actual source:

- `crates/kaleidoscope-gateway/src/main.rs` emits, **in main(), in this
  order**:
  1. `tracing::info!(event = "gateway_starting", pillar_root = ...)`
     — main.rs line 89, BEFORE any subscriber is installed.
  2. `tracing::error!(event = "health.startup.refused", substrate =
     e.substrate_descriptor(), reason = %e)` — main.rs line 102, on the
     `probe_or_refuse` failure arm, also BEFORE any subscriber, then
     returns `Err(...)` (non-zero exit).
  3. `aperture::spawn(config, sink)` — main.rs line 116. This is where
     the subscriber actually gets installed and where `listener_bound`
     fires (see below).
- `aperture::spawn` delegates to `aperture::compose::spawn`
  (`crates/aperture/src/lib.rs:270`), whose **first statement is
  `observability::install_subscriber()`** (`crates/aperture/src/compose.rs:111`).
  So the JSON-to-stderr subscriber IS installed — but only once the
  gateway reaches line 116.
- `listener_bound` (info, `transport`+`addr`) is emitted by aperture's
  transport layer INSIDE spawn (`crates/aperture/src/transport.rs:47,114`;
  `observability.rs:32 LISTENER_BOUND`). It therefore **already renders**
  for the gateway, because it fires after install.
- **The precise gap:** `gateway_starting` and `health.startup.refused`
  are emitted in the gateway's own `main` BEFORE the `aperture::spawn`
  call installs the subscriber. With no subscriber active at that point,
  both events are silently dropped. The operator sees nothing at startup
  and nothing on a fail-closed refusal.
- `crates/kaleidoscope-gateway/Cargo.toml` has `tracing = "0.1"` (line 43)
  but **NO `tracing-subscriber`**. It must be added (flag 1).
- `crates/kaleidoscope-gateway/src/composition.rs` EXISTS and hosts
  `probe_or_refuse` (the Earned-Trust seam). It is a candidate home for
  the install (flag 3), but today it holds only pure probe logic, not
  lifecycle/subscriber install.
- aperture's own `main` (`crates/aperture/src/main.rs`) keeps its
  pre-subscriber failures (argv/config) on `eprintln!("aperture: ...")`
  with a non-zero `ExitCode`; tracing is the only post-init stderr path.
  This is the convention US-04 mirrors.

## US-01: Operator sees the gateway start and bind

### Problem

Priya Nair is a platform SRE running `kaleidoscope-gateway` as a
long-lived ingest process. Today she starts it and stderr shows nothing
attributable to the gateway's own lifecycle: `gateway_starting` is
dropped because no subscriber is installed when it fires. She cannot
confirm from the logs that the gateway came up, which pillar root it
opened, or that it bound its listeners. She resorts to checking the
process table and poking the port by hand.

### Who

- Platform SRE | operates the ingest tier in production | wants a single
  grep-able stderr stream proving the gateway is up and listening.

### Solution

Install the JSON-to-stderr tracing subscriber early enough in the
gateway's `main` that its own `gateway_starting` event renders, and
confirm aperture's in-spawn `listener_bound` continues to render on the
same stream with the same shape. The gateway's startup story becomes
visible end-to-end on one stderr stream.

### Elevator Pitch

Before: Priya starts `kaleidoscope-gateway` and stderr shows nothing about the gateway coming up; `gateway_starting` is silently dropped.
After: run `kaleidoscope-gateway /srv/pillar` → sees `{"event":"gateway_starting","pillar_root":"/srv/pillar",...}` then `{"event":"listener_bound","transport":"grpc","addr":"..."}` (and the http line) as JSON on stderr.
Decision enabled: Priya confirms the gateway is up and listening on the expected addresses, so she marks the node healthy in rotation.

### Domain Examples

#### 1: Happy Path — Priya starts the gateway with an explicit pillar root

Priya runs `kaleidoscope-gateway /srv/kaleidoscope/pillar` with
`KALEIDOSCOPE_DEFAULT_TENANT=acme`. stderr shows a JSON
`gateway_starting` line carrying `pillar_root=/srv/kaleidoscope/pillar`,
then `listener_bound` lines for the grpc and http transports with their
bound `addr`. Priya greps `event=listener_bound` and sees the ports.

#### 2: Default pillar root — Dev runs the bare binary

Marco Bianchi runs `kaleidoscope-gateway` with no args in a writable
working directory. stderr shows `gateway_starting` with
`pillar_root=kaleidoscope-data` (the default), then the `listener_bound`
lines. Marco confirms the default path was used.

#### 3: Filter boundary — Priya raises the log floor

Priya runs the gateway with `RUST_LOG=warn` (or the env var DESIGN
pins). The info-level `gateway_starting` and `listener_bound` lines are
filtered out; stderr is quiet on a clean start, matching the read tier
and aperture filter behaviour. She lowers it back to `info` to see the
lifecycle again.

### UAT Scenarios (BDD)

#### Scenario: Gateway start is visible on stderr

Given Priya starts `kaleidoscope-gateway` with pillar root `/srv/kaleidoscope/pillar` and a default tenant set
When the gateway reaches a healthy listening state
Then stderr shows a JSON line with `event=gateway_starting` carrying `pillar_root=/srv/kaleidoscope/pillar`

#### Scenario: Listeners bound is visible on stderr

Given Priya starts the gateway and both transports bind successfully
When the listeners are up
Then stderr shows JSON `event=listener_bound` lines, each carrying `transport` and the bound `addr`

#### Scenario: Lifecycle events honour the log filter

Given Priya starts the gateway with the log floor raised to `warn`
When the gateway starts cleanly
Then the info-level `gateway_starting` and `listener_bound` lines are absent from stderr

### Acceptance Criteria

- [ ] A `gateway_starting` JSON line with field `pillar_root` renders on stderr at startup.
- [ ] `listener_bound` JSON lines with fields `transport` and `addr` render on stderr after bind.
- [ ] The rendered line shape (JSON, flattened, `event` field, no target/span noise) matches the read tier and aperture.
- [ ] With the log floor raised to `warn`, the two info events are absent from stderr.

### Outcome KPIs

- **Who**: platform SREs and developers running `kaleidoscope-gateway`.
- **Does what**: observe the gateway's start and bind on stderr.
- **By how much**: `gateway_starting` + at least one `listener_bound` line present on every clean start (was 0).
- **Measured by**: black-box subprocess spawn + stderr JSON grep (the verifier's G01 method).
- **Baseline**: today `gateway_starting` is dropped; only `listener_bound` (from aperture spawn) renders.

### Technical Notes (Optional)

- `listener_bound` already renders today (aperture emits it inside spawn,
  after install). The new behaviour this story buys is `gateway_starting`
  rendering; the `listener_bound` ACs are a regression guard that the
  shape and stream are unchanged.

## US-02: Operator sees why the gateway refused to start (fail-closed)

### Problem

When the Earned-Trust composition probe refuses (sink probe fails, or the
fsync-honesty probe catches a lying substrate), the gateway emits
`health.startup.refused` and exits non-zero. But that event fires at
main.rs line 102, BEFORE `aperture::spawn` installs the subscriber, so it
is dropped. Priya sees a non-zero exit with no structured reason. She has
to reproduce the failure under a debugger to learn the substrate lied.

### Who

- Platform SRE | diagnoses a gateway that refuses to boot | needs the
  refusal reason and substrate class on stderr before the process exits.

### Solution

Ensure the subscriber is installed BEFORE the `probe_or_refuse` arm
emits `health.startup.refused`, so the structured refusal line (with its
`substrate` and `reason` fields) renders on stderr ahead of the non-zero
exit. This is the part of the gap the bind path does not cover, because
the refusal happens before spawn is ever reached.

### Elevator Pitch

Before: a fail-closed gateway exits non-zero with no structured reason on stderr; `health.startup.refused` is dropped.
After: run `kaleidoscope-gateway /srv/pillar` against a substrate that lies about fsync → sees `{"event":"health.startup.refused","substrate":"fsync-noop","reason":"..."}` on stderr, then a non-zero exit.
Decision enabled: Priya reads `substrate=fsync-noop` and decides the volume cannot honour durable writes, so she repoints the pillar root at a real disk instead of escalating blind.

### Domain Examples

#### 1: fsync-honesty refusal — lying volume

Priya points the gateway at a pillar root on a volume whose backend
ignores fsync. The fsync-honesty probe refuses. stderr shows
`event=health.startup.refused` with `substrate=fsync-noop` and a
`reason`, then the process exits non-zero. Priya repoints at a real disk.

#### 2: Sink-probe refusal — unwritable sink

Marco runs the gateway where the storage sink's active-write probe fails
(read-only pillar directory). stderr shows `event=health.startup.refused`
with `substrate=sink` and the underlying reason, then non-zero exit.
Marco fixes permissions.

#### 3: Refusal survives the warn filter

Priya runs the failing case with `RUST_LOG=warn`. The error-level
`health.startup.refused` line still renders (error survives any filter at
warn or laxer), so the refusal is never hidden by a raised floor.

### UAT Scenarios (BDD)

#### Scenario: Fail-closed refusal is visible before exit

Given Priya starts the gateway against a substrate that fails the Earned-Trust composition probe
When the probe refuses
Then stderr shows a JSON line with `event=health.startup.refused` carrying `substrate` and `reason`, AND the process exits non-zero, AND the refusal line precedes the exit

#### Scenario: Refusal names the substrate class

Given the fsync-honesty probe catches a no-op fsync backend
When the gateway refuses to start
Then the `health.startup.refused` line carries `substrate=fsync-noop`

#### Scenario: Refusal survives a raised log floor

Given Priya starts the failing gateway with the log floor raised to `warn`
When the probe refuses
Then the error-level `health.startup.refused` line is still present on stderr

### Acceptance Criteria

- [ ] On a composition-probe refusal, a `health.startup.refused` JSON line with `substrate` and `reason` renders on stderr.
- [ ] The refusal line is emitted before the non-zero process exit.
- [ ] The `substrate` value reflects the refusal class (`sink`, `fsync-noop`, `fsync-truncating`, `fsync-corrupting`, `fsync-io`).
- [ ] With the log floor raised to `warn`, the error-level refusal line is still present.

### Outcome KPIs

- **Who**: platform SREs diagnosing a gateway that will not boot.
- **Does what**: read the structured refusal reason and substrate class on stderr.
- **By how much**: `health.startup.refused` present on 100% of fail-closed exits (was 0; dropped pre-install today).
- **Measured by**: black-box subprocess spawn against a lying-fsync/unwritable substrate + stderr JSON grep + non-zero exit assertion.
- **Baseline**: today the refusal is emitted before subscriber install and dropped; operator sees a bare non-zero exit.

### Technical Notes (Optional)

- This story is the reason the install point must precede main.rs line
  102, not merely precede line 116 (spawn). An install that sits inside
  spawn (the current state) would render `listener_bound` but never the
  refusal, because the refusal arm short-circuits before spawn.

## US-03: Gateway and aperture share the same write-side posture

### Problem

aperture (the OTLP gateway library) and `kaleidoscope-gateway` (the host
composition binary) are both write-side processes. aperture renders its
lifecycle as JSON-to-stderr via its own subscriber; the gateway, missing
the early install, renders an incomplete subset. An operator parsing both
streams must special-case the gateway. Consistency on the write side is
the property that lets one JSON parser cover the whole ingest tier.

### Who

- Platform SRE building one log pipeline over the ingest tier | wants
  aperture and the gateway to render the same line shape from the same
  posture, with no read-tier coupling leaking in.

### Solution

Adopt the aperture subscriber posture for the gateway: same JSON-to-stderr
builder family, env-filtered, idempotent install, no dependency on
`query-http-common`. The gateway either replicates aperture's builder
inline or reuses a write-side helper if aperture exposes one (DESIGN
decides, flag 2); either way the rendered shape matches aperture and the
read tier, and the anti-coupling invariant holds.

### Elevator Pitch

Before: the gateway renders a different (incomplete) subset of lifecycle events than aperture, so an operator must special-case the gateway in their log pipeline.
After: run `kaleidoscope-gateway` and `aperture` side by side → both emit the same JSON line shape on stderr (flattened `event` field, no target/span noise), and the gateway pulls in no read-tier crate.
Decision enabled: Priya points one JSON log parser at the whole ingest tier and trusts a uniform field schema, instead of maintaining a gateway-specific path.

### Domain Examples

#### 1: Identical line shape — side-by-side diff

Priya captures stderr from both `aperture` and `kaleidoscope-gateway` on
a clean start. The `gateway_starting`/`listener_bound` lines and
aperture's `startup`/`listener_bound` lines share the same JSON envelope
(flattened, `event` field, no `target`, no span list). Her parser needs
no per-binary branch.

#### 2: No read-tier coupling — dependency audit

Marco runs `cargo tree -p kaleidoscope-gateway` and confirms there is no
`query-http-common` edge. The write-side binary does not import the read
tier's scaffolding; the posture is borrowed from aperture, not the read
helper.

#### 3: Same env filter behaviour — uniform floor control

Priya sets the log floor to `warn` once for the whole ingest tier. Both
aperture and the gateway suppress their info lifecycle lines identically,
so the floor control is uniform across the write side.

### UAT Scenarios (BDD)

#### Scenario: Gateway renders the same JSON shape as aperture

Given Priya captures stderr from both aperture and the gateway on a clean start
When she compares the lifecycle lines
Then both render JSON with a flattened `event` field and no target or span-list noise

#### Scenario: Gateway takes no read-tier dependency

Given the gateway's dependency tree
When Priya audits it
Then there is no dependency edge to `query-http-common`

#### Scenario: Uniform filter floor across the write side

Given Priya raises the log floor to `warn` for the ingest tier
When both aperture and the gateway start cleanly
Then both suppress their info-level lifecycle lines

### Acceptance Criteria

- [ ] The gateway's rendered line shape matches aperture's (JSON, flattened, `event` field, no target/span noise).
- [ ] The gateway has no dependency edge to `query-http-common`.
- [ ] The gateway honours an env-driven log filter with the same default-`info` behaviour as aperture.

### Outcome KPIs

- **Who**: operators running one log pipeline over aperture plus the gateway.
- **Does what**: parse both write-side streams with one schema.
- **By how much**: 0 gateway-specific parser branches; 0 read-tier dependency edges on the gateway.
- **Measured by**: line-shape diff of captured stderr + `cargo tree -p kaleidoscope-gateway | grep query-http-common` returns nothing.
- **Baseline**: today the gateway renders an incomplete subset; no read-tier edge exists yet and must not be introduced.

### Technical Notes (Optional)

- The anti-coupling invariant is the load-bearing constraint here. If
  DESIGN finds a shareable write-side helper on aperture, reuse is
  acceptable; otherwise an inline replication of aperture's builder is
  the correct fallback. A read-tier (`query-http-common`) dependency is
  never acceptable.

## US-04 (optional): Pre-subscriber failures still print a reason

### Problem

The gateway has a narrow window before the subscriber is installed where
fallible steps run: `resolve_pillar_root` is infallible, but
`std::fs::create_dir_all(&pillar_root)` (main.rs line 65) and the three
`FileBacked*Store::open` calls (lines 67-78) can fail and propagate via
`?` before any `tracing::` event. If one fails there today, the operator
gets the runtime's bare `Err` Debug print, not a clear line. aperture
handles its own pre-init window with `eprintln!("aperture: ...")`.

### Who

- Platform SRE | hits a pillar-root permission or disk error at the
  earliest startup step | wants a clear stderr line, not a bare `Err`.

### Solution

Mirror aperture's pre-init convention: for the fallible steps that run
before the subscriber is installed, convert the bare `?` propagation into
an `eprintln!("kaleidoscope-gateway: ...: {e}")` followed by a non-zero
exit. Post-install, tracing remains the only stderr-writing path. This
story is optional and may be folded into US-01/US-02 at DESIGN's
discretion if the chosen install point makes the pre-subscriber window
empty.

### Elevator Pitch

Before: a pre-subscriber failure (unwritable pillar root, unopenable store) surfaces as the runtime's bare `Err` Debug print.
After: run `kaleidoscope-gateway /read-only/path` → sees `kaleidoscope-gateway: failed to open pillar root /read-only/path: Permission denied` on stderr, then a non-zero exit.
Decision enabled: Priya reads the path and the OS error and fixes permissions, instead of decoding a Debug-formatted error chain.

### Domain Examples

#### 1: Unwritable pillar root

Priya points the gateway at a read-only directory. `create_dir_all`
fails. stderr shows `kaleidoscope-gateway: ...: Permission denied`, then
non-zero exit. She fixes the mount.

#### 2: Corrupt store WAL

Marco points the gateway at a pillar root whose lumen WAL is corrupt.
`FileBackedLogStore::open` fails. stderr shows a
`kaleidoscope-gateway: ...` line naming the open failure, then non-zero
exit.

#### 3: Empty pre-subscriber window

If DESIGN places the install as the very first statement of `main`
(before `create_dir_all`), the pre-subscriber window is empty and this
story collapses into US-01/US-02. The example records that outcome as
acceptable: no bare `Err` is reachable either way.

### UAT Scenarios (BDD)

#### Scenario: Pre-subscriber failure prints a named line

Given Priya starts the gateway against a pillar root that cannot be created or opened
When the failure occurs before the subscriber is installed
Then stderr shows a `kaleidoscope-gateway: ...` line naming the failure, AND the process exits non-zero

#### Scenario: No bare runtime Err reaches the operator

Given any pre-subscriber fallible step fails
When the gateway exits
Then the operator sees a deliberate `eprintln!` line, not the runtime's bare `Err` Debug output

### Acceptance Criteria

- [ ] Pre-subscriber fallible steps that fail emit an `eprintln!("kaleidoscope-gateway: ...: {e}")` line on stderr.
- [ ] The process exits non-zero after such a line.
- [ ] Post-install, tracing remains the only stderr-writing path (no stray `eprintln!`).

### Outcome KPIs

- **Who**: operators hitting earliest-stage gateway startup failures.
- **Does what**: read a named failure line instead of a bare `Err`.
- **By how much**: 100% of pre-subscriber failures produce a named line (was bare `Err` Debug print).
- **Measured by**: black-box spawn against an unwritable/corrupt pillar root + stderr grep + non-zero exit.
- **Baseline**: today pre-subscriber failures surface as the runtime's bare `Err`.
