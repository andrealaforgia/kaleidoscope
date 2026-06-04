# Wave Decisions — tls-config-reject-v0 (DESIGN)

- **Feature ID**: tls-config-reject-v0
- **Wave**: DESIGN (nWave)
- **Architect**: Morgan (nw-solution-architect)
- **Date**: 2026-06-04
- **Mode**: PROPOSE (autonomous; options enumerated, one recommended per load-bearing decision)
- **Scope**: Application / components — a config-validation / startup-behaviour change in `aperture`.

## Inputs read

- `docs/feature/tls-config-reject-v0/discuss/user-stories.md` (US-TLS-01, its 7 ACs, 6 BDD scenarios).
- `docs/feature/tls-config-reject-v0/discuss/wave-decisions.md` (DISCUSS; ADR-0008-supersession note with line citations).
- `docs/product/architecture/adr-0008-aperture-configuration-schema.md` (the warn-and-ignore decision being superseded; the forward-compat schema being preserved).
- Real code: `crates/aperture/src/{config/mod.rs, compose.rs, sinks.rs, main.rs, observability.rs, lib.rs}`.
- `docs/feature/aperture/design/component-design.md` (authoritative aperture design; the `ConfigInvalid → exit 2 → config_validation_failed` mapping at lines 490-491, 577, 1066).

## Key Decisions

### D1 — Refusal point: config validation in `RawConfig::into_config` (NOT compose time)

The refusal is a **post-deserialise config-validation invariant** in
`crates/aperture/src/config/mod.rs` (`RawConfig::into_config`, beside the existing
identical-bind-address check that lives in `ConfigBuilder::build`). It returns
`Err(ConfigError(...))`.

**Why this seam guarantees AC-4 (no plaintext bind on refusal):**

- `main.rs:30` calls `Config::from_toml_path(&path)` **before** `main.rs:55` calls
  `aperture::run`. A `ConfigError` hits the existing exit-2 arm at `main.rs:32-40`.
- The **only** path that binds a listener is `run` → `wire_sink` → `spawn` →
  `spawn_grpc`/`spawn_http` (`compose.rs:164,182`). That path is only entered after a
  `Config` is successfully constructed. If `Config` is never constructed, no listener,
  no Tokio task, no runtime subscriber install on the refusal path.
- The guarantee is **structural** (Config-never-constructed), not ordering discipline
  inside `spawn`. It survives future refactors of `spawn`.

Rejected alternative (compose-time refusal): weaker AC-4 guarantee (depends on the
check staying above `spawn_grpc`), and `run` runs `wire_sink`'s sink probe *before*
`spawn` (`lib.rs:206-207`), so a sink probe could fire its own refusal before the
security check is reached. See ADR-0061 Option B.

`warn_if_v0_security_knob_set` (`compose.rs:56-76`) and its call site (`compose.rs:127`)
are **removed**.

### D2 — Refusal event + exit code

- **Event**: `event=config_validation_failed` (level `error`). It is, per ADR-0008 §Decision
  bullet 5 and `component-design.md:1066`, the event for "the operator's config is invalid for
  this binary", discovered at config-validation time. A requested-but-unimplemented knob is
  exactly that. It rides the `ConfigError` → exit-2 channel the identical-bind-address check
  already uses.
- **NOT `health.startup.refused`**: that is the runtime substrate-probe refusal axis ("a probed
  dependency lied at runtime"; carries `substrate=`). A static config knob is not a probed
  substrate. Keeping the two axes separate matches the rest of the codebase (cinder/pulse fsync
  probes) and the alerting grouping (`monitoring-alerting.md:297`). See ADR-0061 §"Refusal event".
- **NOT `tls_not_supported_in_v0` re-levelled**: that name means "not supported, continuing" — the
  superseded contract. Misleading for a "refused, not continuing" behaviour. Retired from call sites.
- **Exit code**: **2** — the established config-error / pre-bind-refusal code (`main.rs:19-21`).
  Reused verbatim; no new exit path.
- **Field carrying the knob identity**: the `reason` string NAMES the requested knob(s) verbatim
  (`tls.enabled=true`, `auth.spiffe.enabled=true`, or both) so an operator and a black-box
  string-matching test both identify the cause. Whether the knob identity is a single `reason`
  string or an additional structured `requested_knobs` field is a DELIVER detail; the
  architectural contract is: event `config_validation_failed`, level error, names the knob(s).

### D3 — Behaviour matrix

| `tls.enabled` | `auth.spiffe.enabled` | Result | Event | Exit | Listener bound? |
|---|---|---|---|---|---|
| true | false | Refuse | `config_validation_failed` names `tls.enabled` | 2 | No |
| false | true | Refuse | `config_validation_failed` names `auth.spiffe.enabled` | 2 | No |
| true | true | Refuse | `config_validation_failed` names **both** | 2 | No |
| false | false | Start (unchanged) | `startup`/`ready`; no refusal | binds then runs | Yes (4317+4318) |
| `[security]` absent | absent | Start (unchanged) | identical to both-false | binds then runs | Yes |

Both-true names the requested knob(s) and refuses; it does NOT silently pick one and proceed.
The two negative-control rows are the non-regression guard — today's knobs-off behaviour is
preserved byte-for-byte.

### D4 — Comment correction at `sinks.rs:94-95`

Current (false): "Plaintext at v0; `tls.enabled=true` is reserved by Slice 07 and the config
validator rejects it ahead of this sink." Under ADR-0061 this becomes **true**. DELIVER updates it
to state the now-real behaviour: `tls.enabled=true` / `auth.spiffe.enabled=true` cause config
validation to refuse startup (ADR-0061) before this sink is ever constructed; no plaintext sink
runs when encryption or auth was requested. No comment may claim a rejection the code does not
perform.

### D5 — ADR authored: ADR-0061 (next free number)

Highest existing ADR was 0060 (`store-fsync-durability`); new ADR is **ADR-0061** at
`docs/product/architecture/adr-0061-aperture-refuse-unimplemented-security-knob.md`. It records
the decision, four considered alternatives (A accepted; B compose-time/`health.startup.refused`;
C keep-warn-fix-comment-only; D reject-tls-not-spiffe — all rejected with rationale), the precise
supersession scope, consequences, and the ATAM trade-off (Security-Confidentiality vs Availability).

## ADR-0008 supersession (precise scope)

| ADR-0008 clause | Location | Old (superseded) | New (ADR-0061) |
|---|---|---|---|
| "emits exactly one warn-level event and continues plaintext" | line 36 | warn-and-continue | refuse (`config_validation_failed`, exit 2) |
| `tls.enabled = true` → "One warn line. Continue plaintext." | line 164 | warn-and-continue | refuse |
| `auth.spiffe.enabled = true` → same warn line, "Continue plaintext." | line 166 | warn-and-continue | refuse |
| US-AP-01 AC quoted in ADR-0008 | line 19 | warn-and-continue | refuse |

**PRESERVED, NOT superseded** — ADR-0008's forward-compat **schema** decision: TLS/SPIFFE keys
remain present in the v0 schema, default off (ADR-0008 lines 62-71), with **no Phase-2 (Aegis)
schema break** (ADR-0008 line 159). The schema is still structurally identical at v0 and Phase 2;
only the v0 reaction to `= true` changes. ADR-0008's `Superseded by` header was updated with this
scope note (`adr-0008-...md` lines 7-15).

## Reuse Analysis (RCA hard gate — extend the existing fail-closed seam)

| Existing machinery | Path | Decision |
|---|---|---|
| `ConfigError` + `RawConfig::into_config` validator | `config/mod.rs:178-188, 481-530` | **EXTEND** — one more invariant beside the identical-bind-address check. |
| `main.rs` exit-2 config-error arm | `main.rs:32-40` | **REUSE verbatim** — `ConfigError` already maps to `eprintln!` + exit 2. |
| `event::CONFIG_VALIDATION_FAILED` | `observability.rs:49` | **REUSE** — designated event for config-invalid-for-this-binary. |
| `event::HEALTH_STARTUP_REFUSED` | `observability.rs:48` | **REJECTED for this use** — runtime substrate-probe axis, not config-is-wrong. |
| `warn_if_v0_security_knob_set` + call site | `compose.rs:56-76, 127` | **REMOVE** — reaction moves to config validation. |
| `event::TLS_NOT_SUPPORTED_IN_V0` | `observability.rs:47` | **RETIRE from call sites** — superseded "ignored/continuing" semantics. |

**Net new code: one validation branch.** No new error variant, exit code, event name, or file.

## Event + exit-code contract (handoff-ready)

```
On tls.enabled=true OR auth.spiffe.enabled=true at config load:
  return Err(ConfigError) from RawConfig::into_config
  → main.rs exit-2 arm
  → process exit code 2
  → stderr: level=error event=config_validation_failed reason="<names the requested knob(s)>"
  → NO listener bound on 0.0.0.0:4317 or 0.0.0.0:4318
On both knobs false / [security] absent:
  unchanged — event=startup, event=ready, binds 4317 + 4318, accepts telemetry.
```

## Constraints / risks carried into DELIVER and DISTILL

| Constraint / risk | Note |
|---|---|
| Pre-subscriber `eprintln!` window | When `--config` is given, `main.rs:33-38` catches the loader error before `install_subscriber`. DELIVER must ensure the stderr line is structured and names the knob in that window (same situation the identical-bind-address rejection already faces per `component-design.md:1066`). Observable asserted: exit 2 + stderr line naming the knob + no bound listener. |
| Existing tests/golden assert warn-and-continue | `crates/aperture/tests/slice_07_tls_schema_knob.rs` and any golden asserting `tls_not_supported_in_v0` warn + bound listener encode the superseded contract. DELIVER updates them. (DISCUSS risk table, high.) |
| Embedder depends on unconditional bind | `gateway` or any embedder setting the knob unconditionally would now fail to start. Reuse/negative-control confirms no embedder sets the knob; the negative-control scenario guards the non-regression. (DISCUSS risk table.) |
| Per-feature mutation 100% (ADR-0005 Gate 5) | The two-knob truth table (3 refusal rows) + 2 negative controls give the kill coverage for the new reject branch. |
| No external integration | No third-party API / webhook / OAuth. No contract-test recommendation. |

## Artifacts produced (DESIGN)

- `docs/product/architecture/adr-0061-aperture-refuse-unimplemented-security-knob.md` (new ADR).
- `docs/product/architecture/adr-0008-aperture-configuration-schema.md` (`Superseded by` header updated).
- `docs/product/architecture/brief.md` (new `## Application Architecture — tls-config-reject-v0` section + "For Acceptance Designer" note).
- `docs/feature/tls-config-reject-v0/design/wave-decisions.md` (this file).

## Peer review

`nw-solution-architect-reviewer` could not be dispatched from this subagent context. A rigorous
structured self-review was performed instead (see "Self-review" below). **A top-level
`@nw-solution-architect-reviewer` run is recommended** before DISTILL.

### Self-review (nw-sa-critique-dimensions)

```yaml
review_id: "arch_rev_self_tls-config-reject-v0"
reviewer: "nw-solution-architect (self-review; top-level reviewer recommended)"
artifact: "adr-0061, adr-0008 (header), brief.md (tls-config-reject-v0 section)"
iteration: 1

strengths:
  - "Refusal seam (config validation) gives a STRUCTURAL no-bind guarantee (Config-never-constructed), strictly stronger than ordering discipline inside spawn (ADR-0061 D1 / Option A vs B)."
  - "Zero new architectural surface: reuses ConfigError, main.rs exit-2 arm, config_validation_failed event, exit code 2 (Reuse Analysis). The feature is one validation branch."
  - "Supersession scope is surgical and explicit: runtime reaction superseded, forward-compat schema preserved, with line-cited ADR-0008 clauses and a header update."
  - "Event choice resolves a real ambiguity (config_validation_failed vs health.startup.refused) on a principled axis — config-is-wrong vs dependency-lied — that the rest of the codebase already honours."

issues_identified:
  architectural_bias:
    - issue: "No technology-preference / resume-driven / latest-tech bias — no new tech introduced; the change reuses existing seams."
      severity: "none"
  decision_quality:
    - issue: "ADR-0061 carries 4 alternatives (A accepted, B/C/D rejected with rationale), context, consequences (incl. the breaking-behaviour negative), and ATAM. Meets the min-2-alternatives bar."
      severity: "none"
    - issue: "Pre-subscriber eprintln! window is a real seam where the structured-event AC could be under-delivered if DELIVER prints an unstructured line."
      severity: "medium"
      location: "ADR-0061 §Refusal event caveat; wave-decisions constraints"
      recommendation: "Flagged for DELIVER: the stderr line MUST name the knob and carry event=config_validation_failed even in the pre-init window. The identical-bind-address rejection already faces this; DISTILL asserts the observable (knob named + exit 2 + no bind), leaving the JSON-vs-structured-eprintln mechanism to DELIVER."
  completeness_gaps:
    - issue: "Security quality attribute (confidentiality/authenticity) is the driving attribute and is addressed (fail-closed refusal). Availability trade-off is named in the ATAM. Observability addressed (named event + exit code). No performance attribute is relevant to a startup config check."
      severity: "none"
  implementation_feasibility:
    - issue: "Testability: every AC is black-box (exit code, stderr event, listener presence). The negative controls and two-knob truth table are directly expressible against the existing from_toml_str / binary seams. Strong."
      severity: "none"
  priority_validation:
    q1_largest_bottleneck:
      evidence: "Verifier issue 008 (HIGH, security) re-verified in code 2026-06-04; warn-and-continue ships plaintext in 100% of security-knob-set startups (baseline 0.0)."
      assessment: "YES"
    q2_simple_alternatives:
      assessment: "ADEQUATE — Option C (comment-fix-only, the simplest) explicitly considered and rejected because it leaves the HIGH security defect open."
    q3_constraint_prioritization:
      assessment: "CORRECT — the fix is one validation branch (small solution) for a HIGH-severity security defect (large problem); not inverted."
    q4_data_justified:
      assessment: "JUSTIFIED — outcome KPI baseline 0.0 / target 1.0 with the truth-table + negative-control measurement plan."

approval_status: "conditionally_approved"
critical_issues_count: 0
high_issues_count: 0
medium_issues_count: 1
note: "Single MEDIUM (pre-subscriber structured-event window) is a DELIVER mechanism flag, not an architecture gap. Top-level nw-solution-architect-reviewer run recommended before DISTILL."
```

## Handoff to DISTILL

Recipient: `nw-acceptance-designer`. Driving port = `aperture --config <path>` (or the in-process
`Config::from_toml_str` / `from_toml_path` seam the existing slice tests use). Observables per AC
are enumerated in `brief.md > For Acceptance Designer — tls-config-reject-v0`. Do NOT proceed into
DEVOPS/DISTILL here; this wave produced specs and the ADR only.
