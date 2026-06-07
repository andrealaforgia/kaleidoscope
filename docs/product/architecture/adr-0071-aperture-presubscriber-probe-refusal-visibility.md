# ADR-0071: Aperture pre-subscriber probe-refusal visibility

- **Status**: Accepted
- **Date**: 2026-06-07
- **Feature**: `aperture-presubscriber-probe-stderr-v0`
- **Author**: `nw-solution-architect` (Morgan), DESIGN wave, mode PROPOSE (autonomous)
- **License**: AGPL-3.0-or-later (docs only; this ADR records a behaviour
  change implemented under the crate's existing licence)
- **Relates to**: ADR-0007 (Probe trait, separate from `OtlpSink`),
  ADR-0009 (closed event vocabulary), ADR-0061 (refuse-to-start
  precedent / direct-stderr config-error line), ADR-0066
  (post-bind serve-loop death, exit 3 — the post-subscriber swallow sibling).
  Belongs to the **swallowed-errors family** (cinder/sluice/serve-loop siblings).
- **Supersedes / superseded by**: none. Does not change probe semantics,
  the refusal decision, or fail-closed behaviour (ADR-0007/0061 stand).

## Context

Aperture is the OTLP forwarding gateway. At startup it runs an
Earned-Trust probe (ADR-0007, Principle 12) against the configured sink:
if the downstream is not accepting telemetry the gateway **refuses to
start**, binds no listener, and exits non-zero (fail-closed). That
decision is correct and stays.

The defect is **silence, not the decision**. The refusal is emitted
through `tracing::error!(event = HEALTH_STARTUP_REFUSED, …)` inside
`probe_or_refuse` (`compose.rs:96-104`). For a `Forwarding` sink the
production call chain probes **twice**:

1. `run()` (`lib.rs:223`) → `wire_sink(&config)` → `probe_or_refuse(&sink)`
   — runs **BEFORE** the tracing subscriber is installed.
2. `run()` (`lib.rs:224`) → `spawn_with_readiness` → at `compose.rs:157-167`,
   the passed sink is **discarded**, a fresh `ForwardingSink` is built
   against the configured endpoint, and `probe_or_refuse(&forwarding)`
   runs **again** — this time AFTER `install_subscriber()` (`compose.rs:134`).

Because step 1 fails fast, step 2 is never reached when the downstream is
down. The step-1 tracing event has **no subscriber to flow through**, so
it is dropped: the operator sees an empty stderr and a bare `exit 1`.
`main.rs`'s `run()`-Err handler (`main.rs:57`) ALSO logs through
`tracing` and is ALSO dropped for a pre-subscriber failure. The
config-error case already prints a helpful pre-init line via
`emit_config_error` (`main.rs:80-82`, direct `eprintln!`), but the
probe-refusal case does not. This is "the small honesty gap" Bea
Verifier flagged (msg 038).

### The decisive ordering finding (read in source, 2026-06-07)

In `spawn_with_readiness` (`compose.rs:130-263`) the strict execution order is:

| # | Line | Step |
|---|------|------|
| 1 | `compose.rs:134` | `install_subscriber()` — subscriber installed (idempotent `OnceLock`, `observability.rs:146-165`) |
| 2 | `compose.rs:157-167` | post-subscriber `probe_or_refuse(&forwarding)` for `Forwarding` |
| 3 | `compose.rs:196` | `spawn_grpc` — **FIRST listener bind** |
| 4 | `compose.rs:215` | `spawn_http` — second listener bind |

**The post-subscriber probe (step 2) runs strictly AFTER the subscriber
is installed (step 1) AND strictly BEFORE any listener binds (step 3).**
This single fact decides the mechanism: the post-subscriber probe is
*already* in exactly the right place to be both **visible** (subscriber
up) and **fail-closed with no bind** (before `spawn_grpc`). The
pre-subscriber probe in `wire_sink` is therefore not just silent — for
`Forwarding` it probes a sink object that is **thrown away** and never
used, making it doubly redundant.

## Decision

**Mechanism (c): drop the redundant pre-subscriber probe from `wire_sink`
and let the EXISTING post-subscriber probe in `spawn_with_readiness`
(`compose.rs:157-167`) carry the refusal.**

Concretely:

- Remove the `probe_or_refuse(...)` call from `wire_sink`'s `Forwarding`
  arm (`compose.rs:81`) and from its `Stub` arm (`compose.rs:73`).
  `wire_sink` reverts to a pure type-selection/erasure step (build sink,
  return `Arc<dyn OtlpSink>`); its doc comment is updated to say the
  Earned-Trust probe now runs in `spawn_with_readiness`, after the
  subscriber and before any bind. (For `Stub` the probe is statically
  `Ok(())`; removing it changes nothing observable. For `Forwarding` the
  `wire_sink`-built sink is already discarded at `compose.rs:157-167`.)
- The **post-subscriber probe at `compose.rs:157-167` is unchanged**. On
  refusal it already emits
  `tracing::error!(event = HEALTH_STARTUP_REFUSED, reason = %e)` and
  returns `Err(ApertureError("sink probe failed: {e}"))`. With the
  subscriber installed one line earlier, that event now **flows to
  stderr** through the normal post-init JSON layer — no new code path, no
  direct `eprintln!`, no `main.rs` pre/post-window discrimination.
- The `Err` propagates `spawn_with_readiness` → `run()` → `main.rs:54-60`
  → `ExitCode::FAILURE`. No listener was bound (the refusal precedes
  `spawn_grpc`). Fail-closed is preserved byte-for-byte.

### Event / line shape (reused, unchanged)

The refusal reuses the existing closed vocabulary
(`observability.rs:49`, ADR-0009): `event = health.startup.refused`,
field `reason = %e` where `e` is the probe error. The probe error text
carries the sink/downstream identity and the underlying cause (the `{e}`
from `sink probe failed: {e}`, `compose.rs:102`). Because the line is now
emitted through the installed JSON-stderr subscriber (not a hand-rolled
`eprintln!`), the sink-identity + error reach stderr via the **same**
structured layer every other post-init line uses. No token is invented;
US-01 AC `a-probe-refusal-emits-a-structured-stderr-line` and
`the-line-names-the-sink-and-the-error` are satisfied by the existing
event plus the existing error text.

### Why the double-probe is rationalised, not preserved

The pre-subscriber probe in `wire_sink` had exactly one job on the
production `Forwarding` path: refuse early. It does so **silently and
against a discarded sink**. The post-subscriber probe does the same job
**visibly, against the sink actually used, and still before any bind.**
Keeping both is keeping a strictly worse duplicate that also *wins the
race* and produces the silence. Removing it is the fix. (The in-process
test path constructs sinks directly with their own probes verified at
construction — `aperture::testing::RecordingSink`, whose probe is
statically `Ok(())` — so the test path is unaffected; `wire_sink` was
never the test seam.)

## Alternatives considered

### (a) Install the subscriber before `wire_sink` — REJECTED

Move `install_subscriber()` ahead of the probe (into `run()` or `main`)
so the existing pre-subscriber `wire_sink` event flows through tracing.

- Rejected: **more invasive for no benefit over (c).** It disturbs the
  idempotent `install_subscriber` ordering (`observability.rs:146-165`)
  and the ADR-0066 serve-failure seam, and it *keeps* the redundant
  double-probe (the `Forwarding` sink built in `wire_sink` is still
  discarded). (c) achieves visibility AND removes the duplicate with
  *less* code moved. The story explicitly lists (a) but flags its cost
  (touches `install_subscriber` ordering); (c) avoids that cost entirely.

### (b) Direct-stderr for the pre-subscriber window (mirror `emit_config_error`) — REJECTED (kept as fallback rationale)

Have `wire_sink` / `main.rs` write a structured-shape line directly to
stderr (`eprintln!`) for the pre-subscriber window, mirroring
`emit_config_error` (`main.rs:63-82`).

- Rejected: **only necessary if a refusal could occur before the
  subscriber AND before a bind with no other emission path** — and the
  ordering finding shows it does not. The post-subscriber probe already
  runs after the subscriber and before any bind, so the direct-stderr
  bridge is unneeded. (b) would *also* require `main.rs` to discriminate
  pre- vs post-subscriber failures (flagged Decision 3) and would restate
  the `health.startup.refused` literal in a second place (the same
  crate-private-constant duplication the config-error line already
  suffers, `main.rs:72`). (c) needs neither. **(b) remains the correct
  choice in the counterfactual** where the post-subscriber probe ran
  *after* a bind — there it would preserve fail-closed-no-bind where (c)
  could not — but that counterfactual does not hold here.

### (c) Drop the redundant pre-subscriber probe; rely on the post-subscriber one — **ACCEPTED**

Chosen because the verified ordering makes it the cleanest: the
post-subscriber probe is *already* visible-and-before-bind, so the fix is
a deletion (the duplicate) plus a doc update, not an addition. No new
stderr path, no `main.rs` window discrimination, no literal restated, no
subscriber-ordering churn, and the double-probe is rationalised as a
free side effect.

## Consequences

### Positive

- **Visibility**: a `Forwarding` probe refusal now emits
  `event=health.startup.refused` to stderr through the normal subscriber
  (0% → 100% of probe-refusal starts carry an operator-visible reason).
- **Simplicity**: net **deletion** on the production path (the redundant
  `wire_sink` probe), not a net addition. Smallest possible diff.
- **No double-probe**: `Forwarding` is probed exactly once, against the
  sink it actually uses.
- **No new coupling**: no `main.rs` pre/post-window discrimination, no
  second restating of the `health.startup.refused` literal, no
  `install_subscriber` ordering change, no touch to the ADR-0066 seam.

### Negative / trade-offs

- `wire_sink` no longer probes, so its name slightly over-promises the
  "wire → probe → use" hook described in its old doc comment. Mitigated
  by updating the comment to point at the single probe site in
  `spawn_with_readiness`. The "wire → probe → use" invariant still holds
  for the *system* — it is just consolidated at one site.
- A future caller that invokes `wire_sink` **without** going through
  `spawn_with_readiness` would skip the probe. None exists today
  (`run()` is the only caller and always follows with
  `spawn_with_readiness`); the structural xtask check (Principle 12c) and
  the gold runner still guarantee the `Probe` impl exists and bites. If
  such a caller is ever added, it must run the probe itself — noted for
  the structural-check layer.

### Fail-closed preserved

The refusal still returns `Err` and **no listener is bound** — the probe
at `compose.rs:157-167` precedes `spawn_grpc` (`compose.rs:196`). Exit is
non-zero via `main.rs:58` (`ExitCode::FAILURE`). US-01 AC
`fail-closed-exit-is-unchanged` holds.

### No regression

- **Post-init tracing path (ADR-0066)**: untouched. A drain-deadline or
  serve-loop death still reports through `tracing` exactly as today; this
  ADR removes only the *pre*-subscriber probe, not any post-init emission.
- **Config-error pre-init line (ADR-0061)**: untouched. `emit_config_error`
  (`main.rs:80-82`) still prints
  `event=config_validation_failed` and exits 2; config validation refuses
  *before* `run()` even reaches `wire_sink`, so this path is independent.
- **Healthy downstream**: `wire_sink` no longer probes, `spawn_with_readiness`
  probes once and succeeds, listeners bind, the usual `event=startup` /
  `event=ready` lines emit, **no** refusal line. US-01 AC
  `healthy-downstream-and-config-error-paths-unchanged` holds.

## Test seam (for Acceptance Designer)

A **probe-substrate-lie subprocess test** at the **binary-start surface**:
spawn the `aperture` binary with `sink_kind=forwarding` pointed at a
downstream that lies (the catalogued v0 lie: **200 on OPTIONS preflight,
503 on POST**, exactly the `tests/probe_gold_runner.rs` fixture pattern,
`probe_gold_runner.rs:40-64`/`92-128`), then assert:

1. stderr carries a structured-shape line with `event=health.startup.refused`
   (no longer silent);
2. the line carries the underlying probe error / downstream identity
   (the `{e}` from `sink probe failed: {e}`);
3. the process exits **non-zero**;
4. **no listener bound** — neither configured port accepts a connection
   after exit (or, equivalently, no `event=ready`/`listener_bound` line
   precedes the refusal).

This is a NEW assertion at the **binary-start surface** (the
`probe_gold_runner` unit-level gold tests enter at the probe surface
directly and are **unaffected** — they keep guarding that the probe
*bites* on the lie). The new subprocess test guards that the *bite* is
now *visible at startup*. It reuses the gold runner's 200-OPTIONS/503-POST
substrate-lie scenario as the fixture. A negative control asserts a
healthy downstream start emits no refusal line, and a config-error start
still emits `event=config_validation_failed` exit 2.

This test **fails on today's code** (silent exit 1, no stderr line) and
passes only when the refusal is surfaced — the Earned-Trust / cinder
`FailingFsyncBackend` precedent (a test that passes on the swallow is no
test).

## Reuse Analysis (MANDATORY — RCA F-1 hard gate)

| Item | Path | Decision | Rationale |
|---|---|---|---|
| Post-subscriber probe | `compose.rs:157-167` (`pub(crate)`) | **REUSE as-is** | Already after `install_subscriber` and before `spawn_grpc`; it becomes the single, visible, fail-closed probe. Zero change. |
| `probe_or_refuse` | `compose.rs:96-104` | **REUSE as-is** | Emits `event=health.startup.refused reason=%e` and returns `Err`. Unchanged; only its *caller set* shrinks (no longer called from `wire_sink`). |
| `wire_sink` | `compose.rs:68-85` (`pub(crate)`) | **EDIT (delete probe calls)** | Remove `probe_or_refuse` from both arms; revert to pure sink selection/erasure; update doc comment. Net deletion. |
| `install_subscriber` | `observability.rs:146-165` | **REUSE as-is** | Idempotent `OnceLock`; ordering NOT moved (this is why (c) beats (a)). |
| `HEALTH_STARTUP_REFUSED` | `observability.rs:49` (ADR-0009) | **REUSE as-is** | Existing closed-vocab constant; no new event name. |
| `emit_config_error` | `main.rs:80-82` | **REUSE / DO NOT TOUCH** | Config-error pre-init line stays; this ADR does NOT add a second direct-stderr path (that was rejected option (b)). |
| `main.rs` run()-Err handler | `main.rs:54-60` | **REUSE as-is** | Returns `ExitCode::FAILURE`; the now-visible event was emitted by the post-subscriber probe before the Err reached here. No pre/post-window discrimination added. |
| 200-OPTIONS/503-POST lie fixture | `tests/probe_gold_runner.rs:40-128` | **REUSE pattern** | The new binary-start subprocess test reuses this catalogued substrate-lie scenario. |
| `aperture::testing::RecordingSink` | test path | **REUSE as-is** | Test path never went through `wire_sink`'s probe; unaffected. |
| New crate / new public type / new dependency | — | **NONE** | Pure internal behaviour change (a deletion + a moved-emission). |

**Verdict: EDIT-AND-DELETE only.** No new crate, no new public type, no
new dependency, no new event constant, no new stderr path. The only
production change is removing the redundant probe from `wire_sink`; the
visibility falls out of the existing post-subscriber probe now that it is
no longer pre-empted.

## Public API / semver

**None.** `wire_sink`, `spawn_with_readiness`, `probe_or_refuse` are all
`pub(crate)`; the event vocabulary addition count is zero (the constant
already exists). aperture is **not** in the Gate 2/3 public-API set
(unlike `spark`). No `cargo public-api` diff, no `cargo semver-checks`
classification, **no version bump** (and **NEVER 1.0.0** — Andrea's call,
CLAUDE.md / MEMORY).
