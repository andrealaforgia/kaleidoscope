# DISCUSS Decisions — read-api-tracing-subscriber-v0

## Origin

Verified defect from an EDD-verifier black-box pass (issue 005, severity
medium, dimension operability). The three Kaleidoscope read binaries
(query-api, log-query-api, trace-query-api) emit `tracing` lifecycle
events but install NO tracing subscriber. Every event is silently
discarded; container stderr is empty; the only operator-visible signal on
a fail-closed startup is the bare `Err()` the Rust runtime prints from
`main`. The verifier confirmed the gap across all three (empty stderr
captured on LQ02/LQ03/TQ01). The aperture posture is correct and is the
reference to align to.

## Interactive Decisions

- [D1] Feature type: **Backend** (operability). Modifies three `main.rs`
  files plus their `Cargo.toml`. No new crate, no HTTP contract change.
- [D2] Walking skeleton: **No**. The three binaries already exist; this
  is an operability hardening of existing entry points.
- [D3] Research depth: **Lightweight**. Aligning to an existing,
  verified-correct sibling posture (aperture); the happy path is clear.
- [D4] JTBD analysis: **No**. The job (operator needs startup visibility
  on stderr) is concrete and already evidenced by the EDD-verifier issue.

## Grounding Results (files read)

### aperture posture (reference to imitate)

`crates/aperture/src/main.rs`:

- aperture's `main` does NOT call `tracing_subscriber::...::init()`
  directly. Pre-init failures (argv parse, config load) are reported with
  `eprintln!("aperture: ...")` and return `ExitCode::from(2)`. See the
  inline comment at the config-error arm: "Pre-init failure path: tracing
  subscriber not yet installed (config feeds into compose, which inits the
  logger). Use stderr directly for this narrow window".
- The subscriber is installed downstream inside `aperture::run` ->
  `compose` (the library), not in `main`. Post-init, tracing is the only
  stderr-writing path.
- `main` returns `std::process::ExitCode`; the final error arm logs
  `tracing::error!(error = %e, "aperture exited with error")` and returns
  `ExitCode::FAILURE`.

`crates/aperture/Cargo.toml`:

- `tracing-subscriber = { version = "0.3", default-features = false,
  features = ["fmt", "json", "env-filter", "registry"] }`
- This is a JSON-layer-to-stderr subscriber with an EnvFilter
  (RUST_LOG-aware) per ADR-0009. It is NOT a bare `fmt().init()`.
- `tracing = "0.1"`, `serde_json = "1"` also present (the latter feeds the
  `aperture::testing::stderr_capture` seam used by integration tests).

### read binaries (the gap)

`crates/query-api/src/main.rs` — emits, in order:

- `tracing::info!(event = "query_api_starting", pillar_root, tenant_resolved)`
- `tracing::error!(event = "health.startup.refused", reason)` (fail-closed
  arm, then `return Err(reason.into())`)
- `tracing::info!(event = "listener_bound", transport = "http", addr)`
- Final error arm: bare `Err` from `axum::serve` / binding propagated out
  of `main` (returns `Result<(), Box<dyn Error>>`).

`crates/log-query-api/src/main.rs` — emits:
`log_query_api_starting`, `health.startup.refused`, `listener_bound`.
Same shape and order.

`crates/trace-query-api/src/main.rs` — emits:
`trace_query_api_starting`, `health.startup.refused`, `listener_bound`.
Same shape and order. Gap confirmed by the verifier (TQ01).

All three:

- Return `Result<(), Box<dyn std::error::Error>>` (NOT `ExitCode` like
  aperture). A `health.startup.refused` error is logged via `tracing` then
  returned as `Err`, so today both the structured event AND the bare `Err`
  are lost/printed-bare respectively.
- Carry `#[mutants::skip]` on `main` (entry point is unkillable wiring).
- Install NO subscriber anywhere (neither in `main` nor in their library
  `composition` seam).

### dependency state

- `crates/query-api/Cargo.toml`: has `tracing = "0.1"`, NO
  `tracing-subscriber`.
- `crates/log-query-api/Cargo.toml`: has `tracing = "0.1"`, NO
  `tracing-subscriber`.
- `crates/trace-query-api/Cargo.toml`: has `tracing = "0.1"`, NO
  `tracing-subscriber`.
- **`tracing-subscriber` must be ADDED to all three read crates.**
- Workspace root `Cargo.toml`: `tracing-subscriber` is NOT a
  `[workspace.dependencies]` entry. The only tracing-adjacent workspace
  deps are `serde`, `serde_json`. aperture declares its own
  `tracing-subscriber` pin locally. The three read crates should follow
  aperture's local declaration (DESIGN to confirm whether to promote to a
  workspace dep or replicate aperture's local pin; the simplest
  uniformity is to replicate aperture's exact local line).

## Flags to DESIGN

1. **tracing-subscriber dependency is MISSING in all three read crates.**
   None of query-api, log-query-api, trace-query-api declare
   `tracing-subscriber`; they only have `tracing = "0.1"`. DESIGN must add
   it. Recommended: replicate aperture's exact line
   `tracing-subscriber = { version = "0.3", default-features = false,
   features = ["fmt", "json", "env-filter", "registry"] }` so the read
   tier matches aperture byte-for-byte. The workspace Cargo.lock already
   resolves a tracing-subscriber 0.3.x via aperture, so adding the same
   edge to three more crates should produce zero lockfile churn beyond new
   dependency edges.

2. **Subscriber configuration: match aperture EXACTLY, not a bare
   `fmt().init()`.** aperture uses a JSON layer to stderr with EnvFilter
   (RUST_LOG-aware), per ADR-0009 and its feature set
   `["fmt", "json", "env-filter", "registry"]`. A bare `fmt().init()`
   would diverge (human-format text, no JSON, no RUST_LOG filter) and
   break the "one pattern in the read tier" goal. Morgan pins the exact
   builder expression. NOTE: aperture installs the subscriber inside its
   library `compose`, not in `main`. The read binaries have no equivalent
   compose seam that installs a logger, so DESIGN must decide WHERE the
   install lives for the read tier (first line of `main`, or a shared
   helper in `query-http-common` to keep a single source of truth for the
   three). A shared `query-http-common` init helper is the cleanest path
   to genuine uniformity and is worth Morgan's consideration.

3. **Install point and ordering.** The subscriber must be installed as the
   FIRST action in `main`, before any `tracing::info!`/`tracing::error!`
   call (the current `*_starting` info event is the first emitter). Any
   failure occurring BEFORE the subscriber is installed (there is little
   such surface today, but `create_dir_all`, `*::open`, and `resolve_addr`
   all run before the first event in the read binaries) must be reported
   via `eprintln!` exactly as aperture does for its pre-init window. DESIGN
   to decide whether to also convert the read binaries' final `Err`
   propagation into an explicit `tracing::error!` + non-zero exit (aperture
   does this; the read binaries currently rely on the runtime's bare
   `Err` print). Recommended: align to aperture, so the bare `Err` is
   replaced by a structured event on stderr plus a non-zero exit.

4. **ADR: NOT required (recommended).** This change aligns the read tier
   to aperture's existing, ADR-0009-blessed posture; it is not a new
   architectural decision. Luna recommends no new ADR; reference ADR-0009
   in the DESIGN brief instead. Morgan confirms.

5. **Black-box verifiability.** The events must reach process stderr so an
   external harness can capture and grep them. The EDD-verifier will
   tighten LQ01/Q01/TQ01 and the fails-closed assertions to assert the
   structured `health.startup.refused` event instead of the bare `Err`.
   Acceptance criteria must therefore be observable from OUTSIDE the
   process: spawn the binary, capture stderr, grep for the JSON event.
   DESIGN/DISTILL should confirm the JSON layer renders `event` as a
   greppable field (aperture's `stderr_capture` test seam parses each line
   as a `serde_json::Value`, which is the proven approach).

## Requirements Summary

- Primary user need: an operator running any of the three read binaries
  must see startup lifecycle on stderr (service starting, listener bound
  with address) and, on a fail-closed refusal, a structured
  `health.startup.refused` event naming the reason BEFORE the non-zero
  exit, instead of an empty stderr followed by a bare `Err`.
- Scope: install a tracing subscriber in all three read binaries, matching
  aperture's posture, in one slice for read-tier uniformity. Add the
  missing `tracing-subscriber` dependency to the three crates.
- Walking skeleton scope: not applicable (binaries exist).
- Feature type: backend / operability.

## Constraints Established

- C1: No HTTP contract change. The read APIs' request/response bodies and
  status codes are untouched; this is stderr-only.
- C2: No new crate. Modify three `main.rs` + three `Cargo.toml` only (plus
  an optional shared helper in `query-http-common` if DESIGN chooses the
  shared-init path).
- C3: Match aperture's subscriber configuration exactly (JSON layer to
  stderr, EnvFilter / RUST_LOG-aware) for single-pattern read-tier
  uniformity. No bare `fmt().init()`.
- C4: Events must reach process stderr as greppable structured lines so a
  black-box harness can assert them.
- C5: Pre-init failures (before the subscriber is installed) use
  `eprintln!`, as aperture does.
- C6: `#[mutants::skip]` stays on each `main`; the entry point remains
  unkillable wiring and the gate-5 100% kill rate is preserved.
- C7: No version bumped to 1.0.0.

## Upstream Changes

None. No DISCOVER or DIVERGE artifacts exist for this feature; it
originates from an EDD-verifier issue. No prior-wave assumptions changed.
