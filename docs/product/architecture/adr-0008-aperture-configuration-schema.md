# ADR-0008 — Aperture configuration schema and loader: TOML + figment with forward-compatible TLS/SPIFFE knobs

- **Status**: Accepted
- **Date**: 2026-05-04
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `aperture` v0
- **Supersedes**: none
- **Superseded by**: ADR-0061 — **runtime reaction only**. ADR-0061 supersedes the
  warn-and-continue reaction to `tls.enabled = true` / `auth.spiffe.enabled = true`
  (this ADR's §Decision bullet 4 line 36, §"TLS / SPIFFE forward-compat" table rows at
  lines 164 and 166, and the US-AP-01 AC quoted at line 19): v0 now **refuses to start**
  (`event=config_validation_failed`, exit 2, no listener bound) instead of warning and
  continuing plaintext. **The forward-compat SCHEMA decision in this ADR is NOT
  superseded and stands**: the TLS/SPIFFE keys remain present in the v0 schema, default
  off, with no Phase-2 (Aegis) schema break. Only the `= true` reaction changed.

## Context

Aperture is a long-lived service whose behaviour is controlled by a configuration file at startup. DISCUSS Q5 locks two structural requirements on the v0 schema:

1. Plaintext + no auth at v0.
2. **A configuration knob (TLS yes/no, SPIFFE yes/no) MUST exist in the v0 config schema, defaulting off.** This avoids a schema break in Phase 2 when Aegis ships.

DISCUSS US-AP-01 AC adds:
- Identical bind addresses for grpc and http MUST be rejected at config validation.
- `tls.enabled = true` on v0 MUST emit exactly one `event=tls_not_supported_in_v0` warn line and continue plaintext (US-AP-01 Domain Example #3).
- Default bind addresses are `0.0.0.0:4317` (gRPC) and `0.0.0.0:4318` (HTTP).

DISCUSS shared-artefacts registry calls out the following config keys: `aperture.transport.{grpc,http}.bind_addr`, `aperture.transport.{grpc,http}.max_recv_msg_size`, `aperture.transport.{grpc,http}.max_concurrent_requests`, `aperture.sink.kind`, `aperture.sink.forwarding.endpoint`, `aperture.sink.forwarding.timeout_ms` (default 5000), `aperture.shutdown.drain_deadline_ms` (default 30000).

What DESIGN must lock:
1. The config-file format.
2. The loader library.
3. The environment-variable override convention.
4. The exact TLS / SPIFFE schema (defaulting off but present at v0).
5. The validation strategy (post-deserialise checks; how `config_validation_failed` events are produced).

## Decision

- **Format**: TOML.
- **Loader**: `figment` (caret `^0.10`) with `Toml::file(path)` + `Env::prefixed("APERTURE__")` providers, in that order (file first, env overrides file).
- **Schema**: a single root struct `ApertureConfig` deserialised via `serde::Deserialize` with `#[serde(deny_unknown_fields)]` on every nested struct so misspelled keys are loud.
- **Forward-compat TLS/SPIFFE knobs**: present in the v0 schema, defaulting off; setting any of them to true on v0 emits exactly one warn-level event and continues plaintext.
- **Validation**: post-deserialise validator function `config::validate_config` runs the cross-field invariants (bind addresses different, forwarding endpoint non-empty when sink=forwarding, etc.). Failure returns `ApertureError::ConfigInvalid` with a specific message; exit code 2; stderr `event=config_validation_failed`.

## Schema (concrete)

```toml
[aperture.transport.grpc]
bind_addr = "0.0.0.0:4317"
max_recv_msg_size = 4194304        # 4 MiB
max_concurrent_requests = 1024

[aperture.transport.http]
bind_addr = "0.0.0.0:4318"
max_recv_msg_size = 4194304
max_concurrent_requests = 1024

[aperture.sink]
kind = "stub"                      # "stub" | "forwarding"

[aperture.sink.forwarding]
endpoint = ""
timeout_ms = 5000

[aperture.shutdown]
drain_deadline_ms = 30000

[aperture.security.tls]
enabled = false
cert_path = ""
key_path = ""

[aperture.security.auth.spiffe]
enabled = false
workload_api_socket = ""
trust_domain = ""
```

Environment-variable overrides use `APERTURE__` prefix and `__` as the path separator (figment's standard convention):
- `APERTURE__TRANSPORT__GRPC__BIND_ADDR=0.0.0.0:14317`
- `APERTURE__SINK__KIND=forwarding`
- `APERTURE__SINK__FORWARDING__ENDPOINT=http://otel-backend:4318`

The exact schema definitions, validation invariants, and tracing macro paths are in `docs/feature/aperture/design/component-design.md > Configuration schema`.

## Alternatives Considered

### Option A — TOML + figment with deny-unknown-fields (RECOMMENDED, accepted)

**Pros**:
- TOML is the de-facto Rust ecosystem configuration format. Cargo, rustfmt, clippy all use TOML; operators reading an Aperture config feel at home.
- `figment` is the canonical Rust layered-config library. It cleanly handles file-then-env overrides, returns precise locator errors ("invalid value at key X in file Y, line Z"), and integrates with serde. ~3M downloads/month, MIT, mature.
- `serde(deny_unknown_fields)` on every nested struct catches typos at deserialise time. Without it, `enabledd = true` silently uses the default.
- Forward-compatibility: the TLS/SPIFFE knobs are part of `serde::Deserialize`'s schema; their absence on v0 means `enabled = false` (the default). Aegis (Phase 2) flips defaults and adds behaviour without breaking the schema.

**Cons**:
- TOML's array-of-tables syntax is awkward for some shapes (multiple sinks, multi-tenancy). Acceptable: at v0 there is one sink; multi-tenancy is Aegis's domain.
- `figment`'s "Provider" abstraction adds a small mental-model cost. Acceptable: the loader is one function in `config::load_config`, written once.

### Option B — TOML + plain `serde + toml` (no env-var layer)

**Pros**:
- One fewer dependency.
- Marginally simpler.

**Cons**:
- Operators expect to override config via env vars in containers (12-factor). Without `figment`, every env-var override is hand-written merging code in `load_config`.
- Re-implementing layered config is exactly the kind of thing libraries like `figment` exist to remove.

**Rejected** because the env-var layer is operator-table-stakes for a service deployed in containers.

### Option C — TOML + `config-rs` (the older alternative)

**Pros**:
- Mature; production-proven.
- Integrates with serde.

**Cons**:
- API is less ergonomic than figment's; "build a Config object and call `try_deserialize`" is an extra step compared to figment's `Figment::new().merge(...)::extract()`.
- Smaller community momentum; figment has overtaken it for new Rust projects since 2023.

**Rejected** because figment's ergonomics are noticeably nicer and the maturity gap is closed.

### Option D — YAML

**Pros**:
- Operators familiar with k8s manifests.

**Cons**:
- YAML's whitespace-and-anchors complexity is a known operational footgun. Cargo and the Rust ecosystem use TOML; using YAML for Aperture would diverge from project convention without justification.
- TOML's strict structure is the opposite kind of error mode (loud parse errors) which is what an operator wants.

**Rejected** because TOML matches the Rust ecosystem and is structurally less footgun-prone.

### Option E — JSON

**Pros**:
- Universal.

**Cons**:
- No comments. Operators commenting out a config knob during incident triage is impossible without a shadow file. Configuration files MUST allow comments.
- Diff readability is worse than TOML (every line is "key": "value", with quoting overhead).

**Rejected** for the no-comments deal-breaker.

### Option F — In-binary config + reload via SIGHUP

Rejected at the DISCUSS level (DISCUSS Q4 implicit; "v0 ships restart-as-process-exit"). Listed here for completeness; no DESIGN-level decision needed.

## Consequences

### Positive
- TOML + figment + deny-unknown-fields is the canonical Rust ecosystem stack for this pattern. No novelty.
- Forward-compatible with Aegis (Phase 2): TLS/SPIFFE keys present at v0, defaulting off; turning them on at Phase 2 is purely additive (new behaviour gated on existing keys; no schema break).
- Misspelled keys are loud (`config_validation_failed` with the offending key). No silent default-value-use.
- Env-var override is operator-table-stakes; figment provides it free.
- Validation errors are structured (`ApertureError::ConfigInvalid` with a specific message; exit code 2; structured stderr line).

### Negative
- `figment` adds a transitive dep (`uncased`, `pear`, `serde_yaml` if YAML feature were enabled — it is not). Net dep cost is small, all permissive licences.
- The `deny_unknown_fields` posture means a future schema addition is a major-version bump if a forward-config is loaded by an old binary. Acceptable: Aperture is the only consumer of its config; backward-config-on-new-binary works (deny_unknown_fields fires on the new binary's reading of an old config only if the OLD config has a key the NEW binary doesn't recognise, which is the wrong direction). The right direction (new binary, old config) is allowed by serde defaults.

### TLS / SPIFFE forward-compat (the load-bearing detail)

The schema is **structurally identical at v0 and Phase 2**. The difference is:

| Knob | v0 behaviour | Phase 2 (Aegis) behaviour |
|---|---|---|
| `tls.enabled = false` | Plaintext. Default. | Plaintext. Default. |
| `tls.enabled = true` | One warn line `event=tls_not_supported_in_v0`. Continue plaintext. | Read `cert_path` and `key_path`; bind listeners with rustls. |
| `auth.spiffe.enabled = false` | No auth. Default. | No auth. Default. |
| `auth.spiffe.enabled = true` | One warn line `event=tls_not_supported_in_v0` (same event name covers both the TLS and SPIFFE forward-compat warnings; the closed event vocabulary in DISCUSS D1 lists this name). Continue plaintext. | Connect to `workload_api_socket`; validate caller SPIFFE IDs against `trust_domain`. |

Aegis ships purely additive code paths; no schema break, no consumer-facing change.

The single warn event covers both TLS and SPIFFE because operators porting from a Phase-2+ config to v0 are typically setting both at once; one warn line per config-load, not two, is the cleaner stderr stream. (DISCUSS D1's closed vocabulary already permits this; the event is `tls_not_supported_in_v0` and the message body names what was set.)

### Trade-off ATAM

**Sensitivity point** for **Maintainability — Modifiability** (the schema is forward-compatible by design) and **Compatibility — Adaptability** (operators on v0 can write configs that work on Phase-2 unchanged).

Not a trade-off point.
