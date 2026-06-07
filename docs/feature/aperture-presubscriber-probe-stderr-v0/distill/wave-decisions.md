# Wave Decisions — aperture-presubscriber-probe-stderr-v0 (DISTILL)

DISTILL wave (nWave). Owner: Quinn (nw-acceptance-designer). Date: 2026-06-07.
Mode: autonomous. Upstream: DISCUSS `user-stories.md` (US-01, 4 AC) +
DESIGN `wave-decisions.md`/ADR-0071 (mechanism (c)).

## Scope (bounded by US-01)

ONE acceptance test file at the binary-start surface that proves the
observable contract US-01 names: a probe-refusal start emits a structured
`event=health.startup.refused` stderr line (today silent), still fails closed
(exit non-zero, no bind), and leaves the healthy-downstream + config-error
paths unchanged. DISTILL writes the RED test only; it does NOT touch `src/`
(DELIVER lands mechanism (c): drop the redundant pre-subscriber probe).

## Walking-skeleton strategy = Strategy C (real-local-IO subprocess)

The single driving port is the real `aperture` BINARY. The acceptance test
spawns it as a subprocess (`CARGO_BIN_EXE_aperture`) and observes ONLY:

1. its **exit code** (refusal = non-zero; config error = 2; healthy = runs),
2. its **structured stderr** (the JSON line the subscriber renders to the
   stderr fd, `observability.rs:156` `.json().with_writer(stderr)`),
3. a **connect-refused / connect-succeeds probe** on its EPHEMERAL listener
   ports (the black-box "no listener bound" / "bound" observable).

The downstream is a **real liar HTTP server** (`wiremock::MockServer`, the
catalogued v0 200-OPTIONS / 503-POST substrate lie reused from
`probe_gold_runner.rs`). The auth secret + tenant catalogue are **real temp
files**. There are NO in-memory doubles anywhere on the path — this is a
genuine real-local-IO walking skeleton: real binary, real downstream, real
config files, real TCP. The litmus test "if I deleted the real adapter would
this still pass?" is satisfied: deleting the real subprocess / real liar would
collapse the test entirely.

Why a subprocess (not the in-process `aperture::spawn` that `slice_10` uses):
US-01's contract is about the BINARY's stderr and exit code in the operator's
terminal / `journalctl`. The defect is specifically that the pre-subscriber
`tracing::error!` is dropped because no subscriber is installed yet — an
in-process test that installs a capture subscriber would NOT reproduce the
silence. Only the real binary, observed from outside, exercises the exact
pre-subscriber window the silence lives in.

## How the test reaches the probe past mandatory ingest-auth config

CRUCIAL reconciliation with the landed `aegis-ingest-auth-v0` feature:
aperture now REFUSES TO START (exit 2, `event=config_validation_failed`)
without a complete, readable `[aperture.security.auth.jwt]` block
(`config/mod.rs:677` `validate_jwt_auth`, ADR-0068 DD4). `validate_jwt_auth`
eagerly (a) requires issuer/audience/secret_file/catalogue_path, (b)
`std::fs::metadata`-checks the secret file, (c) `aegis::load_catalogue`s the
catalogue. So every forwarding-config in the RED scenarios writes a **real
temp secret file** + a **real temp tenant catalogue** (`[[tenants]]\nid =
"acme-prod"`) and names a complete jwt block — mirroring
`slice_10_ingest_auth_config_reject.rs`'s setup. Without this aperture would
exit 2 at config validation BEFORE the forwarding-sink probe ever runs, and
the test would be measuring the wrong refusal.

**Proven by the exit code**: the `--ignored` run shows `exit=Some(1)` for the
refusal scenarios — exit 1 (run()-Err / probe refusal), NOT exit 2 (config
error). That single fact certifies the config got PAST `validate_jwt_auth` and
the binary reached the forwarding-sink probe, which refused on the liar.

## Falsifiability note (the line is absent today)

`run()` (`lib.rs:222-224`) calls `wire_sink` BEFORE `spawn_with_readiness`.
For `SinkKind::Forwarding`, `wire_sink` (`compose.rs:81`) runs
`probe_or_refuse`, which emits `tracing::error!(event=health.startup.refused,
reason=%e)` (`compose.rs:96-104`) — but `install_subscriber()` only runs
inside `spawn_with_readiness` (`compose.rs:134`), never reached because
`wire_sink` returned `Err` first. So the event is emitted to a NON-installed
subscriber and dropped: the binary exits 1 with **empty stderr**. That is the
silence US-01 fixes.

Proven RED evidence (`cargo test ... -- --ignored --test-threads=1`):

```
probe_refusal_emits_health_startup_refused_on_stderr ... FAILED
  exit=Some(1) stderr: ""
probe_refusal_line_names_the_sink_and_the_underlying_error ... FAILED
  expected the refusal line before checking its fields; exit=Some(1) stderr: ""
probe_refusal_is_fail_closed_and_visible ... FAILED
  fail-closed must be VISIBLE: stderr must carry event=health.startup.refused
  alongside the non-zero exit (silent today); stderr: ""
```

All three fail behaviourally on the ABSENT line — `stderr: ""` — not on a
missing symbol (RED, not BROKEN; Mandate 7). The `is_fail_closed_and_visible`
scenario's fail-closed half (exit!=0 + no bind) passes silently first and the
panic fires ONLY on the missing visible line, which is the design intent: the
combined assertion is RED today (silent) and GREEN only when DELIVER surfaces
the line WITHOUT regressing fail-closed.

## #[ignore]-until-DELIVER decision

The three visibility scenarios carry
`#[ignore = "RED until DELIVER: aperture-presubscriber-probe-stderr-v0 —
refusal is pre-subscriber-silent today"]` so `cargo test` is GREEN at the
DISTILL commit (trunk-green discipline). DELIVER removes the ignores (one at a
time per the inner loop) once mechanism (c) surfaces the refusal. The two
negative controls (healthy-downstream binds; config-error exits 2 with its
existing line) and `red_reason_is_documented` are GREEN today and run
un-ignored.

## Ephemeral-port + reaping note (PORT/PROCESS HYGIENE)

The binary binds **ephemeral** loopback ports (`free_port()` reserves a free
`127.0.0.1:0` port per listener), NEVER the fixed 4317/4318 — those collide
with slice_09/slice_10 under the parallel suite. Two Drop-guards reap on every
exit path (success, assertion failure, panic): `ChildReaper` (kill + wait the
aperture child) and the `wiremock::MockServer` guard (torn down on drop);
`TempFiles` removes the temp config / secret / catalogue. Post-run verified:
`pgrep -fl 'target/debug/aperture'` empty, no `aperture-probevis-*` temp
litter.

## Reconciliation with upstream waves

- **DISCUSS US-01 + 4 AC**: all four AC mapped to scenarios (see
  `acceptance-test-scenarios.md`). The titles describe what Priya OBSERVES
  (stderr line, exit, no bind), not how the subscriber is wired —
  solution-neutral, as the story requires.
- **DESIGN ADR-0071 mechanism (c)**: the test is mechanism-NEUTRAL (it asserts
  the observable line + fail-closed, not the deletion of the pre-subscriber
  probe). It will pass for (c) and would equally have passed for (a)/(b). The
  ordering finding (post-subscriber probe is after install_subscriber, before
  first bind) is what makes the combined fail-closed-AND-visible assertion
  satisfiable.
- **No KPI contract file** under `docs/product/` for this feature delta; the
  US-01 Outcome-KPI ("operator-visible reason in 100% of probe-refusal
  starts") is exactly what the visibility scenarios assert at the binary
  boundary. No separate `@kpi` observability scenario added (the visibility
  assertion IS the KPI evidence; soft gate, noted).

## Adapter coverage (Strategy C — every driven surface is real I/O)

| Driven surface | Real I/O in this suite? |
|---|---|
| aperture binary (driving port) | YES — `CARGO_BIN_EXE_aperture` subprocess |
| forwarding sink → downstream HTTP | YES — real `wiremock` liar/healthy server over real TCP |
| config loader (TOML + secret + catalogue) | YES — real temp files on disk |
| listener bind (gRPC/HTTP) | YES — real ephemeral-port TCP connect probes |

No InMemory double anywhere — there is no wiring this suite cannot catch.

## Constraints carried (unchanged from DESIGN)

- Fail-closed UNCHANGED (asserted: exit non-zero + no bind on refusal).
- Probe semantics UNCHANGED — only surfaced.
- No regression: ADR-0066 post-init tracing path; ADR-0061 config-error line
  (the config-error negative control certifies the latter).
- DISTILL does NOT modify `crates/aperture/src` (DELIVER drops the redundant
  probe). probe_gold_runner + slice_0x + slice_10 stay green.
- Inherits ADR-0005 five gates; per-feature mutation 100% on modified files
  (`gate-5-mutants-aperture`) — a DELIVER concern. Never 1.0.0.
