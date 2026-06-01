# DISCUSS Decisions — gateway-tracing-subscriber-v0

## Origin

Defect closure for the black-box operability verifier, **issue 005**.
Issue 005 named both `query-api` and `kaleidoscope-gateway`. The feature
`read-api-tracing-subscriber-v0` closed the three read APIs (query-api,
log-query-api, trace-query-api) by installing a shared `init_tracing` in
`query-http-common`. `kaleidoscope-gateway` is the FOURTH binary with the
same gap. The verifier confirmed the gateway gap and set issue 005 to
`partial` (read tier resolved, gateway still open). This feature closes
the gateway, moving issue 005 to RESOLVED.

## Key Decisions

- **[D1] Posture: match aperture, NOT query-http-common.** The gateway is
  write/ingest-side, so it aligns to aperture's subscriber posture
  directly. It MUST NOT depend on `query-http-common`, the read tier
  crate. Anti-coupling rationale: the write side does not import the read
  tier's scaffolding. (see: user-stories.md US-03, System Constraints)

- **[D2] The precise gap is an ordering gap, not a missing subscriber.**
  The gateway calls `aperture::spawn`, which installs the subscriber
  inside `compose::spawn` (`crates/aperture/src/compose.rs:111`). But the
  gateway emits `gateway_starting` (main.rs:89) and
  `health.startup.refused` (main.rs:102) in its own `main` BEFORE that
  call, so both are dropped. `listener_bound` already renders because
  aperture emits it after install. The fix installs the subscriber in the
  gateway before main.rs line 102. (see: dor-validation.md WIRE table)

- **[D3] Feature type Backend, no walking skeleton, lightweight research,
  no JTBD.** Operability defect closure, one crate, fixed WIRE contract.
  (see: dor-validation.md Decisions recorded)

- **[D4] No new crate.** Changes confined to
  `crates/kaleidoscope-gateway/src/main.rs`, optionally `composition.rs`,
  and `Cargo.toml`. (see: story-map.md Scope Assessment)

## Exact events the gateway emits (for the landing ping to the verifier)

Read from source, not assumed:

| Event | Level | Fields | Current site | Renders today? |
|---|---|---|---|---|
| `gateway_starting` | info | `pillar_root` | crates/kaleidoscope-gateway/src/main.rs:89 | NO — fires before subscriber install |
| `health.startup.refused` | error | `substrate`, `reason` | crates/kaleidoscope-gateway/src/main.rs:102 (probe_or_refuse fail arm) | NO — fires before subscriber install |
| `listener_bound` | info | `transport`, `addr` | crates/aperture/src/transport.rs:47 (grpc) and :114 (http), inside aperture::spawn | YES — fires after install in compose::spawn |

`substrate` values on `health.startup.refused`: `sink`, `fsync-noop`,
`fsync-truncating`, `fsync-corrupting`, `fsync-io` (from
`composition.rs > CompositionError::substrate_descriptor` and pulse's
`FsyncProbeError`).

After this feature: `gateway_starting` and `health.startup.refused`
render on stderr as JSON, same shape as `listener_bound`, same shape as
the read tier and aperture.

## Flags to DESIGN (Morgan)

1. **tracing-subscriber dependency: ADD.** Confirmed by reading
   `crates/kaleidoscope-gateway/Cargo.toml`: it has `tracing = "0.1"`
   (line 43) but NO `tracing-subscriber`. Add the 0.3 line matching
   aperture's feature set (`fmt`, `json`, `env-filter`, `registry`;
   aperture also uses `default-features = false`). Per-crate dep, not a
   workspace promotion (mirrors the read tier's DD1).

2. **Posture / home: MATCH aperture, NOT query-http-common.** The
   anti-coupling invariant (write side does not import the read crate) is
   load-bearing. Options for Morgan:
   - (a) replicate aperture's `install_subscriber` builder inline in the
     gateway (the read tier did exactly this in `query-http-common`, with
     `RUST_LOG` instead of `APERTURE_LOG`); or
   - (b) reuse a shareable write-side helper IF aperture exposes one.
     Note: aperture's `observability::install_subscriber` is
     `pub(crate)`, so it is NOT reusable as-is today; reuse would require
     aperture to expose a public init fn. Morgan decides whether to
     replicate inline or ask for an aperture public seam. Luna flags;
     does not choose.
   - The env-filter var name (aperture uses `APERTURE_LOG`; the read tier
     chose the operator-conventional `RUST_LOG`) is a DESIGN call. Luna
     recommends `RUST_LOG` for write-side uniformity with the read tier,
     but does not pin it.

3. **Install point: before main.rs line 102.** The gateway HAS a
   `composition.rs` seam (confirmed; it hosts `probe_or_refuse`), but
   that module is pure probe logic today, not lifecycle. The install must
   sit in `main` (or a new lifecycle helper) BEFORE the
   `health.startup.refused` emission at line 102 — stricter than merely
   before `aperture::spawn` at line 116. Placing it as the first
   statement of `main` (before `create_dir_all` at line 65) also shrinks
   the pre-subscriber window for US-04. Morgan picks the exact line.

4. **ADR: NOT recommended.** This aligns the gateway to the existing
   aperture posture (ADR-0009) already in force across the other four
   binaries; it introduces no new architectural decision. Luna
   recommends no new ADR. Morgan confirms.

## Requirements Summary

- Primary operator need: see the gateway's own startup lifecycle
  (`gateway_starting`) and fail-closed refusal (`health.startup.refused`)
  on stderr as structured JSON, matching aperture and the read tier, so
  issue 005 closes for the fourth binary.
- Walking skeleton scope: none (Decision 2 = No). Minimum slice is
  US-01 + US-02 (install before line 102).
- Feature type: backend / operability, write-side.

## Constraints Established

- Anti-coupling: no dependency edge from `kaleidoscope-gateway` to
  `query-http-common`.
- No new crate; changes confined to the gateway crate.
- Stable WIRE contract for the verifier: the three events above, JSON to
  stderr, read-tier/aperture line shape; subscriber wiring is the
  implementer's choice.
- British English; no em dashes in body.
- No 1.0.0 version bump.

## Upstream Changes

- No DISCOVER or DIVERGE artifacts exist for this feature. For an
  operability defect closure with a fixed WIRE contract this is
  acceptable; recorded here as a non-blocking gap rather than a risk to
  resolution.

## Scope / process notes

- Peer review not run (self-contained delivery, per orchestrator
  instruction). Orchestrator owns the commit; this wave does not commit.
