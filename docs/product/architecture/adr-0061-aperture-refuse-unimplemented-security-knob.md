# ADR-0061 — Aperture refuses to start when an unimplemented security knob is requested

- **Status**: Accepted
- **Date**: 2026-06-04
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `tls-config-reject-v0`
- **Supersedes**: ADR-0008 (the *runtime reaction* to `tls.enabled = true` / `auth.spiffe.enabled = true` only — see "Scope of supersession" below; ADR-0008's forward-compat **schema** decision is preserved unchanged)
- **Superseded by**: none

## Context

Aperture v0 ships plaintext-only transport and no authentication. ADR-0008 deliberately
placed two forward-compat security knobs in the v0 schema — `aperture.security.tls.enabled`
and `aperture.security.auth.spiffe.enabled` — defaulting off, so an operator's config rolls
forward into Phase 2 (Aegis) without a schema break. That schema decision is sound and stands.

ADR-0008 also chose a **runtime reaction** for the `= true` case: emit exactly one
`event=tls_not_supported_in_v0` *warn* line and **continue binding plaintext listeners**
(ADR-0008 §Decision bullet 4, line 36; §"TLS / SPIFFE forward-compat" table, lines 164 and 166;
US-AP-01 AC cited at ADR-0008 line 19). This is the warn-and-continue contract.

Verifier issue 008 (HIGH, security), re-verified in code on 2026-06-04, shows the operator
consequence of warn-and-continue:

| Evidence | Location | What it shows |
|---|---|---|
| `tls_enabled: bool` parsed from `tls.enabled` | `crates/aperture/src/config/mod.rs:56` | The knob is read. |
| `spiffe_enabled` from `auth.spiffe.enabled` | `crates/aperture/src/config/mod.rs:58` | Sibling knob, same shape. |
| `warn_if_v0_security_knob_set` warns and returns | `crates/aperture/src/compose.rs:56-76` | When either knob is true, aperture LOGS A WARNING and proceeds. |
| `spawn` then binds plaintext listeners | `crates/aperture/src/compose.rs:127` then `:164-205` | After the warn line, plaintext listeners bind regardless. |
| FALSE comment "the config validator rejects it ahead of this sink" | `crates/aperture/src/sinks.rs:94-95` | Claims a rejection that does not exist. Contradicted by `compose.rs:127`. |

An operator who sets `tls.enabled = true` believing they get transport encryption gets
**plaintext** plus a single `warn` line their log pipeline may sample away. That is a *silent
security downgrade*: the operator ships cleartext telemetry across a regulated fleet believing
it is encrypted, and the only way to discover the downgrade is a packet capture after every
deploy — exactly the manual vigilance the collector was supposed to remove.

This is the one place Aperture's Earned-Trust / fail-closed posture leaked. Aperture already
refuses malformed config (`deny_unknown_fields`), refuses a lying downstream (the Earned-Trust
probe → `event=health.startup.refused`), and fails closed on tenancy. A requested-but-unimplemented
security knob must get the same reflex: **refuse loudly rather than downgrade silently.**

### What DESIGN must lock (this ADR)

1. **Where** the refusal happens, such that NO listener can bind on refusal (US-TLS-01 AC-4).
2. The exact **refusal event** (name + fields naming the requested knob) and the **exit code**.
3. The **behaviour matrix** for the two-knob truth table, with negative controls unchanged.
4. The correction of the false comment at `sinks.rs:94-95`.

## Decision

When Aperture loads a configuration in which `tls.enabled = true` **OR**
`auth.spiffe.enabled = true`, it **refuses to start**: it does not construct a usable
`Config`, it exits non-zero with the established config-error code **2**, it emits a
structured **`event=config_validation_failed`** line on stderr that names the offending
knob(s), and **no listener binds**. When both knobs are off or absent, startup and binding
are byte-for-byte unchanged from today.

### Refusal point (the seam) — config validation, in `RawConfig::into_config`

The refusal is enforced as a **post-deserialise validation invariant**, co-located with the
existing identical-bind-address check, in the config-load path
(`crates/aperture/src/config/mod.rs`, `RawConfig::into_config` →
`ConfigBuilder::build`-adjacent validation). It returns
`Err(ConfigError("aperture v0 does not implement … : tls.enabled / auth.spiffe.enabled"))`.

This seam is chosen because it is the **earliest point that structurally guarantees AC-4**
(no plaintext bind on refusal):

- `main.rs` calls `Config::from_toml_path(&path)` **before** it ever calls `aperture::run`
  (`crates/aperture/src/main.rs:30` vs `:55`). A `ConfigError` returned from the loader hits
  the existing `Err(e) => { eprintln!(…); return ExitCode::from(2); }` arm at `main.rs:32-40`.
- `run` → `wire_sink` → `spawn` → `spawn_grpc` / `spawn_http` is the **only** path that binds a
  listener (`compose.rs:164,182`). If `Config` is never successfully constructed, that path is
  never entered. There is no listener, no Tokio task, no `install_subscriber` runtime call on
  the refusal path. The guarantee is structural, not a matter of ordering discipline inside
  `spawn`.

The `warn_if_v0_security_knob_set` helper (`compose.rs:56-76`) and its call site
(`compose.rs:127`) are **removed**: the reaction now lives one layer earlier, at config
validation, and warn-and-continue no longer exists.

### Refusal event — `config_validation_failed` (not `health.startup.refused`)

The closed v0 vocabulary (`observability.rs:30-51`) offers two shape-compatible candidates.
DESIGN selects **`event=config_validation_failed`** on a clean semantic axis:

- `config_validation_failed` is, per ADR-0008 §Decision bullet 5 and `component-design.md`
  (`ConfigInvalid → exit 2 → config_validation_failed`), the event for *"the operator's
  configuration is invalid for this binary"* — discovered at config-validation time, before
  any runtime wiring. A requested-but-unimplemented knob is precisely that: a config the
  operator wrote that this version cannot honour. It rides the exact `ConfigError` → exit-2
  channel the identical-bind-address check already uses.
- `health.startup.refused` is the **runtime substrate-probe** refusal: *"a dependency I wired
  and probed lied to me at runtime"* (cinder fsync, aperture sink probe — `compose.rs:78-96`,
  `cinder_crash_target.rs`). It carries a `substrate=` descriptor. A static config knob is not
  a probed substrate; using `health.startup.refused` here would blur the two distinct
  fail-closed axes (config-is-wrong vs dependency-lied) that the rest of the codebase keeps
  separate.
- `tls_not_supported_in_v0` is rejected: it is semantically "ignored / continuing", currently
  warn-level, and is the very contract being superseded. Re-levelling it to error would
  preserve a name that says "not supported, continuing" for a behaviour that is now "refused,
  not continuing" — a misleading name. It is **retired** from the call sites (the constant may
  remain in the closed vocabulary under `#[allow(dead_code)]` per the vocabulary's stated
  policy, or be removed; that is a DELIVER cleanup detail, not an architectural decision).

**Event fields** (the operator and a black-box harness both read which knob caused the refusal):

```
level  = error
event  = "config_validation_failed"
reason = "<human-readable: aperture v0 implements neither transport encryption nor SPIFFE auth>"
# the reason string NAMES the requested knob(s) verbatim:
#   tls only    → "...refusing to start: tls.enabled=true is not implemented in v0"
#   spiffe only → "...refusing to start: auth.spiffe.enabled=true is not implemented in v0"
#   both        → "...refusing to start: tls.enabled=true and auth.spiffe.enabled=true are not implemented in v0"
```

The exact field name(s) carrying the knob identity (a single `reason` string vs a structured
`requested_knobs` field) is a DELIVER/DISTILL detail; the **architectural contract** is: the
event is `config_validation_failed`, level error, and it names `tls.enabled` and/or
`auth.spiffe.enabled` such that both a human and a string-matching test can identify the
offending knob.

**Caveat on emission timing (flagged for DELIVER, not a blocker):** when `--config <path>` is
given, `main.rs` catches the loader `ConfigError` *before* `install_subscriber` runs and prints
via `eprintln!` (the established pre-init narrow window, `main.rs:33-38`). To honour the AC's
"structured refusal event on stderr", DELIVER routes the security-knob refusal through the same
structured channel the identical-bind-address rejection uses in the authoritative design
(`component-design.md:1066`: `config_validation_failed` is emitted in `main()`'s config-error
mapping). The observable the acceptance suite asserts is: exit code 2 **and** a stderr line
naming the knob with `event=config_validation_failed`. Whether that line is JSON-structured via
the subscriber or a structured-shape `eprintln!` in the pre-init window is a DELIVER mechanism
choice constrained by this ADR to: (a) exit 2, (b) `event=config_validation_failed`, (c) names
the knob, (d) no listener bound.

### Exit code — 2 (reuse)

Exit code **2** is the established config-error / pre-bind-refusal code (`main.rs:19-21`,
`component-design.md:490-491`). No new code is introduced; the security-knob refusal is a
config error and exits with the config-error code, reusing the `main.rs:32-40` arm verbatim.

### Behaviour matrix

| `tls.enabled` | `auth.spiffe.enabled` | Result | Event | Exit | Listener bound? |
|---|---|---|---|---|---|
| true | false | **Refuse** | `config_validation_failed` reason names `tls.enabled` | 2 | No |
| false | true | **Refuse** | `config_validation_failed` reason names `auth.spiffe.enabled` | 2 | No |
| true | true | **Refuse** | `config_validation_failed` reason names **both** requested knobs | 2 | No |
| false | false | **Start** (unchanged) | `startup` then `ready` (no refusal event) | binds, then runs | Yes (4317 + 4318) |
| absent `[security]` | absent | **Start** (unchanged) | identical to both-false (serde `#[serde(default)]` → false) | binds, then runs | Yes |

The two negative-control rows are the non-regression guard: today's behaviour with the knobs
off MUST be byte-for-byte preserved (same `startup`/`ready` events, same bound ports, telemetry
accepted). The refusal on `true` does not "silently pick one and proceed" in the both-true case;
it names the requested knob(s) and refuses.

### Comment correction at `sinks.rs:94-95`

The comment "Plaintext at v0; `tls.enabled=true` is reserved by Slice 07 and the config
validator rejects it ahead of this sink" — currently **false** (no such rejection existed) —
becomes **true** under this ADR. DELIVER updates it to state the now-real behaviour, e.g.:
"Plaintext at v0. `tls.enabled=true` / `auth.spiffe.enabled=true` cause config validation to
refuse startup (ADR-0061) before this sink is ever constructed; no plaintext sink runs when
encryption or auth was requested." No comment may claim a rejection the code does not perform.

## Scope of supersession (precise)

This ADR supersedes **only the runtime reaction** clauses of ADR-0008 for these two knobs:

| ADR-0008 clause | Location | Old contract (superseded) | New contract (this ADR) |
|---|---|---|---|
| "setting any of them to true on v0 emits exactly one warn-level event and continues plaintext" | ADR-0008 line 36 | warn-and-continue | refuse-to-start (`config_validation_failed`, exit 2) |
| `tls.enabled = true` → "One warn line. Continue plaintext." | ADR-0008 line 164 | warn-and-continue | refuse-to-start |
| `auth.spiffe.enabled = true` → same warn line, "Continue plaintext." | ADR-0008 line 166 | warn-and-continue | refuse-to-start |
| US-AP-01 AC: `tls.enabled=true` "MUST emit exactly one `event=tls_not_supported_in_v0` warn line and continue plaintext" | ADR-0008 line 19 | warn-and-continue | refuse-to-start |

**Explicitly PRESERVED, NOT superseded** — ADR-0008's forward-compat **schema** decision:

- The `tls.enabled`, `cert_path`, `key_path`, `auth.spiffe.enabled`, `workload_api_socket`,
  `trust_domain` keys **remain present in the v0 schema**, defaulting off (ADR-0008 §Schema,
  lines 62-71).
- **No Phase-2 / Aegis schema break.** The schema is still structurally identical at v0 and
  Phase 2 (ADR-0008 §"TLS / SPIFFE forward-compat", line 159). Aegis still flips behaviour on
  the same existing keys, purely additively. The only difference this ADR introduces is that
  the v0 *reaction to `= true`* changes from warn-and-continue to refuse-to-start. A v0 config
  with the knobs **off** is unchanged and rolls forward exactly as ADR-0008 designed.

ADR-0008's `Superseded by` header is updated to point at this ADR with the scope note "runtime
reaction to `tls.enabled` / `auth.spiffe.enabled` = true only; forward-compat schema preserved".

## Alternatives Considered

### Option A — Refuse at config validation, event `config_validation_failed`, exit 2 (RECOMMENDED, accepted)

Refuse in `RawConfig::into_config` (config-load), reusing the `ConfigError` → `main.rs` exit-2
arm and the `config_validation_failed` event.

**Pros**:
- Structurally guarantees AC-4 (no listener binds): the bind path is never entered because
  `Config` is never constructed. The guarantee does not depend on ordering discipline inside
  `spawn`.
- Reuses three existing seams verbatim: the `ConfigError` type, the `main.rs:32-40` exit-2 arm,
  and the `config_validation_failed` event already designated for "config is invalid for this
  binary". No new vocabulary, no new error variant, no new exit code.
- Correct semantic axis: a requested-unimplemented knob is a config error, not a runtime
  substrate lie. Keeps `config_validation_failed` (config-wrong) and `health.startup.refused`
  (dependency-lied) cleanly separated, as the rest of the codebase does.

**Cons**:
- The pre-subscriber `eprintln!` window in `main.rs` means DELIVER must ensure the stderr line
  is structured (named knob) in that window — a small, well-scoped mechanism detail already
  faced by the identical-bind-address rejection. Acceptable and flagged above.

### Option B — Refuse at compose time, event `health.startup.refused`, exit non-zero

Keep `Config` construction permissive; move the check into `compose::spawn` (where
`warn_if_v0_security_knob_set` lives today), refuse before the bind calls, emit
`health.startup.refused`.

**Pros**:
- Minimal diff to the existing `warn_if_v0_security_knob_set` call site.
- The subscriber is installed by the time `spawn` runs, so the structured event is trivially JSON.

**Cons**:
- **Weaker AC-4 guarantee**: refusal now depends on the check sitting *before* `spawn_grpc` and
  never being reordered below it by a future edit. Option A's "Config never constructed" guarantee
  is stronger and survives refactors.
- **Wrong semantic axis**: `health.startup.refused` means "a probed dependency lied". A static
  config knob is not a probed substrate; overloading the event blurs the two fail-closed axes
  and pollutes the substrate-refusal alerting (`monitoring-alerting.md:297` groups
  `health.startup.refused` with bind failures as runtime infra faults, not config errors).
- `run` calls `wire_sink` (which runs the *sink* probe) **before** `spawn`
  (`lib.rs:206-207`); a sink probe could fire `health.startup.refused` for the wrong reason
  before the security check is even reached, muddying the operator signal.

**Rejected**: weaker non-bind guarantee and wrong event semantics.

### Option C — Keep warn-and-continue; only fix the false comment

Leave the runtime behaviour as warn-and-continue and merely correct `sinks.rs:94-95` to say
"the validator does NOT reject this; v0 continues plaintext".

**Pros**:
- Smallest possible change; zero behaviour risk; no ADR-0008 supersession needed.

**Cons**:
- **Does not close the security defect.** The silent plaintext downgrade — the entire reason
  verifier issue 008 is HIGH-severity — remains. The operator still ships cleartext believing
  it is encrypted. This directly contradicts the project's Earned-Trust / fail-closed posture
  (no-lying-downstream, fail-closed tenancy, `deny_unknown_fields`): warn-and-continue *is* the
  lie. Fixing only the comment makes the code honest about being dishonest, which is worse, not
  better, for the operator.

**Rejected**: leaves the HIGH-severity security defect open.

### Option D — Reject `tls` but keep warn-and-continue for `spiffe` (or vice versa)

Refuse only on `tls.enabled`, treat `auth.spiffe.enabled` as a softer warn-and-continue.

**Pros**:
- Marginally smaller blast radius if one feared `spiffe` configs were more common in the field.

**Cons**:
- The two knobs share one failure mode: both promise a security property (confidentiality;
  authenticity) that v0 does not deliver. Requesting SPIFFE auth and getting *no auth* is the
  same class of silent downgrade as requesting TLS and getting plaintext. Splitting the reaction
  is an arbitrary inconsistency the operator would have to memorise, and US-TLS-01 Domain
  Example #2 and Scenario 2 explicitly require SPIFFE to refuse identically.
- Both-true (Domain Example #3) would then have ambiguous behaviour.

**Rejected**: inconsistent, contradicts US-TLS-01 ACs 2 and 3, leaves half the downgrade silent.

## Consequences

### Positive
- The Earned-Trust / fail-closed posture is now complete: the one leaking knob-pair fails closed.
  Requesting a security property v0 cannot deliver produces a loud refusal, not a silent downgrade.
- 100% of startups with a requested-but-unimplemented security knob now exit non-zero with a
  named refusal and zero plaintext listeners (outcome KPI target 1.0; baseline 0.0).
- Reuses existing exit code (2), existing event (`config_validation_failed`), existing error
  type (`ConfigError`), and existing `main.rs` exit-2 arm. No new architectural surface.
- The false `sinks.rs:94-95` comment becomes true.

### Negative
- **Breaking behaviour change for any config that set the knobs to `true` and relied on
  warn-and-continue.** Such a config now refuses to start. This is intended (it was shipping
  plaintext), but it is a behaviour break and must be called out in release notes. The negative
  controls (knobs off / absent) guarantee the common case is unaffected.
- **Existing tests/golden output asserting warn-and-continue encode the superseded contract and
  will fail.** Specifically `crates/aperture/tests/slice_07_tls_schema_knob.rs` and any golden
  output asserting `event=tls_not_supported_in_v0` + a bound listener. DELIVER updates these to
  the refusal contract. Flagged in the feature wave-decisions risk table.
- An embedder of aperture that sets the knob unconditionally would now fail to start. Reuse
  analysis (feature wave-decisions) and the negative-control scenario confirm no embedder
  (notably `gateway`) sets the knob; the negative control guards the non-regression.

### Trade-off ATAM
- **Sensitivity point** for **Security — Confidentiality / Authenticity**: the refusal converts a
  silent confidentiality/authenticity downgrade into a fail-closed startup refusal.
- **Trade-off point** Security vs Availability: a config that previously started (degraded) now
  refuses to start. The trade is deliberate and aligns with the fail-closed posture — a collector
  that refuses to ship cleartext is preferable to one that ships it silently. The negative-control
  invariant bounds the availability cost to configs that actually requested an unimplemented
  security property.

## Enforcement

- The behaviour is covered by integration tests across the two-knob truth table plus the two
  negative controls (US-TLS-01 ACs 1-7), supplying the per-feature 100% mutation kill coverage
  required by CLAUDE.md / ADR-0005 Gate 5 on the new reject branch.
- No new architectural-style rule is introduced; the existing `deny_unknown_fields` /
  config-validation discipline already governs this seam. The refusal is one more invariant in
  the same `RawConfig::into_config` validator that already rejects identical bind addresses.
