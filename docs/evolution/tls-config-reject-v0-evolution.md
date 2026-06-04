# Evolution archive — tls-config-reject-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md` and
`store-fsync-durability-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0061
  (`docs/product/architecture/adr-0061-aperture-refuse-unimplemented-security-knob.md`),
  surgically superseding the runtime reaction of ADR-0008
  (`docs/product/architecture/adr-0008-aperture-configuration-schema.md`)
  for the two security knobs only.
- Closes: verifier four-quadrants issue 008 (HIGH, security axis).

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| discuss | `c0ddf60` | refuse to start, do not ship plaintext by promise |
| design | `7fae0a1` | ADR-0061, refuse at config validation, structural no-bind |
| devops | `766b0ec` | slim wave, existing aperture gate covers, deterministic refusal test |
| distill | `ea72f1e` | refusal acceptance tests, RED-ready (slice_09, 11 scenarios) |
| feat | `a56c317` | aperture refuse to start on tls.enabled / auth.spiffe.enabled instead of binding plaintext |
| docs | `afbc44e` | narrative + slide closure |

## The problem, in Earned-Trust framing

aperture parsed two forward-compat security knobs, `tls.enabled` and
`auth.spiffe.enabled`. v0 implements neither transport encryption nor
SPIFFE authentication. The ADR-0008 reaction when an operator set either
to true was warn-and-ignore: aperture logged a single warning
(`event=tls_not_supported_in_v0`) and then bound PLAINTEXT anyway, while a
sink comment at `crates/aperture/src/.../sinks.rs:94-95` falsely claimed
the configuration validator rejected the knob.

This is a silent security downgrade, and the most dangerous shape of
substrate lie the project exists to forbid. An operator who writes
`tls.enabled = true` into their configuration believes they have
transport encryption. They have plaintext. The warning rides a log stream
they may never read; the bind succeeds; traffic flows in the clear under
a belief that it does not. The false sink comment compounded the lie by
asserting a rejection that never happened. Of the four quadrants this was
the HIGH-security finding (issue 008): a config knob whose presence
promises a security property the binary does not deliver and does not
refuse.

This feature extends the project's fail-closed posture to the security
knobs. It is the same principle that drives `deny_unknown_fields` on the
config schema (an unrecognised key is refused, not silently dropped),
fail-closed tenancy (an unresolved tenant denies, it does not fall
through), and the no-lying-downstream rule (a store does not ack a write
it has not persisted). Earned-Trust says a component must not claim a
property it does not hold. A binary that accepts `tls.enabled = true` and
binds plaintext claims encryption it does not hold. The fix makes the
claim true the only honest way available at v0: by refusing to hold it at
all, loudly, before any listener exists.

## The architecture decision

### Refuse in into_config, so the no-plaintext-bind guarantee is structural

The reject lands in `RawConfig::into_config`, which returns
`Err(ConfigError)` naming the offending knob BEFORE any `Config` value is
constructed. Because the bind path is reachable only from a constructed
`Config`, and the refusal short-circuits construction, the bind path is
never entered on a true security knob. The no-plaintext-bind guarantee is
therefore STRUCTURAL, not behavioural: it is not that the bind code
checks a flag and declines, it is that the bind code is unreachable in
the rejecting case. There is no plaintext-bind branch to get wrong, no
flag to read in the wrong order, no later edit that can reintroduce the
downgrade without first reaching past the `into_config` seam. The
strongest form of a security guarantee is one the type-and-control-flow
shape makes unrepresentable, and that is what placing the refusal at
`into_config` buys.

This is the Rust-idiomatic shape per CLAUDE.md: a fallible constructor
(`into_config -> Result<Config, ConfigError>`) is the validation seam, and
an invalid configuration simply has no `Config` value, so the rest of the
program cannot proceed on it. Data plus a free conversion function, no
inheritance, no `dyn`.

### config_validation_failed vs health.startup.refused: two distinct axes

`main.rs` emits a structured stderr line `event=config_validation_failed`
naming the knob, and exits with code 2. This event was deliberately kept
distinct from the existing runtime `health.startup.refused` event. The two
sit on different axes:

- `config_validation_failed` is the config-is-wrong axis. The operator
  asked for something the binary cannot honestly provide; the
  configuration itself is the fault, and the binary refuses before it
  starts doing work.
- `health.startup.refused` is the substrate-lied axis. The configuration
  was acceptable, but a runtime probe (for example the fsync probe)
  detected that the substrate does not honour a promised property, so the
  binary refuses after construction but before serving.

Collapsing them into one event would blur the operator's diagnosis: the
remedy for `config_validation_failed` is edit the config, the remedy for
`health.startup.refused` is fix the substrate. Distinct events keep the
two remedies distinct. Exit code 2 is the existing config-error exit code;
no new code was minted.

### Reuse, not invention: one new validation branch, nothing else

The feature adds exactly one new validation branch inside `into_config`
and nothing else of structure. No new error type (it reuses
`ConfigError`), no new event family (it reuses the structured stderr line
shape already carried by the identical-bind-address rejection), no new
exit code (it reuses exit 2), no new file. The entire change is one branch
that converts a true security knob into a named `Err(ConfigError)`, plus
the `main.rs` emission of the already-shaped stderr line. This restraint
is the point: the cheapest correct fix that closes a HIGH security finding
is one branch placed at the one seam where it makes the guarantee
structural, reusing every surrounding mechanism the codebase already owns.

### The surgical ADR-0008 supersession

ADR-0061 supersedes ONLY ADR-0008's warn-and-ignore RUNTIME REACTION for
these two knobs. ADR-0008's forward-compat SCHEMA decision is preserved
intact and stands: the `tls` and `auth.spiffe` keys remain present in the
v0 schema, default off, so a Phase-2 config that sets them does not break
to parse and a v0 operator who leaves them unset sees no change. The only
thing that changed is what happens when they are set true: warn-and-bind
became refuse-and-exit. ADR-0008's `Status` stays `Accepted`; its
`Superseded by` header was edited to record `ADR-0061 — runtime reaction
only`, explicitly noting that the schema decision is NOT superseded. This
is a precision edit, not a wholesale supersession: the forward-compat
schema that lets a later phase add real TLS without a config break is
exactly what makes the v0 refusal a temporary honest stance rather than a
dead end.

## Verification

- slice_09, 11 acceptance scenarios, all real local I/O. Six are refusal
  scenarios driving the two-knob truth table (the rejecting rows for
  `tls.enabled = true` and `auth.spiffe.enabled = true`, alone and in
  combination), plus negative controls that pin the tolerance narrow: a
  default config (both knobs off or absent) starts and binds normally, and
  `enabled = false` is explicitly distinguished from `enabled = true` so
  the refusal does not over-fire on the off case. The negatives are
  co-equal with the positives: a refusal that also rejected a valid
  default config would be strictly worse than the prior behaviour.
- The structural no-bind is asserted through two complementary seams. At
  the unit seam, `into_config` returns `Err(ConfigError)` naming the knob,
  proving construction is refused before any `Config` exists. At the
  binary seam, the COMPILED aperture binary is launched as a child process
  on a rejecting config, and the test asserts exit code 2, the
  `event=config_validation_failed` stderr line naming the knob, and a
  connect-refused on the port aperture would otherwise have bound. The
  connect-refused is the operator-visible proof that no plaintext listener
  came up: there is nothing to connect to.
- slice_07, the prior superseded contract, was FLIPPED. slice_07 had
  asserted the warn-and-continue contract (one warn line, then a bound
  plaintext listener); it was rewritten to assert the refusal contract, so
  the old behaviour is no longer pinned green anywhere. A superseded
  contract left asserted would keep the lie alive in the test suite.
- The false sink comment at `sinks.rs:94-95`, which claimed the validator
  rejected the knob, was corrected to describe the actual behaviour now
  that the actual behaviour matches what the comment had falsely claimed.
  A project whose thesis is structural honesty must not ship a comment
  asserting a rejection the code did not perform; the comment is now true
  because the code now does what it said.
- 100% mutation kill on the reject branch (ADR-0005 Gate 5; CLAUDE.md
  per-feature 100%). The existing `gate-5-mutants-aperture` `--in-diff`
  job (ci.yml lines 505-604, path-filtered on `crates/aperture/**`,
  `--package aperture`) picks up the new `into_config` branch from the
  diff automatically; the two-knob truth table refusal rows plus the
  negative controls kill the mutants. No new CI job was needed.
- Gate 1 (cargo test --workspace) and Gate 4 (cargo deny) auto-cover the
  change. The positive-bind negative control runs on the ephemeral
  `127.0.0.1:0` seam the codebase already provides, so it carries no
  fixed-port collision risk in the parallel suite.

## Process note: the DEVOPS reviewer wave-order rejection (methodology clarification, not a defect)

This is recorded for honesty about the delivery, in the same spirit as the
prior archives' honest-finding sections. During DEVOPS, the independent
top-level `nw-platform-architect-reviewer` returned REJECTED. The
rejection rested solely on a wave-ORDER misunderstanding: the reviewer
applied the conventional code-then-CI sequence and reasoned that because
the refusal code and tests did not yet exist, DEVOPS was reviewing a
feature out of order.

That is backwards for nWave. The wave order is
DISCUSS -> DESIGN -> DEVOPS -> DISTILL -> DELIVER. nWave positions DEVOPS
BEFORE DISTILL and DELIVER precisely so the delivery infrastructure is
proven ready before the acceptance tests and the production code exist.
Absent code is therefore the EXPECTED, correct state at the DEVOPS wave,
not a defect.

Every substantive CI check the reviewer actually ran PASSED, and it listed
them as strengths: `gate-5-mutants-aperture` exists and is `--in-diff`
path-filtered on `crates/aperture/**`; gate-1-test and gate-4-deny
auto-cover; no new CI job is needed; the refusal subprocess test is
deterministic (exit code plus stderr grep plus connect-refused, no
wall-clock or p95 dependency); and the fixed-port caveat for the
positive-bind control is correctly mitigated via the ephemeral
`127.0.0.1:0` seam. The reviewer's "recommendations" were the same
DISTILL/DELIVER handoff items already recorded in the DEVOPS constraints
table.

The orchestrator OVERRODE the rejection as a methodology error rather than
a defect, and documented the override in the DEVOPS wave-decisions
(`docs/feature/tls-config-reject-v0/devops/wave-decisions.md`, "Peer
review outcome"). The DEVOPS wave was sound and approved on its merits.
This is recorded here as a methodology-clarification note, NOT as a defect
in the feature or the reviewer: it is a reminder that an independent
reviewer carrying the conventional code-then-CI mental model will read an
nWave DEVOPS wave as out of order, and that the correct response is to
check whether every substantive verification passed (it did) before
acting on the order objection.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward unchanged; this
feature neither introduced nor closed them.

1. claims-honesty-pass. Stale prose claiming behaviour the shipped code no
   longer matches: loom, spark, strata, cinder cold-tier, harness, codex
   and query-http-common docs; the README "durable" claim; and the
   query-api step prose. Also the `__SCAFFOLD__ ... RED` doc markers still
   present in query-http-common and aperture, which describe shipped
   functions as unimplemented scaffolding. Belongs to the claims-honesty
   pass, not to this feature. Open.

2. cinder-wal-error-surfacing-v0. cinder's `place()` and `evaluate_at()`
   swallow the result of `append_wal` rather than surfacing it
   (`crates/cinder/src/file_backed.rs`, the `if let Err(_e)` and `let _ =`
   sites). A failed durable append on these two paths is silently dropped,
   itself a residual substrate lie now that the append is fsync-honest.
   Tracked as `cinder-wal-error-surfacing-v0`. Open.

3. sluice nack-past-cap. sluice's behaviour when a write is nacked past its
   cap needs its own slice. Open.

4. ADR-0059 Decision 8 layer b, the AST structural check, remains UNWIRED.
   The structural pre-commit check asserting in-scope stores delegate to
   the shared wal-recovery routine and retain no inline replay loop; the
   tool choice was deferred to DELIVER and remains deferred. It is
   feedback, not a gate, consistent with the pure trunk-based,
   no-required-checks posture; when wired it belongs in the local
   pre-commit stage. Carried forward unchanged. Open.

5. aegis unwired. No path authenticates; the aegis surface exists but is
   not on any request path. Open.

6. beacon SLO unreachable. The beacon SLO as specified is not reachable by
   the current implementation. Open.

7. OTLP partial_success never populated. The OTLP `partial_success`
   response field is never populated, so partial-accept signalling is not
   surfaced to clients. Open.

8. verifier issue 009 (CLI non-atomic ingest), accepted and queued for its
   own slice. Open.
