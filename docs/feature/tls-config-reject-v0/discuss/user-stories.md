<!-- markdownlint-disable MD024 -->

# User Stories — tls-config-reject-v0

## System Constraints

- **Platform**: CLI/long-lived service. The operator-invocable entry point is the
  `aperture` binary: `aperture --config <path>` (`crates/aperture/src/main.rs`).
- **Observable surface**: process exit code + structured events on stderr (the
  tracing stream). No interactive TUI; no JSON-on-stdout API for this path.
- **Exit-code contract (existing)**: `2` = config error / pre-bind refusal
  (`crates/aperture/src/main.rs:19-21`). The refusal in this feature is a
  config-level rejection and exits non-zero with this established code.
- **Refusal-event shape (existing, solution-neutral here)**: a structured stderr
  event naming the cause, the same fail-closed shape as the existing
  `health.startup.refused` and `config_validation_failed` events
  (`crates/aperture/src/observability.rs:30-51`). DISCUSS asserts the *observable*
  (event present, names the unsupported knob, no plaintext bind); DESIGN picks the
  exact event constant and authors the ADR-0008-superseding ADR.
- **Fail-closed invariant**: when a security knob is requested-but-unimplemented,
  NO plaintext listener (gRPC `:4317` / HTTP `:4318`) may bind. Refusal precedes
  any listener bind.
- **Schema unchanged**: the TLS/SPIFFE keys remain present in the v0 schema,
  defaulting off (ADR-0008 forward-compat schema decision stands). Only the runtime
  reaction to `=true` changes.

## JTBD (job this feature serves)

> When I configure transport encryption (or SPIFFE auth) that this version does not
> implement, the collector REFUSES TO START and tells me, instead of silently
> shipping my telemetry in plaintext.

Forces:

- **Push**: "I set `tls.enabled = true` and aperture started — so my telemetry is
  encrypted." (It is not. It is plaintext. The warn line scrolled past unseen.)
- **Pull**: "If a security knob can't be honoured, I want the collector to stop and
  tell me, so I never ship cleartext telemetry by accident."
- **Anxiety**: "Will refusing to start take down my collector fleet on a config I
  thought was harmless?" — Addressed: the refusal is loud, names the exact knob, and
  the negative control (knob off) starts exactly as before.
- **Habit**: operators already expect aperture to refuse malformed config
  (`deny_unknown_fields`) and a lying downstream (`health.startup.refused`). This is
  the same reflex applied to the security knobs.

---

## US-TLS-01: Aperture refuses to start when an unimplemented security knob is requested

### Elevator Pitch

- **Before**: Priya Nadkarni sets `tls.enabled = true` in `aperture.toml`, runs
  `aperture --config aperture.toml`, and the collector starts and binds — shipping
  her telemetry in plaintext while a single `warn` line she never reads claims the
  knob was "ignored".
- **After**: Priya runs `aperture --config aperture.toml` with `tls.enabled = true`;
  aperture **refuses to start** — it exits non-zero (code 2) and prints a structured
  refusal event on stderr naming `tls.enabled` as an unsupported-in-v0 knob, and **no
  plaintext listener binds**. With the knob off (or absent), aperture starts and binds
  exactly as it does today.
- **Decision enabled**: Priya immediately learns her encryption expectation cannot be
  met by this version and decides — pin to a version that implements TLS, terminate
  TLS at a sidecar/mesh, or remove the knob — **before** any cleartext telemetry leaves
  the host. She never has to discover the downgrade from a packet capture.

### Problem

Priya Nadkarni is a platform/SRE engineer rolling out the aperture OTLP collector
across a regulated telemetry fleet. Her change request says "enable transport
encryption for the collector". She sets `tls.enabled = true` in `aperture.toml` and
deploys. The collector comes up green. She finds it **intolerable** that the only
signal her telemetry is still plaintext is a single `warn` line buried in startup
logs — a line her log pipeline samples away — so she ships cleartext telemetry across
the fleet believing it is encrypted, and the workaround (auditing every collector's
on-the-wire bytes after every deploy) is exactly the manual vigilance the collector
was supposed to remove.

### Who

- **Platform / SRE engineer**, deploying aperture in a regulated environment | runs
  `aperture --config <path>` in a container or systemd unit | motivated to guarantee
  telemetry-in-transit confidentiality and to *fail loudly* rather than silently
  downgrade.
- Secondary: **security/compliance reviewer** auditing the fleet | reads startup
  events and exit codes | motivated to prove no collector binds plaintext when
  encryption was requested.

### Solution

When aperture loads a configuration in which `tls.enabled = true` OR
`auth.spiffe.enabled = true`, it refuses to start: it exits non-zero (code 2) and
emits a structured refusal event on stderr that names the offending knob, and it
binds **no** listener. When both knobs are off or absent, startup and binding are
unchanged from today. The false comment at `crates/aperture/src/sinks.rs:94-95`
("the config validator rejects it ahead of this sink") is corrected to describe the
now-real rejection.

### Domain Examples

#### 1: Happy path (the refusal) — Priya enables TLS, aperture refuses

Priya Nadkarni's `aperture.toml` contains `[aperture.security.tls] enabled = true`.
She runs `aperture --config /etc/aperture/aperture.toml`. Aperture exits with code 2,
prints a structured refusal event on stderr whose message names `tls.enabled` as
unsupported in this version, and binds neither `0.0.0.0:4317` nor `0.0.0.0:4318`. No
telemetry is accepted; nothing is shipped in plaintext.

#### 2: Sibling knob — Marcus enables SPIFFE auth, aperture refuses identically

Marcus Bell, hardening the same fleet, sets `[aperture.security.auth.spiffe]
enabled = true` (leaving `tls.enabled = false`). `aperture --config aperture.toml`
exits code 2 and the refusal event names `auth.spiffe.enabled` as the unsupported
knob. No listener binds. The two security knobs share one refusal behaviour.

#### 3: Both knobs set — Priya ports a Phase-2 config back to v0

Priya copies a future Aegis-era config that sets both `tls.enabled = true` and
`auth.spiffe.enabled = true` onto a v0 collector. Aperture exits code 2 and the
refusal event names the requested-but-unimplemented security knob(s); it does not
silently pick one and proceed, and it binds no listener.

#### 4: Negative control — Wei runs the default plaintext config, aperture starts

Wei Tanaka runs `aperture --config aperture.toml` where
`[aperture.security.tls] enabled = false` and `[aperture.security.auth.spiffe]
enabled = false` (the defaults). Aperture starts exactly as today: it emits
`event=startup`, binds `0.0.0.0:4317` (gRPC) and `0.0.0.0:4318` (HTTP), and accepts
telemetry. No refusal event is emitted. (A config that omits the `[security]` tables
entirely behaves identically — absent means off.)

### UAT Scenarios (BDD)

#### Scenario: Collector refuses to start when transport encryption is requested but unimplemented

```gherkin
Given Priya's aperture config sets tls.enabled = true
When Priya starts aperture with that config
Then aperture exits with a non-zero status (code 2)
And a structured refusal event on stderr names tls.enabled as unsupported in this version
And no plaintext listener is bound on the gRPC or HTTP port
And no telemetry is accepted
```

#### Scenario: Collector refuses to start when SPIFFE auth is requested but unimplemented

```gherkin
Given Marcus's aperture config sets auth.spiffe.enabled = true and tls.enabled = false
When Marcus starts aperture with that config
Then aperture exits with a non-zero status (code 2)
And a structured refusal event on stderr names auth.spiffe.enabled as unsupported in this version
And no plaintext listener is bound on the gRPC or HTTP port
```

#### Scenario: Collector refuses when both security knobs are requested

```gherkin
Given Priya ports a Phase-2 config that sets both tls.enabled = true and auth.spiffe.enabled = true onto a v0 collector
When Priya starts aperture with that config
Then aperture exits with a non-zero status (code 2)
And a structured refusal event on stderr names the requested-but-unimplemented security knob(s)
And aperture does not silently choose to proceed in plaintext
And no plaintext listener is bound
```

#### Scenario: Collector starts normally when no security knob is requested (negative control)

```gherkin
Given Wei's aperture config sets tls.enabled = false and auth.spiffe.enabled = false
When Wei starts aperture with that config
Then aperture starts successfully
And it emits event=startup
And it binds the gRPC listener on 0.0.0.0:4317 and the HTTP listener on 0.0.0.0:4318
And no refusal event is emitted
And telemetry is accepted as it is today
```

#### Scenario: Collector starts normally when the security tables are absent (negative control)

```gherkin
Given Wei's aperture config omits the [security] tables entirely
When Wei starts aperture with that config
Then aperture treats both security knobs as off
And aperture starts and binds its listeners exactly as it does today
And no refusal event is emitted
```

#### Scenario: The false "validator rejects it" comment is corrected to match real behaviour

```gherkin
Given the sink module previously claimed "the config validator rejects it ahead of this sink"
When a reader inspects the security-knob handling after this change
Then the source comment accurately describes that aperture refuses to start on a requested security knob
And no comment claims a rejection that the code does not perform
```

### Acceptance Criteria

- [ ] Starting aperture with `tls.enabled = true` exits non-zero (code 2) and emits a
  structured refusal event on stderr naming `tls.enabled` as unsupported in v0.
  (Scenario 1)
- [ ] Starting aperture with `auth.spiffe.enabled = true` exits non-zero (code 2) and
  emits a structured refusal event naming `auth.spiffe.enabled`. (Scenario 2)
- [ ] Starting aperture with both knobs true exits non-zero (code 2) and the refusal
  event names the requested-but-unimplemented security knob(s); aperture does not
  silently proceed. (Scenario 3)
- [ ] On any refusal, **no** plaintext listener binds on `0.0.0.0:4317` or
  `0.0.0.0:4318` and no telemetry is accepted. (Scenarios 1-3)
- [ ] With both knobs false, aperture starts, emits `event=startup`, and binds both
  listeners exactly as today; no refusal event is emitted. (Scenario 4)
- [ ] With the `[security]` tables absent, behaviour is identical to both-knobs-false.
  (Scenario 5)
- [ ] The false comment at `crates/aperture/src/sinks.rs:94-95` is corrected to
  describe the real refusal; no comment claims a non-existent rejection. (Scenario 6)

### Outcome KPIs

- **Who**: aperture operators (platform/SRE engineers) deploying the v0 collector
  with a security knob set.
- **Does what**: stop receiving a silently-downgraded plaintext collector — instead
  get a loud refuse-to-start — when they request `tls.enabled` or
  `auth.spiffe.enabled`.
- **By how much**: 100% of startups with a requested-but-unimplemented security knob
  result in non-zero exit + refusal event + zero plaintext listeners bound (target:
  1.0; current: 0.0 — today 100% of such startups bind plaintext).
- **Measured by**: integration tests asserting exit code, refusal event presence/field,
  and absence of any bound listener across the two-knob truth table; plus the negative
  controls asserting unchanged start-and-bind.
- **Baseline**: today, a security-knob-set startup binds plaintext in 100% of cases
  (warn-and-continue, `compose.rs:127`).

### Technical Notes (constraints / dependencies for DESIGN)

- **Supersedes ADR-0008** warn-and-continue reaction for these two knobs (lines 19,
  36, 164, 166). The forward-compat *schema* decision stands; only the runtime
  reaction to `=true` changes. DESIGN authors the superseding ADR and updates
  ADR-0008's `Superseded by` header. See `wave-decisions.md`.
- **Refuse before bind**: the refusal must occur at config-validation / compose time,
  prior to any listener bind — same sequencing as the existing
  `health.startup.refused` probe path (`compose.rs:78-96, 110-127`).
- **Exit code 2** is the established config-error code (`main.rs:19-21`); reuse it.
- **Event constant**: DESIGN selects from the closed v0 vocabulary
  (`observability.rs:30-51`) — `config_validation_failed` and `health.startup.refused`
  are both present and shape-compatible; `tls_not_supported_in_v0` exists but is
  currently `warn`-level and semantically "ignored", so re-levelling vs. replacing is
  a DESIGN call.
- **Test fallout**: existing tests/golden output asserting warn-and-continue encode
  the superseded contract and will need updating in DELIVER. Flagged in
  `wave-decisions.md` risk table.
- **Mutation 100%** (CLAUDE.md / ADR-0005 Gate 5): the two-knob truth table plus the
  negative controls supply the kill coverage for the new reject branch.
- **Dependencies**: aperture config loader (ADR-0008, shipped), compose/spawn path,
  and observability event module — all exist. No new external dependency. No blocker.
