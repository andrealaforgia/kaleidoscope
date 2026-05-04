# Wave Decisions — `otlp-conformance-harness-v0` (DESIGN)

> **Wave**: DESIGN (`nw-solution-architect` / Morgan).
> **Date**: 2026-05-03.
> **Mode**: Propose (Decision 1 of `/nw-design`).
> **Author**: Morgan.
> **Companion documents**: `../../../product/architecture/brief.md` (application-architecture section), `../../../product/architecture/adr-0001..0005-*.md` (one ADR per load-bearing decision).

---

## Inherited decisions (recorded for posterity, not re-derived)

The platform-level architecture is laid in `docs/architecture/kaleidoscope-architecture.md` and the implementation roadmap in `docs/roadmap/kaleidoscope-implementation-roadmap.md`. The following are inherited from those documents and from the DISCUSS wave:

- **Licence**: CC0-1.0 (DISCUSS US System Constraint 6, roadmap A).
- **Substrate exemption**: `opentelemetry-proto` (Apache-2.0) is on the substrate boundary, exempt from port-and-adapter discipline (architecture stratum diagram).
- **No telemetry from telemetry**: harness emits nothing on stdout/stderr/logging facades (roadmap A.2; DISCUSS US System Constraint 4).
- **Library not service** (DISCUSS D1).
- **Three locked function signatures** (DISCUSS US-06 AC 5, line 583 of `user-stories.md`).
- **No type shadowing** (DISCUSS US-04 AC 2).
- **Closed rule set** (DISCUSS US System Constraint 3, W2).
- **Signal-type asserted not inferred** (DISCUSS US System Constraint 7, W3).

DESIGN does not re-litigate any of the above.

---

## Load-bearing decisions made in DESIGN

### D1. Public API surface and crate layout

**Decision**: Free `pub fn`s in `lib.rs`, internal modules from day one, no `Validator` struct.
**ADR**: [ADR-0001](../../../product/architecture/adr-0001-public-api-surface-and-crate-layout.md).

Three options were considered:
- (A) Free functions, internal modules — **recommended and accepted**.
- (B) Methods on a `Validator` struct — rejected (dead-weight constructor; diverges from the locked function signatures).
- (C) Methods on a `&Harness` builder — rejected (premature configurability; threads through every call site).
- (D) Single-file flat `lib.rs` — rejected (~600 lines covering five concerns by US-07; refactor cost later).

The choice optimises for **Functional Suitability — Appropriateness** (the call shape every consumer wants) and for **Maintainability — Modularity** (modules align with conceptual boundaries from day one). It does not regress US-06 AC 5's locked signatures or US-04 AC 2's type-path identity contract.

### D2. `OtlpViolation` error-type design

**Decision**: Nested `Rule::WireType(WireTypeRule)` enum, `#[non_exhaustive]` everywhere, `std::error::Error` impl with single-line `Display`, `prost::DecodeError` wrapped via `source()` chain (crate-private field, accessed only through the trait).
**ADR**: [ADR-0002](../../../product/architecture/adr-0002-otlp-violation-error-type-design.md).

Five options were considered:
- (A) Nested + `#[non_exhaustive]` + `Error`/`Display`/`Debug` + `source` chain — **recommended and accepted**.
- (B) Flat enum (no rule families) — rejected (diverges from user-stories naming; pollutes namespace).
- (C) Trait-object error type — rejected (defeats pattern matching; user-story signatures are concrete).
- (D) `#[non_exhaustive]` only on enums — rejected (struct evolution becomes a major bump).
- (E) No `source` chain — rejected (loses prost diagnostics; small win for big information loss).

The choice optimises for **Maintainability — Modifiability** (additive evolution non-breaking) and **Functional Suitability — Correctness** (pattern matching is exhaustive within the closed rule set, with `#[non_exhaustive]`'s opt-in escape hatch).

### D3. `opentelemetry-proto` dependency pinning policy

**Decision**: Exact-version pin (`opentelemetry-proto = "=0.27.0"`) for v0, with documented escalation paths to caret-pin (v0.x) and to vendored protos (v1+).
**ADR**: [ADR-0003](../../../product/architecture/adr-0003-opentelemetry-proto-pinning-policy.md).

Four options were considered:
- (A) Exact pin (`=0.27.0`) — **recommended and accepted**.
- (B) Caret pin (`^0.27`) — rejected for v0 (no corpus net during slices 01–06; revisit at v0.x once US-07 ships).
- (C) Vendored protos with in-tree codegen — rejected (violates US-04 AC 2's type-path identity contract; reserved as a v1+ break-glass option).
- (D) No pin — rejected outright (non-reproducible builds).

The choice optimises for **Reliability — Maturity** (zero version drift in v0) and is enforced by `cargo deny` in Gate 4 (ADR-0005). The escalation path is documented so future maintainers see the decision history.

### D4. Conformance test-vector layout and corpus runner contract

**Decision**: Two-level hierarchy `tests/vectors/{signal}/{verdict}/{vector}.{bin,expected.json}`, sibling `.expected.json` per `.bin`, SHA-256 hex content hash, recursive filesystem walk by the corpus runner, hand-checked-in vectors with optional capture program (`examples/capture_corpus_vectors.rs`) for accept vectors.
**ADR**: [ADR-0004](../../../product/architecture/adr-0004-conformance-test-vector-layout.md).

Five options were considered:
- (A) `{signal}/{verdict}/` two-level hierarchy with descriptor-driven walk — **recommended and accepted**.
- (B) Flat hierarchy with descriptor-driven everything — rejected (conflicts with user-story paths; ugly filename parsing).
- (C) Per-rule subdirectories under each signal/verdict — rejected (path coupling; reserved as future evolution if corpus grows >50 vectors).
- (D) Single `manifest.toml` declaring every vector — rejected (duplicate source of truth; user stories use sibling-`.expected.json`).
- (E) Auto-generation of accept vectors on every test run — rejected outright (defeats the corpus's contract; non-reproducible).

The choice optimises for **Functional Suitability — Correctness** (the corpus is the regression net for KPI 1) and for **Maintainability — Modifiability** (descriptor-driven means rule renames are one-spot edits).

### D5. CI contract (mechanism deferred to DEVOPS)

**Decision**: Five blocking gates on every commit affecting `crates/otlp-conformance-harness/**`:
1. `cargo test --all-targets --locked` (test suite + corpus runner).
2. `cargo public-api` (public-surface diff).
3. `cargo semver-checks` (SemVer compatibility).
4. `cargo deny check` (licence + dependency policy).
5. `cargo mutants` (mutation testing, 100% caught for v0).

**ADR**: [ADR-0005](../../../product/architecture/adr-0005-ci-contract.md).

Five options were considered:
- (A) Five gates — **recommended and accepted**.
- (B) Three gates (`test`, `deny`, `public-api`) — rejected (misses SemVer + mutation testing).
- (C) `cargo test` only — rejected (misses every cross-cutting concern).
- (D) Add `cargo-audit` as a sixth gate — rejected (redundant with `cargo deny check`'s `advisories` table).
- (E) Add `clippy` as a sixth gate — rejected (style enforcement, not contract).

The mechanism (workflow runner: GitHub Actions, Gitea Actions, etc.) is **explicitly deferred to DEVOPS**, per US-07's technical notes. The contract is runner-agnostic.

---

## Reuse Analysis (HARD GATE)

### Components of Kaleidoscope reuse

| Existing Component | File | Overlap | Decision | Justification |
|---|---|---|---|---|
| (none — greenfield repository) | — | — | — | The Kaleidoscope repository contains no Rust code prior to this feature. The `otlp-conformance-harness` crate is the first Rust crate in the project. There are no in-house components to extend. |

The Reuse Analysis table is empty by structural fact, not by oversight. Acknowledged honestly.

### Upstream FOSS libraries embedded as substrate

These are **not Kaleidoscope-component reuse** — they are substrate-level dependencies already established by the architecture document. Listed here for completeness and for the DELIVER wave's `Cargo.toml` reference:

| Crate | Version | Licence | Role | ADR |
|---|---|---|---|---|
| `opentelemetry-proto` | `=0.27.0` | Apache-2.0 | OTLP message types and prost decoders | ADR-0003 |
| `prost` | (transitive via `opentelemetry-proto`) | Apache-2.0 | Protobuf decoding; `prost::DecodeError` wrapped per ADR-0002 | ADR-0002 |
| `sha2` | `^0.10` | MIT/Apache-2.0 | SHA-256 for corpus content hashing (used in the corpus runner only, `[dev-dependencies]`) | ADR-0004 |
| `serde` + `serde_json` | `^1.0` each | MIT/Apache-2.0 | `.expected.json` descriptor parsing in the corpus runner (`[dev-dependencies]`) | ADR-0004 |

Capture program (`examples/capture_corpus_vectors.rs`) additionally depends on the OpenTelemetry Rust SDK, isolated to the example so it does not pollute the harness's runtime dependency tree.

CI tooling (not crate dependencies):

| Tool | Licence | Role | ADR |
|---|---|---|---|
| `cargo-public-api` | MIT/Apache-2.0 | Public-surface diffing | ADR-0005 |
| `cargo-semver-checks` | MIT/Apache-2.0 | SemVer compatibility | ADR-0005 |
| `cargo-deny` | MIT/Apache-2.0 | Licence + dependency policy | ADR-0005 |
| `cargo-mutants` | MIT/Apache-2.0 | Mutation testing | ADR-0005 |

All open-source, all well-maintained, all on permissive (MIT or Apache-2.0) licences. None on the disqualified-licence list (roadmap A.1).

---

## Technology Stack

| Layer | Choice | Licence | Rationale |
|---|---|---|---|
| Language | Rust (stable, MSRV TBD by DEVOPS via `rust-toolchain.toml`) | dual MIT/Apache-2.0 | Roadmap C: Rust is the chosen language for storage-plane components and conformance work. The harness is the first crate in the family. |
| Paradigm | Idiomatic Rust: data + free functions + traits where polymorphism is required | n/a | Recommended in `brief.md > Paradigm`. Matches `serde_json`/`prost`/`regex` ergonomics for stateless validation libraries. |
| Build system | Cargo | MIT/Apache-2.0 (rustc) | The Rust ecosystem standard. Workspace-ready for future Phase-0 crates. |
| Protobuf decoder | `prost` (transitive via `opentelemetry-proto`) | Apache-2.0 | The upstream-chosen decoder. Substrate per the architecture document. |
| OTLP types | `opentelemetry-proto` v0.27.0 (exact pin) | Apache-2.0 | The substrate-level OTLP type source. Pinning policy in ADR-0003. |
| Hashing (corpus) | `sha2` | MIT/Apache-2.0 | SHA-256 chosen by US-07 technical notes; widely used, audited, no special properties needed beyond collision resistance. |
| Descriptor format | JSON (`serde_json`) | MIT/Apache-2.0 | Named in user stories; readable; ubiquitous. |
| Test framework | Cargo's built-in `#[test]` | n/a | The Rust ecosystem standard; the corpus runner is one integration test. |
| Bench framework | `criterion` (slice 07 only) | MIT/Apache-2.0 | KPI 7 informational benchmark. Standard Rust microbenchmark tool. |
| Public-surface lint | `cargo-public-api` | MIT/Apache-2.0 | ADR-0005 Gate 2. |
| SemVer lint | `cargo-semver-checks` | MIT/Apache-2.0 | ADR-0005 Gate 3. |
| Dependency policy | `cargo-deny` | MIT/Apache-2.0 | ADR-0005 Gate 4. |
| Mutation testing | `cargo-mutants` | MIT/Apache-2.0 | ADR-0005 Gate 5. |

No proprietary technology. No technology with a disqualified licence. Every choice has a documented rationale, an ADR or a roadmap reference, and an alternative-considered list (in the relevant ADR).

---

## Quality Attribute Coverage (ISO 25010)

Summary table; full discussion in `brief.md > Quality attributes addressed`:

| Attribute | Mechanism |
|---|---|
| Functional Suitability — Correctness | Closed-rule discipline; corpus runner with hash + rule-coverage checks (US-07); five CI gates (ADR-0005). |
| Performance Efficiency | Synchronous, no I/O, single decode + at most two fallback decodes on signal-mismatch failure. KPI 7 informational benchmark. |
| Compatibility — Interoperability | Upstream `opentelemetry-proto` types returned unchanged on the accept path (US-04 AC 2). |
| Reliability — Maturity | No internal state; no I/O; no panics on user input (US System Constraint 5). Mutation testing (ADR-0005 Gate 5) defends test-suite quality. |
| Security — Integrity | `EmptyInput`, `ProtobufDecode`, `SignalMismatch` shield downstream from confused-deputy / cross-signal pollution. |
| Maintainability — Modularity, Testability | Internal module split; corpus runner; mutation testing. |
| Maintainability — Modifiability | `#[non_exhaustive]` on every public enum + struct; additive evolution non-breaking (ADR-0002). |
| Portability | Pure Rust, no platform-specific code, no `unsafe`. |

ATAM trade-off summary: `#[non_exhaustive]` introduces a **Modifiability vs Operability-for-the-Consumer** trade-off (ADR-0002), correctly biased toward modifiability because the user stories explicitly anticipate additive evolution.

---

## Conway's Law check

Single-author crate built by a single AI agent. Modular split is for readability and audit, not parallel development. Conway's Law satisfied trivially.

---

## Earned Trust (Principle 12)

The harness has no runtime ports — it is an in-process pure function. The only dependency-on-the-world is `opentelemetry-proto` actually decoding as documented at the pinned version. The corpus runner **is** the probe contract:

1. Decodes every accept vector and asserts `Ok(_)` (probe of the substrate's accept-path semantics).
2. Decodes every reject vector and asserts the declared rule (probe of the substrate's reject-path semantics).
3. SHA-256 verification before each validation (probe that the corpus itself has not drifted).
4. Static `Rule` enumeration (probe that every rule has at least one defending vector).

The three Earned-Trust enforcement layers reduce to two for a pure-function leaf:

- **Subtype layer**: degenerate (no traits to check at composition root).
- **Structural layer**: `cargo public-api` (Gate 2) + `cargo semver-checks` (Gate 3) catch signature drift at compile/CI time.
- **Behavioural layer**: the corpus runner (Gate 1) and mutation testing (Gate 5) catch behavioural drift.

For environments-known-to-lie: the reject vectors `bad_varint.bin`, `bad_tag.bin`, `truncated.bin` **are** the catalogued substrate lies — bytes that look reasonable but that `prost` must refuse, asserted to fail with the harness's `ProtobufDecode` rule. KPI 6 is the structural enforcement that this catalogue grows with the rule set.

---

## Architectural rule enforcement (Principle 11)

| Rule | Enforcement mechanism |
|---|---|
| `opentelemetry-proto` exactly pinned | `cargo deny check` (Gate 4) |
| Public surface stable across non-version-bump commits | `cargo public-api` (Gate 2) |
| SemVer correctness on version bumps | `cargo semver-checks` (Gate 3) |
| No disqualified licences | `cargo deny check` (Gate 4) |
| Test suite quality (mutation resistance) | `cargo mutants` (Gate 5) |
| Type-path identity (US-04 AC 2) | `cargo public-api` (Gate 2) — public function signatures contain return-type paths |
| No telemetry from the harness | Hand-written integration test (per `shared-artifacts-registry.md > CI invariants`) capturing stdout/stderr/log writes; runs as part of Gate 1 |
| Closed-rule discipline | Static `Rule` enumeration in the corpus runner (US-07 AC 4) — Gate 1 |
| Corpus integrity (hash check) | Corpus runner SHA-256 verification — Gate 1 |

Every architectural rule has a language-appropriate automated enforcement tool. No rule is enforced only by convention.

---

## Risks and mitigations

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| `prost::DecodeError` does not provide useful byte locus for some inputs | Medium | Low | DISCUSS already mitigates via `ByteOffset::Unknown` (US-02 technical notes). DESIGN preserves this via the `ByteOffset` enum (ADR-0002 / ADR-0001). |
| Upstream `opentelemetry-proto` cuts a breaking change before Phase 1 | Low | Medium | Exact pin in ADR-0003. Bumping the pin is a deliberate maintainer action with corpus revalidation. |
| `cargo-mutants` runtime exceeds tolerable CI budget | Medium | Low | ADR-0005 risk register: scope mutations to source modules, then to changed-files-only, in escalation order. |
| `cargo public-api` produces noisy diffs on nightly | Low | Low | ADR-0005: pin to stable Rust toolchain in CI. |
| New consumers of the harness pin a different `opentelemetry-proto` version | Medium | High | Workspace-level `cargo metadata` consistency check (deferred per `shared-artifacts-registry.md > otlp_wire_format`); v0 has only the harness as consumer, so risk is dormant until Phase 1. Brief.md flags this for DEVOPS attention. |
| `OtlpViolation`'s `source` field leaks the prost type via `downcast_ref` | Low | Low | This is the standard `std::error::Error` escape hatch and is intentional. Documented in ADR-0002. |

---

## Back-propagation to DISCUSS (`upstream-changes.md`)

**None required.** No DESIGN-driven need to change a story or AC was discovered. Every load-bearing decision sits within the latitude DISCUSS explicitly granted to DESIGN.

A `docs/feature/otlp-conformance-harness-v0/design/upstream-changes.md` file is **not** created (per the SA skill's back-propagation rule: only create if a change is needed).

---

## Handoff to DISTILL

Recipient: `nw-acceptance-designer`. Required inputs:

1. This file (`design/wave-decisions.md`).
2. `docs/product/architecture/brief.md` — application-architecture section.
3. ADRs 0001–0005.
4. The DISCUSS artefacts (locked, do not modify).

The DISTILL wave turns the BDD scenarios in `discuss/user-stories.md` and `discuss/journey-validate-otlp-bytes.yaml` into executable Cargo acceptance tests against the public surface defined in ADR-0001. The function signatures, the violation type shape, and the corpus layout are all locked at the level of detail the DISTILL wave needs.

The DISTILL wave does **not** need:
- The CI contract (DEVOPS receives that).
- The dependency-pinning policy (relevant only at `Cargo.toml` time, owned by DELIVER's `nw-software-crafter`).

## Handoff to DEVOPS

Recipient: `nw-platform-architect`. Required inputs:

1. `discuss/outcome-kpis.md` — the seven KPIs and measurement plans.
2. ADR-0005 — the CI contract (five gates, runner-agnostic).
3. ADR-0003 — the dependency-pinning policy enforced by `cargo deny` in Gate 4.
4. The recommended `deny.toml` excerpt in ADR-0005.

DEVOPS chooses the workflow runner, writes the runner-specific YAML, and configures caching and triggering. No external integrations exist; **no contract-test recommendations apply for v0**.

---

## DESIGN-wave summary

- 5 load-bearing decisions, each with an ADR, each with 2–5 options enumerated and one recommended.
- 0 platform-level decisions re-litigated.
- 0 changes back-propagated to DISCUSS (locked scope honoured).
- 5 CI gates, all open-source, all runner-agnostic.
- 4 substrate dependencies (`opentelemetry-proto`, `prost` (transitive), `sha2`, `serde`/`serde_json`), all Apache-2.0 or permissive.
- 0 external runtime integrations, hence 0 contract-test recommendations.
- 0 components-of-Kaleidoscope reused (greenfield).

DESIGN was a tight pass, as the iteration-1 reviewer (Luna) anticipated in `peer-review-iteration-1.md`. The architectural ground was laid in the four prior documents; DESIGN's job was to crystallise the crate-level shape without re-deriving the platform-level posture. Done.
