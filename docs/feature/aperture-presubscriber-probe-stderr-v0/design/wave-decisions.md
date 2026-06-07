# Wave Decisions — aperture-presubscriber-probe-stderr-v0 (DESIGN)

DESIGN wave (nWave). Owner: Morgan (nw-solution-architect). Date: 2026-06-07.
Mode: PROPOSE (autonomous). Decision record: **ADR-0071**.

## Mechanism resolved — option (c)

**Drop the redundant pre-subscriber probe from `wire_sink` and let the
EXISTING post-subscriber probe (`compose.rs:157-167`) carry the refusal.**

This is option (c) from the DISCUSS flag (the candidate the story noted as
"removing the pre-subscriber probe from `wire_sink` and relying on the
post-subscriber one"). It is chosen over (a) and (b) on the strength of
the post-subscriber-probe-vs-bind ordering finding below.

### Why (c), not (a) or (b)

- **(c) ACCEPTED** — the post-subscriber probe is *already* visible
  (after `install_subscriber`) AND fail-closed-no-bind (before
  `spawn_grpc`). The fix is a **net deletion** (the duplicate, silent
  probe) plus a doc update. No new stderr path, no `main.rs` window
  discrimination, no event literal restated, no `install_subscriber`
  ordering churn, no ADR-0066 seam touch. The double-probe is rationalised
  for free.
- **(a) REJECTED** — moving the subscriber before `wire_sink` is more
  invasive, disturbs the idempotent `install_subscriber` ordering and the
  ADR-0066 seam, AND keeps the redundant double-probe (the `wire_sink`
  `Forwarding` sink is discarded anyway).
- **(b) REJECTED (kept as fallback rationale)** — a direct-stderr bridge
  mirroring `emit_config_error` is only needed if the refusal could occur
  before the subscriber AND before a bind with no other emission path. The
  ordering shows that does not happen. (b) would also force `main.rs` to
  discriminate pre/post-subscriber failures and restate the
  `health.startup.refused` literal. **(b) WOULD be correct in the
  counterfactual** where the post-subscriber probe ran *after* a bind —
  there it preserves fail-closed-no-bind where (c) could not — but that
  counterfactual does not hold here.

## Post-subscriber-probe-vs-bind ordering finding (CRUCIAL, read in source)

In `spawn_with_readiness` (`crates/aperture/src/compose.rs:130-263`):

| # | Line | Step |
|---|------|------|
| 1 | `compose.rs:134` | `install_subscriber()` — subscriber installed (idempotent `OnceLock`, `observability.rs:146-165`) |
| 2 | `compose.rs:157-167` | post-subscriber `probe_or_refuse(&forwarding)` for `Forwarding` |
| 3 | `compose.rs:196` | `spawn_grpc` — **FIRST listener bind** |
| 4 | `compose.rs:215` | `spawn_http` — second bind |

**The post-subscriber probe (step 2) is strictly AFTER subscriber install
(step 1) and strictly BEFORE any listener bind (step 3).** Therefore (c)
is both visible and no-bind-before-refuse — the cleanest mechanism. Had
step 2 been *after* a bind, (b) would have been preferred to keep
fail-closed-no-bind; it is not, so (c) wins.

## The double-probe finding

`Forwarding` is probed TWICE on the production path:

1. `wire_sink` (`compose.rs:81`, via `lib.rs:223`) — PRE-subscriber, the
   silent one that wins the race; for `Forwarding` it probes a sink object
   that `spawn_with_readiness` then **discards** (`compose.rs:157-167`
   rebuilds a fresh `ForwardingSink`). Doubly redundant.
2. `spawn_with_readiness` (`compose.rs:157-167`) — POST-subscriber, against
   the sink actually used, before any bind — the one that *would* log.

(c) deletes #1 (both the `Forwarding` arm `compose.rs:81` and the `Stub`
arm `compose.rs:73`, whose probe is statically `Ok(())`), leaving #2 as
the single, visible, fail-closed probe. The "wire → probe → use"
invariant holds for the system, consolidated at one site.

## Event / line shape (reused)

`event = health.startup.refused` (`observability.rs:49`, ADR-0009 closed
vocab) + `reason = %e`, where `e` carries the sink/downstream identity and
the underlying cause (`sink probe failed: {e}`, `compose.rs:102`). Emitted
through the installed JSON-stderr subscriber — no hand-rolled `eprintln!`,
no new token.

## Fail-closed + no-regression

- **Fail-closed preserved**: refusal returns `Err` before `spawn_grpc`
  (`compose.rs:196`) ⇒ no listener bound; exit non-zero via `main.rs:58`.
- **ADR-0066 post-init tracing path**: untouched (only the *pre*-subscriber
  probe is removed).
- **ADR-0061 config-error pre-init line** (`main.rs:80-82`): untouched
  (config validation refuses before `run()` reaches `wire_sink`).
- **Healthy downstream**: probes once, succeeds, binds, emits usual
  `event=startup`/`event=ready`, no refusal line.

## Reuse Analysis (MANDATORY)

| Item | Path | Decision |
|---|---|---|
| Post-subscriber probe | `compose.rs:157-167` (`pub(crate)`) | REUSE as-is — becomes the single visible+fail-closed probe |
| `probe_or_refuse` | `compose.rs:96-104` | REUSE as-is — caller set shrinks only |
| `wire_sink` | `compose.rs:68-85` (`pub(crate)`) | EDIT — delete probe calls (both arms); doc update. Net deletion |
| `install_subscriber` | `observability.rs:146-165` | REUSE as-is — ordering NOT moved (why (c) beats (a)) |
| `HEALTH_STARTUP_REFUSED` | `observability.rs:49` (ADR-0009) | REUSE as-is — no new event |
| `emit_config_error` | `main.rs:80-82` | REUSE / DO NOT TOUCH — no second direct-stderr path added |
| `main.rs` run()-Err handler | `main.rs:54-60` | REUSE as-is — no pre/post-window discrimination |
| 200-OPTIONS/503-POST lie fixture | `tests/probe_gold_runner.rs:40-128` | REUSE pattern for the new binary-start test |
| `aperture::testing::RecordingSink` | test path | REUSE as-is — never went through `wire_sink`'s probe |
| New crate / public type / dependency / event constant / stderr path | — | NONE |

**Verdict: EDIT-AND-DELETE only.** Visibility falls out of the existing
post-subscriber probe once it is no longer pre-empted by the silent one.

## Test seam

Probe-substrate-lie **subprocess test at the binary-start surface**: start
`aperture` with `sink_kind=forwarding` against a 200-OPTIONS/503-POST
liar (the gold-runner fixture pattern), assert stderr carries
`event=health.startup.refused` + the probe error/sink identity, exit
non-zero, and NO listener bound. Fails on today's silent exit 1; passes
only when surfaced. The `probe_gold_runner` unit gold tests stay green
(unaffected). Negative controls: healthy = no refusal line; config error =
`event=config_validation_failed` exit 2.

## Constraints carried into DELIVER

- Fail-closed UNCHANGED (exit non-zero, bind nothing on refusal).
- Probe semantics / refusal decision UNCHANGED — only surfaced.
- No regression: ADR-0066 post-init tracing path; ADR-0061 config-error line.
- Secrets / no-token rules out of scope.
- Inherits ADR-0005 five gates; per-feature mutation 100% on modified
  aperture files (`gate-5-mutants-aperture`).
- Rust idiomatic (data + free functions + traits; no inheritance, no
  needless `dyn`). NEVER 1.0.0.
- No public-API / semver concern: all touched items `pub(crate)`; aperture
  not in Gate 2/3.

## Upstream Changes (back-propagation to DISCUSS)

- **DISCUSS Decision 1 mechanism options (a)/(b) resolved to (c).** DISCUSS
  flagged (a) and (b); DESIGN found a third (the candidate noted under
  option (a)'s sub-bullet) and proved it cleanest via the ordering. No
  AC change — US-01's four AC are mechanism-neutral and all hold.
- **DISCUSS Decision 3 (main.rs run()-Err discrimination): NOT NEEDED.**
  Under (c) the refusal is emitted by the post-subscriber probe before the
  `Err` reaches `main.rs`, so `main.rs` needs no pre/post-window
  discrimination. The DISCUSS-flagged decision is closed as "unnecessary".
- **Double-probe rationalised** (DISCUSS noted it as a non-required nuance;
  DESIGN folds its removal INTO the fix — it is the mechanism, not scope
  creep, because the silent probe IS the defect).
