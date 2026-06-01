# Application Architecture — read-api-tracing-subscriber-v0

Operability hardening: install a tracing subscriber in the three read
binaries so their existing lifecycle events become visible on operator
stderr. Aligns the read tier to aperture's posture (ADR-0009). No HTTP
contract change, no new crate, no new ADR.

## The aperture posture

aperture installs a JSON-to-stderr, env-filtered subscriber inside its
library `compose::spawn` (NOT in `main`), reading the builder from
`crates/aperture/src/observability.rs > install_subscriber`. The builder
(verbatim from that file) is:

```rust
let filter =
    EnvFilter::try_from_env("APERTURE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
let _ = tracing_subscriber::registry()
    .with(filter)
    .with(
        tracing_subscriber::fmt::layer()
            .json()
            .with_writer(std::io::stderr)
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false)
            .with_target(false),
    )
    // (aperture also adds a CaptureLayer here for its in-process tests;
    //  the read tier does NOT — it is verified black-box, DD5)
    .try_init();
```

Install is idempotent (`OnceLock`-guarded). aperture's `main` reports the
narrow pre-subscriber window (argv/config errors) with
`eprintln!("aperture: ...")` and a non-zero `ExitCode`; post-init,
tracing is the only stderr-writing path.

The read-tier helper replicates this builder with ONE deliberate change:
the filter env var is **`RUST_LOG`** (the conventional, operator-facing
name the user stories pin), not `APERTURE_LOG`. Everything else — JSON,
stderr, flattened events, `info` default, no target/span noise — is
identical, so the rendered line shape matches aperture and one JSON
parser covers all four binaries.

## Init point per binary

The read binaries differ structurally from aperture: they have **no
lifecycle compose seam**. Each `composition` module holds only pure
resolvers; all lifecycle work (dir create, store open, tenant resolve,
probe, bind, serve) runs inline in `main`, and the first `tracing::` call
is the `*_starting` info event in `main`. There is therefore no library
function to host the install on; the install point is the **first
statement of `main`**, via the shared helper `query_http_common::init_tracing()`.

| Binary | File | Current first `tracing::` call | New first statement of `main` |
|---|---|---|---|
| query-api | crates/query-api/src/main.rs:75 (`query_api_starting`) | line 75 | `query_http_common::init_tracing();` before line 57 (`resolve_pillar_root`) |
| log-query-api | crates/log-query-api/src/main.rs:69 (`log_query_api_starting`) | line 69 | `query_http_common::init_tracing();` before line 51 (`resolve_pillar_root`) |
| trace-query-api | crates/trace-query-api/src/main.rs:72 (`trace_query_api_starting`) | line 72 | `query_http_common::init_tracing();` before line 53 (`resolve_pillar_root`) |

`init_tracing()` is the first statement, before `create_dir_all`,
`*Store::open`, and `resolve_addr` (all of which run before the first
event and propagate via `?` today). This guarantees every event from
`*_starting` onward is captured. The narrow pre-subscriber window is
empty (the helper is infallible and runs first); the earliest fallible
steps are handled by the pre-init `eprintln!` rule below.

## Events per binary

These are the structured `event` field values the verifier will grep on
stderr. Names are unchanged by this feature (the events already exist);
this feature only makes them render.

| Binary | Events emitted (in order) | Level | Key fields |
|---|---|---|---|
| query-api | `query_api_starting` | info | `pillar_root`, `tenant_resolved` |
| | `health.startup.refused` (fail-closed arm) | error | `reason` |
| | `listener_bound` | info | `transport="http"`, `addr` |
| log-query-api | `log_query_api_starting` | info | `pillar_root`, `tenant_resolved` |
| | `health.startup.refused` (fail-closed arm) | error | `reason` |
| | `listener_bound` | info | `transport="http"`, `addr` |
| trace-query-api | `trace_query_api_starting` | info | `pillar_root`, `tenant_resolved` |
| | `health.startup.refused` (fail-closed arm) | error | `reason` |
| | `listener_bound` | info | `transport="http"`, `addr` |

On a clean start: `*_starting` then `listener_bound`. On a fail-closed
start: `*_starting` then `health.startup.refused`, then non-zero exit
(the `probe` arm returns `Err(reason.into())`). Under `RUST_LOG=warn`,
the two info events are filtered; the error-level `health.startup.refused`
survives any filter at `error` or laxer.

## Changes per file

| File | Change | Approx LOC |
|---|---|---|
| crates/query-http-common/src/lib.rs | New `init_tracing()` free fn (`OnceLock`-guarded; the aperture builder with `RUST_LOG`) in a small `observe` module | ~25 |
| crates/query-http-common/Cargo.toml | Add `tracing = "0.1"` and the `tracing-subscriber` 0.3 line | +2 deps |
| crates/query-api/src/main.rs | Call `query_http_common::init_tracing();` as first statement of `main`; convert pre-init `?` failures to `eprintln!` + non-zero return (US-06) | ~4 |
| crates/log-query-api/src/main.rs | Same one-line call + pre-init `eprintln!` | ~4 |
| crates/trace-query-api/src/main.rs | Same one-line call + pre-init `eprintln!` | ~4 |

Workspace root `Cargo.toml`: **no change** (per-crate dep, DD1; the
subscriber lives on query-http-common, not promoted to a workspace dep).
~5 files total. No new crate.

## Error contract / stderr

- **Post-init (subscriber installed):** structured JSON lines on stderr,
  one per event, flattened, with an `event` field, matching aperture.
  `health.startup.refused` (error) renders with its `reason` field
  BEFORE the process returns `Err` / exits non-zero.
- **Pre-init window (before `init_tracing` could help):** the earliest
  fallible steps — `create_dir_all`, `*Store::open`, `resolve_addr` —
  emit a direct `eprintln!("{binary}: ...: {e}")` line (US-06, aperture
  convention) before the non-zero exit, instead of the runtime's bare
  `Err`. Since `init_tracing()` is the first and infallible statement,
  this window is only the fallible-`?` steps that precede the first event,
  not anything before the subscriber.
- **Exit status:** unchanged shape — fail-closed and pre-init failures
  exit non-zero; clean start runs until the listener stops.

## Verification strategy

Black-box subprocess + stderr grep is the pinned acceptance approach
(DD5), the less fragile choice and the same shape the EDD verifier uses:

1. Spawn the binary as a child process with controlled env
   (`KALEIDOSCOPE_*_TENANT`, `KALEIDOSCOPE_PILLAR_ROOT`, `RUST_LOG`).
2. Capture stderr; parse each line as a `serde_json::Value`.
3. Clean-start cases: assert an `event = *_starting` line and an
   `event = listener_bound` line with the bound `addr`.
4. Fail-closed cases (tenant unset, unprobeable store): assert an
   `event = health.startup.refused` line carrying `reason`, AND assert
   the child exits non-zero, AND assert the refusal line precedes exit.
5. Filter cases: with `RUST_LOG=warn`, assert the info events are absent;
   with `RUST_LOG=error`, assert `health.startup.refused` still present.
6. Pre-init cases (US-06): force a malformed `KALEIDOSCOPE_*_ADDR` /
   unopenable store; assert a plain `eprintln!` line on stderr and
   non-zero exit.

A compose-function unit test is not available: the read tier has no
lifecycle compose function, and the subscriber is process-global and
`try_init`-guarded, so only a spawned process can assert it writes to the
real stderr fd. DISTILL/Crafty own the test mechanics; the strategy and
the event-name contract above are pinned.
