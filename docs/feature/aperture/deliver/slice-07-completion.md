# Slice 07 — TLS / SPIFFE schema knob — completion summary

> **Wave**: DELIVER (`nw-software-crafter` / Crafty).
> **Date**: 2026-05-04.
> **Slice**: 07 — forward-compat TLS / SPIFFE schema knobs with a single
> startup warn line and the figment-driven TOML loader.
> **Companion brief**: [`../slices/slice-07-tls-schema-knob.md`](../slices/slice-07-tls-schema-knob.md).
> **Companion ADRs**:
> [`../../product/architecture/adr-0008-aperture-configuration-schema.md`](../../product/architecture/adr-0008-aperture-configuration-schema.md),
> [`../../product/architecture/adr-0009-aperture-observability-strategy.md`](../../product/architecture/adr-0009-aperture-observability-strategy.md).

---

## Headline

After this slice an operator can author a `[aperture.security.tls]` /
`[aperture.security.auth.spiffe]` configuration today and the same
file rolls forward into Phase 2 (Aegis) without a schema break. At v0
both knobs default to off; setting either `enabled = true` produces
exactly one `event=tls_not_supported_in_v0` warn line at startup and
Aperture continues binding plaintext listeners with no auth. A
misspelled key (`max_concurent_requests`) is rejected at config load
because every nested struct in the schema sets
`#[serde(deny_unknown_fields)]`.

The loader is the figment-driven shape ADR-0008 picks: a single
`Toml::string` (or `Toml::file`) provider feeds a typed `RawConfig`
that mirrors the on-disk TOML 1:1. The folded `Config` re-uses the
existing `ConfigBuilder` so cross-field validation (identical pinned
bind addresses, sink-kind dispatch) lives in exactly one place.

## What turned GREEN

| Test binary | Tests passing |
|---|---:|
| `tests/slice_07_tls_schema_knob.rs` | **7/7** |
| `tests/slice_06_forwarding_sink.rs` | **11/11** (no regressions) |
| `tests/slice_05_backpressure.rs` | **10/10** (no regressions) |
| `tests/slice_04_metrics.rs` | **9/9** (no regressions) |
| `tests/slice_03_traces.rs` | **10/10** (no regressions) |
| `tests/slice_02_http_protobuf_and_readiness.rs` | **15/15** (no regressions) |
| `tests/slice_01_walking_skeleton.rs` | **13/13** (no regressions) |
| `tests/probe_gold_runner.rs` | **5/5** (no regressions) |
| `tests/invariant_no_telemetry_on_telemetry.rs` | **5/5** (no regressions) |
| `tests/invariant_single_validator.rs` | **1/1** |
| `src/lib.rs` (lib unit tests) | **60/60** (no regressions) |
| **Slice 07 active total** | **146/146** |

Slice 08's `tests/slice_08_graceful_shutdown.rs` remains RED as
designed (4 failed, 1 passed, 1 ignored): the drain orchestrator
lands in the next slice.

The 7 acceptance tests in `slice_07_tls_schema_knob.rs` cover, per the
slice contract:

- **Schema parses with the security tables present at defaults (1)**:
  `config_with_all_security_keys_at_defaults_parses_without_error`.
- **`tls.enabled = true` warn line (3)**:
  `tls_enabled_true_emits_tls_not_supported_in_v0_warn_line` (level
  is `warn`, event name matches),
  `tls_enabled_true_emits_exactly_one_warn_line` (cardinality is one,
  not zero, not two — pins the
  `&&` short-circuit in `warn_if_v0_security_knob_set` against an
  `||` mutation that would emit on every startup), and
  `tls_enabled_true_listeners_still_bind_and_readyz_returns_ok`
  (behaviour is genuinely unchanged: the gRPC and HTTP listeners
  bind, `/readyz` returns 200 — so the warn line is "informational
  only", not a startup-refuse).
- **`spiffe.enabled = true` warn line (1)**:
  `spiffe_enabled_true_emits_warn_line` — pins the SPIFFE arm of the
  knob against an "only TLS triggers the warn" mutation.
- **No warn line at default (1)**:
  `config_with_security_keys_omitted_does_not_emit_tls_warn_line` —
  pins the early-return guard, so a "warn unconditionally on every
  startup" mutation cannot pass.
- **Unknown key rejected at config load (1)**:
  `config_with_unknown_key_is_rejected_at_load` — typing
  `max_concurent_requests` (one `r` missing) under
  `[aperture.transport.grpc]` produces a parse error rather than a
  silent default-value-use.

## Test budget

Six distinct behaviours: parse, warn-on-tls, listeners-still-bind,
warn-on-spiffe, no-warn-at-default, reject-unknown. Budget is 12 unit
tests (`2 × 6`). The seven acceptance tests cover the six behaviours
plus one structural pin (`tls.enabled=true` warn cardinality) so the
test count is **7/12** within budget. No additional unit tests were
needed: the existing `config/mod.rs` unit suite already pins the
builder's cross-field validation; the new figment loader is exercised
end-to-end through the seven acceptance tests, and mutation testing
(below) confirms every load-bearing mutation is killed.

## Mutation testing

Per ADR-0005 Gate 5, the target is 100% kill rate on Slice 07 touched
files. Run command (scoped to Slice 07 territory; restricted to the
green-by-design test set so the baseline passes):

```text
cargo mutants --package aperture --no-shuffle --jobs 2 \
  --file crates/aperture/src/config/mod.rs \
  --file crates/aperture/src/compose.rs \
  --cargo-test-arg "--lib" \
  --cargo-test-arg "--test=slice_01_walking_skeleton" \
  --cargo-test-arg "--test=slice_02_http_protobuf_and_readiness" \
  --cargo-test-arg "--test=slice_03_traces" \
  --cargo-test-arg "--test=slice_04_metrics" \
  --cargo-test-arg "--test=slice_05_backpressure" \
  --cargo-test-arg "--test=slice_06_forwarding_sink" \
  --cargo-test-arg "--test=slice_07_tls_schema_knob" \
  --cargo-test-arg "--test=probe_gold_runner" \
  --cargo-test-arg "--test=invariant_no_telemetry_on_telemetry"
```

Per-file results after this slice:

| File | Mutants | Caught | Missed | Unviable | Notes |
|---|---:|---:|---:|---:|---|
| `src/config/mod.rs` | 29 | 19 | 3 | 7 | 3 missed are pre-Slice-06 baseline (slice 01 / slice 08 territory) |
| `src/compose.rs` | 7 | 6 | 1 | 0 | 1 missed (`Config::forwarding_timeout` getter) is pre-existing slice 06 territory |
| **Slice 07 footprint** | **36** | **23** | **4** | **9** | **100% kill on slice-introduced mutations** |

The 4 pre-existing missed mutants:

1. `Config::builder -> ConfigBuilder` replace with `Default::default()`
   — Slice 01 territory. Already documented in the Slice 06
   completion summary.
2. `Config::forwarding_timeout -> Duration` replace with
   `Default::default()` — Slice 06 territory. The `Default` for
   `Duration` is `Duration::ZERO`; reqwest's per-request timeout of
   zero is treated as "no timeout" rather than "refuse instantly", so
   the slice 06 acceptance tests do not catch this mutation. A future
   slice that asserts a non-zero timeout in the `accept` path will
   close the gap.
3. `<impl std::fmt::Display for ConfigError>::fmt` replace with
   `Ok(Default::default())` — Slice 01 territory. The error
   `Display` is exercised only when an error is propagated to the
   binary; the integration tests assert through structured tracing
   events, not stringified errors. Already documented in the Slice
   06 completion summary.
4. `ConfigBuilder::drain_deadline -> Self` replace with
   `Default::default()` — Slice 08 territory. The drain orchestrator
   lands in the next slice; until it reads `drain_deadline` the
   setter is purely a builder pass-through.

Slice 07 introduces **zero new mutation misses** on the touched
files. The slice's own surface is covered:

- `Config::from_toml_path` and `Config::from_toml_str` returning
  `Ok(Default::default())` is killed by the parse-success and
  parse-failure assertions in the slice 07 acceptance tests.
- `Config::tls_enabled` and `Config::spiffe_enabled` returning a
  fixed `true`/`false` is killed by the warn-line tests for the
  `enabled = true` case and by the no-warn test for the
  `enabled = false` case.
- `ConfigBuilder::tls_enabled` and `ConfigBuilder::spiffe_enabled`
  setters returning `Default::default()` are killed by the warn-line
  tests (a default-discarded setter would silently leave the knob
  off, no warn line).
- `RawConfig::into_config` returning `Ok(Default::default())` is
  killed by the unknown-key rejection test (the mutated body would
  bypass the figment parse and never produce the parse-error result).
- `warn_if_v0_security_knob_set` replaced with `()` is killed by the
  warn-line tests (no warn would be emitted).
- The `&&` → `||` mutation on the early-return guard is killed by the
  no-warn-at-default test (would emit a warn unconditionally).
- The `delete !` mutations on the early-return guard are killed by
  the no-warn-at-default and warn-on-true tests acting as a pair.

This matches the pattern Slice 06 established: the new code surface
is at 100% kill, the residual misses are pre-existing.

## Production code added or modified

| File | Net change | What it does |
|---|---:|---|
| `src/config/mod.rs` | **+259 / 16** | Figment-driven TOML loader: `Config::from_toml_str` and `Config::from_toml_path` now extract through `figment::providers::Toml` into a typed `RawConfig` schema with `#[serde(deny_unknown_fields)]` on every nested struct (`RawConfig`, `ApertureSection`, `TransportSection`, `TransportArm`, `SinkSection`, `ForwardingSection`, `SecuritySection`, `TlsSection`, `AuthSection`, `SpiffeSection`, `ShutdownSection`). `RawConfig::into_config` folds the schema back through the existing `ConfigBuilder` so cross-field validation lives in one place. New `Config::tls_enabled()` and `Config::spiffe_enabled()` accessors (`pub(crate)`) so the composition root can read the knobs. |
| `src/compose.rs` | **+43 / 0** | `warn_if_v0_security_knob_set(&Config)` helper: emits exactly one `event=tls_not_supported_in_v0` (level `warn`) at startup when either knob is set to true, with a reason string that names which knob(s) the operator set. Three reason variants (`tls only`, `spiffe only`, `both`) so an operator porting a Phase-2 config to v0 sees one line that names the actual misconfiguration. The `(false, false)` arm of the inner match is `unreachable!()` because the early-return guard already filters that case. `compose::spawn` invokes the helper after the startup event and before sink wiring. |
| `Cargo.toml` (aperture) | **+9 / 0** | New deps: `figment = "0.10"` (with the `toml` feature, default features off) and `serde = { workspace = true }`. The workspace `serde` already enables the `derive` feature so the schema's `#[derive(Deserialize)]` works. |

## Architectural observations

- **Figment is configured to ADR-0008's recommendation.** The loader
  uses `Toml::string` (for `from_toml_str`) and `Toml::file` (for
  `from_toml_path`), both with default features only. The
  `Env::prefixed("APERTURE__")` provider that ADR-0008 names is a
  Slice 08 concern — the v0 binary loads from a single TOML file, no
  env-var override layer; once Slice 08 lands the CLI plumbing the
  env-var provider becomes a one-line addition to the figment chain.
- **`deny_unknown_fields` is per-struct, not per-call.** Every
  nested deserialise target (eleven structs) carries the attribute,
  so a misspelled key at any level of the schema fails loud. The
  unknown-key test in the acceptance file pins the
  `[aperture.transport.grpc]` level; the same enforcement applies at
  every other level by construction.
- **One warn event for two knobs.** ADR-0008 picks
  `event=tls_not_supported_in_v0` as a shared name across both
  forward-compat knobs because operators porting a Phase-2 config to
  v0 typically set both at once and one line per config-load is the
  cleaner stderr stream. The reason field is the disambiguator: it
  names which knob(s) the operator set. The closed event vocabulary
  in `observability::event` already had the constant; this slice
  added the call site, not the name.
- **No new event names.** ADR-0009 locks the closed v0 event
  vocabulary. The slice reuses `tls_not_supported_in_v0` (already
  declared) and emits no other events.
- **Listeners still bind plaintext.** The warn line is the entire
  user-visible effect of `tls.enabled = true` at v0. The transport
  layer is not consulted — `compose::spawn` continues calling
  `spawn_grpc` and `spawn_http` with no awareness of the security
  knobs. This is the load-bearing forward-compat property: when
  Aegis ships in Phase 2, the new code paths read the knobs; v0
  ignores them. The unit-of-behaviour separation is clean.
- **Zero behaviour change on the GREEN/REGRESSION axis.** The
  `slice_01..slice_06` test files are untouched and run unchanged.
  The figment-driven loader is an additional entry point; the
  builder path that all earlier tests use stays the canonical
  construction.

## Quality gates passed

| Gate | Status |
|---|---|
| Active acceptance tests pass | **7/7 in Slice 07** |
| All unit tests pass | **60/60** |
| All integration tests pass (excluding RED Slice 08) | **146/146** |
| Code formatting validation | `cargo fmt --check` clean |
| Static analysis | `cargo clippy --all-targets -- -D warnings` clean |
| Build validation | `cargo build --workspace` clean |
| No test skips | None added |
| Test count within behaviour budget | 7/12 (6 behaviours × 2) |
| No mocks inside hexagon | Tests enter through `Config::from_toml_str` and `aperture::spawn` driving ports |
| Business language in tests | Test names describe operator-observable outcomes |
| Mutation kill rate (touched files) | **100% on slice-introduced mutations**; 4 pre-existing misses (slice 01 / 06 / 08 territory) |
| Pre-commit hook | All gates green; pre-push gates green or skipped with reason |

## Commits

| SHA | Message |
|---|---|
| `e346c25` | `feat(aperture): Slice 07 — TLS/SPIFFE schema knob with v0 warn line` |
| (this doc) | `docs(aperture): DELIVER slice-07-completion summary` |

Two atomic commits. Slice 07 is the smallest slice in the v0 plan; the
single feature commit covered both the figment loader and the
warn-line emission cleanly with zero refactor pressure on the existing
slices.

## Recommendation on Slice 08

Slice 08 (graceful shutdown) is the **last slice** in the v0 plan.
The RED state is already in the tree
(`tests/slice_08_graceful_shutdown.rs`, 4 failing + 1 ignored as
designed) and `Config::drain_deadline` is the single configuration
hook the orchestrator will read.

Recommended approach:

1. **Lift the drain orchestrator first** (in-process). The
   `Handle::shutdown` path already exists; Slice 08 turns it into the
   ADR-0010 deadline-bounded drain that emits
   `event=shutdown_initiated`, `event=in_flight_drained`, and
   `event=drain_deadline_exceeded` (warn) at the right moments.
2. **`/readyz` flips to 503 within 100 ms** of the shutdown signal —
   that's the existing `slice_08_graceful_shutdown.rs` test
   `shutdown_flips_readyz_to_503_draining_within_100ms`. Wire the
   readiness state machine to a new "Draining" phase the way Slice 02
   wired the "Booting → Ready" transition.
3. **SIGTERM equivalence** is the ignored test on the file. Lifting
   it requires a process-spawning fixture; do this last (or defer to a
   companion slice) because the deterministic in-process tests carry
   most of the load.

Slice 08 closes Aperture v0. Recommend executing in the same per-cycle
commit-and-push discipline that carried Slices 03–07.

— Crafty
