# DESIGN Decisions — read-api-tracing-subscriber-v0

## Context

Operability hardening of three existing read binaries (`query-api`,
`log-query-api`, `trace-query-api`). They already emit `tracing`
lifecycle events but install no subscriber, so every event is discarded
and the operator's container stderr is empty. Origin: EDD black-box
verifier issue 005 (medium, operability). The reference posture is
aperture, which installs a JSON-to-stderr, env-filtered subscriber
(ADR-0009). This DESIGN aligns the read tier to that posture. Mode:
propose. Scope: application. No new ADR (DD4).

## Key Decisions

### DD1 — Add `tracing-subscriber` to all three read crates; declare per-crate, matching aperture's line byte-for-byte

None of `query-api`, `log-query-api`, `trace-query-api` declare
`tracing-subscriber` (each has only `tracing = "0.1"`). The workspace
root `Cargo.toml` does NOT carry `tracing-subscriber` in
`[workspace.dependencies]`; aperture declares its own local pin. The
only tracing-adjacent workspace deps are `serde` and `serde_json`.

Decision: **replicate aperture's exact local line in each of the three
read crates** (per-crate, not promoted to a workspace dependency):

```toml
tracing-subscriber = { version = "0.3", default-features = false, features = ["fmt", "json", "env-filter", "registry"] }
```

Rationale: promoting to a workspace dep would require also migrating
aperture's existing local declaration to the workspace level to avoid a
split source of truth, which is scope creep into a crate this feature
does not otherwise touch. The repo's established idiom is per-crate
declaration with permissive `version = "0.3"`; aperture already proves
it. Adding the same dependency edge to three more crates produces zero
lockfile churn beyond the new edges, because `Cargo.lock` already
resolves a `tracing-subscriber` 0.3.x via aperture. Promotion to a
workspace dep is a reasonable future tidy but is out of scope here.

Because the shared helper lives in `query-http-common` (DD2), the
`tracing-subscriber` dependency is added to **`query-http-common`**, and
`tracing` is added there too. The three read crates already depend on
`query-http-common`, so they consume the subscriber transitively through
the helper and do NOT each need their own `tracing-subscriber` edge.
This is the cleaner placement: one crate owns the substrate, one
function owns the configuration. See DD2.

### DD2 — One shared `init_tracing()` helper in `query-http-common`, not three inline copies

aperture installs its subscriber inside its library `compose::spawn`,
reading the exact builder from `observability::install_subscriber`. The
three read binaries have **no equivalent lifecycle compose seam**: their
`composition` modules hold only pure resolvers (`resolve_addr`, `probe`,
`resolve_tenant`, ...), and all lifecycle work runs inline in `main`.
So there is no existing library function to hang the install on.

Two candidate homes for the install expression:

1. inline in each `main` (three copies of the same builder), or
2. a single `query_http_common::init_tracing()` free function called as
   the first line of each `main`.

Decision: **option 2 — a shared `init_tracing()` in `query-http-common`.**
That crate is already the single source of truth for read-tier HTTP
scaffolding (ADR-0054: caps, time-range parser, error envelope,
fail-closed tenant seam) and is depended on by all three binaries. A
single helper gives genuine byte-for-byte uniformity (US-05), one place
for the verifier's stderr-format contract, and one place to evolve the
filter. It mildly widens query-http-common's charter (it is otherwise
pure data + free functions with no side effects), so the helper is
documented as the one deliberate effectful seam, isolated in its own
module and idempotent (guarded by a `OnceLock`, exactly as aperture's
`install_subscriber` is) so repeated or test-time calls are safe.

No new crate (constraint C2). No `dyn` indirection. Free function +
`OnceLock`, matching the repository's Rust-idiomatic paradigm
(CLAUDE.md: data + free functions, no class hierarchies).

### DD3 — Subscriber configuration matches aperture's builder exactly, with one deliberate divergence: filter env var is `RUST_LOG`, not `APERTURE_LOG`

aperture's `observability::install_subscriber` builds:

```rust
let filter = EnvFilter::try_from_env("APERTURE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
tracing_subscriber::registry()
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
    .try_init();
```

The read-tier helper replicates this builder verbatim (JSON layer to
stderr, flattened events, env filter, `info` default) with ONE
deliberate change: the filter env var is **`RUST_LOG`**, not
`APERTURE_LOG`.

Rationale: the DISCUSS grounding repeatedly describes aperture's
subscriber as "RUST_LOG-aware", but aperture's actual code keys off
`APERTURE_LOG`. The user stories pin the operator contract to `RUST_LOG`
(US-01/02/03/04 acceptance criteria assert `RUST_LOG=warn` /
`RUST_LOG=error` behaviour by name). The read tier is a distinct tier
from the gateway; choosing the conventional, ecosystem-standard
`RUST_LOG` is the right operator contract and is what the verifier will
assert. The builder shape (JSON, stderr, flatten, defaults) is otherwise
identical, so the rendered event format matches aperture byte-for-byte
and the verifier's one JSON parser covers all four binaries (US-05). The
`CaptureLayer` test seam from aperture is NOT replicated: the read tier
is verified black-box by subprocess + stderr grep (DD5), so no in-crate
capture layer is needed.

This divergence is recorded here (no ADR) and flagged to DISTILL as the
single behavioural difference from aperture.

### DD4 — No new ADR

This change aligns the read tier to aperture's existing,
ADR-0009-blessed posture (JSON-to-stderr, env-filtered subscriber). It
introduces no new architectural decision; it closes an operability gap
by applying an already-decided pattern to three more entry points. The
one divergence (RUST_LOG vs APERTURE_LOG, DD3) is an operator-contract
choice, not an architectural one, and is documented here. Reference
ADR-0009 in the brief; do not author a new ADR. ADR immutability is
preserved.

### DD5 — Acceptance strategy: black-box subprocess + stderr grep is the primary; a compose-function test is unavailable here

The events must reach process stderr so an external harness can capture
and grep them (constraint C4). Two acceptance approaches were weighed:

1. **subprocess test** — spawn the binary, drive it to a clean start and
   to each fail-closed condition, capture stderr, parse each line as a
   `serde_json::Value`, assert the `event` field
   (`*_starting`, `listener_bound`, `health.startup.refused`); assert
   non-zero exit on the fail-closed runs.
2. **compose-function test** — call a library function and assert the
   subscriber is installed.

Decision: **the subprocess + stderr grep approach is primary and is the
less fragile choice here.** Option 2 is not genuinely available: the
read binaries have no lifecycle compose function (DD2), and a global
subscriber is process-global and `try_init`-guarded, so an in-process
test cannot meaningfully assert "installed and writing to the real
stderr fd" without spawning a process anyway. The subprocess test is
also exactly what the EDD verifier does (it already captures empty
stderr at LQ02/LQ03/TQ01), so the acceptance test and the verifier share
one shape. The fail-closed path is the highest-value assertion:
`health.startup.refused` must appear on stderr BEFORE the non-zero exit.
DISTILL/Crafty own the test mechanics; this is the pinned strategy.

## Reuse Analysis

| Existing Component | File | Overlap | Decision | Justification |
|---|---|---|---|---|
| aperture subscriber builder | crates/aperture/src/observability.rs (`install_subscriber`) | The exact JSON-to-stderr env-filtered builder | REUSE (replicate verbatim, one env-var divergence) | The reference posture (ADR-0009); copied into the shared helper, not imported (aperture is a separate platform binary, not a library the read tier should depend on) |
| query-http-common | crates/query-http-common/src/lib.rs | Single source of truth for read-tier shared scaffolding | EXTEND (add `init_tracing()` + `tracing`/`tracing-subscriber` deps) | Already the read-tier shared crate (ADR-0054), already depended on by all three binaries; the natural home for a one-pattern subscriber, ~25 LOC vs a new crate |
| query-api `main` | crates/query-api/src/main.rs | Entry point; first `tracing::` call | EXTEND (call `init_tracing()` as first line) | One-line wiring at the existing entry point |
| log-query-api `main` | crates/log-query-api/src/main.rs | Entry point; first `tracing::` call | EXTEND (call `init_tracing()` as first line) | One-line wiring at the existing entry point |
| trace-query-api `main` | crates/trace-query-api/src/main.rs | Entry point; first `tracing::` call | EXTEND (call `init_tracing()` as first line) | One-line wiring at the existing entry point |
| query-http-common Cargo.toml | crates/query-http-common/Cargo.toml | Dependency surface | EXTEND (add `tracing`, `tracing-subscriber`) | Helper lives here, so the substrate lives here |
| New crate for tracing init | — | — | CREATE NEW: REJECTED | C2 forbids a new crate; query-http-common is the correct existing home |

No CREATE NEW components.

## Architecture Summary

- Pattern: existing modular workspace; ports-and-adapters preserved
  unchanged. This feature touches only the composition-root entry points
  and one shared library helper; no HTTP contract, no port, no adapter
  changes (constraint C1).
- Paradigm: Rust idiomatic — data + free functions; one effectful free
  function (`init_tracing`) guarded by `OnceLock`. No `dyn`, no
  inheritance. Matches CLAUDE.md.
- Install point: **first line of each `main`**, before any `tracing::`
  call and before `create_dir_all`/store-open/`resolve_addr`. (The read
  tier has no compose seam to host it, unlike aperture; DD2.)
- Pre-init window: failures before `init_tracing()` returns cannot occur
  (it is the first statement and is infallible); failures in the
  earliest fallible steps after it — `create_dir_all`, `*Store::open`,
  `resolve_addr` — are reported via `eprintln!` per US-06 / aperture's
  convention (C5), since they propagate as `?` today and would otherwise
  print bare.
- Key components touched: 3 `main.rs` (one-line call each) +
  `query-http-common` (`init_tracing` helper + 2 deps).

## Technology Stack

- `tracing-subscriber = { version = "0.3", default-features = false,
  features = ["fmt", "json", "env-filter", "registry"] }` — MIT/Apache-2.0,
  the canonical Rust structured-logging subscriber; already resolved in
  `Cargo.lock` via aperture, so zero new transitive resolution. Declared
  on `query-http-common`.
- `tracing = "0.1"` — added to `query-http-common` (it already exists on
  the three binaries). MIT/Apache-2.0.
- `serde_json` — already a workspace dep; the verifier parses each stderr
  line as a `serde_json::Value` (DD5), no production-side addition
  needed.
- Filter env var: `RUST_LOG` (DD3), default `info`.

## Constraints Established

- C1: No HTTP contract change. stderr-only.
- C2: No new crate. Shared helper lands in existing `query-http-common`.
- C3: Subscriber configuration matches aperture's builder exactly, save
  the `RUST_LOG` env-var divergence (DD3). No bare `fmt().init()`.
- C4: Events reach process stderr as greppable JSON lines.
- C5: Pre-init failures (`create_dir_all`, store open, addr parse) use
  `eprintln!` before the non-zero exit.
- C6: `#[mutants::skip]` stays on each `main`; gate-5 100% kill rate
  preserved. `init_tracing` is `OnceLock`-guarded effectful wiring; its
  body is exercised by the black-box acceptance run, and the
  `#[mutants::skip]` posture extends to it as unkillable global-install
  wiring (DISTILL/Crafty confirm).
- C7: No version bumped to 1.0.0.

## DEVOPS Handoff

- No new crate, no new workspace member, no new binary, no new CI job.
- No new external integration (no contract tests needed).
- The three read crates' existing gate-5 mutant runs already scope the
  modified files; `query-http-common`'s gate-5 run scopes the new helper.
  The `#[mutants::skip]` posture covers the global-install wiring (C6).
- DEVOPS wave is **slim / doc-only** for this feature: confirm the new
  dependency edges pass `cargo deny` Gate 4 (no wildcards — `version =
  "0.3"` and `version = "0.1"` are not wildcards), and confirm the
  black-box verifier suite (LQ01/LQ02/LQ03, Q01, TQ01) is re-pointed to
  assert the structured `health.startup.refused` event instead of bare
  `Err`. No pipeline change.

## Upstream Changes

None. No DISCOVER/DIVERGE artifacts exist for this feature; it originates
from an EDD-verifier issue. No prior-wave assumptions changed. The one
grounding correction (aperture keys off `APERTURE_LOG`, not `RUST_LOG`;
DD3) is a clarification of the reference posture, not a changed DISCUSS
assumption — the user stories already pin `RUST_LOG`.
