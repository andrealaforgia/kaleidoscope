# Wave Decisions — wal-torn-tail-recovery-v0 (DESIGN)

British English. No em dashes in body.

## Wave: DESIGN (Morgan, nw-solution-architect)

## Autonomous run note

This was an autonomous overnight run in **propose mode**. All interactive
decisions were made by the agent per the standing instruction; no decision
was deferred to the user. The five DISCUSS flags were resolved with
evidence read from the actual code, not assumed. The decisions below are
within authorised scope (DESIGN only; no DEVOPS, no DISTILL).

## Paradigm

Rust idiomatic per CLAUDE.md and the otlp-conformance-harness DESIGN
brief: data + free functions + traits only where polymorphism is genuinely
needed. The shared recovery routine (D-FLAG-4) is a **free function**,
generic over the record type `R: DeserializeOwned` and the caller error
`E`, monomorphised per pillar with **no `dyn Trait`** indirection; the two
closures (`apply`, `on_parse_error`) are the seam that absorbs per-pillar
differences. Composition over inheritance; no class hierarchies (Rust has
none).

## Constraints (encoded, not re-litigated)

- **Brownfield**. The stores, WAL append, snapshot-plus-replay recovery
  (ADR-0040), and fsync honesty (ADR-0049) exist and are unchanged in
  shape. This feature changes only the parse-failure arm of the WAL replay
  loop on `open`, plus one cinder doc.
- **Fail-closed stays the default**. The ONLY newly-tolerated shape is a
  parse failure on the final WAL line when that line has no trailing
  newline. Every other parse failure still returns `PersistenceFailed`.
  Narrowing, not abandonment (AC-5, AC-6).
- **No data fabricated or partially served**. The torn tail is dropped,
  never repaired, never partially decoded.
- **No trait change**. `LogStore`, `TraceStore`, `TieringStore`,
  `MetricStore` byte-identical; Gate 2 (`cargo public-api`) enforces
  (AC-8).
- **Per-feature mutation testing at 100% kill** scoped to modified files
  (ADR-0005 Gate 5; CLAUDE.md).

## Key Decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | Refined recovery contract: tolerate the torn final line ONLY (is-last-line AND no-trailing-newline AND parse-failed), drop it, recover the intact prefix, warn. Every other parse failure stays fail-closed. | Encodes the DISCUSS requirement (US-01, AC-1..AC-7) verbatim. The narrowness is the value; AC-5/AC-6 are the guards. Authored as ADR-0059 Decision 1. |
| D2 | Detection mechanism = trailing-byte inspection (`ends_with_newline`) + line-index-vs-last comparison. The physical trailing byte is the honest discriminator; `BufRead::lines()` hides it. Micro-mechanism (read-whole-file vs one-line read-ahead vs final-byte seek) is the crafter's DELIVER choice, constrained to the three observable conditions. | Resolves FLAG 2. A crash-torn record provably lacks the closing `\n` the append path writes last; a complete-malformed record provably has it. Inferring the tear from `serde_json` error class is fragile (rejected as ADR-0059 alt E). |
| D3 | Warning event = `event="wal.recovery.torn_tail_dropped"`, fields `pillar` (`"lumen"`/`"ray"`/`"cinder"`/`"pulse"`), `line` (1-based, matching the `idx+1` reason-text convention), `dropped_bytes` (byte length of the torn line, excludes the absent newline). `tracing::warn!`, at most once per open. | Resolves FLAG 3. Confirms the DISCUSS-proposed dotted name; pins the three field spellings consistent with the existing `event=...` convention (`health.startup.refused`, `listener_bound`). No new metric or dashboard. |
| D4 | Shared recovery routine in a NEW leaf crate `crates/wal-recovery`, generic over `R`/`E` with `apply` + `on_parse_error` closures. NOT per-pillar copy-paste. | Resolves FLAG 4. Six identical loops (four in-scope, two follow-up); copy-paste guarantees the divergence ADR-0054 ended and ADR-0040 case B warned about. One mutation site for the three guards. See Reuse Analysis. |
| D5 | Scope = lumen + ray + cinder + **pulse** (all four in this slice). sluice + strata explicitly out, tracked as a one-line-closure follow-up. | Resolves FLAG 1. Pulse's `tenant_counts` reseed is a post-loop pass over the rebuilt map, transparent to dropping the torn tail (one fewer `apply_ingest`). No deferral warranted. |
| D6 | ADR-0059 authored: recovery-contract change, narrow-tolerance decision, alternatives (tolerate-all, checksum, length-prefix, per-pillar replication, EOF-detection, read-side repair), Earned-Trust lineage extending ADR-0040/0049/0050. | Resolves FLAG 5. `docs/product/architecture/adr-0059-earned-trust-wal-torn-tail-recovery.md`. ADRs immutable; 0040/0049/0050/0054 referenced, not edited. |
| D7 | cinder module doc (`:36-38`) and `open` doc (`:104-106`) corrected to describe the actual (newly correct) behaviour. | AC-7, K5. The false robustness claim is made true by the code change AND the prose. |
| D8 | Earned-Trust enforced via three orthogonal layers: subtype (generic bound + call-site `cargo check`), structural (AST pre-commit: each in-scope pillar calls the shared routine and retains NO inline parse-or-die loop), behavioural (gold-test exercising the five catalogued substrate lies). | Methodology principle 12 + ADR-0049 precedent. `import-linter` rejected (import-graph only). Self-application: the gold-test probes the routine, the AST layer probes that pillars call it. |

## Reuse Analysis (MANDATORY)

Every store with an overlapping WAL open/replay loop, read directly from
the code. EXTEND/SHARE vs IMPLEMENT-LOCAL decided per store with evidence.

| Store / crate | Open replay loop | Error type | `WalRecord` | Decision | Evidence |
|---|---|---|---|---|---|
| `lumen` `FileBackedLogStore` | `file_backed.rs:107-121` | `LogStoreError` | `Ingest { tenant, records }` | **SHARE** (in-scope) | Identical loop: `reader.lines().enumerate()` → skip empty → `serde_json::from_str` → `PersistenceFailed { reason: "WAL parse error at line N: {e}" }` → `per_tenant.extend`. Post-loop re-sort stays in pillar. |
| `ray` `FileBackedTraceStore` | `file_backed.rs:120-135` | `TraceStoreError` | `Ingest { tenant, spans }` | **SHARE** (in-scope) | Identical loop; differs only in `apply_ingest` (dual-index rebuild) which becomes the `apply` closure body. `sort_all` post-loop stays in pillar. |
| `cinder` `FileBackedTieringStore` | `file_backed.rs:135-163` | `MigrateError` | `Place{..}` / `Migrate{..}` | **SHARE** (in-scope) | Same loop, written as a `match` rather than `map_err` but semantically identical (`PersistenceFailed` on first parse failure). Has the false doc (`:36-38`, `:104-106`) corrected here. `apply_to_entries` becomes `apply`. |
| `pulse` `FileBackedMetricStore` | `file_backed.rs:164-187` | `MetricStoreError` | `Ingest { tenant, metrics }` | **SHARE** (in-scope) | Identical loop. `apply_ingest(&mut series, &mut tenant_counts, .., enforce_cap=false)` becomes the `apply` closure; the post-loop `tenant_counts` reseed (`:189-205`) reads the rebuilt map, NOT the WAL, so dropping the tail is transparent. |
| `sluice` `file_backed` | `file_backed.rs:167-177` | `EnqueueError` | (own enum) | **SHARE-LATER** (out this slice) | Same `reader.lines().enumerate()` → `serde_json::from_str` → `EnqueueError::PersistenceFailed` shape (read, lines 167-177). NUANCE: `apply_record` is FALLIBLE (`?` on line 176), unlike the four in-scope pillars; the SHARE-LATER seam needs a fallible-`apply` variant for sluice. Not in feature scope; tracked follow-up. |
| `strata` `file_backed` | `file_backed.rs:114-122` | `ProfileStoreError` | (own enum) | **SHARE-LATER** (out this slice) | Same shape: `reader.lines().enumerate()` → `serde_json::from_str` → `ProfileStoreError::PersistenceFailed` (grep-confirmed lines 114-122). Not in feature scope; one-line follow-up. |

**Decision: build ONE shared `crates/wal-recovery` free function and have
all four in-scope pillars SHARE it.** The evidence is decisive: six stores,
one loop, four error types and four record enums that differ only in the
record payload and the error constructor. The loop body that contains the
three new guard conditions, the trailing-byte inspection, and the warning
emission is byte-for-byte the part that must NOT drift. Replicating it
four times now and two times later is precisely the copy-paste divergence
ADR-0054 was extracted to end (the read-tier `query-http-common` rule of
three) and the latent-bug-via-copy ADR-0040 case B warned a recovery
copy-paste produces. The per-pillar `WalRecord` and error types are
absorbed by generics plus two closures with no `dyn`. This is the
Rust-idiomatic seam.

## Five flags resolved (one line each)

- **FLAG 1 (pulse scope)**: IN SCOPE this slice. `tenant_counts` reseed is
  a post-loop pass over the rebuilt map, transparent to dropping the torn
  tail; no deferral.
- **FLAG 2 (detection)**: trailing-byte `ends_with_newline` + line-index
  comparison; the physical absent `\n` is the honest tear discriminator,
  not `serde_json` error class.
- **FLAG 3 (warning)**: `event="wal.recovery.torn_tail_dropped"` with
  `pillar`, `line` (1-based), `dropped_bytes`; `tracing::warn!`, at most
  once per open, no new metric.
- **FLAG 4 (factoring)**: ONE shared generic free function in new crate
  `crates/wal-recovery`, SHARE across all four in-scope pillars; per-pillar
  replication rejected.
- **FLAG 5 (ADR)**: ADR-0059 authored extending ADR-0040/0049/0050 lineage,
  with six alternatives considered and the three-layer Earned-Trust
  enforcement specified.

## Upstream changes

- New workspace member crate `crates/wal-recovery` (leaf; depends on
  `serde_json` + `tracing` + `serde`). One more compilation unit; the
  function monomorphises and inlines. Same modest cost ADR-0054 accepted
  for `query-http-common`.
- `tracing = "0.1"` enters the storage-library dependency closure via
  `wal-recovery` (already in `Cargo.lock` via aperture and the read tier,
  per `read-api-tracing-subscriber-v0`; zero resolution churn). Pillars do
  not each add `tracing` directly; they get the warning through the shared
  crate.
- New `gate-5-mutants-wal-recovery` expectation (DEVOPS wires the CI; the
  expectation is annotated here, not assumed).
- No trait change, no WAL format change, no write-path change, no snapshot
  change.

## Handoff

- **To DISTILL (acceptance-designer)**: the driving port for AC-1 is the
  **store reopen path** exercised end-to-end through lumen's
  `GET /api/v1/logs` read (the binary opens `FileBackedLogStore::open` at a
  crashed `pillar_root`, binds, and the query returns the intact prefix).
  The five Gherkin scenarios (three positive, two negative) plus AC-1..AC-10
  hold; verifier D04 intact-prefix path is the headline. See the brief's
  "For Acceptance Designer" note.
- **To DEVOPS (platform-architect)**: new `crates/wal-recovery` member and
  its `gate-5-mutants-wal-recovery` expectation; the WARN event rides the
  existing structured `tracing` stream (no new metric/dashboard at v0). No
  external integration; no contract-test recommendation.
- **To DELIVER (crafter)**: implement the shared free function and rewire
  the four pillars' `open` to call it; correct the cinder docs; the
  detection micro-mechanism is the crafter's choice within D2's observable
  constraints.
- DESIGN only. Does NOT proceed into DEVOPS or DISTILL.

## Peer review

- Intended reviewer: `nw-solution-architect-reviewer` (Atlas). The reviewer
  subagent is **not dispatchable from within this DESIGN subagent context**
  (the Agent tool is unavailable to subagents). Per the methodology a
  structured self-review was performed against the five SA critique
  dimensions with the same rigour, and the two highest-risk claims were
  re-verified against the actual code (pulse `tenant_counts` reseed reads
  the rebuilt `series` map, not the WAL — transparent to dropping the tail;
  sluice/strata carry the identical loop shape, with sluice's `apply_record`
  being fallible). The orchestrator should run `@nw-solution-architect-reviewer`
  on these artefacts at the top level to obtain Atlas's independent YAML.
- **Self-review outcome**: `approved`. 0 critical, 0 high. One low-severity
  refinement surfaced and folded in: sluice's fallible `apply_record` means
  the SHARE-LATER seam needs a fallible-`apply` variant (recorded in the
  Reuse Analysis and ADR-0059 Decision 5). The four in-scope pillars use the
  infallible seam unchanged.
- Priority validation: Q1 YES (the torn tail is the single post-crash WAL
  residue; K1 0%→100%), Q2 ADEQUATE (six alternatives in the ADR), Q3
  CORRECT (the tolerance is narrowed, not widened), Q4 JUSTIFIED (KPI
  baseline read from the four replay loops).
