# DESIGN Decisions — beacon-v0

## Key Decisions

- **[D1] Two-crate workspace.** `crates/beacon` (library) +
  `crates/beacon-server` (binary). The library is testable + reusable;
  the binary owns the runtime. (See: ADR-0033.)
- **[D2] CUE schema with file + line + field diagnostics.** Loader
  produces operator-readable errors with edit-distance suggestions
  (`nearest_blessed_match` from Codex). (See: ADR-0034.)
- **[D3] Sink trait with five implementations.** `Webhook`, `SMTP`,
  `Mattermost`, `Zulip`, `OnCall`. Header redaction invariant
  shared with Prism ADR-0027. Secrets via environment variable
  names declared in CUE. (See: ADR-0035.)
- **[D4] MWMBR synthesis from Google SRE workbook table.** Four-row
  table (1h/5m × 14.4, 6h/30m × 6, 1d/2h × 3, 3d/6h × 1) inlined as
  Rust constants with workbook citation. 30-day budget only at v0.
  (See: ADR-0036.)
- **[D5] Pure evaluator + Scheduler seam.** Same shape as Prism's
  reducer + Scheduler. Testable as pure function; binary owns the
  runtime. (See: ADR-0037.)
- **[D6] Tokio + reqwest substrate.** Same choices as Aperture and
  Prism. No new substrate dependencies.
- **[D7] Slice 02 SPIKE on CUE parsing.** The Rust CUE ecosystem is
  sparse; the slice-02 pre-slice SPIKE chooses between an existing
  binding and a hand-written subset parser. (See: ADR-0034
  Knowledge Gap.)
- **[D8] Per-sink retry with exponential backoff.** 1s/5s/30s,
  three attempts on transient failure, permanent failure recorded
  immediately. The Sink's `SinkError` variant is the
  classification.
- **[D9] No public `tokio` types in library API.** The library is
  `async fn` over arbitrary executors; the binary owns the runtime
  choice. (See: ADR-0033.)
- **[D10] `MAX_CONCURRENT_FETCHES` default 16.** Operator-tunable.
  Caps outbound HTTP load for the binary.

## Infrastructure summary

- **Workspace**: two new crates under `crates/`.
- **Test posture**: real Prometheus container fixture for slice 01
  (same digest-pin pattern as Prism's E2E).
- **CI gates**: existing Rust gates apply; mutation testing scope
  extends to `crates/beacon`.

## Constraints established

- Library does not depend on Tokio runtime types in its public API
- CUE parser library choice is deferred to slice-02 SPIKE
- 30-day SLO budget only at v0
- Single backend at v0 (no multi-Prom routing)

## Upstream changes

None. DISCOVER (the architecture doc §C.12) named Beacon's role;
DISCUSS decided the scope; DESIGN crystallises the structure
without revising the DISCUSS assumptions.
