# DESIGN Decisions — gateway-tracing-subscriber-v0

Author: Morgan (nw-solution-architect). Scope: application. Mode: propose.
Self-contained delivery; no peer review run; orchestrator owns the commit.

## Origin

Closes the ordering gap in `kaleidoscope-gateway`, the fourth (write/ingest
side) Kaleidoscope binary. Two of its own lifecycle events fire in `main`
BEFORE `aperture::spawn` installs the tracing subscriber, so they are
dropped. This is the same class of gap the read tier closed in
`read-api-tracing-subscriber-v0`, but the gateway aligns to the **aperture**
posture, not to `query-http-common` (the read tier crate). Moves verifier
issue 005 from `partial` to RESOLVED.

## Key Decisions

- **[DD1] Home / posture: replicate aperture's builder INLINE in the
  gateway, do NOT reuse an aperture public seam, do NOT depend on
  `query-http-common`.** Read from source: aperture's
  `observability::install_subscriber` (`crates/aperture/src/observability.rs:145`)
  is `pub(crate)`, so it is not reusable as-is. Two options were on the
  table: (a) replicate aperture's builder inline in the gateway, or
  (b) ask aperture to expose a public `init_tracing` seam. Option (a) wins.
  The read tier already validated this exact shape: `query-http-common`
  replicated aperture's builder verbatim in its own `init_tracing`
  (`crates/query-http-common/src/lib.rs:317`, OnceLock + `try_init`, JSON to
  stderr, `RUST_LOG` filter). Replicating in the gateway keeps the
  anti-coupling invariant clean (the gateway already depends on aperture for
  `spawn`/`Handle`, but it does not reach into aperture's private
  observability module), forces no aperture API-surface change, and leaves
  aperture standalone untouched. Option (b) is rejected: exposing a public
  aperture seam couples the gateway to aperture's internal observability
  module shape and widens aperture's public API for no benefit over inline
  replication. The builder body is roughly twelve lines; the coupling cost
  of a new public seam outweighs the duplication cost. (see: user-stories.md
  US-03; flag 2)

- **[DD2] The gap is an ordering gap, fixed by an EARLY install.** The
  subscriber IS installed today, but only inside `aperture::spawn`
  (`crates/aperture/src/compose.rs:111`, first statement of
  `compose::spawn`). `gateway_starting` (`main.rs:89`) and
  `health.startup.refused` (`main.rs:102`, the `probe_or_refuse` fail arm)
  both fire before `aperture::spawn` at `main.rs:116`, so both are dropped.
  `listener_bound` already renders because aperture emits it from its
  transport layer AFTER its own install
  (`crates/aperture/src/transport.rs:62` grpc, `:127` http). The fix
  installs an idempotent subscriber as the FIRST statement of the gateway's
  `main`, strictly before line 102. (see: DISCUSS wave-decisions.md D2)

- **[DD3] Install point: first statement of `main`, before
  `create_dir_all` (main.rs:65).** Placing the install ahead of every
  fallible step shrinks the pre-subscriber window to empty: there is no
  fallible `?`-propagating step left between process entry and the first
  `tracing::` call. This collapses US-04 (pre-subscriber `eprintln!`
  fallback) into US-01/US-02: with an empty pre-subscriber window, the
  `create_dir_all` and `FileBacked*Store::open` failures now run AFTER the
  subscriber is live and can be surfaced as `tracing::error!` lines rather
  than bare `Err` Debug prints or `eprintln!`. The gateway keeps the
  `Box<dyn Error>` return on `main` for the process exit code; the
  difference is that the failure now also renders structured on stderr.
  (see: user-stories.md US-04 example 3; flag 3)

- **[DD4] No ADR.** This aligns the gateway to the aperture posture already
  fixed by ADR-0009 and in force across the other four binaries. It
  introduces no new architectural decision: same builder family, same JSON
  envelope, same idempotent-install discipline. (see: DISCUSS
  wave-decisions.md D4; flag 4) Confirmed: NO new ADR.

- **[DD5] Env-filter variable: `RUST_LOG`.** Mirrors the read tier's
  `query-http-common::init_tracing`, which keys off `RUST_LOG` (the
  operator-conventional name) rather than aperture's `APERTURE_LOG`. This
  gives the whole observable Kaleidoscope surface (three read binaries plus
  the gateway) a single floor-control variable. aperture standalone keeps
  `APERTURE_LOG`; the gateway never invokes aperture's `main`, only its
  `spawn`, so the two filter vars do not collide. (see: flag 2, Luna's
  recommendation)

## Double-install resolution

The technically load-bearing point. Two installs occur on the gateway path:
the new EARLY install in the gateway's `main`, and aperture's own
`install_subscriber()` at `compose::spawn:111` reached via
`aperture::spawn`. `tracing` permits exactly one global default subscriber;
a second hard install (`set_global_default`/`init`) panics.

Read from source, the gateway path is safe by construction:

1. **The gateway's early install uses `try_init` under an `OnceLock` guard**
   (the read-tier `init_tracing` shape, verbatim). `try_init` returns
   `Result` and never panics; the gateway discards the result with `let _ =`.
2. **aperture's `install_subscriber` (`observability.rs:145-163`) is ALSO
   `OnceLock`-guarded AND uses `try_init`** — double-protected. When
   `aperture::spawn` runs it at `compose:111` AFTER the gateway has already
   installed the global default, `try_init` observes a default already set
   and returns `Err`, which aperture already discards with `let _ =`. No
   panic; the second install is a silent no-op.

So the resolution is: **the gateway's early `try_init` is the EFFECTIVE
install on the gateway path; aperture's in-spawn `try_init` becomes a no-op.**
Both layers use `try_init`, so order does not matter and neither panics. The
two crates hold SEPARATE `OnceLock` statics (one in the gateway, one in
aperture); that is harmless because `try_init` itself is the real guard
against the global-default conflict — the `OnceLock` only suppresses a
redundant builder construction within a single crate.

Critically, the gateway's subscriber is built with the SAME layer set as
aperture's (JSON to stderr, flattened, `event` field, no target/span noise),
so even though aperture's `listener_bound` now renders through the gateway's
subscriber rather than aperture's, the rendered line shape is identical. The
verifier's WIRE contract is satisfied regardless of which crate's `try_init`
won the race.

**Aperture standalone is preserved with no change required.** When `aperture`
runs as its own binary, the gateway code never executes; aperture's
`compose:111` `try_init` is the first and only install and succeeds normally.
The gateway path does not modify aperture, so aperture's standalone behaviour
is byte-for-byte unchanged.

## Reuse Analysis

| Existing Component | File | Overlap | Decision | Justification |
|---|---|---|---|---|
| `query-http-common::init_tracing` | crates/query-http-common/src/lib.rs:317 | Idempotent JSON-to-stderr subscriber install (RUST_LOG, OnceLock, try_init) | REUSE PATTERN, do NOT depend | Read-tier crate; importing it would breach the load-bearing anti-coupling invariant (write side must not import the read tier). The gateway replicates the same twelve-line builder inline instead. |
| `aperture::observability::install_subscriber` | crates/aperture/src/observability.rs:145 | Identical builder + idempotent install | REUSE PATTERN, cannot import | `pub(crate)`, not reachable from the gateway crate. Exposing it as `pub` rejected in DD1 (couples gateway to aperture internals for no net saving over inline replication). |
| gateway `main` | crates/kaleidoscope-gateway/src/main.rs | Process lifecycle entry; emits the two dropped events | EXTEND | Add one early `init_tracing()` call as the first statement plus a tiny inline `init_tracing` fn. ~15 LOC, no new module needed. |
| gateway `composition.rs` | crates/kaleidoscope-gateway/src/composition.rs | Earned-Trust probe seam, candidate install home | NOT TOUCHED | Holds pure probe logic today. Lifecycle/subscriber install belongs in `main`, not in the probe seam; mixing them would muddy a clean, unit-tested boundary. |
| gateway `Cargo.toml` | crates/kaleidoscope-gateway/Cargo.toml | Dependency surface | EXTEND | Add `tracing-subscriber` (it has `tracing` but not the subscriber). Per-crate dep, not a workspace promotion. |
| `query-http-common` dependency edge | n/a | n/a | CREATE NONE (forbidden) | Anti-coupling invariant. `cargo tree -p kaleidoscope-gateway \| grep query-http-common` must stay empty. |
| new crate | n/a | n/a | CREATE NONE | DISCUSS D4: changes confined to the gateway crate. |

No unjustified CREATE NEW. The only new code is an inline fn plus its call
site in an existing binary.

## Architecture Summary

- Pattern: host composition binary (unchanged); ports-and-adapters posture
  inherited from aperture. No structural change — one observability seam
  moved earlier in the composition root.
- Paradigm: Rust idiomatic (data + free functions), per project CLAUDE.md.
  The `init_tracing` fn is a free function with no state beyond a private
  `OnceLock`.
- Key change: the subscriber install moves from "inside aperture::spawn" to
  "first statement of the gateway's own main", so all three lifecycle events
  render on one JSON-to-stderr stream.
- Components touched: `kaleidoscope-gateway` only. aperture, the three
  pillars, and the storage sink are unchanged.

## Technology Stack

- `tracing-subscriber` 0.3, `default-features = false`, features
  `["fmt", "json", "env-filter", "registry"]` — matched verbatim to
  aperture's existing line (`crates/aperture/Cargo.toml:60`) and the read
  tier. Already resolved in the workspace lockfile via aperture and
  `query-http-common`; adding it to the gateway adds no new transitive
  graph, only a new direct edge. License: MIT/Apache-2.0 (tokio-rs).
- `tracing` 0.1 — already present (`Cargo.toml:43`).
- No proprietary dependency. No workspace dependency promotion (per-crate,
  mirrors read-tier DD1).

## DEVOPS Handoff

- **No new crate, no new workspace member, no new CI job.** The Gate 5
  mutation job already exists: `gate-5-mutants-kaleidoscope-gateway`
  (`.github/workflows/ci.yml:2318`, name "Gate 5 — cargo mutants
  (kaleidoscope-gateway)", runs `cargo mutants (kaleidoscope-gateway,
  in-diff)` at line 2353, uploads `mutants-out-kaleidoscope-gateway`). It
  was added in the `gate-5-mutants-batch-v0` batch and covers this crate's
  in-diff mutants at the 100% kill gate (ADR-0005 Gate 5). The new
  `init_tracing` fn's killable surface (the `OnceLock` idempotence guard)
  is mutation-tested by the crafter the way the read tier pinned it with
  `test_init_tracing_is_idempotent_and_never_panics`
  (`crates/query-http-common/src/lib.rs:648`).
- **Slim wave.** No infra, no contract tests (no external integration — the
  only boundary is the gateway-to-aperture in-process `spawn` call, already
  in the dependency graph), no deployment change. DEVOPS scope reduces to:
  confirm the existing gate-5 job picks up the changed files and the
  `tracing-subscriber` edge resolves against the locked workspace version.

## Constraints Established

- Anti-coupling: zero dependency edge from `kaleidoscope-gateway` to
  `query-http-common`.
- aperture standalone behaviour unchanged (no aperture source modification).
- Install is the first statement of `main`; pre-subscriber window is empty.
- Idempotent install via `try_init` under `OnceLock`; never panics on the
  double-install path.
- Rendered line shape identical to aperture and the read tier (verifier
  WIRE contract).

## Upstream Changes

- **None to aperture.** The double-install resolution (DD-double-install)
  works WITHOUT modifying aperture, because aperture's `compose:111` install
  already uses `try_init`. No coordinated change is required; aperture's
  `install_subscriber` was already idempotent and panic-free by construction
  before this feature. Recorded explicitly so the crafter does not "fix"
  aperture unnecessarily.
- **US-04 collapses into US-01/US-02** (DD3): the empty pre-subscriber
  window means the `eprintln!` fallback story is no longer needed; the
  fallible early steps now render structured `tracing::error!` lines instead.
  This is a simplification, not a contradiction of the DISCUSS contract —
  US-04 example 3 anticipated and authorised this outcome.
