# Wave Decisions — tls-config-reject-v0 (DISCUSS)

- **Feature ID**: tls-config-reject-v0
- **Wave**: DISCUSS (nWave)
- **Analyst**: Luna (nw-product-owner)
- **Date**: 2026-06-04
- **Mode**: Autonomous overnight run (all decisions made by the agent; no user gating)

## Origin

Four-quadrants assessment, verifier issue **008** (HIGH, security). Re-verified in
code on 2026-06-04. Item #2 on the implementer backlog, after
`store-fsync-durability-v0` (shipped).

## The Defect (verified in code 2026-06-04)

| Evidence | Location | What it shows |
|---|---|---|
| `tls_enabled: bool` parsed from `tls.enabled` TOML knob | `crates/aperture/src/config/mod.rs:56` | The knob exists and is read. |
| Sibling `spiffe_enabled` from `auth.spiffe.enabled` | `crates/aperture/src/config/mod.rs:58` | Second security knob, same shape. |
| `warn_if_v0_security_knob_set` warns and **returns** — no error | `crates/aperture/src/compose.rs:56-76` | When either knob is true, aperture LOGS A WARNING and proceeds. |
| `spawn` calls the warn helper then binds plaintext listeners | `crates/aperture/src/compose.rs:127` | After the warn line, plaintext listeners bind regardless. |
| FALSE comment: "the config validator rejects it ahead of this sink" | `crates/aperture/src/sinks.rs:94-95` | Claims a rejection that does not exist. Contradicted by `compose.rs:127`. |

**Net**: an operator who sets `tls.enabled = true` believing they get transport
encryption gets **plaintext** plus a `warn` log line they may never see. A silent
security downgrade — exactly the failure mode the project's Earned-Trust /
fail-closed posture forbids.

## The Operator Job (JTBD — Earned-Trust / fail-closed framing)

> When I configure transport encryption (or SPIFFE auth) that this version does
> not implement, the collector REFUSES TO START and tells me, instead of silently
> shipping my telemetry in plaintext.

This extends the posture aperture already holds: it refuses malformed config
(`deny_unknown_fields`), refuses a lying downstream (Earned-Trust probe →
`health.startup.refused`), refuses fail-closed on tenancy. The security knobs are
the one place the posture leaked. This feature closes that leak.

## The Fix to Encode (decided — encode as requirement, do not re-litigate)

Aperture REFUSES TO START when `tls.enabled = true` OR `auth.spiffe.enabled = true`,
because v0 implements neither. Reject at config validation / compose time:

- exit non-zero (the established config-error code is **2**; see `main.rs:19-21`);
- emit a structured refusal event naming the unsupported knob, same fail-closed
  shape as the existing `health.startup.refused` / `config_validation_failed`
  events;
- NO plaintext listener binds when a security knob was requested.

The false comment at `sinks.rs:94-95` is corrected to describe the real rejection.

Negative control: `tls.enabled = false` (or absent) and `spiffe.enabled = false`
start normally and bind as today.

## Decisions (autonomous)

| Decision | Value | Rationale |
|---|---|---|
| Feature Type | **Cross-cutting** | Config validation + security posture; affects aperture startup. |
| Walking Skeleton | **No** | Brownfield. Aperture, its config loader, and the listeners already exist. |
| UX research | **Lightweight** | One operator persona starting a misconfigured collector. |
| JTBD | **Recorded above** | Fail-closed security job; grounds all stories. |
| Slice count | **Single thin slice** | Small, sharp behaviour change: refuse-to-start on a requested-but-unimplemented security knob. Not padded. |
| DIVERGE artifacts | **Absent** | No `diverge/recommendation.md` or `job-analysis.md`. JTBD recorded inline here (see above). Noted as accepted: brownfield defect-fix needs no DIVERGE direction-selection. |

## KNOWN UPSTREAM CHANGE FOR DESIGN — ADR-0008 supersession

This feature **supersedes** the warn-and-ignore contract for the TLS and SPIFFE
knobs that **ADR-0008** (`docs/product/architecture/adr-0008-aperture-configuration-schema.md`)
deliberately specified. The superseded clauses are concrete and load-bearing:

| ADR-0008 clause | Location | What it mandates (now superseded for these two knobs) |
|---|---|---|
| "setting any of them to true on v0 emits exactly one warn-level event and continues plaintext" | ADR-0008 line 36 | warn-and-continue |
| `tls.enabled = true` → "One warn line `event=tls_not_supported_in_v0`. Continue plaintext." | ADR-0008 line 164 | warn-and-continue |
| `auth.spiffe.enabled = true` → same warn line, "Continue plaintext." | ADR-0008 line 166 | warn-and-continue |
| US-AP-01 AC (cited in ADR-0008 line 19): "`tls.enabled = true` on v0 MUST emit exactly one `event=tls_not_supported_in_v0` warn line and continue plaintext" | ADR-0008 line 19 | warn-and-continue |

The forward-compat **schema** decision (knobs present at v0, defaulting off, no
schema break into Phase 2 / Aegis) is **NOT** superseded — it stands. Only the
runtime **reaction** to `=true` changes: from warn-and-continue to refuse-to-start.

**Action for DESIGN (solution-architect):** author a new ADR that supersedes the
warn-and-ignore reaction for these two knobs on security grounds (silent downgrade
is worse than refusal), updates ADR-0008's `Superseded by` header, and decides the
exact refusal event constant. The closed v0 vocabulary
(`crates/aperture/src/observability.rs:30-51`) already contains two candidates:
`config_validation_failed` (matches ADR-0008's stated validation event +
exit-code-2 shape) and `health.startup.refused` (the fail-closed model). DESIGN
picks one (or reuses `tls_not_supported_in_v0` re-levelled to error); DISCUSS stays
solution-neutral and asserts only the observable.

## Risks Surfaced (for downstream waves)

| Risk | Prob | Impact | Mitigation |
|---|---|---|---|
| Existing tests/golden output assert the warn-and-continue behaviour (e.g. an integration test expecting `tls_not_supported_in_v0` warn + bound listener) | High | Medium | DESIGN/DELIVER updates those assertions; they encode the superseded contract. Flag for solution-architect. |
| `gateway` (or any embedder of aperture) may depend on aperture binding regardless of config | Low | High | Negative-control scenario (tls disabled → binds) guards the non-regression. DESIGN confirms no embedder sets the knob unconditionally. |
| Per-feature mutation 100% (CLAUDE.md / ADR-0005 Gate 5) on the new reject branch | Medium | Low | The two-knob truth table + negative control give the kill coverage; note for DELIVER. |

## Artifacts Produced

- `docs/feature/tls-config-reject-v0/discuss/user-stories.md`
- `docs/feature/tls-config-reject-v0/discuss/story-map.md`
- `docs/feature/tls-config-reject-v0/discuss/outcome-kpis.md`
- `docs/feature/tls-config-reject-v0/discuss/dor-validation.md`
- `docs/feature/tls-config-reject-v0/discuss/wave-decisions.md` (this file)
