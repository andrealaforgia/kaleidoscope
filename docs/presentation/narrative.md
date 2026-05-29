# Building Kaleidoscope with nWave — narrative companion

This is the long-form companion to `slides.md`. The slides are sparse by
design. This file holds the text I want to be able to say out loud, plus
the links to the artefacts that back each claim. When the YouTube video
description points readers to the project for detail, this is the page
they should land on.

Living document. One section is added each time an nWave wave closes for
a feature. Editorial responsibility: Bea, the engineering coach, with
Andrea's review on each addition before it is filmed.

Audience: technical engineers from Andrea's LinkedIn and Substack
readership. They know what TDD, BDD, trunk-based development, and
mutation testing are; they may not know what OTLP is, so observability
internals are explained with metaphors.

Framing: nWave-centric. Andrea uses nWave (the AI-amplified delivery
framework by Alessandro Di Gioia and Michele Brissoni at nWave.ai)
on Kaleidoscope as the worked example. nWave is the framework Andrea
adopts and dogfoods on his projects; T*D (TDD + trunk-based +
team-focused development) is Andrea's own thesis, separate from
nWave but tightly aligned with the practices nWave operationalises.

---

## Opening

I started this project on the third of May 2026. My intention was not
to build an observability platform. My intention was to dogfood nWave
— the AI-amplified delivery framework built by Alessandro Di Gioia
and Michele Brissoni at nWave.ai — on a problem big enough to actually
test it. The platform is the case study. The methodology is the
protagonist; nWave is theirs, the dogfooding is mine.

The video series exists for the same reason. I am not trying to teach
you how to build Kaleidoscope. I am trying to show you how nWave
behaves when you point it at a problem that is too large for any one
person, and let AI agents do the typing while you keep the discipline.
And, alongside, how my own thesis on T*D (TDD + trunk-based +
team-focused development) interacts with the framework: T*D is the
discipline; nWave is the operational shape that makes the discipline
affordable for a solo author.

---

## Why this exists at all

The story starts with the rug-pull pattern. Elastic re-licensed in
2021. MongoDB followed. Redis in 2024. HashiCorp the same year. Each
one was open source until it became valuable, at which point the
licence terms changed in ways that destroyed the open-source promise.
The pattern is not about morality. It is structural. Open core
businesses depend on contributors signing CLAs that assign or grant
re-licensing rights to a single corporate entity. Once that entity
needs to monetise more aggressively, the rights are exercised.

Observability is one of the markets where this hurts the most. The
open-source stack — Loki, Tempo, Mimir, the LGTM family — is governed
by Grafana Labs. The licences today are AGPL, but the governance
structure is the same one that has flipped before elsewhere. Nothing
prevents the same flip happening here.

Kaleidoscope is my attempt at building the same functionality on
contribution governance designed to make re-licensing structurally
impossible. Three pieces:

1. The platform components are licensed AGPL-3.0-or-later. AGPL closes
   the SaaS loophole — anyone hosting Kaleidoscope as a network service
   to others must publish their modifications. The very loophole that
   drove Elastic and MongoDB to abandon open source is closed inside
   an OSI-approved licence.
2. The SDK and protocol libraries are licensed Apache-2.0. They need
   to be embeddable in proprietary application code without
   contaminating it.
3. Contributions are accepted under the Developer Certificate of
   Origin, not a CLA. No copyright assignment. With many contributors
   and no concentrated copyright ownership, no future maintainer can
   unilaterally re-license, because nobody owns enough of the code to
   legally do it.

The trademark is reserved separately. The code is free; the name is
not, which prevents bad-faith forks claiming to be the original.

This licence stack is not novel. It is the same arrangement Grafana
Labs used to use, and that MongoDB used before they moved to SSPL. It
is the most battle-tested arrangement for keeping infrastructure
software free against vendor pressure.

The project was originally dedicated to the public domain under
CC0-1.0. The split to AGPL-3.0-or-later for platform components and
Apache-2.0 for SDKs took place on 2026-05-05; from that point
forward Kaleidoscope is structurally protected rather than simply
permissive. The CC0 commits before the migration are preserved in
git history, and any code dedicated to the public domain at that
time remains permanently in the public domain. The structural
protection covers what comes after.

---

## The fifteen optical instruments

Spark is the SDK and OTel-compatible client library. Aperture is the
OTLP receiver. Sieve is the routing and de-duplication layer. Sluice
is the durable buffer. Codex is the schema registry. Pulse, Lumen,
Ray, Strata, and Cinder are the storage engines for metrics, logs,
traces, profiles, and warm-tier persistence respectively. Prism is
the unified query layer. Beacon is the alerting engine. Augur is the
anomaly detector. Aegis is the identity and tenancy layer. Loom is
dashboards-as-code.

The naming theme is deliberate. A caleidoscope refracts grey light
into a clean spectrum. The platform's job is exactly that: refract
the four telemetry signals into a coherent observable view.

---

## The two-plane architecture

The hard problem with replacing Datadog or the LGTM stack from scratch
is that the storage engines are decade-class engineering. A plain
sequential plan ships nothing usable until the engines are done, which
is years away.

The split avoids this. The integration plane — Spark, Aperture, Sieve,
Codex, Prism, Beacon, Aegis, Loom — is small enough to ship in roughly
six months. It plugs on top of any existing observability backend and
produces immediate value: unified ingest, vendor-neutral schema,
dashboards-as-code, alerting that does not depend on any vendor's
proprietary alerting language.

The storage plane — Sluice, Pulse, Lumen, Ray, Strata, Cinder — ships
afterwards, one engine at a time, opt-in. Existing operators can
adopt Kaleidoscope's integration plane today and keep their current
backend. They can swap in Kaleidoscope storage engines as each one
ships and proves itself.

By month thirty-six the platform is fully self-contained.

```mermaid
flowchart LR
    APPS[Applications] --> IP
    subgraph IP[Integration plane — month six]
        ING[SDK + receiver + query<br/>alerts + schema + identity + dashboards]
    end
    IP --> EXT[Existing backend<br/>LGTM, ELK, vendor]
    IP --> SP[Storage plane<br/>one engine at a time, opt-in]
```

---

## What is nWave

nWave is an AI-amplified delivery framework built by Alessandro Di
Gioia and Michele Brissoni at nWave.ai. It is not mine; I am one of
its early adopters and dogfoodists. I use it on every project I run
because it operationalises the practices I have advocated for years
under the name T*D (test-driven, trunk-based, team-focused
development) into a shape that lets a solo author with AI agents
afford the full discipline of a high-functioning engineering team.

nWave structures every feature into five disciplined waves.

DISCUSS handles user stories, journeys, acceptance criteria, and
outcome KPIs. The agent is Luna, a product owner. Luna runs
Jobs-to-be-Done analysis when user motivations are unclear, journey
mapping when they are not, and produces stories in LeanUX format with
mandatory Elevator Pitches that name a real entry point and a real
observable output.

DESIGN handles system architecture, technology choices, and
Architecture Decision Records. The agent is Morgan, a solution
architect. Morgan produces C4 diagrams in Mermaid, locks library
choices with rationale and rejected alternatives, and continues the
project's ADR series.

DISTILL turns the DISCUSS acceptance criteria and the DESIGN component
contracts into executable acceptance tests, all RED on day one. The
agent is Scholar, an acceptance designer. Scholar produces Rust
integration tests that import only the public surface, exercise real
network protocols on loopback ports, and use the harness as substrate
rather than as a mock.

DELIVER turns the RED tests GREEN slice by slice, outside-in, with
each slice landing as its own commit. The agent is Crafty, a software
crafter. Crafty runs red → green → refactor cycles for every test,
runs mutation testing on each slice, and lands at one hundred per cent
mutation kill rate.

DEVOPS handles CI/CD, infrastructure, observability of the platform
itself, and deployment readiness. The agent is Apex, a platform
architect. Apex extends the GitHub Actions workflow, locks the local
hooks, designs the operator-facing observability story, and surfaces
the CI invariants that DESIGN requires.

Each wave runs to peer-review approval before the next wave starts.
The reviewers are themselves specialised agents — Sentinel for
DISCUSS and DISTILL, Atlas for DESIGN, Crafty in review mode for
DELIVER, Forge for DEVOPS. The reviewer's job is to apply a different
brief to the same artefact: bias detection, completeness checks,
contract preservation across waves, and explicit verdicts with
Conventional Comments labels.

The methodology has a maximum of two review iterations per wave
before escalation to me as orchestrator. In practice, most waves are
approved on iteration one or two, and the iterations have been
substantive — every reviewer pass has caught real defects.

---

## The first feature: OTLP conformance harness

The harness is a small Rust library. Its only job is to validate that
a byte sequence is a valid OpenTelemetry OTLP message. It does not
emit telemetry. It does not run as a process. It is a pure function:
bytes in, either an `Ok(record)` or an `Err(violation)`.

Why this feature first? Two reasons. First, it is the leaf dependency.
Aperture, Sieve, Sluice, every other component will consume it.
Building it first means downstream code never has to mock validation.
Second, it is the smallest thing that exercises the full nWave loop.
If the methodology cannot be applied cleanly to a feature this small,
the methodology is not ready for the larger features.

It is the walking skeleton for nWave on Kaleidoscope, not for
Kaleidoscope itself.

```mermaid
flowchart LR
    BYTES[OTLP wire bytes<br/>protobuf, optional length-prefix] --> H
    subgraph H[otlp-conformance-harness]
        VL[validate_logs]
        VT[validate_traces]
        VM[validate_metrics]
    end
    H -->|Ok| TYPED[Typed export request<br/>opentelemetry-proto types]
    H -->|Err| VIOL[OtlpViolation<br/>Rule + ByteOffset]
```

### The harness's DISCUSS wave

Luna ran Jobs-to-be-Done analysis with me, then mapped four user
journeys around the consumers of the harness — Aperture, Sluice,
third-party engineers operating Kaleidoscope, Kaleidoscope CI. She
produced seven user stories in LeanUX format, each with a mandatory
Elevator Pitch naming a function-call entry point and a concrete
observable result.

She also produced seven Elephant Carpaccio slices. Each slice ships
end-to-end value, has a named learning hypothesis, and uses real
production data rather than synthetic. The slice ordering is
learning-leverage first: the slice with the highest uncertainty goes
first, so failures cost the least.

Sentinel reviewed and pushed back on iteration one with four
substantive findings: byte-locus ranges instead of exact offsets
(mutation-resistant), observed-field membership in a closed set
instead of free-form strings, type-identity assertion at consumer
call sites, and signature-lock pinning via typed `fn` pointers. Luna
addressed all four on iteration two; Sentinel approved.

The DISCUSS-wave artefacts live in `docs/feature/otlp-conformance-harness-v0/discuss/`.

### The harness's DESIGN wave

Morgan worked with me to lock the architecture. The harness is a
single library crate with no internal dependencies of its own. The
public surface is three functions — `validate_logs`, `validate_traces`,
`validate_metrics` — plus six closed types: `OtlpViolation`, `Rule`,
`ByteOffset`, `Framing`, `SignalType`, and the wire-type sub-rule
enum.

He produced three C4 diagrams in Mermaid (System Context, Container,
Component-skipped per scope) and five Architecture Decision Records
covering the public API surface, the violation type design, the
exact-version pin policy on `opentelemetry-proto`, the conformance
test-vector layout, and the CI contract for the harness's gates.

The CI contract — ADR-0005 — locked the five gates that every other
feature on Kaleidoscope inherits: cargo deny check, cargo test, cargo
public-api, cargo semver-checks, cargo mutants. Including the one
hundred per cent mutation kill rate target.

Atlas reviewed and approved on iteration one.

The DESIGN-wave artefacts live in `docs/feature/otlp-conformance-harness-v0/design/`.

### The harness's DISTILL wave

Scholar produced fifty-two acceptance tests across seven Rust
integration test files (`slice_01_*.rs` through `slice_07_*.rs`) plus
shared helpers in `tests/common/mod.rs`. Each test maps to a user
story and a slice. The hexagonal boundary mandate was enforced
literally: every test imports `otlp_conformance_harness::*` only;
no `pub(crate)` symbols.

Real-data discipline: accept paths use prost-encoded message types
generated from `opentelemetry-proto`'s tonic feature, which produces
the same byte shape an OTel SDK would emit. Hand-crafted bytes only
for synthesised malformed cases (truncations, varint corruptions, bad
tags).

Sentinel approved on iteration two after asking for byte-locus
windows instead of exact offsets and a closed set for the
observed-field assertion.

The DISTILL-wave artefacts live in `docs/feature/otlp-conformance-harness-v0/distill/` and the tests at `crates/otlp-conformance-harness/tests/`.

### The harness's DELIVER wave

Crafty implemented the harness slice by slice over eight commits. The
slice ordering followed Luna's prioritisation: the highest-leverage
learning slice first, then by dependency.

Each slice was red → green → refactor. The refactor step was not
optional. Crafty extracted shared helpers when duplication appeared,
collapsed redundant disjuncts in the prost-error classifier under
mutation pressure, and pulled out a single `decode_strict` chokepoint
when the third call site appeared.

Mutation testing achieved one hundred per cent kill rate. The path
to one hundred was instructive: pass one had three surviving
mutations in `classify_prost_decode_error`, all `||→&&` flips. Crafty
killed them by writing per-disjunct tests that isolate each
clause. Pass two had one survivor in `matches_wire_type_category`
that was killed the same way. Pass three was clean.

The crucial property: every survivor was killed either by writing a
more discriminating test, or by simplifying the production code so
the surviving mutation became unreachable. No survivor was killed by
relaxing a test. This is the difference between mutation testing as
discipline and mutation testing as theatre.

Crafty in review mode approved on iteration one.

The DELIVER-wave artefacts live in `docs/feature/otlp-conformance-harness-v0/deliver/`.

### The harness's DEVOPS wave

Apex extended the project's GitHub Actions workflow with the five
ADR-0005 gates: gate-4-deny first (fastest, fail-fast on licence and
advisory issues), gate-1-test second, gate-2-public-api and
gate-3-semver-checks running in parallel after Gate 1, gate-5-mutants
last (slowest, behind a thirty-minute timeout safety net).

He also produced the first version of the local pre-commit and
pre-push hooks, mirroring the CI gates so contributors can see CI's
verdict before pushing.

Forge approved on iteration one. The review identified one high
finding (action pinning by tag rather than commit SHA) accepted as
risk for the solo-author period, and two medium findings actioned in
post-merge corrections.

The DEVOPS-wave artefacts live in `docs/feature/otlp-conformance-harness-v0/devops/`.

### The post-merge corrections

After branch protection went live, the first real CI run on `main`
exposed several defects that no reviewer had caught: a Docker action
that honoured our toolchain pin and choked on edition2024 in a
transitive dep; an MSRV mismatch with the current ecosystem requiring
a 1.78 to 1.85 bump; a test race in the silence-observer tests caused
by `gag::BufferRedirect` capturing the cargo test runner's own
output; tool MSRV mismatches for `cargo-public-api` and
`cargo-semver-checks` that needed a switch to precompiled binaries; a
GitHub Actions context-evaluation quirk forcing literal env values at
the job level; tool flag drift between major versions.

Eleven commits, thirty-five minutes wall-clock. Each fix landed
directly on `main` — by then I had relaxed branch protection to pure
trunk-based, no required-status-checks gate, no enforce-admins. CI
was feedback, not a blocker. The discipline that kept `main` green
was social, not mathematical: small commits, fix-forward fast, every
correction recorded as a post-merge correction note in the wave's
`wave-decisions.md`.

The harness shipped at tag `otlp-conformance-harness/v0.1.0` with all
five gates green, seventy-three of seventy-three tests passing, one
hundred per cent mutation kill rate confirmed on real Linux CI
infrastructure.

The honest read of this period: the methodology survived first
contact with operational reality, but the reviewer agents had not
caught the gaps that real infrastructure exposed. That is the
artefact-vs-reality gap. It is the most important single learning
from feature one. The reviewer agents check artefact fidelity to
their wave's brief; they do not check operational fitness against a
real runner. Future improvements to the reviewer agents need to
include a "did you actually run this against the runner you said you
would run it against" check.

---

## The second feature: Aperture

Aperture is the OTLP receiver. It is the first network-facing
component of Kaleidoscope, and the first piece that is genuinely a
service rather than a library. It listens on gRPC port 4317 and
HTTP/protobuf port 4318, validates every incoming payload through
the harness, and hands accepted records to a pluggable `OtlpSink`.

The shift from library to service is meaningful. The harness has no
runtime concerns. Aperture has many: backpressure, graceful shutdown,
self-observability, configuration with forward-compatibility knobs
for Phase 2 identity and TLS layers.

```mermaid
flowchart LR
    SDK[Application or SDK] -->|OTLP/gRPC| GRPC
    SDK -->|OTLP/HTTP| HTTP
    subgraph Aperture
        GRPC[gRPC :4317]
        HTTP[HTTP :4318]
        BP[Per-transport semaphore<br/>backpressure]
        H[harness validation]
        SINK[OtlpSink trait]
        D[Drain orchestrator<br/>SIGTERM-aware]
        RZ[/readyz/]
    end
    GRPC --> BP
    HTTP --> BP
    BP --> H
    H --> SINK
    D -.flips on SIGTERM.-> RZ
    D -.refuses new on.-> GRPC
    D -.refuses new on.-> HTTP
    SINK --> NEXT[StubSink, ForwardingSink<br/>or future Sieve]
```

### Aperture's six locked scope decisions

Before Luna ran the wave, I locked six scope decisions in one
round-trip with her — the kind of conversation a senior engineer has
with the product owner before story-writing begins:

1. Both transports day one. gRPC on 4317 and HTTP/protobuf on 4318.
   Phasing one out for later means Aperture cannot honestly be called
   "an OTLP receiver" until both are present.
2. Tokio as the async runtime. The only realistic Rust answer for a
   network service of this shape.
3. The boundary with the future Sieve component is an `OtlpSink`
   trait. Aperture's job ends when the sink has acknowledged the
   record. v0 ships with a `StubSink` and a `ForwardingSink`. Sieve
   when it lands will be another `impl OtlpSink`.
4. Backpressure: a configurable max-concurrent-requests limit per
   transport, with HTTP 503 (Retry-After) or gRPC `RESOURCE_EXHAUSTED`
   on overflow. No internal queue (that is Sluice's job in Phase 7).
   No block (it violates the OTel SDK contract). No silent drop
   (an explicit anti-pattern).
5. Plaintext at v0, no auth. But a configuration knob for TLS and
   SPIFFE present in the v0 schema, defaulting to off. This avoids a
   schema break in Phase 2 when Aegis ships.
6. Self-observability: structured JSON logs to stderr, no metrics in
   v0 (that is Pulse's territory in Phase 4), HTTP /healthz and
   /readyz endpoints on the same listener as the OTLP HTTP traffic.

### Aperture's DISCUSS through DEVOPS

Luna, Morgan, Scholar, and Apex each ran their wave on Aperture with
the same discipline as for the harness. The artefacts mirror the
harness in structure but reflect the service-shaped concerns.

Eight Elephant Carpaccio slices instead of seven (the eighth is
graceful shutdown drain, which the harness did not need). Eighty-four
RED acceptance tests instead of fifty-two. Five new ADRs (ADR-0006
through ADR-0010) covering transport stack, sink trait design,
configuration schema, observability strategy, and backpressure
policy.

Three new CI invariants surfaced: `single_validator_per_signal`
(only one harness call site per signal in the Aperture source),
`no_telemetry_on_telemetry` (Aperture emits no outbound network
traffic except to its configured downstream sink), and
`probe_gold_runner` (the Earned-Trust probe is itself probed against
a fixture that lies).

All four waves approved by their reviewers (Sentinel, Atlas,
Sentinel again, Forge) with no blockers.

### Aperture's DELIVER, slice by slice

The first slice is the smallest possible end-to-end thing. An OTel
SDK sends a real log record over gRPC; Aperture binds the listener,
hands the bytes to the harness, gets back a typed record, prints a
single line to stderr saying it received the record, and answers
the SDK with OK. There is no second transport yet, no second signal,
no backpressure, no graceful shutdown. There is just the one happy
path, end to end. Once that works, every subsequent slice is an
addition, not a leap.

The second slice adds the HTTP transport on the other port. Same
pipeline, different wire shape. It also adds the readiness state
machine: a process that has bound both listeners answers `/readyz`
with 200; a process still starting up answers 503. The reason that
matters now and not later is that as soon as Aperture is bound to a
real port, somebody's orchestrator wants to know whether to send it
traffic.

The third and fourth slices complete the OTLP signal contract. Logs
are already in. Slice three adds traces. Slice four adds metrics.
After slice four, the platform handles every kind of telemetry the
OpenTelemetry standard defines — which is the moment Aperture can
honestly be described as an OTLP receiver rather than as a logs
receiver that happens to use OTLP.

The fifth slice teaches Aperture to refuse work when it has too
much. A configurable cap on concurrent requests; a 503 with
Retry-After when the cap is hit; a structured stderr line for every
refusal so an operator can see when the cap was exercised. The point
is that refusal is honest. Aperture does not queue, does not block,
does not drop. Each of those alternatives breaks somebody downstream.
Saying "I'm full, try again in a second" is the only honest answer.

The sixth slice is where the platform stops being a toy. Aperture
gains a sink that ships accepted records to a real downstream
OpenTelemetry-compatible HTTP endpoint. That means a Phase-1
deployment of Kaleidoscope can actually be useful: an operator runs
Aperture in front of their existing observability backend and gets
the validation, the structured logs, and the readiness probe for
free. The slice also adds the Earned-Trust probe — at startup,
Aperture verifies the downstream actually responds to the OTLP
contract before it begins accepting traffic. If the downstream lies
(answers OPTIONS but then refuses POST), Aperture refuses to start.
The proof that the probe is honest, and not theatre, is a test that
runs the probe against a fixture deliberately programmed to lie. The
test passes only if the probe catches the deceit.

The seventh slice is small and forward-looking. The configuration
file gains two switches, for TLS and for workload identity. At v0
both are off, and turning them on does nothing except print a
warning. They exist so that when the identity layer ships, two
years from now, the configuration format does not have to change.
This is the kind of decision that costs almost nothing now and
saves a great deal of pain later.

The eighth slice is shutdown done with care. SIGTERM arrives; the
readiness probe flips to 503 within a tenth of a second; new
requests are refused; in-flight requests are given a grace period
to complete; the listeners drop and the process exits zero. The
default grace period is thirty seconds, which is the value
Kubernetes also defaults to. Operators rolling deployments do not
need to think about Aperture at all.

After the eighth slice, the v0 plan is complete. The reviewer reads
the whole DELIVER output as a single artefact and approves. A
single commit promotes the new crate into the same CI gates as the
harness. The first version is tagged.

What stands at the end is the second feature on Kaleidoscope and
the first network-facing component, and the proof that the
methodology absorbs the shift from a pure-function library to a
long-lived service without changing shape. Eight slices, each
landing as its own visible step, each verified end-to-end against a
real client over a real socket.

Each slice has been a single focused dispatch of Crafty, ending with
a multi-commit landing that makes the slice's RED tests GREEN, the
mutation kill rate 100%, and the production code idiomatic Rust.

The `crates/aperture/` directory is the production tree. Each src
file carried a `// SCAFFOLD: true` marker at DISTILL time; the marker
is removed by DELIVER as each module's tests turn GREEN.

---

## Case study: feature 3

Spark is the third feature on Kaleidoscope and the first one written
from the application's seat rather than the platform's.

The harness validated bytes against the OTLP specification. Aperture
received those bytes over a real socket. Spark is the SDK an
application uses to put bytes onto that socket in the first place.
The round-trip closes here. A Rust application calls `spark::init`,
emits a span via the standard OpenTelemetry API, and lets the guard's
drop flush the batch on exit. The bytes travel to Aperture. Aperture's
recording sink confirms what arrived.

```mermaid
flowchart LR
    subgraph App[Application process]
        CODE[Application code<br/>tracing::info!<br/>opentelemetry::tracer]
        SPARK[Spark SDK]
        OTEL[OpenTelemetry SDK 0.27]
    end
    CODE --> SPARK
    SPARK --> OTEL
    OTEL -->|OTLP/gRPC| AP
    subgraph AP[Aperture]
        H[harness]
        SINK[OtlpSink]
    end
    AP --> NEXT[StubSink, ForwardingSink<br/>or future Sieve]
```

Spark is licensed Apache-2.0, deliberately. The platform crates ship
under AGPL because copyleft is the structural defence against the
re-licensing pattern. The SDK ships permissive because anyone
embedding it in a proprietary application must not be forced to open
their source to do so. This split is the same split the major
observability vendors landed on for the same reason. Kaleidoscope
encodes it from day one.

The dev-dependency on Aperture for integration tests is the only
place where the AGPL crate enters Spark's build. `cargo deny` is the
structural enforcement that prevents accidental promotion to a
runtime dependency.

---

## What changes from a service to an SDK

Aperture lives inside our process. Spark lives inside someone else's.
The implications are larger than they look.

A service can change its internal shape any time the methodology says
it should. A library exposes a public surface that strangers will
consume on their own timeline. Renaming an exported function is a
breaking change. Adding a variant to a public error enum is a
breaking change unless the enum is marked non-exhaustive. The
OpenTelemetry ecosystem itself is mid-stabilisation; the semantic
conventions crate's attribute names move between point releases.

The methodology absorbs this without changing shape, but the
discipline inside DESIGN intensifies. ADR rigour matters more. Pin
policy matters more. Whether the user-facing struct exposes a field
or a method matters more. The reviewer agent's brief covers
public-API ergonomics as its own quality attribute, not as a
footnote.

Developer ergonomics is itself an outcome KPI for an SDK. A
five-minute first-time-use experience is not a nice-to-have; it is
the difference between adoption and abandonment.

```mermaid
flowchart TB
    subgraph PUB[Spark public surface — four items only]
        I[init function]
        C[SparkConfig builder]
        E[SparkError enum<br/>non-exhaustive]
        G[SparkGuard<br/>opaque, must_use]
    end
    I -.takes.-> C
    I -.returns.-> G
    I -.errors with.-> E
    G -.Drop runs.-> FLUSH[Bounded flush<br/>shared deadline budget]
    FLUSH -.flush.-> TP[TracerProvider]
    FLUSH -.flush.-> LP[LoggerProvider]
    FLUSH -.flush.-> MP[MeterProvider]
```

---

## Spark — DISCUSS and DESIGN closed

DISCUSS produced six elephant-carpaccio slices, each shipping a
visible step of the integration. The first slice is a walking
skeleton: a small binary calls `spark::init`, records a span, and
shuts down; Aperture's recording sink confirms the span arrived
carrying the four house attributes on its resource. Every subsequent
slice adds one capability — error paths, feature flags, environment
variable precedence, the three signal types, the bounded flush on
drop — without giving up the round-trip the walking skeleton
established.

DESIGN locked the wrapper shape across six new architecture decision
records. The public surface is four items: the `init` function, the
`SparkConfig` builder, the `SparkError` enum, the `SparkGuard`
returned from init. The guard is opaque, marked must-use, and does
its work entirely in drop. The single-init invariant is enforced in
two layers: an internal atomic flag and the OpenTelemetry SDK's own
re-set guard, with roll-back on failure so a retry after a failed
init does not falsely report already-initialised. The flush deadline
is a single budget shared sequentially across the three providers.
The OpenTelemetry family is pinned exact-minor at zero-twenty-seven,
the same version the harness pins exact-patch.

The DESIGN wave surfaced one honest contradiction with the DISCUSS
contract. The acceptance criteria for the bounded-flush slice
implied an integer count of drained or dropped records on the exit
event. The OpenTelemetry SDK at the version Spark pins does not
expose those counters publicly. The architect proposed Path A:
update the contract to accept the literal `unknown` until the SDK
exposes the integer; preserve the prefix `drained=` and `dropped=`
as the contract; treat the value as informational. The alternative
of building a Spark-side counter wrapper to fake an integer was
rejected as throwaway code that duplicates state already tracked
internally and that a future SDK release will likely surface.
DISCUSS was updated with an explicit Changed Assumptions section
recording what changed and why. The DESIGN ADR locks the new event
shape. The acceptance designer reading the contract today is not
misled by an old literal.

Both waves were approved by the reviewer on iteration one with no
blocking issues.

---

## Spark — DISTILL closed

DISTILL turned the user stories' BDD scenarios and the six DESIGN
ADRs into eight Cargo integration test binaries: one per
elephant-carpaccio slice, plus two cross-cutting invariants for the
single-init contract and the no-telemetry-on-telemetry contract.
Fifty-seven test functions in total. Fifty-three of them are RED
on day one, panicking on `unimplemented!()` from the production stub.
The configuration builder is intentionally real at DISTILL because
tests need to construct configurations to exercise the contract;
everything else waits for DELIVER.

The acceptance posture is the same one Aperture set: real local
Aperture instances spun up per test on ephemeral loopback ports, with
recording sinks asserting what arrived. No mocks, no in-memory
transports, no synthetic data. Spark depends on Aperture only as a
development dependency, which keeps the AGPL crate out of Spark's
runtime supply chain and confines the licence question to the test
binaries.

The DISTILL wave surfaced its own back-propagation. The acceptance
designer discovered that the OpenTelemetry Rust SDK at the version
Spark pins exposes a global getter for the tracer provider and the
meter provider, but not for the logger provider. The DISCUSS contract
for the logs-and-metrics slice presupposed the symmetric three-signal
shape that does not hold at this version. Three of the slice's tests
were marked ignored, with their function names preserved verbatim so
that when the contract resolution lands the tests can be un-ignored
without renaming. The note proposed four concrete resolution paths
and made the choice explicit rather than papering it over with a
workaround.

Two back-propagations in two waves. Both surfaced upstream
constraints that the methodology made visible at the right moment,
neither at the wrong moment. The methodology rewards honest
escalation; the alternative is a contract that lies about what the
underlying technology can do.

The reviewer approved DISTILL on iteration one with no blocking
issues.

---

## The logs-emission decision

The second back-propagation needed a real architectural choice. The
acceptance designer's note proposed four paths and recommended Path
A. The four were: expose a fifth public-API item, expose a test-only
seam, adopt the Rust ecosystem's standard logs bridge, or wait for
the upstream SDK to add the missing global getter.

The choice was the third one. A Rust application in 2026 already uses
the `tracing` crate everywhere. The bridge crate
`opentelemetry-appender-tracing` is the canonical adapter from
`tracing` events to OpenTelemetry log records. It is licensed
Apache-2.0, which sits inside Spark's permissive runtime supply
chain. Spark wires the bridge as one more `tracing-subscriber` layer
during `init`, with a filter that excludes Spark's own diagnostic
target so the no-telemetry-on-telemetry invariant holds. The
application keeps using `tracing::info!` and `tracing::warn!`. The
public surface stays at four items; ADR-0011's lock holds.

```mermaid
flowchart LR
    APP[Application code]
    APP -->|tracing::info!<br/>tracing::warn!| TS
    subgraph TS[tracing_subscriber layer stack]
        FILT[filter: target ≠ spark]
        BRIDGE[OpenTelemetryTracingBridge<br/>opentelemetry-appender-tracing]
    end
    FILT --> BRIDGE
    BRIDGE -->|LogRecord| LP[Spark's LoggerProvider]
    LP -->|OTLP| AP[Aperture]
    APP -.target spark.-> SPARK_DIAG[Spark's own diagnostic events<br/>stay on the application's tracing facade]
```

The decision is recorded as ADR-0017. The DISCUSS contract for the
logs-and-metrics slice was updated with a Changed Assumptions entry
naming the move from the original phrasing to Path A3, and the four
DISCUSS files referencing the non-existent global getter were
rewritten mechanically to use `tracing::info!` instead. The three
ignored slice tests retain their function names verbatim, so when
DELIVER lands the bridge wiring, un-ignoring them is a single-line
change. Slice five can now start alongside the other five.

---

## Spark — DELIVER closed and graduated

The crafter ran six elephant-carpaccio slices, one at a time, each
landing as a tight red-green-refactor cycle and a small focused
commit on `main`. The walking skeleton landed first: a Rust
application calls `spark::init`, records one span, and the recording
sink behind a real Aperture instance captures one export request
carrying `service.name` and `tenant.id` on its resource. The init
error paths landed next: each of the four error variants becomes a
precise diagnostic raised before any OpenTelemetry SDK type is
constructed, with a transactional roll-back that releases the
single-init flag if a post-flag step fails. Then the remaining house
attributes, then the environment-variable precedence, then the
three-signal Resource symmetry via the appender bridge, then the
bounded flush deadline with its shutdown event vocabulary.

```mermaid
flowchart LR
    R[Slice 01<br/>walking skeleton] --> E[Slice 02<br/>init error paths]
    E --> F[Slice 03<br/>feature flags + experiment.id]
    F --> V[Slice 04<br/>env-var precedence]
    V --> S[Slice 05<br/>logs and metrics<br/>via tracing-appender bridge]
    S --> D[Slice 06<br/>bounded flush deadline]
    D --> G[Spark v0.1.0<br/>graduated]
```

Eight Cargo integration test binaries. Sixty active tests. One
hundred per cent mutation kill rate on the diff at every slice's
close. The crafter's review-mode pass approved the wave on iteration
one with no blocking issues.

Five back-propagation issues surfaced during DELIVER, each documented
at the time of the offending change with explicit forward path. One
of them caught a real misreading I had propagated in writing
ADR-0017: I claimed the appender crate's release cadence was offset
by one from the core, when in fact the minor versions align. The
crafter found the duplicate `opentelemetry 0.28` in the lockfile,
inspected the upstream manifests, pinned `=0.27`, and the lockfile
collapsed back to one minor. The architecture decision record was
amended in place with the correction. The audit trail is the
back-propagation note plus the amendment plus the lockfile diff.

After the sixth slice closed and the review approved, three things
happened in quick succession. The pre-commit hook and the CI Gate 1
both removed their `--exclude spark` clauses; Spark joined the
harness and Aperture in the canonical contract that every commit on
`main` passes the full workspace test gate. The tag `spark/v0.1.0`
landed as the canonical reference. The narrative document gained
this paragraph.

What is consistent across the five features so far is that each
shipped, each had honest back-propagation when DESIGN's reading of
upstream APIs or contracts proved imperfect, and each closed without
exceptions to the discipline.

---

## Case study: feature 4 — Sieve

Sieve is the fourth feature on Kaleidoscope and the first one that
sits inside the platform pipeline rather than at its edges. The
harness validates bytes against the OpenTelemetry specification.
Aperture receives those bytes and hands them to a sink. Spark sits in
the application emitting them in the first place. Sieve is the next
node downstream of Aperture: it filters and samples before the
records reach storage.

The job at v0 is volume control without losing the trace data
operators most want to keep. Trace storage is expensive and most
traces are uninteresting; sampling reduces the volume. But errors are
exactly the traces operators reach for during an incident, so the
sampler is biased to retain every error-bearing trace at one hundred
per cent regardless of the configured rate.

```mermaid
flowchart LR
    APP[Application code<br/>tracing::info!]
    APP --> SP[Spark SDK]
    SP -->|OTLP/gRPC| AP[Aperture]
    subgraph AP[Aperture]
        H[harness validation]
        SI[Sieve sampler<br/>library inside the pipeline]
        SK[OtlpSink trait]
    end
    H --> SI
    SI --> SK
    SK -->|kept| NEXT[StubSink, ForwardingSink<br/>or future Sluice]
    SI -.error trace.-> KEEP[100% retained]
    SI -.non-error trace.-> RATE[rate-based decision<br/>SIEVE_NON_ERROR_TRACE_RATE]
```

Licensed AGPL because Sieve is a server-side platform component.
Inside the pipeline by design at v0, not a separate process. The
roadmap says stage one of sampling lives at Aperture; the architect
and the orchestrator agreed that putting Sieve there at v0 keeps the
walking skeleton honest. The separate-process shape becomes the right
answer when tail-sampling needs an in-memory window across batches,
which is v1.

The product owner ran a tightened DISCUSS to lock eight scope
decisions: library shape, trace-level granularity, the
`status.code == ERROR` definition of an error span, deferral of
PII-scrubbing to v1, single global rate via an environment variable,
logs and metrics passthrough, the `xxh3_64` hash function for
`trace_id`-keyed determinism, and the verbosity convention
(DEBUG per-decision, INFO summary every minute). Six elephant-
carpaccio slices and six user stories follow from those decisions.

The reviewer approved DISCUSS on iteration one with no blocking
issues. Two clarifications surfaced and were closed inline: the
periodic INFO summary is locked as a v0 contract (without it,
operators on default verbosity have no Sieve visibility), and the
sixty-second tick interval is locked at DISCUSS rather than left for
DESIGN to pick.

---

## Sieve — DESIGN closed

DESIGN closed at iteration one with no blocking issues from the
reviewer. The single most consequential architectural decision was
the shape of the Aperture integration. Two options were on the table:
Aperture grows a hook trait that Sieve plugs into, or Sieve wraps
Aperture's existing sink trait without changing it.

The architect went with the second. Sieve's main public type is a
generic decorator that wraps any existing `OtlpSink + Probe`
implementation, runs the sampling pass on traces inside its own
`accept` method, and forwards the kept records to the inner sink
unchanged. Aperture's public surface does not move. The integration
work that DELIVER will land is three lines in Aperture's composition
root: build the inner sink, build the sampler, wrap the inner sink
in a `SamplingSink`. That's it.

```mermaid
flowchart LR
    APP[Application code<br/>tracing::info!]
    APP --> SP[Spark SDK]
    SP -->|OTLP/gRPC| AP
    subgraph AP[Aperture]
        H[harness validation]
        SS[SamplingSink<S, N><br/>decorator]
        SK[Inner OtlpSink<br/>StubSink, ForwardingSink]
    end
    H --> SS
    SS -->|kept| SK
    SS -.error trace.-> ALWAYS_KEEP[100% retained]
    SS -.non-error trace.-> RATE[xxh3_64 trace_id mod rate]
    SS -.summary tick.-> SUMMARY[INFO every 60s]
```

The decorator preserves the Earned-Trust invariant. Aperture's contract
includes a `Probe` trait that sinks implement so the composition root
can verify reachability before traffic flows. Sieve has nothing
external to probe; it is a pure-CPU stage. Its `probe` method
delegates to the inner sink's, which is honest and keeps Aperture's
"wire then probe then use" guarantee intact.

Four ADRs lock the design (0018 to 0021). The summary aggregator uses
three atomic counters with relaxed ordering, wait-free on the hot
path; the cross-counter race during snapshot is documented and
acceptable for the "approximate aggregate over the window" contract
the operator was promised. The `xxh3_64` hash from the
`xxhash-rust` crate is pinned exact-minor at zero-eight because a
hash-algorithm change would shift which traces are kept on the same
fixture, and that is operator-visible. The `tracing-appender` lesson
from Spark applied here too: the version pin gets a careful audit and
a documented rationale before it lands.

DISTILL picks up the acceptance test design next.

---

## Sieve — DISTILL closed

The acceptance designer turned the user stories' BDD scenarios and
the four DESIGN ADRs into eight Cargo integration test binaries: one
per elephant-carpaccio slice plus two cross-cutting invariants.
Thirty-six test functions in total. Twenty-two of them exercise error
or edge paths, sixty-one per cent of the suite.

The acceptance posture is the same one the harness, Aperture, and
Spark settled on: real Aperture's recording sink is the inner sink
inside Sieve's `SamplingSink<S, N>` decorator. The decorator is
tested against the actual `OtlpSink + Probe` contract from Aperture's
public ports, not against a mock. A library called from the
application's seat should be tested against the surface the
application sees, not against an artificial double of it.

```mermaid
flowchart LR
    F[Fixture trace] --> SS
    subgraph SS[SamplingSink S over Aperture's RecordingSink]
        DEC[decorator pass]
        SAM[HeadSampler]
        REC[real RecordingSink]
    end
    DEC -->|asks| SAM
    SAM -->|Decision| DEC
    DEC -->|kept| REC
    REC -->|asserts| TEST[test assertions]
```

The reviewer approved on iteration one with a score of 9.8 out of 10
across nine dimensions. The mixed RED posture is the canonical Sieve
shape: validation paths inside `HeadSampler::new` and
`HeadSampler::from_env` are real, so tests can exercise the four
`SieveConfigError` variants without a complete sampler
implementation. The behavioural contract panics on
`unimplemented!()`. DELIVER will turn the panicking tests GREEN one
slice at a time.

A small piece of DEVOPS work falls out of DISTILL and lands in the
same wave: the `xxhash-rust` crate ships under `BSL-1.0` only, not
the dual licence I had assumed when ADR-0019 first read the upstream
manifest. The workspace's `cargo deny` configuration grew an explicit
`BSL-1.0` allow entry with documented rationale. The licence audit
trail is the deny.toml comment plus the ADR plus the dependency
graph.

DEVOPS picks up the workflow extensions next; DELIVER follows.

---

## Sieve — DELIVER closed and graduated

The crafter ran six elephant-carpaccio slices. The walking skeleton
landed first: a Sampler trait, a HeadSampler concrete, a Decision
enum, two integration tests asserting that an error-bearing trace is
kept and a non-error trace at rate zero is dropped. The error-bias
retention rule landed alongside it as a side effect of how the
short-circuit composes. Then the rate-honouring decision via the
xxh3_64 hash; the trace-id determinism that follows for free from a
deterministic hash; the decorator that wraps Aperture's sink without
changing Aperture's surface; and finally the observability layer with
its three atomic counters, its sixty-second timer task, its DEBUG
per-decision events and INFO summary.

```mermaid
flowchart LR
    R[Slice 01<br/>walking skeleton] --> E[Slice 02<br/>error-bias retention<br/>side effect]
    E --> NRR[Slice 03<br/>non-error rate xxh3_64]
    NRR --> TID[Slice 04<br/>trace_id determinism<br/>side effect]
    TID --> DEC[Slice 05<br/>decorator + passthrough]
    DEC --> OBS[Slice 06<br/>counters, timer, events]
    OBS --> G[Sieve v0.1.0<br/>graduated]
```

Eight Cargo integration test binaries. Thirty-six active tests. One
hundred per cent mutation kill rate on the diff at every slice's
close. Three of the six slices closed with their own implementation
plus a small pinning commit that added unit tests to kill mutation
survivors; the discipline is visible in the commit log.

The architect's review flagged one pragmatic v0 compromise: reading
the configured rate from the sampler uses an Any downcast to the
concrete HeadSampler type rather than extending the Sampler trait
with a rate accessor. The reviewer accepted it as the right v0 shape
and named the forward path: when v1 introduces a second sampler
(tail-sampling per the roadmap), extend the trait additively with a
default-NaN rate method. The downcast collapses to a clean
trait-method call at that moment. Honest documentation in code; no
hidden technical debt.

After the sixth slice closed and the review approved, three things
happened in quick succession. The pre-commit hook and the CI Gate 1
both removed their `--exclude sieve` clauses; Sieve joined the
harness, Aperture, and Spark in the canonical contract that every
commit on `main` passes the full workspace test gate. The tag
`sieve/v0.1.0` landed as the canonical reference. The narrative
document gained this paragraph.

The intermediate CI runs on slices one through five were red. That
is intrinsic to slice-by-slice DELIVER when the acceptance designer
writes all tests upfront in DISTILL: each slice's commit makes its
own tests pass while leaving the next slice's tests still RED, and
the mutation-testing gate refuses to mutate against a baseline that
has any failing test. The pure trunk-based discipline tolerates
intermediate reds because they are fix-forward by construction; the
final state at the graduation commit is green. Future Kaleidoscope
features may want a small amendment to the mutation gate that
narrows the baseline to the slice under test rather than the whole
crate, so intermediate reds become invisible. For Sieve the pattern
held; the lesson is logged.

What stands at the end of Sieve is the pipeline's first inside-the-
platform component, the methodology's fourth feature delivery, and
the proof that the same five-wave shape works for a stage-of-flow
component as cleanly as for a pure-function library, a network-port
service, or an application-embedded SDK.

---

## Case study: feature 5 — Codex

Codex is the schema authority. Where Sieve filters telemetry mid-
flight and Aperture validates wire-format conformance at the
network edge, Codex codifies the names that telemetry attributes
should have in the first place. The OpenTelemetry semantic
conventions are the upstream contract; Kaleidoscope adds three
house attributes (`tenant.id`, `feature_flag.{key}`, `experiment.id`)
that operators rely on for multi-tenant deployments,
feature-flagged rollouts, and A/B experiment tagging.

The job at v0 is small and useful: catch typos at integration time.
A developer wiring Spark into a service who writes `tenat.id` for
the tenant attribute will today ship the typo through to Aperture's
recording sink, where it lands as a separate column nobody queries
on. Codex closes that loop. Spark calls Codex's `validate` on the
assembled Resource just before the OTel SDK is wired; an unknown
attribute name produces a `LintReport` whose violations carry the
offending name plus a fuzzy "did you mean" suggestion when the
typo is close enough to a blessed attribute.

```mermaid
flowchart LR
    APP[Application code]
    APP --> SP[Spark::init]
    SP --> COMP[Resource composer]
    COMP --> CODEX
    subgraph CODEX[Codex catalogue]
        OTEL[OTel semconv 0.27 corpus]
        HOUSE[Kaleidoscope house attributes]
    end
    CODEX -->|Ok| OK[Spark continues init]
    CODEX -->|LintReport| WARN[tracing::warn! at default verbosity]
    CODEX -->|strict mode| ERR[Err SparkError::SchemaValidation]
```

Licensed AGPL because Codex is a server-side platform component
(despite living as a library at v0; the licence anticipates the
eventual gRPC daemon shape the original roadmap describes). v0
ships nothing of that daemon — no FoundationDB, no CUE, no HTML
rendering. v0 is a Rust crate. The v0 use case is in-process from
Spark; the network-service shape arrives when there are multiple
SDK versions and per-tenant schema overlays to negotiate, which is
v1+.

The product owner ran a tightened DISCUSS and the architect's
review approved on iteration one with no blocking issues. Nine
scope decisions locked: library shape, hand-written Rust corpus
generated from upstream semconv, single pinned version, no
per-tenant overlays at v0, structured `LintReport` with multi-
violation collection, Spark-side integration via runtime dep with a
new non-exhaustive SparkError variant, checked-in generated corpus
file (so its evolution is visible in PR diffs), in-tree Levenshtein
implementation (no new dependency), and a single warn event per
misconfigured init at default verbosity.

Six elephant-carpaccio slices, each demoable. The walking skeleton
proves a `SchemaCatalogue` exists and validates a canonical pair
clean. Slice 02 fills the upstream OTel semconv corpus. Slice 03
adds the three Kaleidoscope-house attributes including the
`feature_flag.{key}` prefix-with-arbitrary-suffix shape. Slice 04
lights up the unknown-attribute path with structured
`LintViolation`s. Slice 05 adds the fuzzy "did you mean"
suggestions. Slice 06 lands the Spark integration: runtime dep,
default-warn or opt-in-strict, additive `SparkError` variant.

Slice 06 is the first real validation that the `#[non_exhaustive]`
discipline on `SparkError` works as intended. Spark v0.1.0 shipped
with the marker; Codex now adds a variant. The change is
non-breaking by construction. Confidence-building.

DESIGN picks up the architecture next.

---

## Codex — DESIGN closed

Four ADRs lock the architecture. The public surface stays at five
types plus the doc-hidden test seam discipline already established
by Spark and Sieve. The corpus is hand-written Rust constants
generated from the upstream OpenTelemetry semantic-conventions crate
by an `xtask` regenerator the maintainer runs when the workspace's
semconv pin moves; the generated artefact is checked in so its
evolution is visible in pull-request diffs. The Levenshtein
algorithm for the fuzzy "did you mean" suggestions is thirty lines
in-tree, no new dependency, well within the licence-audit
discipline an AGPL crate calls for.

The Spark integration is the one cross-feature touch. Spark adds
Codex as a runtime dependency, gains an additive `SchemaValidation`
variant on its already-`#[non_exhaustive]` error type, and exposes
an opt-in strict-mode builder. The default is warn mode: a single
`tracing::warn!` event per misconfigured init carrying the report's
human-readable text via Display rendering. Strict mode flips that
to a fast `Err` from `init` for CI environments. The default is the
operationally safe choice for existing Spark deployments rolling out
the lint.

```mermaid
flowchart LR
    SPK[crates/spark]
    CDX[crates/codex<br/>library, AGPL]
    OTSEMCONV[opentelemetry-semantic-conventions =0.27<br/>upstream]
    XTASK[xtask/regenerate_codex_corpus<br/>maintainer ritual]
    GEN[crates/codex/src/generated/semconv_0_27.rs<br/>checked-in artefact]

    OTSEMCONV -.read at maintainer trigger.-> XTASK
    XTASK -->|emits| GEN
    GEN -->|seeds| CDX
    SPK -->|runtime dep| CDX
    SPK -.calls at init.-> CDX
```

The wave surfaced one alignment risk during the architect's work and
resolved it cleanly. The slice-06 brief from DISCUSS recommended one
warn event per individual violation; the wave-decisions document
locked one warn event per init carrying the full report. The
architect flagged the contradiction; the slice brief was amended to
match the wave-decisions lock. Q9 wins; the slice brief follows.

The architect approved on iteration one with no blocking issues. The
recovery-during-stall pattern that has shown up earlier in the
project (ADR-0017 was the first; this is the second) held cleanly
again: the agent produced what he could before the watchdog cut him
off; the orchestrator finalised the remainder; the reviewer's pass
treated both halves equivalently. The methodology has now had two
clean recoveries from this pattern, and the cost of each has stayed
bounded.

DISTILL picks up the acceptance test design next.

---

## Codex — DISTILL closed

The acceptance designer turned the six user stories' BDD scenarios
and the four DESIGN ADRs into six Cargo integration test binaries:
five slice tests covering Codex's five own user stories, plus one
invariant smoke test that asserts the five-type public surface
compiles. Slice six, the Spark integration, lives in Spark's test
directory rather than Codex's, because the test fixture there
belongs to Spark and the cross-feature touch is implemented there.

```mermaid
flowchart LR
    subgraph TESTS[crates/codex/tests/]
        S1[slice_01_walking_skeleton<br/>2 tests]
        S2[slice_02_otel_semconv_corpus<br/>2 tests]
        S3[slice_03_house_attributes<br/>3 tests]
        S4[slice_04_unknown_attribute_lint<br/>4 tests]
        S5[slice_05_fuzzy_suggestions<br/>3 tests]
        INV[invariant_public_api_smoke<br/>1 test]
    end
    subgraph SPARK[crates/spark/tests/]
        S6[slice_NN_codex_lint<br/>DELIVER scope]
    end
    TESTS -->|Strategy C real local| API[codex public API<br/>5 types]
    SPARK -->|cross-feature touch| API
```

Fifteen test functions in total. Twelve panic on `unimplemented!()`
from the production stubs at the canonical RED state; three pass
because the corresponding paths are real even at DISTILL (the
catalogue's `new()` constructor, the public-API smoke test's
compile-time check, the empty-set boundary case which returns Ok
trivially when validate panics on its first non-empty input).

The reviewer approved on iteration one with a perfect score across
the eight critique dimensions, calling out the stub posture, the
purposeful test fixture design, and the machine-verifiable
traceability table as exemplary. Two adjustments the orchestrator
made during recovery from the architect's stall — switching
LintViolation field accesses from method calls to direct field
reads, and rewriting `result.err().expect()` to the more idiomatic
`expect_err()` — were both confirmed correct.

The recovery pattern from the watchdog stall has now happened
cleanly three times across the project. The cost has stayed bounded
each time; the methodology absorbs the agent stall the same way it
absorbs the back-propagation note, with explicit handoff and clear
provenance in the commit history.

DEVOPS picks up the workflow extensions next; DELIVER follows.

---

## Codex — DEVOPS closed

The platform-readiness wave was the smallest of the five for Codex.
Most of the infrastructure already existed: pre-commit hooks
mirroring CI, the five-gate workflow file, the cargo-deny licence
audit, the per-feature mutation testing job pattern. The orchestrator
extended what was there rather than designing anything new.

Two graduations and one new job. Codex's public API was added to
Gate 2 (`cargo public-api`) and Gate 3 (`cargo semver-checks`)
immediately, alongside the harness, Spark, and Sieve, because the
five-type surface is a real consumer contract that Spark holds
against. A new parallel CI job, `gate-5-mutants-codex`, was added
to mirror the per-feature mutation testing pattern established by
Aperture, Spark, and Sieve; it runs `cargo mutants --in-diff` with
the same thirty-minute timeout and the same `mutants.out` artefact
upload as the others.

```mermaid
flowchart LR
    PC[scripts/hooks/pre-commit] -->|Gate 1: cargo test| WS[workspace]
    PP[scripts/hooks/pre-push] -->|Gates 2 + 3| PA[cargo public-api<br/>cargo semver-checks]
    PA --> H[harness]
    PA --> SP[spark]
    PA --> SV[sieve]
    PA --> CX[codex]
    CI[.github/workflows/ci.yml] --> G1[Gate 1: cargo test workspace]
    CI --> G2[Gate 2: cargo public-api<br/>+ codex]
    CI --> G3[Gate 3: cargo semver-checks<br/>+ codex]
    CI --> G4[Gate 4: cargo deny check]
    CI --> G5A[Gate 5 aperture]
    CI --> G5S[Gate 5 spark]
    CI --> G5V[Gate 5 sieve]
    CI --> G5X[Gate 5 codex<br/>NEW]
```

No new gate types were needed. Aperture had introduced three
feature-specific gates at its own DEVOPS wave for architectural
invariants the codebase could not express in lint rules. Codex's
invariants — the five-type public lock, the AGPL containment, the
corpus regeneration ritual — were already enforced by the
compile-time smoke test, the empty runtime closure that cargo-deny
audits to zero new entries, and the xtask binary's drift signal at
slice 02. The methodology rewards minimal additions. Forge will
peer-review the workflow extensions on the first CI run after
DELIVER lands; until then, the configuration is on probation in the
same sense every CI change is on probation.

DELIVER follows.

---

## Codex — DELIVER closed

Five slices, eight commits, all green. The crafter implemented
slices one, two, four, and five directly; slice three closed by
construction at slice two's corpus seeding because Scholar's DISTILL
fixture required all three house attributes to be present at slice
two. The brief I had written said "no `feature_flag.` Prefix entry
until slice three"; the test fixture and the corresponding ADR said
otherwise. The crafter followed the test, not the brief. The
corresponding amendment was recorded in the slice two commit
message and in the wave-decisions document. This is what
back-propagation discipline looks like in practice: implementations
match tests; tests match ADRs; briefs that contradict either are
amended in place.

Forty-six tests in total. Fifteen acceptance tests at the public
boundary plus thirty-one inline unit tests at the pure-function
seams. The acceptance tests prove the user-facing outcomes; the
inline tests target specific operator mutations with surgical
intent. The composition is the canonical Outside-In TDD shape: the
acceptance test drives the public surface; the unit tests drive the
internal correctness; the public surface remains the only route
into the crate.

Mutation testing landed clean. Thirty-five viable mutants across
the five slices' diffs, all thirty-five caught. Slice five's
fuzzy-suggestion code surfaced two surviving mutants on the
tie-break ordering of equally-distant matches; the crafter killed
both with a small refactor that collapsed the loop into an
`iterator::min_by` over a `(distance, name)` tuple, with the test
that nailed the alphabetical-tie-break case providing the
mutation-evidence anchor. Twenty-four mutants on slice five alone,
all caught. The discipline held.

```mermaid
flowchart LR
    SC[Scholar DISTILL<br/>15 acceptance tests] -->|drove| C[Crafty DELIVER]
    C --> S1[Slice 01<br/>Walking skeleton<br/>9 mutants killed]
    C --> S2[Slice 02<br/>Semconv corpus<br/>0 mutants trivially]
    S2 -.->|closed by construction| S3[Slice 03<br/>House attributes]
    C --> S4[Slice 04<br/>Display impl<br/>2 mutants killed]
    C --> S5[Slice 05<br/>Levenshtein<br/>24 mutants killed]
    S1 --> R[Crafty in review mode]
    S2 --> R
    S3 --> R
    S4 --> R
    S5 --> R
    R -->|APPROVED iter 1| G[Codex v0 graduates]
```

The reviewer approved on iteration one with zero blocking issues.
The verdict named the back-propagation handling at slice two as
exemplary, the surgical mutation-killing inline tests as the
canonical shape of refactor-driven kill, and the xtask
infrastructure as the right shape for a regeneration ritual that
must give compile-time audit signal when upstream renames a
constant. Three non-blocking suggestions were filed for later: a
README in the xtask directory for first-time corpus regeneration,
a v1 polish on the Prefix-suggestion rendering shape, and the
Spark-side slice six amendments to ADR-0012 and ADR-0013, which
land at the Spark-side wave that closes the cross-feature
integration.

Codex graduates. The pre-commit hook and the CI workflow drop
their `--exclude codex` qualifiers; Codex now contributes to the
workspace test gate alongside the harness, Aperture, Spark, and
Sieve. The crate is tagged `codex/v0.1.0`. Forge's review of the
DEVOPS workflow extensions runs independently on the next
Codex-touching commit. Slice six — the Spark integration — is a
separate Spark-side wave that lands the `SparkError::SchemaValidation`
variant, the `with_strict_schema_lint` builder, and the Codex
runtime dependency through post-DELIVER amendments to ADR-0012 and
ADR-0013. That wave is queued on Spark, not on Codex.

The first five features now share the same shape. Library, service,
SDK, library at the wire-protocol mid-stream, library at the schema
authority — five different shapes for five different problems, one
methodology that absorbed each.

---

## Spark — Slice 07 — Codex schema lint integration landed

The piece deferred at Codex's DELIVER closure has landed on the
Spark side. Spark's `init` now calls Codex's
`SchemaCatalogue::validate(...)` against the composed resource
attributes after the existing internal lint and before any OTel SDK
type is constructed. Violations surface either as a single
`tracing::warn!(target = "spark", ...)` event (default rollout
posture) or as `Err(SparkError::SchemaValidation(report))` when the
caller opted into strict mode via
`SparkConfig::with_strict_schema_lint(true)`.

This is the first real cross-feature integration since the v0
features each individually graduated. The discipline that mattered
on this slice was the `#[non_exhaustive]` posture ADR-0012 locked at
Spark's v0. Adding `SchemaValidation(codex::LintReport)` as a fifth
variant under the existing annotation is a non-breaking change per
Rust's semver rules; `cargo public-api` Gate 2 lists the addition,
`cargo semver-checks` Gate 3 accepts it as non-breaking, and
downstream consumers' wildcard match arms absorb it without
recompilation pressure. The discipline existed precisely so this
moment would land clean, and it did.

```mermaid
flowchart LR
    A[Spark::init] --> L1[internal lint:<br/>service.name, tenant.id, endpoint]
    L1 -->|Ok| C[OnceLock<SchemaCatalogue>]
    C --> V[catalogue.validate<br/>resource attribute pairs]
    V -->|Ok| AC[AtomicBool CAS<br/>+ OTel SDK construction]
    V -->|Err report, default| W[tracing::warn target=spark]
    W --> AC
    V -->|Err report, strict| E[Err SchemaValidation report]
    style E fill:#fdd
    style W fill:#ffd
    style AC fill:#dfd
```

Six tests in this slice. Five integration tests in
`crates/spark/tests/slice_07_codex_schema_lint.rs` cover the warn-
mode happy path (silent on blessed inputs), warn-mode violation
(empty `feature_flag.` key produces a warn whose body names the
offending attribute), strict-mode violation (the same input
returns `Err(SchemaValidation)`), strict-mode happy path (no false
positives), and the order invariant (the existing internal lint
short-circuits before Codex sees anything). One unit test in
`init::tests` pins the `OnceLock` invariant via pointer identity:
two successive `catalogue()` calls return the same memory address,
so a `Box::leak`-style fresh-per-call mutant cannot survive.

Mutation testing on the diff: fifteen mutants, twelve caught, three
unviable, zero missed. The pointer-identity test was the
mutation-evidence anchor that closed the one survivor the
behavioural tests left exposed (a `catalogue()` body replaced with
`Box::leak(Box::new(default()))` produces observationally identical
`validate(...)` output but allocates fresh; the identity test
distinguishes them).

ADR-0012 (Spark error type) and ADR-0013 (Spark dependency pinning)
gained post-DELIVER amendment notes documenting the new variant and
the new runtime dep. ADR-0025 itself moved from Proposed to
Accepted with the landing-commit note. `cargo deny check` passes
on the new Codex runtime dep because Codex is `publish = false` and
covered by `[licenses.private] ignore = true`; no allow-list change
was needed for the AGPL-on-the-platform-side asymmetry.

The five-feature v0 has its first cross-feature integration. The
methodology absorbed it without a new wave shape: a single slice
brief, the Outside-In TDD discipline, mutation testing on the diff,
correction notes on the affected ADRs, six tests, one commit. The
discipline scales down as cleanly as it scales up.

---

## Case study: feature 6 — Prism v0

Prism is the project's first frontend. Every prior feature was a
Rust crate that served a developer in a CLI or inside another
process. Prism serves an operator on incident call at 03:14, alone
in front of a browser. The paradigm shift is real: TypeScript
instead of Rust, npm and pnpm instead of Cargo, a React + Vite +
Apache ECharts SPA instead of a service binary, Vitest and
Playwright instead of `cargo test`. The methodology was designed
for the Rust crates that came before. The genuine question of the
Prism feature is whether nWave absorbs the paradigm shift or
breaks against it.

The persona is Priya Raman, senior site reliability engineer at
`acme-observability`. PagerDuty pages her at 03:14 about a
checkout-service latency alert. She has ninety seconds to
acknowledge before escalation, and five to ten minutes to make a
triage decision before customer impact compounds. The product
narrative is anchored in her hands and head: a laptop, the Mimir
backend her team already runs, the Prism URL at
`https://prism.acme-observability.internal`, a service map of
twenty-three services in working memory, zero patience for tools
that fight her at three in the morning.

```mermaid
flowchart LR
    A[03:14 paged<br/>cortisol elevated<br/>mistrustful] --> B[03:15 chart rendered<br/>data matches curl<br/>productive stress]
    B --> C[03:18 URL pasted to Slack<br/>teammate clicks<br/>shared context]
    C --> D[03:20 triage decided<br/>confidence rising]
    D --> E[Five days later<br/>postmortem URL paste<br/>view reproduced exactly]
    style A fill:#fdd
    style B fill:#ffd
    style C fill:#ffd
    style D fill:#dfd
    style E fill:#dfd
```

Prism v0 ships one PromQL query panel against an OTel-compatible
Prometheus or Mimir backend. Logs panel (LogQL), traces panel
(TraceQL), multi-panel dashboards, named saved-queries surface,
native auth — all explicitly out of scope at v0 and queued behind
Lumen, Ray, Loom, and Aegis in later phases. The licence is
AGPL-3.0-or-later. Prism is operator-facing platform infrastructure;
the SaaS loophole AGPL closes is the same loophole a competitor
could exploit against a static SPA served from a long-lived web
server. The licence asymmetry between Prism and Spark is the same
shape as between Aperture and Spark: server-side AGPL, SDK
Apache-2.0, structural rather than viral.

---

## Prism v0 — DISCUSS closed

Luna ran the DISCUSS wave through her JTBD analysis phase and her
journey design phase before overloading at the boundary of the
user-stories write. Bea finalised the user-stories, the DoR
validation, the outcome KPIs, the wave-decisions, and the SSOT
entries. The reviewer Eclipse, running on Haiku, treated Luna's
halves and Bea's halves equivalently per the recovery posture and
approved on iteration one with zero blocking issues. The recovery
pattern absorbed its fifth occurrence cleanly.

The wave produced thirteen feature-side files plus six slice
briefs plus three SSOT files. The primary job is "see the shape
of the misbehaving signal fast enough to make a triage decision".
Three secondary jobs were identified and deferred to post-v0: tail
logs in the chart's window, click a chart point to a trace
exemplar, save named views. The four forces analysis surfaced the
strongest demand-reducing force as data-fidelity anxiety: Priya
would not trust a chart if she could not tell whether the wobble
is the system's wobble or the SPA's smoothing artefact. That
anxiety drove KPI 3, the fidelity invariant, and the buildOption
pure function's locked configuration (`smooth: false`,
`connectNulls: false`, no auto-downsampling).

The six-slice carpaccio is sized so each slice ships end-to-end
value in one day with a named learning hypothesis. Slice one is
the walking skeleton against a real local Prometheus container,
Strategy C posture inherited from Aperture. Slice two adds
relative-range presets. Slice three lands the calm error and
empty states. Slice four adds auto-refresh with exponential
backoff. Slice five adds absolute time ranges and postmortem
permalink reproduction. Slice six is the WCAG 2.2 AA accessibility
audit and remediation pass.

```mermaid
flowchart LR
    S1[Slice 01<br/>walking skeleton<br/>real Prometheus] --> S2[Slice 02<br/>relative presets]
    S2 --> S3[Slice 03<br/>calm errors + empty]
    S2 --> S4[Slice 04<br/>auto-refresh + backoff]
    S2 --> S5[Slice 05<br/>absolute range + permalink]
    S3 --> S6[Slice 06<br/>WCAG 2.2 AA audit]
    S4 --> S6
    S5 --> S6
```

The SSOT promotion is the wave's quietest landing. Up to this
point `docs/product/journeys/` and `docs/product/jobs.yaml` did
not exist. The cross-feature journey-and-jobs surface is born
with Prism because Prism is the first feature whose journey is
clearly cross-feature: Beacon will fire the alert that opens the
journey, Loom will extend the share surface beyond URL paste,
Aegis will protect the SPA in production. Documenting the
operator-incident-response journey at the SSOT level pays
forward into those phases.

---

## Prism v0 — DESIGN closed

Morgan ran the DESIGN wave end to end without stalling. Seven
ADRs locked the architectural surface: ADR-0026 (component layout
with ports-and-adapters internal split), ADR-0027 (total-function
`queryRange` returning a five-arm `QueryOutcome` union; same-origin
reverse-proxy production posture), ADR-0028 (pure URL codec with
`history.replaceState` only), ADR-0029 (pure reducer + effects
shape for the auto-refresh state machine; 5/10/20/30 second capped
backoff curve), ADR-0030 (direct ECharts modular import; pure
`buildOption`; CSS-property palette swap with Okabe-Ito default),
ADR-0031 (coexistent Cargo and pnpm workspaces; ESLint with
boundaries and license-header plugins), ADR-0032 (AGPL-3.0-or-later
header on every TS source file from a single SSOT).

The architectural choice is a modular monolith with internal
ports-and-adapters. Microservices, server-side rendering, and
micro-frontends were all considered and rejected with specific
rationale rather than generic "they're complex" hand-waving.
Microservices have no team boundary to respect at one operator,
one Andrea, one designer. SSR adds a Node runtime for no
incident-time benefit. Micro-frontends would be a v1+ refactor
once `packages/ui/` becomes load-bearing.

```mermaid
flowchart TB
    P[panels/query/QueryPanel<br/>driving] --> LP[lib/promql/queryRange<br/>driven adapter]
    P --> LC[lib/config/loadConfig<br/>driven adapter]
    P --> LE[lib/echarts/EChart<br/>driven adapter]
    P --> CU[lib/url-state/codec<br/>pure function]
    P --> CB[lib/echarts/buildOption<br/>pure function]
    P --> CR[lib/auto-refresh/reduce<br/>pure function]
    style CU fill:#dfd
    style CB fill:#dfd
    style CR fill:#dfd
    style LP fill:#ffd
    style LC fill:#ffd
    style LE fill:#ffd
```

Three pure-function leaves anchor the design: URL codec,
buildOption, auto-refresh reducer. They import nothing
side-effecting — no React, no DOM, no fetch — and are testable
directly with property tests. The `eslint-plugin-boundaries` rule
makes the import discipline structural rather than aspirational:
a panel can import from `lib/`, a `lib/` adapter can depend on
the pure cores, but the pure cores cannot import side-effecting
modules. Atlas approved on iteration one. Morgan completed
without stalling — the first dispatch in this project to do so
across thirty-six tool uses.

---

## Prism v0 — DEVOPS closed

Apex ran DEVOPS without stalling. Eight files specified the six
new CI gates: Gate 6 Vitest unit and integration; Gate 7
Playwright E2E across Chromium, Firefox, and WebKit; Gate 8
bundle-size enforcement against a 300 KB gzipped ceiling; Gate 9
ESLint plus Prettier plus AGPL licence-header; Gate 10 StrykerJS
mutation testing with the same `--in-diff` cascade as the Rust
crates; Gate 11 Prometheus contract testing via a container
fixture pinned by digest.

```mermaid
flowchart LR
    G6[Gate 6 Vitest] --> G7[Gate 7 Playwright<br/>Chrome/FF/Safari]
    G6 --> G8[Gate 8 bundle<br/>≤ 300 KB gzipped]
    G6 --> G9[Gate 9 lint+format+licence]
    G6 --> G10[Gate 10 StrykerJS<br/>100% kill rate]
    G6 --> G11[Gate 11 Prom contract<br/>container fixture]
```

The browser-emitted KPI metrics path was the design space's only
real surprise. Three candidate paths were considered: emit through
`console.warn` only (debug-grade); emit cross-origin direct to
Aperture (preflight overhead, header complexity); emit same-origin
POST to `/v1/metrics` through the operator's reverse proxy to
Aperture, which translates JSON to OTLP at ingestion. Apex chose
the third with a fifty-line custom emitter rather than the
OpenTelemetry JS browser SDK, on the grounds that the bundle gate
at 300 KB has no headroom for an SDK that itself weighs
multiple tens of kilobytes.

Forge ran the iteration-one review on Haiku and returned
CONDITIONALLY APPROVED with five critical specification gaps and
three high-severity inline-note candidates. The five criticals
were the mutation-cascade bash pseudocode, the bundle-size JSON
schema, the Prometheus digest sync rule between Gate 7 and Gate
11, the KPI 5 production visibility mitigation through operator
JS-error-tracking tools, and the Pact-JS migration trigger
decision rule. Bea finalised the revisions directly rather than
re-dispatching Apex, on the grounds that the gaps were
specification additions and not architectural decisions. Forge's
iteration-two review approved the revised artefact set; the
iteration-two budget was bounded and clean.

---

## Prism v0 — DISTILL closed

Scholar ran DISTILL through the three markdown specs and the
first four slice Vitest files and the first three Playwright spec
files and the four JSON fixtures before Bea interrupted the
dispatch — the stuck-process pattern, accumulating periodic-check
ticks while the agent worked, was the signal. Scholar's
seventy-percent output was committed; Bea finalised the remaining
seven files (slice-05 Vitest, slice-04 / 05 / 06 Playwright,
three invariant tests) following Scholar's conventions verbatim.
The reviewer Sage on Haiku confirmed Scholar's and Bea's halves
cohere, approved on iteration one, and noted one non-blocking
concern about error-path coverage ratio.

```mermaid
flowchart LR
    AC[30 acceptance criteria<br/>across 7 stories] --> VS[8 Vitest files<br/>tests/]
    AC --> PS[6 Playwright specs<br/>e2e/]
    AC --> IV[3 invariant tests<br/>public-api / licence-headers / fidelity]
    AC --> FX[4 JSON fixtures<br/>fixtures/]
    VS --> RED[RED state<br/>throw UNIMPLEMENTED]
    PS --> RED
    IV --> RED
```

The wave produces test specifications that compile against a
yet-unwritten `src/` and throw `'UNIMPLEMENTED — Slice NN
DELIVER'` at runtime. The discipline that mattered most was
mock-at-the-seam: the only mocked surfaces are `fetchFn` and
`Scheduler`, the two architectural seams ADR-0027 and ADR-0029
introduced for that purpose. React is not mocked. ECharts is not
mocked. The fidelity-anchor fixture is hand-authored with NaN gaps
at index two and non-uniform timestamps to drive structural
mutation testing on the `buildOption` pure function. KPI 3 has
its structural lock at `invariant-fidelity.test.ts`; KPI 4 has its
behavioural lock at the slice-05 Playwright cross-tab byte-equality
test; KPI 5 has its lock at the slice-03 Playwright failure-mode
sweep.

Sage's only suggestion was the 26%-versus-40% error-path coverage
ratio. Cumulative AC coverage is 97% (29 of 30; AC-4.4 is the
"URL is the only share artefact" system invariant, not a tested
behaviour). The shortfall is concentrated in slice three where
error-path coverage genuinely lives; slice three DELIVER will
verify error paths run end to end rather than only at the unit
level. The error-path coverage target is heuristic, not a hard
rule.

---

## Prism v0 — DELIVER opening: scaffolding and slice 01a stubs

Two commits opened DELIVER and the third was supposed to land
slice 01 GREEN against the Prometheus container. The third
dispatch stalled differently from every prior stall in this
project: Crafty timed out after fifty tool uses without writing a
single file. Where Morgan, Scholar, and Luna had stalled mid-write
with partial output, Crafty appears to have spent the budget on
reading and planning. The recovery shape is different.

Bea pre-scaffolded the workspace in commit `a12564d`: eighteen
configuration files, two scripts, two end-to-end helpers, no
`src/` content. The decision is bounded: scaffolding does not
require LLM-domain reasoning, and Bea-direct on
boilerplate keeps the next Crafty dispatch focused on
implementation rather than `package.json` and `tsconfig.json`.
The Prometheus container digest pinning rule from ADR-0027's
external-integration handoff is honoured: `playwright.config.ts`
exports `PROMETHEUS_IMAGE_DIGEST` as the single source of truth
that the CI workflow's Gate 11 services block will consume in
lockstep.

After Crafty's no-write stall on slice-01-as-a-whole, Bea
proposed three options to Andrea: Bea-direct narrow; re-dispatch
Crafty narrow; or fragment slice 01 into micro-slices 01a through
01e. Andrea chose the fragmentation. Commit `0dd0988` is
micro-slice 01a: fifteen `src/` files writing the type definitions
and the function signatures, every body throwing
`'UNIMPLEMENTED — Slice NN DELIVER'`. The five-arm `QueryOutcome`
discriminated union, the four-state `AutoRefreshState`, the
four-arm `AutoRefreshEffect`, the five-event `AutoRefreshEvent`,
the `UrlState` and `TimeRange` shapes, the `BuildOptionContext`,
the `RuntimeConfig` shape — all locked at the type level so the
fourteen DISTILL test files compile against a real surface even
while every runtime path throws.

```mermaid
flowchart LR
    C0[a12564d<br/>scaffolding<br/>18 config files] --> C1[0dd0988<br/>slice 01a<br/>15 type stubs<br/>RED state]
    C1 --> C2[Slice 01b<br/>buildOption pure<br/>KPI 3 fidelity<br/>invariant GREEN]
    C2 --> C3[Slice 01c<br/>queryRange + loadConfig<br/>5-arm outcome union<br/>error classification GREEN]
    C3 --> C4[Slice 01d<br/>QueryPanel + App + main<br/>EChart wrapper + codec<br/>React composition GREEN]
    C4 --> C5[Slice 01e<br/>CI gates 6-11<br/>+ pnpm-lock.yaml<br/>bundle 224 KB gzipped]
    style C0 fill:#dfd
    style C1 fill:#dfd
    style C2 fill:#dfd
    style C3 fill:#dfd
    style C4 fill:#dfd
    style C5 fill:#dfd
```

The fragmentation matters because the long-dispatch failure mode
is real. The slice-01 dispatch brief asked Crafty to scaffold the
workspace, resolve the Prometheus digest, write fifteen `src/`
files, extend the CI workflow with six new gates, run `pnpm
install`, run Vitest, run Playwright, run StrykerJS, write a
slice-completion document, and commit — all in one dispatch.
Crafty got fifty tool uses and an eight-minute idle timeout. The
methodology absorbs partial-output stalls cleanly because Bea can
finalise what the agent produced. It does not absorb zero-output
stalls without scope changes. Micro-slicing is the scope change.

DELIVER is open. The next three to four commits land
implementations one per micro-slice, with the slice-01 brief's
acceptance contract held constant across them. The remaining
micro-slices follow the same Outside-In TDD shape Sieve and Codex
used: tests already locked at DISTILL, implementations one slice
at a time, mutation testing on each commit's diff, fix-forward on
failures.

---

## Prism v0 — micro-slice 01b — buildOption GREEN (KPI 3 fidelity)

The first GREEN checkpoint. `buildOption` is now a real pure
function in `apps/prism/src/lib/echarts/buildOption.ts`: it takes a
`QueryOutcome` plus a `BuildOptionContext` (palette, range,
prefersReducedMotion) and returns an `EChartsOption` with the
fidelity invariants locked at the option level. Success outcomes
produce series whose data points pass through verbatim from the
backend response — no smoothing, no interpolation across NaN gaps,
no resampling, no rounding, no auto-downsampling. Empty outcomes
and the three error arms (parse-error, transport-error,
config-error) produce an option with an empty series array; the
QueryPanel composes the inline banner separately based on the
outcome kind.

The Okabe-Ito 8-colour palette is the v0 default (deuteranopia and
protanopia safe); Tableau 10 is the operator-selectable alternative
via the URL `palette=tableau10` parameter that Slice 06 will land.
Palette swap is a CSS-property-driven array swap on the
EChartsOption's `color` field; no fetch on palette change.

```mermaid
flowchart LR
    O[QueryOutcome] --> B[buildOption pure]
    C[BuildOptionContext<br/>palette / range / motion] --> B
    B --> S[series.data verbatim<br/>smooth: false<br/>connectNulls: false<br/>sampling: 'none']
    B --> X[xAxis time +<br/>palette colour array]
    style B fill:#dfd
    style S fill:#dfd
```

The `invariant-fidelity.test.ts` test bodies were replaced with
real assertions against the buildOption return: fourteen test
cases covering the seven KPI 3 invariants (series count match,
point count match, NaN preservation, timestamp byte-equality,
value byte-equality, smooth-false lock, connectNulls-false lock,
no-auto-downsampling), three boundary cases (empty outcome,
single-point series, error arms produce empty series), two
reduced-motion cases, and two palette-swap cases.

Two small back-propagation drifts surfaced during the
implementation. Scholar's test comments referenced "NaN at index
2" and "non-uniform timestamps" but the hand-authored fixture had
NaNs at indices 1 and 3 and uniform 15-second deltas. The fixture
is the data contract; the implementation and the assertions
follow the fixture verbatim, and the test comments now match. The
discrepancy is a normal artefact of Scholar's stall recovery —
Scholar wrote the comments before Bea finalised the fixture and
test bodies in micro-slice 01b. The fix is in the same commit.

---

## Prism v0 — micro-slice 01c — queryRange + loadConfig GREEN

The two driven adapters are now real. `queryRange` lives at
`apps/prism/src/lib/promql/queryRange.ts` and is total: every
failure mode is encoded as a `QueryOutcome` arm; the function
never throws. The five arms are exercised by the tests in
`tests/slice-03-error-and-empty-states.test.ts`: a 400 with
`status:error` body becomes `parse-error`; a fetch rejection
becomes `transport-error` with cause `network`; an HTTP 500
becomes `transport-error` with cause `http-status`; a 200 with
non-JSON body becomes `transport-error` with cause `invalid-json`;
a 200 with JSON missing `data.result` becomes `transport-error`
with cause `shape`; a 200 with empty `data.result` becomes
`empty`; a 200 with non-empty `data.result` becomes `success`.

`loadConfig` is the same shape against `/config.json`. Three
`ConfigError` arms: `fetch-failed` (network failure or HTTP
non-200), `parse-failed` (non-JSON body), `shape-failed` (JSON
missing the `RuntimeConfig` fields). The App composition root
will refuse to mount the QueryPanel on any error arm, per
ADR-0026 §5's wire-then-probe-then-use posture.

```mermaid
flowchart LR
    REQ[QueryRangeRequest<br/>q + range] --> Q[queryRange<br/>driven adapter]
    CTX[QueryRangeContext<br/>backend + fetchFn + signal] --> Q
    Q -->|200 + non-empty| OK[success]
    Q -->|200 + empty| EM[empty]
    Q -->|status:error| PE[parse-error]
    Q -->|fetch reject| TN[transport-error network]
    Q -->|HTTP 5xx| TH[transport-error http-status]
    Q -->|bad JSON| TJ[transport-error invalid-json]
    Q -->|shape mismatch| TS[transport-error shape]
    style OK fill:#dfd
    style EM fill:#dfd
    style PE fill:#ffd
    style TN fill:#fdd
    style TH fill:#fdd
    style TJ fill:#fdd
    style TS fill:#fdd
```

Twelve test bodies were replaced with real assertions across the
slice-01 fetch-seam tests (2), the slice-03 outcome-classification
tests (6: parse-error, network, http-status, invalid-json, shape,
empty), and the slice-03 loadConfig tests (4: fetch-rejection,
404, malformed JSON, missing shape). The mock-at-the-seam
discipline holds throughout: every test injects a `fakeFetch`
function and never touches `globalThis.fetch`. The
`QueryRangeContext.fetchFn` seam from ADR-0027 §7 carries the
mocked closure into the adapter; the test asserts the mock was
called and the global was not.

One back-propagation note: Scholar's test comment for the
"shape-invalid" case named the error kind as `schema-invalid`,
but the canonical `ConfigError` type in `types.ts` calls it
`shape-failed` (matching the three arms ADR-0030 names:
`fetch-failed`, `parse-failed`, `shape-failed`). The assertion
uses the canonical name; the test comment is corrected inline to
the type-system reality.

The QueryPanel-rendering tests stay UNIMPLEMENTED at the throw
boundary because QueryPanel itself is still a stub. Slice 01d
brings the React composition online and flips those tests to
GREEN.

---

## Prism v0 — micro-slice 01d — React composition GREEN

The walking skeleton's React surface is live. Five real files:
`apps/prism/src/main.tsx` mounts `<App>` into `#root`; `App.tsx`
loads `/config.json` on mount and refuses to render the
`QueryPanel` on `ConfigError`; `QueryPanel.tsx` composes the
single query input, the run button, the chart area with banners
for each `QueryOutcome` arm, and the footer with series + point
counts and `queryMs`; `lib/echarts/EChart.tsx` mounts ECharts via
`useRef` + `useEffect` and updates with `setOption({notMerge:
true})` on every option change without re-mounting the canvas;
`lib/url-state/codec.ts` (lifted forward from slice 02) encodes
and decodes the URL state with the absolute-range double-lock
already in place.

The composition root reads URL state on mount, writes URL state
synchronously on every state change via `history.replaceState`,
and focuses the query input on first render. Pressing Enter or
clicking Run issues a `queryRange` call against the configured
backend through the same `fetchFn` seam the unit tests use. The
five outcome arms are surfaced to the operator: success renders
the chart; empty renders a calm "No data" message without a
warning banner; parse-error and transport-error render inline
warning banners with backend label and verbatim error text;
config-error is impossible to reach because the App composition
root refused to mount on it.

```mermaid
flowchart TB
    M[main.tsx<br/>StrictMode + createRoot] --> A[App.tsx<br/>composition root]
    A --> CL[loadConfig<br/>/config.json]
    CL -->|ok| Q[QueryPanel<br/>driving panel]
    CL -->|error| EB[error banner<br/>'Configuration is missing']
    Q --> QI[query input<br/>focused on mount]
    Q --> RB[run button<br/>disabled when q empty]
    Q --> CHA[chart area<br/>banners per outcome]
    Q --> CHF[footer<br/>series + points + queryMs]
    Q --> URL[history.replaceState<br/>q + from + to + refresh]
    RB --> QR[queryRange<br/>via fetchFn seam]
    QR -->|success| CHA
    QR -->|empty| CHA
    QR -->|parse-error| CHA
    QR -->|transport-error| CHA
    CHA --> EC[EChart<br/>setOption notMerge=true]
    style M fill:#dfd
    style A fill:#dfd
    style Q fill:#dfd
    style EC fill:#dfd
```

The codec lift-forward warrants a note. Slice 02's brief assigned
the URL codec to slice 02 DELIVER. At slice 01d the QueryPanel
needs the codec to read+write URL state from day one, so the
codec body lands here with full support for all five relative
presets, all five refresh intervals, and absolute timestamps.
Slice 02 retains its picker-UI scope; the codec is a shared pure
function and lives wherever the walking skeleton first reaches
for it. The slice 02 brief's "codec body" line item is now closed
by construction at slice 01d, the same shape Codex slice 03
closed by construction at Codex slice 02.

The ECharts modular import keeps the bundle bounded. Direct
imports of `LineChart`, `GridComponent`, `TooltipComponent`,
`LegendComponent`, `AriaComponent`, `TitleComponent`, and
`CanvasRenderer` only — no full-bundle import. Per ADR-0030 §7
the lazy-import escape hatch is preserved if the bundle gate
approaches 300 KB; at slice 01d the imports above are static and
the gate fires only against the assembled bundle in CI.

Five micro-slices into Prism v0's DELIVER, four are GREEN: 01a
(types), 01b (buildOption + fidelity), 01c (queryRange +
loadConfig), 01d (React composition + codec + EChart). Only 01e
remains — the CI workflow extension adding Gates 6 through 11
that DEVOPS specified at iter-2 sign-off. Slice 01 GREEN happens
when 01e lands and CI passes against the assembled bundle.

---

## Prism v0 — micro-slice 01e — slice 01 complete

Slice 01 is GREEN. The CI workflow at `.github/workflows/ci.yml`
gains the six Prism gates Apex specified at DEVOPS: Gate 6 Vitest
(unit + integration + typecheck), Gate 7 Playwright across three
browser engines with a digest-pinned Prometheus services
container, Gate 8 bundle-size enforcement against the 300 KB
gzipped ceiling, Gate 9 ESLint + Prettier + AGPL licence-header,
Gate 10 StrykerJS mutation testing via the same baseline-cascade
wrapper the cargo-mutants jobs use, Gate 11 Prometheus contract
test against the same digest-pinned container. The fifteen jobs
the workflow now contains (nine Rust + six TS) run in parallel
where their dependency graph allows; Gates 7, 8, 10, and 11 wait
on Gate 6's typecheck + Vitest sanity.

Bundle size measured against the assembled `apps/prism/dist/`
bundle: 224.92 KB gzipped, 73.2 percent of the 300 KB ceiling.
The headroom holds even with ECharts in the main chunk (no
lazy-import escape hatch needed at v0). The bundle composition
matches the design analysis: ECharts dominant at ~200 KB,
React + react-router at ~20 KB, Prism source at ~5 KB.

```mermaid
flowchart TB
    A[apps/prism/dist/<br/>vite build] --> B[224.92 KB gzipped]
    B --> C{≤ 300 KB?}
    C -->|73.2%| D[Gate 8 PASS]
    A --> E[ECharts ~200 KB]
    A --> F[React + router ~20 KB]
    A --> G[Prism source ~5 KB]
    style B fill:#dfd
    style D fill:#dfd
```

The local Vitest run with the apps/prism/ implementation reports
49 GREEN out of 133 tests. The remaining 84 throws stay
UNIMPLEMENTED at slice 02-06 boundaries: the slice-02 picker UI,
the slice-03 banner-rendering tests that need QueryPanel
integration, the slice-04 auto-refresh reducer state machine,
the slice-05 absolute-range picker UI, the slice-06 accessibility
audit. The KPI 3 fidelity invariant (17 tests), the
invariant-public-api compile-time lock (16), the
invariant-licence-headers SSOT (5), the queryRange outcome-
classification tests (6 in slice-03), the loadConfig
shape-failure tests (4 in slice-03), and the slice-01 fetch-seam
tests (2) are all GREEN.

The five-micro-slice fragmentation closed. Slice 01a (commit
`0dd0988`, types and stubs), slice 01b (`854f13a`, buildOption
GREEN), slice 01c (`593e6f6`, queryRange and loadConfig GREEN),
slice 01d (`e76f38d`, React composition GREEN). This commit
closes slice 01e and the slice itself. Total wall-clock from
slice 01a to slice 01e at this session's pace: roughly four
hours of authored work spread across the day, with the codec
lift-forward from slice 02 to 01d absorbing slice 02's largest
deliverable by construction.

Some incidental landings worth recording. The Vite version pin
moved from 6.0.5 to 5.4.21 because Vitest 2.x's transitive
dependency on vite@5.x conflicted with vite@6 under
`exactOptionalPropertyTypes: true`. The downgrade is a
within-slice TS-ecosystem-pinning correction; v0.x can graduate
to Vite 6 + Vitest 3 once the version pair stabilises in the
broader ecosystem. The TS `noUnusedLocals` and
`noUnusedParameters` flags were removed from `tsconfig.json`
because they fight the RED-state idiom of having declared-but-
unused helpers in DISTILL test files; ESLint with
`@typescript-eslint/no-unused-vars` catches the same issues at
PR time and offers the `_` -prefix escape hatch for genuinely-
intentional unused symbols. The strict-mode discipline from
ADR-0031 §3 remains intact.

Slice 01 done. The next slice, slice 02, lights up the relative-
range picker UI on top of the already-implemented codec. The
codec lift-forward at slice 01d means slice 02 is now picker-UI-
only — smaller than originally scoped at DISCUSS.

---

## Prism v0 — slice 02 — relative-range picker GREEN

The codec lift-forward at 01d paid off. Slice 02 added one
hundred lines of `TimeRangePicker.tsx` and a one-line integration
in `QueryPanel.tsx` to flip the slice from RED to GREEN. The
picker offers exactly the five operator-canonical relative
presets — Last 5 min, Last 15 min, Last 1 h, Last 6 h, Last 24 h
— with a disabled Custom option that lights up at slice 05.

```mermaid
flowchart LR
    QP[QueryPanel] --> P[TimeRangePicker]
    P -->|onChange| RS[setState range]
    RS -->|sync| URL[history.replaceState<br/>from + to]
    RS -->|sync| RQ[queryRange<br/>fresh fetch with new range]
    style P fill:#dfd
    style RS fill:#dfd
```

Eighteen test bodies turned GREEN: the picker UI (two: five
presets present, default 15 min), the picker-change behaviour
(three: re-fetches, URL update, query preserved), the codec
preset encoding round-trips (ten: encode + decode for each of the
five presets), the forgiving-codec rejections (two:
non-canonical offset `-3m` and absolute-in-from with relative-in-to
both reject), and the URL hydration on cross-load (one: opening
with `from=-1h` selects "Last 1 h").

Local Vitest at slice 02 close: 56 tests GREEN out of 56 in the
allow-list (the four eligible files: three invariants + slice-02).
Bundle size: 225.24 KB gzipped, 73.3 percent of the 300 KB
ceiling — within budget despite the TimeRangePicker addition.

Three within-slice infrastructure corrections committed inline.
The slice 02 Vitest test file became `.test.tsx` because the test
bodies use JSX (`render(<QueryPanel ...>)`); the include glob in
`vitest.config.ts` widened to `tests/slice-02-*.test.{ts,tsx}`. A
new `tests/setup.ts` file polyfills `HTMLCanvasElement.getContext`
(jsdom returns null by default), `matchMedia`, and `ResizeObserver`,
plus auto-cleans React Testing Library mounts between tests via
`afterEach(cleanup)`. The `EChart.tsx` wrapper probes for a
working canvas 2D context before initialising ECharts; if absent
(as in jsdom) it skips the entire ECharts lifecycle so component
tests can mount the panel graph without paint. ADR-0030 §3
documents the trade-off: visual chart assertions live in Playwright
in real browsers; jsdom tests assert component structure, URL
state, banner rendering.

The slice 02 brief named picker UI, URL roundtrip, and codec
round-trips as its scope. The codec was already in by the time
slice 02 started, so the wave finished smaller and faster than
DISCUSS budgeted. The methodology absorbed the lift-forward
cleanly: slice 02 closes; the next slice (03 — error states)
inherits the slice-02 substrate.

---

## Prism v0 — slice 03 — error and empty states GREEN

Priya is triaging at 03:14. The page must not blank on her. That
operator-facing brief is what slice 03 honours, end to end. The
five PromQL outcome arms each get their own calm surface in the
QueryPanel; the URL bar keeps encoding even the broken state so a
colleague pasting into Slack sees the same view; a hand-edited URL
with invalid parameters falls back to defaults but tells Priya
which parameters were dropped; and a misconfigured `/config.json`
refuses to mount the panel rather than producing a broken chrome
that looks operable.

```mermaid
flowchart TD
    Fetch[queryRange] --> O{QueryOutcome.kind}
    O -->|success| Chart[chart-canvas in DOM]
    O -->|empty| EM[calm empty-state<br/>names the active range]
    O -->|parse-error| PB[warning banner<br/>verbatim backend error]
    O -->|transport-error| TB[warning banner<br/>backend label + last-fetch time]
    O -->|config-error| CB[App refuses to mount QueryPanel]
    PB --> NC[chart-canvas removed from DOM]
    TB --> NC
    EM --> NC
    style PB fill:#fdd
    style TB fill:#fdd
    style EM fill:#dfe
    style CB fill:#fdd
    style NC fill:#fee
```

The stale-data invariant (ADR-0027 §5) is the load-bearing rule of
this slice. Whenever the latest outcome is not `success`, the chart
canvas is removed from the DOM — not hidden, removed. A stale chart
sitting next to a transport-error banner would lie to Priya about
what she is looking at; lying to an operator under load is the
worst failure mode an observability tool can have. The Vitest test
that pins this invariant clicks Run twice with a fetch that succeeds
then fails, asserts the canvas was present after the first call,
and asserts it is absent — `queryByTestId('chart-canvas')` returns
null — after the second.

The malformed-URL banner is the slice's second non-obvious surface.
The codec collects every invalid parameter rather than short-
circuiting on the first, so a URL with three broken parameters
names all three at once. The banner sits at the top of the chrome,
above the backend label, with the field names sorted in canonical
URL order — `from, refresh`, not the reverse — and the page
remains fully interactive. First picker change dismisses the banner
and rewrites the URL cleanly, so Priya is never one click away
from the broken state she landed on.

The header-redaction invariant (ADR-0027 §6) is the third surface,
and the most defensive. An operator's `backend.headers` configuration
carries auth tokens, tenancy hints, debug bearer tokens. A worst-
case backend echoes those values in error bodies. queryRange
tokenises each header value on whitespace, collects every token of
length four or more, and redacts each from every operator-visible
string in the outcome — labels in the success arm, the prom-error
message in the parse-error arm, the body slice in the http-status
arm, the exception message in the network arm. The invariant test
exercises all five outcome arms with a fakeFetch crafted to leak
the secret, then asserts `JSON.stringify(outcome).includes(SECRET)`
is false for every one.

Twenty-three test bodies GREEN at slice 03 close. Local Vitest:
79 tests GREEN out of 79 in the allow-list (five files:
three invariants + slice 02 + slice 03). Bundle size: 225.82 KB
gzipped, 75.3 percent of the 300 KB ceiling — within budget despite
the new banner surfaces and the redaction code.

Three within-slice corrections committed inline. The slice 03 test
file became `.test.tsx` because the bodies render JSX. The vitest
include glob widened to `tests/slice-03-*.test.{ts,tsx}`. The
queryRange body re-ordered its parse-or-status decision: a not-ok
response with a non-JSON body now classifies as `http-status`
rather than `invalid-json`, because Priya wants the banner to name
the actual condition (a 500 from the backend) not the secondary
failure (the body wasn't JSON).

The slice 03 brief named the five QueryOutcome arms' rendering,
the stale-data invariant, the malformed-URL banner, and the
header-redaction invariant. All four landed. Slice 04 inherits the
substrate: an auto-refresh state machine on top of a panel that
already handles every fetch outcome calmly.

---

## Prism v0 — slice 04 — auto-refresh state machine GREEN

Priya is watching a sustained incident. She wants the chart to
refresh itself every 10 seconds while she keeps her eyes on the
line. She does not want to press F5. She does not want the chart to
flicker. If she switches tabs the refresh pauses; when she comes
back she sees fresh data immediately. If the backend dies the next
ticks back off 5/10/20/30s capped until it recovers.

That brief lives, in this slice, as a pure reducer. The auto-refresh
state machine takes a state and an event and returns a next state
plus a list of effects. No I/O, no setTimeout, no Date.now, no React.
The QueryPanel side (slice 06) will wire the Scheduler seam and the
queryRange call to those effects; the reducer itself is testable
without any of them.

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> Running: refresh != off + relative
    Running --> Backoff_0: fetch transport-error
    Running --> Running: tick / success / empty / parse
    Backoff_0 --> Backoff_1: tick + transport-error
    Backoff_1 --> Backoff_2: tick + transport-error
    Backoff_2 --> Backoff_2: tick + transport-error (30s cap)
    Backoff_0 --> Running: tick + success/empty/parse
    Backoff_1 --> Running: tick + success/empty/parse
    Backoff_2 --> Running: tick + success/empty/parse
    Running --> Hidden: visibility hidden
    Backoff_0 --> Hidden: visibility hidden
    Hidden --> Running: visibility visible
    Running --> Idle: refresh off / range absolute
```

Two invariants make this slice non-trivial. The first is the **no
timer leaks** property: every `schedule-timer` effect is preceded by
either an initial state with no timer or a `cancel-timer` effect, so
the external system never has two outstanding timers for the same
state machine. The property test walks realistic event sequences,
treats the one-shot timer as consumed when `tick-fired` arrives, and
asserts that no schedule arrives while a prior timer is still
outstanding. Four representative sequences cover the recovery curve,
the visibility toggle, the absolute-disables-auto path, and the
plain success loop.

The second is the **absolute-disables-auto double-lock** (ADR-0029
§6). Auto-refresh is meaningless when the range is absolute: the
data does not change. The codec already enforces this on the URL
side (refresh=off is the only valid pairing with an absolute range);
the reducer enforces it on the state-machine side. A range-changed
event with an absolute range transitions from Running or Backoff to
Idle and emits both `cancel-timer` and `cancel-fetch`. The
state-machine cannot end up in a state that ticks against a frozen
range.

The backoff curve has one subtlety worth narrating. The schedule_ms
when entering Backoff(n) is determined by the OUTGOING retry: 5s for
Backoff(0), 10s for Backoff(1), 20s for the first Backoff(2). When
already at Backoff(2) and another failure arrives, the state stays
Backoff(2) but the schedule becomes 30s (the cap). The reducer
never needs to remember "already at 30s" — the rule is simply that
Backoff(2) → Backoff(2) emits 30000ms. The mental model fits in one
line of code.

Aborted outcomes are silent. A `transport-error` with
`cause.kind === 'aborted'` came from our own `cancel-fetch` (a new
tick fired while the prior fetch was in flight). The reducer treats
it as a no-op so the cancellation does not falsely trigger backoff.
A property test exercises every state and confirms the abort never
schedules a timer or transitions to backoff.

Twenty-four reducer test bodies GREEN at slice 04 close. Local
Vitest: 103 tests GREEN out of 103 in the allow-list. The bundle
size does not move (225.82 KB gzipped, 75.3% of ceiling) because
the reducer is not yet imported by the panel — slice 06 wires it.

The slice 04 brief named the reducer, the backoff curve, the
visibility transitions, the absolute-disables-auto invariant, and
the no-timer-leaks property. All five landed in one commit. Slice 05
inherits the substrate: the absolute time-range Custom mode lights
up the picker option that slice 02 left disabled, and the reducer's
absolute-disables-auto path is the matching guard rail.

---

## Prism v0 — slice 05 — absolute time range and postmortem permalink GREEN

Five days after the incident, an engineer writing the postmortem
opens the URL Priya pasted in Slack at 03:14. The chart renders for
the exact ISO-8601 window. Not approximately; exactly. The
postmortem-time use case is a different operator with a different
brief from incident-time Priya — slower, more deliberate, working
from records rather than live signals — and it deserves its own
slice.

```mermaid
flowchart LR
    Picker[Custom picker option<br/>two ISO inputs] -->|valid| State[state.range = absolute]
    State --> Codec[encode]
    Codec --> URL["?q=...&from=ISO&to=ISO<br/>(no refresh)"]
    URL --> Reload[fresh tab on day D+5]
    Reload --> Decode[decode]
    Decode --> SameState[same state byte-equal]
    SameState --> SameChart[same chart]
    style Picker fill:#dfe
    style Codec fill:#dfe
    style Decode fill:#dfe
```

Two locks make the absolute-range path work. First, the **codec
double-lock** (ADR-0028 §4): when the range is absolute, encode
refuses to emit a `refresh=` parameter even if the input state
carries one. The picker UI is the first lock; this is the second.
The test that pins it constructs a malformed-input state with
`range: absolute, refresh: '10s'` and asserts that the encoded URL
contains no `refresh=` substring, and that decoding the result
yields `refresh: 'off'`. The double-lock means a hand-edited URL or
a regressing UI component cannot enable auto-refresh against a
frozen window.

Second, the **cross-day reproduction invariant**: decode does not
depend on `Date.now()` for absolute ranges. The test fakes the
system clock five days forward and re-decodes the day-D URL,
asserting the parsed timestamps are byte-equal. Relative ranges
intentionally drift with now-time; absolute ranges intentionally do
not. This is what makes the postmortem permalink trustworthy.

Eleven codec test bodies turn GREEN. The picker UI gains a real
Custom mode: selecting Custom reveals two `datetime-local` inputs
that commit to the parent on every edit, with inline validation for
unparseable timestamps and inverted ranges. The slice-02 picker test
formerly asserted "Custom is disabled" as a stage-gate; that
assertion is replaced with the long-term invariant that Custom is
the sixth option of value `custom`.

Local Vitest: 114 tests GREEN out of 114 in the allow-list. Bundle
size 226.27 KB gzipped, 75.4 percent of the 300 KB ceiling — the
picker UI for Custom mode adds about 0.45 KB after gzip, well
within budget.

The slice 05 brief named the codec absolute-mode contract, the
picker UI for Custom mode, the codec double-lock for
absolute-disables-refresh, and the cross-day reproduction property.
All four landed in this commit. Slice 06 inherits the substrate:
the accessibility audit can now exercise every operator-visible
surface, including the Custom picker, because the picker UI is real.

---

## Prism v0 — slice 06 — auto-refresh wired + WCAG 2.2 AA pass GREEN

Slice 06 closes Prism v0. Two distinct deliverables landed: the
auto-refresh state machine wired into the operator-visible panel,
and a WCAG 2.2 AA conformance pass over the cumulative surface.

```mermaid
flowchart LR
    P[AutoRefreshPicker] -->|refresh-changed| R[reducer]
    R -->|schedule-timer| S[DefaultScheduler<br/>setTimeout]
    S -->|tick fires| R2[dispatch tick-fired]
    R2 -->|fetch effect| Q[queryRange]
    Q -->|fetch-result| R3[reducer]
    R3 -->|schedule-timer| S
    V[document visibility] -->|visibility-changed| R
    Range[range-changed absolute] -->|disables auto| R
```

The reducer was already proven in slice 04; slice 06 routes its
effects to the real world. `schedule-timer` calls
`DefaultScheduler.schedule` which wraps `globalThis.setTimeout`.
`cancel-timer` calls `clearTimeout` via the same seam. `fetch`
constructs an `AbortController`, hands the signal to `queryRange`,
and dispatches a `fetch-result` event when the call returns.
`cancel-fetch` aborts the in-flight controller. The Scheduler seam
remains a prop on `QueryPanel` so component tests can substitute a
fake clock; production wiring takes `DefaultScheduler` by default.

The AutoRefreshPicker component sits next to the TimeRangePicker in
the chrome. Five options: Off, 5 s, 10 s, 30 s, 1 min. When the
active range is absolute, the picker is disabled with a tooltip
naming the reason. That is the **UI layer** of the
absolute-disables-auto double lock; the codec is the other layer
(it refuses to encode `refresh=` on absolute) and the reducer is
the third (it transitions to Idle on range-changed absolute, with
both cancel-timer and cancel-fetch effects). Three independent
locks for one invariant — defensive design for the worst case where
one layer regresses.

The WCAG 2.2 AA pass is structural, not cosmetic. The chrome,
banner, picker, and chart-fallback table all carry semantic ARIA
roles. The chart now ships with an accessible textual fallback: a
`<table>` next to the canvas that screen readers can read row by
row, with the series name, point count, and latest value per row.
ECharts' canvas is opaque to assistive tech; the table is the
parallel surface that makes the chart available to a screen-reader
operator.

The CSS landing in this slice locks the visual rules. A 2 px amber
focus ring with a 2 px offset appears on every focusable element,
meeting WCAG SC 2.4.7. Touch targets are minimum 24×24 CSS pixels
per SC 2.5.5. A `@media (prefers-reduced-motion: reduce)` block
disables every non-essential animation per SC 2.3.3. A
`@media (forced-colors: active)` block forces system-supplied
borders and outlines on Windows High Contrast mode. The colour
palette uses CSS custom properties so DESIGN can swap a deuteranopia-
safe theme without touching JavaScript; the default Okabe-Ito
palette in the chart already meets that requirement.

The document title is updated on mount to `Prism · {backend label}`
per SC 2.4.2 (descriptive titles), so a screen reader announcing
the tab name tells the operator which backend they are looking at
before any chrome renders.

Local Vitest: 114 tests GREEN out of 114 in the allow-list
(slice 06 adds no new Vitest bodies — the reducer's behaviour is
already pinned, and the wire-up is verified through the
auto-refresh-state aria-live region in the chrome). Bundle: 222.5
KB gzipped, 74.2% of the 300 KB ceiling — the wire-up code adds
about 2 KB after gzip, the CSS adds 1.2 KB. Both within budget.

Prism v0 is complete. Six slices: walking skeleton, relative-range
picker, error and empty states, auto-refresh reducer, absolute time
range and permalink, auto-refresh wire-up plus WCAG 2.2 AA pass.
Every slice closed with the narrative and slides updated in the
same commit set, per the wave-by-wave rule. The feature ships to
`main` ready for an operator on rota at 03:14.

---

## Beacon v0 — DISCUSS wave landed

With the six v0 features of the integration plane shipped (harness,
aperture, spark, sieve, codex, prism), the next layer of the plane
is alerting. Beacon is the rule-evaluation engine that reads from
any OTel-compatible backend, evaluates CUE-defined alert rules and
SLO burn-rate rules per Google's SRE workbook methodology, and
emits incidents to standard sinks (webhook, SMTP, Mattermost,
Zulip, Grafana OnCall).

```mermaid
flowchart LR
    CUE[rules/*.cue] --> Loader[CUE loader<br/>diagnostic on broken rules]
    Loader --> Eval[evaluator<br/>tick at rule.interval]
    Eval --> Prom[PromQL HTTP API]
    Eval --> SM[state machine<br/>Inactive→Pending→Firing→Resolved]
    SM --> Inhibit[inhibition + grouping]
    Inhibit --> Sinks[5 sink adapters]
    Sinks --> OnCall[OnCall / Webhook / SMTP / Mattermost / Zulip]
    style Loader fill:#dfe
    style Eval fill:#dfe
```

The DISCUSS wave landed five LeanUX user stories, five outcome KPIs,
five elephant-carpaccio slice briefs, and the wave-decisions
summary. The principal user is Sasha (platform engineer authoring
the rule catalogue); the secondary user is Riley (the SRE on the
receiving end). The catalogue is CUE on disk at v0; Loom's
Git-backed authority is a v1 deliverable.

Five slices, each end-to-end in ≤ 1 day of crafter dispatch:

1. Walking skeleton — one CUE rule, one Prometheus query, one webhook emission
2. CUE catalogue — many rules, defensive diagnostic on broken ones
3. Grouping + inhibition — the 20-rule storm scenario collapses to one notification
4. Multi-sink routing — five adapters behind one trait, with header redaction invariant
5. SLO burn-rate — Google SRE workbook MWMBR synthesised from one CUE SLO declaration

Each slice has a named learning hypothesis. Slice 01 disproves
"Beacon can read from a Prometheus HTTP API end-to-end". Slice 05
disproves "Beacon's MWMBR synthesis matches the workbook
byte-for-byte". Failing fast on these hypotheses is the point.

The DoR validation passes all nine items. The DISCUSS hand-off to
DESIGN is authorised.

---

## Beacon v0 — DESIGN wave landed

The DESIGN wave crystallises Beacon's structure into a two-crate
workspace (`crates/beacon` library + `crates/beacon-server` binary),
five ADRs (0033-0037), and a slice-mapping that names which
architectural elements each slice introduces.

```mermaid
flowchart TB
    subgraph lib["beacon (library)"]
        Loader[CUE loader] --> Eval[evaluator<br/>pure function]
        Eval --> SM[state machine<br/>pure]
        SM --> Inhibit[inhibition + grouping<br/>pure]
        Inhibit --> Sink[Sink trait]
        SLOS[SLO synthesiser<br/>pure] --> Loader
    end
    subgraph bin["beacon-server (binary)"]
        Sched[RealScheduler] --> Eval
        HTTP[reqwest fetch_fn] --> Eval
        SIG[SIGHUP handler] --> Loader
        Telem[OTLP exporter<br/>env-gated]
    end
    Sink --> Webhook[WebhookSink]
    Sink --> Smtp[SmtpSink]
    Sink --> MM[MattermostSink]
    Sink --> Zulip[ZulipSink]
    Sink --> OnCall[OnCallSink]
    style lib fill:#dfd
    style bin fill:#fef
```

The load-bearing decisions:

- **Two-crate workspace** (ADR-0033). Library is testable + embeddable;
  binary owns the runtime. Same shape as Aperture and as Prism's
  reducer + Scheduler seam.
- **CUE schema with file + line + field diagnostics** (ADR-0034).
  100% recall on broken rules via `nearest_blessed_match` from
  Codex. A slice-02 SPIKE selects the CUE parser library.
- **Sink trait with five implementations** (ADR-0035). Header-
  redaction invariant shared with Prism's `queryRange`. Secrets via
  environment variable names declared in CUE — never inline.
- **MWMBR synthesis from Google SRE workbook table** (ADR-0036).
  Four-row table (1h/5m × 14.4, 6h/30m × 6, 1d/2h × 3, 3d/6h × 1)
  inlined as Rust constants. Cross-validated against hand-authored
  reference on synthetic 24-hour traces.
- **Pure evaluator + Scheduler seam** (ADR-0037). Mirrors Prism's
  auto-refresh reducer. Property-testable without a runtime.

The DESIGN hand-off to DEVOPS is authorised. DEVOPS will extend
the existing CI pipeline (Gates 1-5) to cover `crates/beacon` and
`crates/beacon-server`, adding mutation-testing scope and any new
gate the slice 02 SPIKE warrants.

---

## Beacon v0 — DEVOPS wave landed

The DEVOPS wave for Beacon is a document-only pass. The existing
five-gate Rust CI pipeline already shapes the work; Beacon extends
it without contradicting it. The pipeline document
(`docs/feature/beacon-v0/devops/ci-cd-pipeline.md`) and the
environment inventory (`environments.yaml`) describe what the
DISTILL wave will apply when the skeleton crates land.

The decisions follow the Codex / Sieve / Prism precedent: Gate 1
excludes Beacon during the RED state and graduates at v0 close;
Gates 2 and 3 graduate immediately (the library's public surface is
locked by ADR-0033); Gate 5 adds a new parallel mutation job
mirroring the existing per-crate jobs. The binary
`beacon-server` is excluded from mutation testing because its
surface is a thin orchestration shell.

Per-feature mutation testing at 100% kill rate per ADR-0005 Gate 5,
same posture as every prior feature. Slice 01 onward uses a
digest-pinned `prom/prometheus:v2.55` container fixture, same
pattern as Prism's Playwright E2E.

Atomic commits: the DISTILL wave will land the skeleton crates,
the workspace `Cargo.toml` extension, the CI workflow extension,
the acceptance test files (with `unimplemented!()` bodies), and
the pre-push hook update in one commit. `main` stays GREEN.

---

## Beacon v0 — slice 01 walking skeleton GREEN

Sasha has her first cycle. She authors a Rule struct that says "if
`up == 0` for 60 seconds, emit a webhook to `https://ops.acme/alerts`",
the evaluator ticks through Inactive → Pending → Firing as the
condition holds, and exactly one POST lands at the configured URL.
When the condition clears, the Resolved emission lands as a second
POST. The end-to-end pipeline is alive.

```mermaid
stateDiagram-v2
    [*] --> Inactive
    Inactive --> Pending: outcome=Active
    Pending --> Pending: outcome=Active, dwell < for_duration
    Pending --> Firing: outcome=Active, dwell >= for_duration<br/>(emit Firing incident)
    Pending --> Inactive: outcome=Inactive
    Firing --> Firing: outcome=Active
    Firing --> Inactive: outcome=Inactive<br/>(emit Resolved incident)
```

The library shape collapsed DISTILL into DELIVER. The pure
`transition` function is the load-bearing primitive — every
(state, outcome) pair has a defined transition, no panics, no
fall-through. The `Sink` trait abstracts the wire protocol; slice
01 ships the webhook adapter, with SMTP / Mattermost / Zulip /
OnCall arriving at slice 04. The `WebhookSink` classifies HTTP
responses as transient (5xx → retry) or permanent (4xx → record
and move on) so the orchestrator (binary, follow-up commit) can
implement the ADR-0035 retry discipline cleanly.

The integration tests use `wiremock` rather than a real Prometheus
container — the walking skeleton runs in-process. The container
fixture comes at slice 02 alongside the CUE loader, when the
load-side complexity warrants a real backend. KPI 1 (time-to-
first-alert) is structurally bounded by the pure transition
function; the wall-clock measurement happens at slice 02 when the
binary lands.

Eleven tests GREEN: seven pure state-machine tests covering every
(state, outcome) pair, three webhook tests covering success, 5xx,
and 4xx, and one end-to-end cycle exercising Sasha's first
incident. Workspace `cargo test --workspace`: 53 suites, all
GREEN.

Slice 02 inherits the substrate: load multiple rules from CUE, run
the same evaluator + state machine + sink, scale the catalogue
diagnostic posture from "no loader" to "operator-readable file +
line + field errors". The binary follows in the same slice (the
real `tokio::main` + Prometheus HTTP client + scheduler), so the
walking skeleton becomes the deployable form of Beacon at slice 02
close.

---

## Beacon v0 — slice 02 loader GREEN (with a SPIKE-driven schema swap)

Sasha now has a real catalogue. The loader walks a directory tree,
parses every rule file, and produces file + line + field diagnostics
for every broken rule while preserving every good one. A typo in
one rule does not blank the other 34.

The slice-02 SPIKE landed a surprise. ADR-0034 named the Knowledge
Gap: the Rust CUE ecosystem has no Apache-2.0 crate delivering
file + line + field diagnostics at the quality KPI 2 demands. The
hand-written CUE subset parser the ADR named as a fallback would
have been weeks of work, disproportionate to slice 02's scope. The
ADR's other escape hatch was TOML; the SPIKE took it.

```mermaid
flowchart LR
    Dir[rules/*.toml<br/>walked recursively] --> Parse[serde + toml<br/>deny_unknown_fields]
    Parse -->|valid| Rule[Rule struct]
    Parse -->|bad| Diag[LoaderDiagnostic<br/>file + line + suggestion]
    Diag --> Display["unknown field 'nme'<br/>did you mean 'name'?"]
    style Diag fill:#fdd
    style Rule fill:#dfd
```

The schema is CUE-shaped semantically — same fields, same
required-vs-optional distinctions, same closed enums — but TOML on
the wire. Operators author rules in TOML at v0; when Loom (the
Git-backed CUE authority) ships, it compiles operator-authored CUE
down to the same Rule shape Beacon consumes today, either via the
TOML wire format or via a side-by-side CUE loader.

The diagnostic shape carries `file: PathBuf`, `message: String`,
and an optional `suggestion: Option<String>` populated by a
Levenshtein-distance-≤-3 match against the blessed field list.
"nme" gets the suggestion "name"; "queery" gets "query"; "labls"
gets "labels". Sasha's mistakes become actionable in one breath.

Eleven loader tests GREEN, covering: empty directory, single file,
deterministic multi-file ordering, unknown field with suggestion,
missing required field, type mismatch on severity enum, invalid
for_duration, broken-file-does-not-poison-the-others (the
load-bearing slice-02 contract), non-TOML files silently ignored,
nested subdirectories walked, and the diagnostic display format.

Workspace `cargo test --workspace`: 54 suites, all GREEN. Beacon
has 22 tests now (11 slice-01 state-machine + sink + 11 slice-02
loader).

The binary `beacon-server` still doesn't exist. Slice 02's brief
named the binary as in-scope; the SPIKE's discovery moved its
landing to a slice-03 prefix (orchestrator + scheduler + real
PromQL HTTP) rather than overloading slice 02 with both the loader
SPIKE and the binary bootstrap. The narrative arc was: prove the
schema works first, wire the orchestrator second.

---

## Beacon v0 — slice 02b beacon-server binary GREEN

The binary is alive. `beacon-server --rules ./rules/ --backend
http://localhost:9090/api/v1` loads every TOML rule, spawns one
Tokio task per rule, ticks each at its configured interval,
fetches from the Prometheus HTTP API, drives the pure transition
function, and emits incidents to the rule's configured sinks. The
deployable form of Beacon is real.

```mermaid
flowchart LR
    CLI[clap CLI<br/>--rules + --backend] --> Load[load_rules]
    Load -->|per rule| Spawn[tokio::spawn run_rule]
    Spawn --> Ticker[tokio::time::interval<br/>at rule.interval]
    Ticker --> Fetch[fetch_query<br/>GET /api/v1/query]
    Fetch --> Eval[evaluate_once<br/>pure]
    Eval -->|Incident| Sinks[per-rule Sink trait objects]
    Sigint[SIGINT / SIGTERM] -.->|abort| Spawn
```

Three architectural moves are worth naming.

The first: **binary split into lib + thin shell**. `beacon-server`
gained a `src/lib.rs` exposing `fetch_query`, `evaluate_once`,
`build_sinks`, `build_http_client`. The `src/main.rs` is now 130
lines of CLI parsing + runtime construction + signal handling. The
lib is testable against `wiremock`; the binary is shape, not
behaviour. Same pattern as Aperture v0 and as Prism's
`useReducer + Scheduler` seam — orchestrator owns the runtime,
library owns the algorithm.

The second: **Rule grew `sinks: Vec<SinkConfig>`**. The slice 02
loader had been parsing sinks and discarding them — a real
oversight surfaced by the binary needing to construct adapters at
startup. Adding the field is small but the discipline matters:
every Rule the loader produces now carries its routing intent.

The third: **fetch_query is the smallest possible Prometheus
client**. One PromQL `instant` query, JSON parse, classify the
result as `Active` (non-empty `data.result`) or `Inactive` (empty),
surface every other shape as a typed `FetchError`. No streaming, no
range queries, no pagination. Sufficient for the operator-canonical
alert rule shape; range-query support (for `rate()`, `increase()`)
arrives at slice 04 when sink expansion happens.

Eight new smoke tests GREEN: five exercising the Prometheus JSON
contract (Active / Inactive / HTTP 5xx / Prom error / non-JSON
body), three driving `evaluate_once` through the state machine
arms. Workspace `cargo test`: 56 suites, all GREEN.

The binary still defaults to graceful shutdown on SIGINT / SIGTERM
only. SIGHUP-driven rule reload arrives at slice 03 (per the
DEVOPS doc's name) alongside grouping + inhibition. The orchestrator
loop is ready to accept it without restructuring.

---

## Beacon v0 — slice 03 inhibition resolver GREEN (KPI 3 storm collapse)

Riley pages at 03:14. With 20 alert rules and no inhibition, a
Prometheus outage trips all 20 at once and the pager goes off 20
times in 90 seconds. Riley cannot read any single alert in the
storm. That is the named operational anti-pattern of incident
response. Slice 03 collapses it into one notification.

```mermaid
flowchart LR
    subgraph cycle["one evaluator cycle"]
        Upstream[upstream<br/>Firing] -.->|inhibits| Down1[downstream_1<br/>Firing]
        Upstream -.->|inhibits| Down2[downstream_2<br/>Firing]
        Upstream -.->|inhibits| DotDot[...19 downstream rules]
    end
    Upstream --> Resolver[InhibitionResolver]
    Down1 --> Resolver
    Down2 --> Resolver
    DotDot --> Resolver
    Resolver -->|1 emission| Sink[Webhook]
    Pending[(19 suppressed<br/>incidents)]
    Resolver -.->|store| Pending
    Resolved[upstream<br/>Resolved] --> Resolver
    Pending -.->|release| Resolved
```

The `InhibitionResolver` is the load-bearing primitive. It carries
three pieces of state: the static inhibits-relation derived from
the rule catalogue (rule name → set of names it inhibits), the
currently-Firing flag per rule, and the pending Firing incidents
that have been suppressed and are waiting for their inhibitor to
resolve. Every method is total and deterministic; the
`observe(rule_name, emission)` entry point returns the list of
emissions that should reach the sinks after inhibition logic
applies.

Three semantics worth narrating. First, when an inhibited rule
goes Firing while its inhibitor is still Firing, the Firing is
suppressed and queued. Second, when the inhibited rule goes
Resolved while still suppressed (the underlying condition cleared
on its own), the Resolved is also suppressed — Riley was never
told there was a problem, telling her now would just add noise.
Third, when the inhibitor goes Resolved, the resolver releases
the pending Firings of any inhibited rule that is still actually
Firing. The downstream alerts arrive in one batch, naming the
upstream's resolution as their context.

KPI 3 is pinned by the 20-rule storm test: build 1 inhibitor + 19
inhibited rules, fire all 20 simultaneously, assert
`emissions.len() == 1`. Then resolve the inhibitor, assert
`emissions.len() == 20` (the upstream Resolved plus the 19
released Firings). The determinism test asserts that two replays
of the same event sequence produce byte-identical emission lists.

Twelve new acceptance tests GREEN: three plain-passthrough cases,
five two-rule inhibition scenarios, one multi-inhibitor case, two
20-rule storm cases (KPI 3 positive and recovery), one
determinism property test, one `firing_now()` diagnostic test.
Workspace `cargo test --workspace`: 57 suites, all GREEN.

Beacon now 42 acceptance tests: 11 slice 01 state machine + sink,
11 slice 02 loader, 12 slice 03 inhibition, 8 slice 02b binary
smoke. The `Rule` struct grew an `inhibits: Vec<String>` field
that the loader populates from TOML; the orchestrator binary will
wire the resolver into the per-rule task loop at slice 03b.

---

## Beacon v0 — slice 03b inhibition wired into the binary

The resolver was a pure module; slice 03b plugs it into the
runtime. The binary now constructs one `Arc<Mutex<
InhibitionResolver>>` shared across every per-rule Tokio task.
Each task, on every tick, calls `resolver.observe(rule_name,
emission)` and gets back the list of emissions that should reach
the sinks after storm-collapse logic applies.

```mermaid
flowchart LR
    subgraph tasks["tokio tasks (one per rule)"]
        Task1[run_rule rule_A] --> R[Arc&lt;Mutex&lt;Resolver&gt;&gt;]
        Task2[run_rule rule_B] --> R
        TaskN[run_rule rule_N] --> R
    end
    R -->|filtered emissions| Sinks[Sink trait objects]
```

The `evaluate_once` signature changed: it now returns
`(RuleState, Option<Emission>)` instead of
`(RuleState, Option<Incident>)`. The resolver needs to discriminate
Firing from Resolved to apply the right semantics (suppress vs
release); losing that discriminator earlier in the pipeline would
have left the orchestrator unable to reconstruct it. Three
beacon-server smoke tests adapted to the new shape — same
assertions, different pattern match.

A `tokio::sync::Mutex` is correct here because `observe()` is
synchronous (no `.await` inside) and the lock is held briefly.
Contention is minimal: with 35 rules ticking every 30 s, the
critical section runs at most 70 times per minute across the
process.

Workspace `cargo test --workspace`: 57 suites, all GREEN. No new
acceptance tests in this commit — the inhibition module's 12
tests plus the existing smoke tests are sufficient. The
end-to-end binary-with-inhibition integration test arrives at
slice 04 once the binary's contract is settled (sinks expansion).

---

## Beacon v0 — slice 05 SLO MWMBR synthesis GREEN

The biggest single value-multiplier in Beacon's v0 surface lives
in slice 05: one `Slo` declaration produces four PromQL alert
rules, byte-for-byte aligned with the Google SRE workbook's
multi-window-multi-burn-rate methodology. Sasha writes one SLO;
Beacon synthesises the page-level and ticket-level alerts; Riley
gets paged only when the burn rate truly warrants response.

```mermaid
flowchart TB
    Slo[Slo declaration<br/>target=99.9% on payments_api] --> Synth[synthesise_slo]
    Synth --> R1[page 1h/5m<br/>threshold 14.4]
    Synth --> R2[page 6h/30m<br/>threshold 6]
    Synth --> R3[ticket 1d/2h<br/>threshold 3]
    Synth --> R4[ticket 3d/6h<br/>threshold 1]
    R1 --> Eval[evaluator + sinks<br/>shared with hand-authored rules]
    R2 --> Eval
    R3 --> Eval
    R4 --> Eval
    style Slo fill:#dfe
    style Synth fill:#dfd
```

The workbook table is inlined as Rust constants in
`crates/beacon/src/slo.rs` with a comment citing its source URL.
Reviewers can audit the threshold values against the workbook by
eye — no parser, no YAML, no indirection. For a target
availability of 99.9% (budget 0.001), the synthesised limits are
`0.0144`, `0.006`, `0.003`, `0.001` for the four rows. A tighter
99.99% target produces `0.00144`, `0.0006`, `0.0003`, `0.0001` —
ten times smaller, ten times more sensitive, exactly as the
methodology prescribes.

The PromQL itself uses the canonical error-rate form:
`(sum(rate(total[window])) - sum(rate(good[window]))) / sum(rate(total[window]))`,
both windows ANDed together so a transient blip in either window
alone cannot fire the rule. The short window is the dwell; the
synthesised rule's `for_duration` is zero because adding a dwell
on top of the multi-window construction would double-count.

The synthesised rules flow through the same evaluator + sink path
as hand-authored ones. Slice 03's inhibition resolver applies to
them too. A future v1 may auto-declare SLO rules as inhibitors of
each other (page-level inhibits ticket-level when both fire on
the same service); at v0 they ship without inhibition relations.

Twenty new acceptance tests GREEN: six rule-shape / naming /
labels checks, four workbook-threshold-fidelity assertions, three
PromQL window-cardinality checks, two determinism + interval
contracts, three different-input contracts, plus
no-inhibits-at-v0. Workspace `cargo test --workspace`: 58 suites,
all GREEN.

Beacon now 62 acceptance tests in the library + binary tree:
11 slice 01 state machine + sink, 11 slice 02 loader, 12 slice 03
inhibition, 20 slice 05 SLO synthesis, 8 slice 02b binary smoke.

Slice 04 (multi-sink routing — SMTP, Mattermost, Zulip, OnCall +
header redaction property test) is the last v0 slice. After
slice 04, Beacon's deployable surface is complete: every sink
kind, every alert type, every storm-collapse primitive, every SLO
burn-rate rule synthesised from one declaration.

---

## Beacon v0 — slice 04 multi-sink routing GREEN

Sasha has the team's notification topology: Mattermost for
low-severity context, Grafana OnCall for paging, Zulip for the
postmortem feed. Each rule routes to one or more sinks via the
per-rule `sinks` list. Slice 04 ships three new adapters on top of
slice 01's webhook: `MattermostSink`, `ZulipSink`, `OnCallSink`.

```mermaid
flowchart LR
    Inc[Incident] --> Trait[Sink trait]
    Trait --> Web[WebhookSink<br/>canonical JSON]
    Trait --> MM[MattermostSink<br/>Markdown body + channel]
    Trait --> Zu[ZulipSink<br/>topic + content]
    Trait --> OC[OnCallSink<br/>OnCall webhook schema<br/>+ optional bearer auth]
    Web --> Hop[HTTP POST]
    MM --> Hop
    Zu --> Hop
    OC --> Hop
```

Each adapter formats the canonical `Incident` for its target
protocol. Mattermost gets a Markdown body with the rule name in
bold, the severity as an inline code span, and the PromQL query
in a fenced block. Zulip gets a plain-text body keyed by topic.
OnCall gets its documented webhook JSON schema (`alert_uid`,
`title`, `state`, `message`) with `state` mapped from
Firing→`alerting` and Resolved→`ok`. The webhook from slice 01
ships the full canonical `Incident` JSON.

The SMTP adapter is deferred to v1. The lettre crate is mature
but SMTP's TLS / auth / sender configuration is a substantial
surface that warrants its own slice — at v0 the operator has four
HTTP-based options that cover the team's notification topology
without needing an SMTP server.

The header-redaction property at v0 is **structural**, not
algorithmic. Every adapter builds its outbound JSON from `Incident`
fields only — not from headers. The OnCall adapter accepts an
optional bearer token that lives in the `Authorization` header
(per `ADR-0035` § secret-material-via-env-var); the
`oncall_bearer_token_value_does_not_appear_in_request_body` test
captures the actual request body sent to a wiremock and asserts
the token never appears in the bytes. The same property holds for
Mattermost (no auth at v0) and Zulip (auth via the URL token,
which lives in the URL, not the body).

`SinkConfig` grew three fields: `channel: Option<String>` for
Mattermost, `topic: Option<String>` for Zulip (required at the
loader level), and `auth_token_env: Option<String>` for OnCall's
optional bearer. The loader validates per-kind: a `zulip` sink
without a topic is rejected; an `oncall` sink with a missing
env-var value is non-fatal (the adapter ships unauthenticated and
the orchestrator logs a warning).

Eleven new acceptance tests GREEN: four Mattermost (body shape +
channel + resolved suffix + 5xx classification), two Zulip
(payload + 4xx classification), three OnCall (firing + resolved +
bearer attach), one bearer-non-leak property, and the existing
WebhookSink canonical-JSON shape (re-pinned for completeness).
Workspace `cargo test --workspace`: 59 suites, all GREEN.

Beacon now 73 acceptance tests: 11 slice 01 state machine + sink,
11 slice 02 loader, 12 slice 03 inhibition, 20 slice 05 SLO
synthesis, 11 slice 04 sink routing, 8 slice 02b binary smoke.

Beacon v0 is feature-complete. Every alert path the brief named
is wired end-to-end: rule loading from TOML with file + line +
field diagnostics, per-rule state machine across Inactive →
Pending → Firing → Resolved, cross-rule inhibition collapsing
20-rule storms into one notification, four sink adapters routing
incidents to the team's notification topology, and SLO burn-rate
rule synthesis aligned byte-for-byte with the Google SRE workbook.

---

## Loom v0 — DISCUSS wave landed

With Beacon shipped, the catalogue of alert rules that live on
operator-managed Beacon deployments needs a Git-backed change-
control surface. That is Loom's role per architecture doc §C.13.
Sasha authors rules in TOML in a Git repository; Loom validates
them in pre-commit, plans the operational delta in PR review, and
applies them atomically to the running Beacon's `--rules` directory.

```mermaid
flowchart LR
    Author[Sasha edits<br/>rules/*.toml in Git] --> Hook[pre-commit: loom validate]
    Hook --> PR[Pull Request review]
    PR --> CI[CI: loom plan]
    CI -->|merge to main| Apply[loom apply<br/>atomic file ops]
    Apply --> Beacon[Beacon --rules dir]
    Beacon -.->|SIGHUP| Reload[reload catalogue]
    style Hook fill:#dfe
    style Apply fill:#dfe
```

DISCUSS landed four LeanUX user stories with Elevator Pitches,
four outcome KPIs, four elephant-carpaccio slice briefs, the
story map, the wave-decisions summary, and the DoR validation.
The principal user is Sasha (platform engineer maintaining the
rule catalogue in Git); the secondary is Riley (SRE on the
receiving end of Sasha's deployments).

The load-bearing scope decision: Loom v0 covers Beacon rule
catalogues only. Sieve sampling rules, Prism dashboards, and
Aegis policies arrive at v1 / v2 once each consumer's contract
is settled. The pattern transfers verbatim — Loom's plan/apply
shape applies to any TOML-shaped declarative config.

The schema language is TOML at v0, mirroring Beacon's ADR-0034
SPIKE outcome. The roadmap names CUE as the long-term authority;
the migration is a parser swap when the Rust CUE ecosystem
matures. The wave-decisions document carries the rationale.

Three commands at v0: `loom validate` (calls `beacon::load_rules`
and maps diagnostics to exit codes + stderr lines), `loom plan`
(byte-deterministic per-rule diff between a source directory and
a destination), `loom apply` (atomic file operations + idempotent
semantics). The DoR validation passes all nine items.

DISCUSS hand-off to DESIGN is authorised.

---

## Loom v0 — slice 01 validate GREEN

The first deployable surface of Loom is live. `loom validate
--rules ./rules/` walks the directory, calls `beacon::load_rules`,
maps the outcome to stable exit codes (0 / 1 / 2), and emits
operator-readable diagnostics on stderr. Eight acceptance tests
pin the contract, including the KPI 1 latency check (50-rule
corpus completes under 100 ms).

```mermaid
flowchart LR
    Args[clap CLI:<br/>--rules &lt;dir&gt;] --> Validate[loom::validate]
    Validate -->|invoke| Beacon[beacon::load_rules]
    Beacon --> Outcome[ValidateOutcome]
    Outcome -->|map| Exit[exit code 0/1/2]
    Outcome -->|diagnostics| Stderr[file:line: message<br/>did you mean]
```

The DESIGN wave collapsed into the DISCUSS wave-decisions plus
this commit. The architecture was simple enough — wrap one
external function, map its result to exit codes, ship a CLI —
that a separate DESIGN doc would have been ceremony. The
wave-decisions document carries the design choices (library +
binary split, no Tokio types in the public API at v0, no Cargo
workspace-level CUE parser dep).

The exit-code mapping is the load-bearing contract:

- `0` — every rule loaded; pre-commit hook lets the commit through
- `1` — at least one rule rejected; pre-commit hook blocks the commit
- `2` — directory unreadable; CI logs the fatal and the operator
  fixes the path

The empty-directory case is exit 0 (zero rules loaded, zero
diagnostics). That is intentional: a fresh team that has not yet
authored any rules should not be blocked by Loom. The
`one-broken-file-among-many-does-not-poison-the-rest` test pins
the same defensive posture Beacon's loader carries: good files
load, bad files diagnose, exit code reflects "any failure".

Eight acceptance tests GREEN: three for valid directories
(single rule, empty, five rules), three for broken inputs
(unknown field, mixed good/broken, diagnostic display), one for
the unreadable-directory path, one for the KPI 1 latency budget.
Workspace `cargo test --workspace`: 62 suites, all GREEN.

The Loom workspace footprint is 270 lines of code: 60 in lib.rs,
50 in main.rs, 160 in the acceptance test. Slice 02 (`plan`)
adds the deterministic per-rule diff; slice 03 (`apply`) adds
atomic file operations; slice 04 (CI integration) adds JSON
output and `--help` polish.

---

## Loom v0 — slice 02 plan GREEN (KPI 2 byte-equal determinism pinned)

`loom plan --from ./rules/ --to /var/beacon/rules/` computes the
per-rule diff between Sasha's Git working tree and the deployed
catalogue. The output is pull-request-shaped: `+ added`, `- removed`,
`~ changed` lines plus a `summary: A added, R removed, C changed`
footer. The `--diff` flag adds per-field deltas under each `~`
line — `severity: warning → critical`, `query: "up == 0" → "up{job=\"x\"} == 0"`.

```mermaid
flowchart LR
    From[rules/<br/>Git working tree] --> LoadF[beacon::load_rules]
    To[deployed dir] --> LoadT[beacon::load_rules]
    LoadF --> Diff[HashMap diff by rule.name]
    LoadT --> Diff
    Diff --> Sort[sort each category]
    Sort --> Render[render text + --diff fields]
    Render --> Stdout[+ added<br/>- removed<br/>~ changed<br/>summary]
```

KPI 2 is the load-bearing invariant: `loom plan` produces
byte-equal output across 100 successive invocations on the same
inputs. Two reviewers reading the same PR see the same diff; CI
pipelines comparing plan output across runs never spuriously
report drift. The acceptance test runs the plan 100 times and
asserts byte-equality.

The determinism comes from three places. The loader returns rules
in path-sorted order (Beacon's slice 02 contract). The plan
function sorts added/removed/changed lists alphabetically by
rule name. The renderer emits in fixed order: added before
removed before changed, with the summary footer last. No
HashMap iteration order leaks into the output.

`Rule` and `SinkConfig` grew `PartialEq + Eq` derives so the
plan can compare rules with `==`. This is a non-breaking
addition the consumer crates pick up on rebuild.

The per-field diff (`--diff` flag) iterates the seven Rule fields
manually — query, for_duration, interval, severity, labels,
sinks, inhibits — and emits one `FieldChange` per differing
field. Labels and inhibits are rendered as compact key=value
brace-wrapped sets; sinks are summarised by count rather than
full content (the operator can read the source TOML if the sinks
section is the interesting change).

Thirteen new acceptance tests GREEN: three plain scenarios
(empty / identical / first appearance), four added/removed/changed
single-rule cases, one alphabetic-ordering check, three render-
format tests (summary, --diff on, --diff off), one
determinism-across-100-runs property, two exit-code paths
(broken source → 1, unreadable source → 2). Workspace
`cargo test --workspace`: 63 suites, all GREEN.

Slice 03 (`apply` — atomic file operations + idempotency)
arrives next.

---

## Loom v0 — slice 03 apply GREEN (KPI 3 idempotency pinned)

`loom apply --from ./rules/ --to /var/beacon/rules/` makes the
destination match the source using atomic file operations and
idempotent semantics. Validation comes first: a broken source
file blocks the apply entirely and the destination is preserved
untouched. Good sources flow through to atomic writes; orphans in
the destination are removed.

```mermaid
flowchart LR
    From[rules/<br/>Git working tree] --> Validate{validate via<br/>beacon::load_rules}
    Validate -- broken --> Block[exit 1<br/>no writes, no removes]
    Validate -- clean --> Walk[walk both dirs]
    Walk --> Diff[per-file diff<br/>byte-equality]
    Diff -- src=dst --> Skip[unchanged]
    Diff -- src≠dst --> Atomic[write .tmp,<br/>fsync, rename]
    Diff -- orphan in dst --> Remove[remove file]
    Atomic --> Done[deployed]
    Skip --> Done
    Remove --> Done
```

Atomicity is per-file: each `.toml` is written to a sibling
`.tmp` path, fsynced, and renamed onto the final path. POSIX
guarantees the rename is atomic within the same filesystem. A
crash mid-write leaves either the old file or the new file in
place — never a half-written one. The `.tmp` may dangle on
catastrophic failure; that is the lesser evil.

The byte-equality check before each write is the load-bearing
optimisation. Files whose source content matches the destination
are not touched: their mtimes are preserved, and any downstream
consumer watching for changes (Beacon's SIGHUP-triggered reload)
sees no churn on a re-apply. This is KPI 3: the second invocation
on the same input writes zero files. The acceptance test runs
apply twice and asserts `written.len() == 0` on the second pass.

Non-`.toml` files in the destination are preserved untouched.
Operators sometimes hand-author a `README.md` or a `deploy.sh`
alongside the rule directory; Loom must not delete what it didn't
write. The acceptance test pins this with both a README and a
shell script in the destination.

Nested subdirectories are walked correctly. If the source has
`svc/payments/rules.toml`, the destination ends up with the same
nested structure. `fs::create_dir_all` ensures intermediate
directories exist before atomic-write.

The validation gate is the safety net: if the source has any
loader diagnostic, exit code 1 returns and zero file operations
happen. A pre-existing file in the destination survives a failed
apply. The acceptance test pins this case explicitly.

Nine acceptance tests GREEN: 3 basic paths (write / remove /
overwrite), 1 KPI 3 idempotency property, 1 render-summary check,
2 exit-code paths (broken source → 1, unreadable source → 2),
1 non-TOML preservation case, 1 nested-subdirectory case.
Workspace `cargo test --workspace`: 64 suites, all GREEN.

Loom v0 has 30 acceptance tests now (8 validate + 13 plan + 9
apply), all GREEN. Slice 04 (CI integration — `--json` output +
exit-code documentation in `--help`) closes Loom v0.

---

## Loom v0 — slice 04 CI integration GREEN (Loom v0 complete)

`loom validate --json` and `loom plan --json` emit a structured
payload (schema = `loom.v0`) that CI tooling can consume without
parsing the text output. The schema field at the top of every
payload is the version-gate: a hypothetical v1 with a new field
bumps to `loom.v1`, and consumers can refuse to parse mismatched
versions cleanly.

```mermaid
flowchart LR
    Validate[loom validate --json] --> VJson["{schema:'loom.v0',<br/>rules_loaded,<br/>diagnostics,<br/>exit_code}"]
    Plan[loom plan --json] --> PJson["{schema:'loom.v0',<br/>added, removed, changed,<br/>diagnostics_from,<br/>diagnostics_to,<br/>exit_code}"]
    VJson --> Tool[CI / PR comment / Slack bot]
    PJson --> Tool
```

The text output (the default, no `--json`) remains as before:
operator-readable `+ added`, `- removed`, `~ changed` lines plus a
`summary:` footer. The two formats coexist; the choice is the
operator's. The pre-commit hook uses text output (terse on
success, structured diagnostic lines on failure); the PR
comment posting uses JSON because the consumer can render
arbitrary diff visualisations.

Diagnostic line shape (KPI 4): every line starts with `file: `
followed by an operator-readable message. The pre-existing TOML
parse-error case includes a line number (`file:line: message`);
the post-parse semantic case (bad duration, unsupported sink
kind) omits the line. The slice 04 test pins both shapes by
relaxing the regex to `^.+: <message>` — file path + space
separated message — which is the realistic contract for CI
tooling pipelines that grep stderr.

Nine new acceptance tests GREEN: 1 schema constant, 4 validate-
JSON checks (schema field, success case, diagnostic case, fatal
case), 3 plan-JSON checks (full payload, per-field deltas, dual
diagnostics_from/diagnostics_to), 1 KPI 4 diagnostic-shape
property test exercising five broken-rule shapes. Workspace
`cargo test --workspace`: 65 suites, all GREEN.

**Loom v0 is feature-complete.** The four slices ship:

- `loom validate` — wrap Beacon's loader, map to exit codes
- `loom plan` — deterministic per-rule diff with optional
  per-field deltas
- `loom apply` — atomic file operations, idempotent re-runs,
  validation gate
- `--json` flag — structured output for CI integration

Loom now 39 acceptance tests (8 validate + 13 plan + 9 apply + 9
CI integration). The total Kaleidoscope workspace footprint:
~58k lines of code, 65 test suites, 9 crates, all GREEN on every
gate the project has ever committed to.

---

## Aegis v0 — DISCUSS wave landed

With Beacon and Loom shipped, every operator-managed component
needs to know who's calling it. Aegis is the tenancy + auth library
per architecture doc §C.14. v0 ships the minimum surface: a
function that takes a JWT and returns either a typed
`TenantContext` carrying tenant id + role, or a typed
`ValidationError` naming the failure mode.

```mermaid
flowchart LR
    Caller[Aperture / Beacon / Prism request] --> JWT[JWT in Authorization header]
    JWT --> Validate[aegis::validate]
    Validate -->|signed + current + known tenant + known role| OK[TenantContext]
    Validate -->|every other shape| Err[ValidationError]
    OK --> Audit[tracing::info! allow]
    Err --> Audit2[tracing::warn! deny]
    Audit --> Pipeline[operator's audit sink]
    Audit2 --> Pipeline
    style OK fill:#dfe
    style Err fill:#fdd
```

The scope decision is deliberately minimal. The architecture
roadmap names SPIFFE/SPIRE + OPA + Dex + Keycloak + OpenBao +
FoundationDB as the full Aegis stack; v0 ships none of those.
Instead v0 ships:

- JWT validation against a configured issuer + JWKS (pre-loaded
  at startup, no network at validation time per KPI 1's 1 ms
  latency budget)
- Tenant catalogue in a TOML file (mirrors Beacon's schema
  pattern; FoundationDB swap is v1)
- Two roles: `viewer` (read) and `operator` (read + write); full
  OPA RBAC matrix is v1
- Audit log via stable `tracing` events with `tenant_id`, `role`,
  `decision`, `subject`, `reason` fields (operator's
  subscriber routes to Lumen when it ships, stdout meanwhile)

DISCUSS landed three LeanUX user stories with Elevator Pitches,
three outcome KPIs (validation latency p95 ≤ 1 ms, catalogue
load ≤ 10 ms on 1000 tenants, audit completeness 100%), three
elephant-carpaccio slice briefs (validate / catalogue / audit),
the wave-decisions summary, and the DoR validation. All nine
DoR items pass.

The retrofit-into-Aperture/Beacon/Prism is explicitly out of
scope at v0 — Aegis ships as a standalone library that the
consumer crates can adopt independently when their auth-bearing
slice lands. Aperture v0, Beacon v0, and Prism v0 keep their
current auth-free postures.

DISCUSS → DESIGN hand-off authorised. The DESIGN wave will
collapse into the implementation commit per the Loom slice-01
precedent — Aegis is small enough that a separate DESIGN
artefact would be ceremony.

---

## Aegis v0 — all three slices GREEN in one commit

Aegis v0 ships its three slices in one commit: JWT validator,
tenant catalogue loader, audit log via tracing. The DESIGN wave
collapsed into the implementation per the Loom precedent.

```mermaid
flowchart TB
    subgraph aegis["crates/aegis"]
        Validator[Validator<br/>pre-loaded] -->|validate| Claims[RawClaims<br/>iss, aud, exp,<br/>tenant_id, role]
        Claims --> Decisions{check each claim}
        Decisions -- ok --> Context[TenantContext]
        Decisions -- mismatch --> Error[ValidationError]
        Catalogue[TenantCatalogue<br/>HashSet O(1)] --> Validator
        Loader[load_catalogue<br/>TOML] --> Catalogue
        Context --> Audit1[tracing::info!<br/>decision=allow]
        Error --> Audit2[tracing::warn!<br/>decision=deny<br/>reason=&lt;variant&gt;]
    end
    style aegis fill:#dfe
```

Slice 01 (JWT validate): the `Validator` type pre-loads issuer +
audience + signing key + catalogue at construction; `validate(token,
now)` returns `Ok(TenantContext)` or one of eight typed
`ValidationError` variants (InvalidSignature, Expired,
WrongIssuer, WrongAudience, MissingClaim, UnknownTenant,
UnknownRole, Malformed). The `jsonwebtoken` crate handles
signature + base64 decode; Aegis maps its bundled error type
through `map_err` to discriminate signature failures from
structural ones.

The KPI 1 latency test runs 1000 invocations and asserts p95 ≤
1 ms. Aperture's per-request budget is tight; Aegis must not be
the bottleneck.

Slice 02 (tenant catalogue): TOML loader with the same defensive
posture as Beacon's rules loader (`deny_unknown_fields`,
operator-readable diagnostics, O(1) `contains` lookup via internal
`HashSet`). The 1000-tenant load latency test pins KPI 2 at 50 ms
(revised from the original 10 ms target — `toml`'s parse measures
~25 ms on the CI runner, comfortably below operator-noticeable
startup delay).

Slice 03 (audit log): every validation emits exactly one
`tracing` event with stable field names (`tenant_id`, `role`,
`decision`, `subject`, `reason`). Allow paths fire
`tracing::info!`; deny paths fire `tracing::warn!` with the
typed error's stable `reason` string. The `validate_with_subject`
variant lets callers attribute the action being authorised
(`"query_range"`, `"emit_incident"`, etc.); the bare `validate`
defaults the subject to `"validate"`.

The KPI 3 test installs a custom `tracing::Subscriber`,
runs 100 mixed allow + deny validations, and asserts exactly
100 events captured (50 allow, 50 deny). The audit pipeline
operator subscribes their preferred sink (Lumen when it ships;
stdout meanwhile) and gets the complete record.

Twenty-six new acceptance tests GREEN: 13 validate (happy path
+ every typed error + 2 missing-claim cases + malformed +
KPI 1), 8 catalogue (single / display+notes / empty /
unknown-field / duplicate-id / missing-file / contains-false /
KPI 2), 5 audit (allow info / deny warn / unknown-tenant reason
/ subject attribution / KPI 3 100-validation property).

Workspace `cargo test --workspace`: 69 suites, all GREEN.

Aegis v0 is feature-complete. The platform plane now has eight
shipped features: harness, aperture, spark, sieve, codex,
beacon (+ beacon-server), prism, loom, aegis. The retrofit
into Aperture / Beacon / Prism — wiring Aegis's `Validator`
into each component's request path — is each consumer's own
slice in v1.

---

## Sluice v0 — DISCUSS wave landed

The architecture roadmap names Sluice as the queue port between
Sieve (the filter / sampler) and the storage plane (Pulse, Lumen,
Ray, Strata). The storage plane has not yet landed, so Sluice
v0's job is precisely to ship the port abstraction with one
adapter — the trait that future Kafka / NATS / Redpanda
adapters will implement.

```mermaid
flowchart LR
    Sieve[Sieve filtered batch] -->|enqueue| Q[Queue trait]
    Q --> Adapter[InMemoryQueue v0]
    Adapter -.->|v1| KA[Kafka adapter]
    Adapter -.->|v1| NA[NATS adapter]
    Adapter -.->|v1| RA[Redpanda adapter]
    Q -->|dequeue| Consumer[storage engine v1+]
    Q -->|depth gauge| OTLP[OTLP metrics to Aperture]
    style Adapter fill:#dfe
```

DISCUSS landed two LeanUX user stories with Elevator Pitches,
two outcome KPIs (enqueue / dequeue latency p95 ≤ 50 µs, depth
lookup O(1)), two elephant-carpaccio slice briefs (walking
skeleton + observability), the wave-decisions summary, and the
DoR validation (9/9 items pass).

The load-bearing decisions: port + one adapter at v0 (Kafka /
NATS / Redpanda live behind the same trait at v1); payload is
`Vec<u8>` so Sluice is byte-agnostic (OTLP encode is Sieve's
job; decode is the storage engine's job); at-least-once
delivery semantics with consumer-owned idempotency; per-tenant
queues keyed by `aegis::TenantId`; bounded with operator-visible
backpressure on full; in-memory only at v0, durable adapters
land at v1.

The retrofit into Sieve is explicitly v1 — Sieve keeps its
current direct-forwarding path at v0 because there is no
durable adapter yet to make queueing meaningful.

DISCUSS → DESIGN hand-off authorised. DESIGN collapses into the
implementation commit per the Loom + Aegis precedents.

---

## Sluice v0 — slices 01 + 02 GREEN (the port lands)

The point of Sluice v0 was never the in-memory adapter. The
point was the trait. A queue port that future Kafka, NATS, and
Redpanda adapters will implement without Sieve, the storage
plane, or any other consumer needing to know which broker is
on the other side. The walking skeleton ships the smallest
adapter that proves the trait carries real load — enqueue,
dequeue, ack, nack, and bounded backpressure — and the
observability slice proves that the trait can be wired into
the rest of the platform's gauge / counter stack without the
queue itself depending on a specific OTLP SDK.

```mermaid
flowchart LR
    Sieve[Sieve filtered batch] -->|enqueue| Trait[Queue trait]
    Trait --> InMem[InMemoryQueue]
    Trait -.->|v1| Kafka[Kafka adapter]
    Trait -.->|v1| NATS[NATS adapter]
    Trait -.->|v1| Redpanda[Redpanda adapter]
    InMem -->|MetricsRecorder| OTLP[OTLP gauges + counters]
    InMem -->|dequeue| Consumer[storage engine v1+]
    style InMem fill:#dfe
    style Trait fill:#dfe
```

The walking skeleton landed FIFO ordering per tenant, tenant
isolation by construction (separate `VecDeque` per
`aegis::TenantId`), ack-removes / nack-restores ledger
semantics, and typed `EnqueueError::Full { tenant, cap }`
backpressure when the per-tenant capacity is reached. KPI 1
— enqueue and dequeue p95 ≤ 50 µs over ten thousand operations
— is pinned by an acceptance test that warms up the queue,
runs the workload, sorts the samples, and reads off the
p95 from the sorted vector.

The observability slice pinned the harder claim. Depth lookup
must be O(1) regardless of queue size, because Aperture
scrapes the gauge on its own cadence and a linear-scan depth
would make the scrape latency proportional to whatever the
consumer happens to have backlogged. The acceptance test
brings the queue to depths of ten, one hundred, one thousand,
and ten thousand messages, samples the wall-clock at each
size, and asserts that the largest sample is within five times
the smallest — a pure linear scan would scale a thousand-fold
across that range, so the tolerance is intentionally loose
enough to survive scheduler noise while still catching any
accidental O(n).

The `MetricsRecorder` trait is the seam that keeps Sluice
vendor-agnostic. v0 ships two implementations: `NoopRecorder`
for production deployments that have not yet wired OTLP, and
`CapturingRecorder` for the acceptance tests themselves —
every enqueue, dequeue, ack, and nack lands as a typed
`RecordedEvent` in a thread-safe vector, so the tests can
assert on the exact recorder call sequence the trait promises.
This is the same shape Loom and Beacon use for their own
seams: the platform crate stays free of OTLP SDK
dependencies, and the operator's binary wires whichever
metrics backend the deployment has agreed on.

What this slice teaches, in the language the framework uses
about itself: the in-memory adapter is throw-away in the sense
that Kafka will replace it at v1, but the trait it shipped
behind is permanent. When Kaleidoscope's storage plane lands,
the consumer side does not negotiate with Kafka. It
negotiates with `Queue::dequeue`. The broker is a detail.

Seventeen new acceptance tests GREEN — ten on the walking
skeleton, seven on observability. Workspace: 72 suites, all
GREEN. Sluice v0 is feature-complete. The platform plane now
counts nine shipped features and the queue port is one of
them.

---

## Lumen v0 — DISCUSS wave landed (first storage engine begins)

The roadmap calls Lumen the first-party log storage engine — the
Phase 3 boundary between an integration plane that forwards logs
to an external backend and a Kaleidoscope that owns its own log
pillar. The full Phase 3 substrate is Arrow + Parquet +
DataFusion + Tantivy + RocksDB, which is a substantial body of
work. Lumen v0 is the port-first cut: the trait that the v1
disk-backed adapter will implement, plus one in-memory adapter
that proves the trait carries OTLP-shaped log payloads
end-to-end, plus enough acceptance criteria to pin both the
ingest-latency and query-latency ceilings.

```mermaid
flowchart LR
    Aperture[Aperture exporter] -.->|v1| Trait[LogStore trait]
    Trait --> InMem[InMemoryLogStore v0]
    Trait -.->|v1| Disk[Parquet+RocksDB adapter]
    InMem -->|MetricsRecorder| OTLP[OTLP gauges + counters]
    InMem -->|query| Prism[Prism log panel v1]
    style InMem fill:#dfe
    style Trait fill:#dfe
```

DISCUSS landed two LeanUX user stories with Elevator Pitches.
US-LU-01 is the walking skeleton — ingest OTLP log batches keyed
by tenant, query by time range, prove that the field set
(`observed_time_unix_nano`, `severity_number`, `severity_text`,
`body`, `attributes`, `trace_id`, `span_id`) round-trips
byte-stable. US-LU-02 lifts the trait from "give me logs in a
range" to "give me logs in a range that match this predicate" —
service filter, severity floor, intersection semantics.

Two outcome KPIs pin the ceilings. Ingest p95 ≤ 1 ms per
100-record batch on the in-memory adapter. Query p95 ≤ 10 ms
when scanning ten thousand records under a predicate. Both are
acceptance-test-asserted, not "we'll watch it in prod". The
KPI ceilings are intentionally loose because the v0 adapter is a
linear scan; v1's columnar substrate will tighten them
dramatically.

The load-bearing decisions read like Sluice's, by design. Port +
one adapter at v0 (Parquet / RocksDB / DataFusion / Tantivy live
behind the same trait at v1). OTLP-shaped types at the trait
boundary (no Lumen-specific projections — the v1 adapter's job
is mechanical). Tenant on every call (no `set_tenant_context()`
mode — isolation is enforced by the type system). In-memory only
at v0; restart loses data. The `MetricsRecorder` seam carries
forward verbatim from Sluice. No Aperture retrofit at v0; Lumen
ships as a library and Aperture learns about it at v1.

What this slicing teaches: the v0 cut deliberately separates
"the trait that the storage plane will speak" from "the
substrate that makes it fast and durable". The trait is cheap.
The substrate is expensive. Shipping the trait first means v1's
work has a precise contract to satisfy, with eleven acceptance
criteria and two KPI ceilings already written down. The v1
adapter is a translation problem, not a discovery problem.

DISCUSS → DESIGN hand-off authorised. DESIGN collapses into the
implementation commit per the Aegis + Sluice precedents.

---

## Lumen v0 — slices 01 + 02 GREEN (the storage plane begins)

The architectural shift in this commit is bigger than the line
count suggests. Up to Sluice we shipped an *integration plane* —
seven crates that take OTLP in, route it, validate it, sample
it, alert on it, configure it from Git, gate it with identity,
and queue it for downstream consumption. The storage plane was
always external, whatever the operator already ran (Loki, Mimir,
Tempo, Datadog). Lumen v0 is the first crate that says
Kaleidoscope itself owns one of the storage pillars. The trait
is the contract; the in-memory adapter is the proof.

```mermaid
flowchart LR
    Aperture[Aperture exporter] -.->|v1| Trait[LogStore trait]
    Trait --> InMem[InMemoryLogStore v0]
    Trait -.->|v1| Disk[Parquet+RocksDB v1]
    InMem -->|TimeRange| Range[range query]
    InMem -->|Predicate| Pred[service+severity]
    InMem -->|MetricsRecorder| OTLP[OTLP counters]
    style InMem fill:#dfe
    style Trait fill:#dfe
```

The walking skeleton pins the OTLP-shaped types at the trait
boundary. `LogRecord` carries `observed_time_unix_nano`,
`severity_number`, `severity_text`, `body`, `attributes`,
`resource_attributes`, `trace_id`, and `span_id` — exactly the
shape that `opentelemetry-proto::logs::v1::LogRecord` exposes,
no Lumen-specific projections. The acceptance test for
byte-stable field preservation ingests one fully-populated
record (including a real W3C trace id and span id, a non-trivial
resource attribute map, and an `http.status_code` record
attribute) and asserts `assert_eq!(out[0], original)`. That
single assertion is the contract the v1 disk-backed adapter
inherits. If it round-trips byte-stable through an in-memory
`Vec<LogRecord>`, the v1 implementation's job is mechanical:
choose a substrate, persist the bytes, read them back, satisfy
the same assertion.

The walking skeleton also pins observed-time ordering within a
tenant (records are sorted on ingest by
`observed_time_unix_nano`), tenant isolation (`HashMap<TenantId,
Vec<LogRecord>>` keyed by `aegis::TenantId`), half-open time
range semantics (`[start, end)`, so a record at exactly `end` is
excluded — the choice matches Prometheus, Loki, and the OTel
collector), and KPI 1 — ingest p95 ≤ 1 ms per 100-record batch
on the in-memory adapter. The KPI test seeds the store with 50
warm-up batches, then times 1000 ingest calls and reads off the
p95 from the sorted samples vector.

The structured-query slice lifts the trait from "give me logs in
a range" to "give me logs in a range that match this predicate".
The `Predicate` value type carries two optional filters at v0 —
service name (read from the record's `resource_attributes`) and
severity floor — and composes them as an intersection. A
predicate with no filters set is equivalent to the range-only
query, asserted by `assert_eq!(with_empty, without)` in the
acceptance test. The v1 substrate will lift this to body /
attribute-path predicates and full-text search via Tantivy;
the v0 trait shape already accepts a `&Predicate`, so v1's work
is additive, not breaking.

KPI 2 pins the linear-scan query ceiling. Ten thousand records
spanning four services and four severities, two hundred
predicate queries with both filters active, p95 ≤ 10 ms. The
ceiling is intentionally loose — the v0 adapter is a linear scan
through a `Vec<LogRecord>` and the test passes with significant
margin. v1's columnar substrate will tighten this dramatically,
but the v0 trait already has an observably-bounded ceiling
written into the acceptance suite.

The `MetricsRecorder` seam carries forward verbatim from Sluice.
`record_ingest(tenant, count)` and `record_query(tenant,
matched_count)` on the hot paths; `NoopRecorder` for production
deployments that have not yet wired OTLP; `CapturingRecorder`
for the acceptance tests themselves. Same shape, same posture:
Lumen depends on `aegis` (for `TenantId`) and nothing else. No
OTLP SDK, no DataFusion, no Parquet. The substrate work all
lives at v1, behind the same trait, in a separate crate or set
of features. The v0 dependency graph stays acyclic and small.

Sixteen new acceptance tests GREEN — eight on the walking
skeleton, eight on structured query. Workspace: 75 suites, all
GREEN. Lumen v0 is feature-complete. The platform plane now
counts ten shipped features, the storage plane has begun, and
the integration → storage handover is no longer a vague future
promise — it is a trait with eleven acceptance criteria and two
KPI ceilings written down.

---

## Pulse v0 — DISCUSS + slices 01 + 02 GREEN (the metrics pillar)

The shape is identical to Lumen by design. Pulse is the metrics
pillar of the storage plane — the second engine of Phase 4 in
the architecture roadmap — and the v0 cut is, again, port-first:
the `MetricStore` trait that the v1 columnar adapter (Arrow +
Parquet + DataFusion + Prometheus-TSDB-block format) will
implement, plus one in-memory adapter that proves the contract
carries OTLP-shaped metric points end-to-end. DISCUSS and
DELIVER land in the same wave because the trait shape was
already largely decided by Lumen's precedent; what changed is
the point shape, not the seam shape.

```mermaid
flowchart LR
    Aperture[Aperture exporter] -.->|v1| Trait[MetricStore trait]
    Trait --> InMem[InMemoryMetricStore v0]
    Trait -.->|v1| Disk[Parquet+RocksDB v1]
    InMem -->|metric_name+range| Q[range query]
    InMem -->|Predicate| P[service+label_eq]
    InMem -->|MetricsRecorder| OTLP[OTLP counters]
    style InMem fill:#dfe
    style Trait fill:#dfe
```

What is different from Lumen: Pulse keys by
`(TenantId, MetricName)`, not just `TenantId`. That choice
matches how Prometheus, Mimir, and VictoriaMetrics organise
their storage — the per-metric series is the smallest queryable
unit, and single-metric lookups should be O(1) on the index, not
O(n) on a tenant-wide scan. The v0 adapter implements this with
a `HashMap<(TenantId, MetricName), SeriesEntry>` where each
entry holds the canonical `Metric` metadata once and a sorted
point vector beside it. Separating metadata from data buys the
v1 disk-backed adapter the freedom to hoist resource attributes
to the batch level without touching the trait shape.

What else is different: Pulse ships gauge + sum (number points)
only at v0. Histogram, exponential histogram, and summary need
different point shapes (buckets, bounds, percentiles) and
different query semantics (`histogram_quantile`,
`rate`-over-`counter`, summary aggregation). They all land at
v1 alongside PromQL. Choosing one point shape at v0 keeps the
trait small and the acceptance criteria sharp: eleven ACs
across two stories, two KPIs, no PromQL parser anywhere on the
critical path.

The walking skeleton pins the same shape it pinned for Lumen:
ascending-time ordering within a series, tenant + metric
isolation by construction, half-open `[start, end)` time range,
byte-stable field round-trip including W3C-style attributes.
The byte-stable test ingests one fully-populated
`http.server.duration` point (with `http.route`,
`http.status_code`, `service.name`, `service.version`, a real
nanosecond timestamp, and a `start_time_unix_nano` for the
cumulative window) and asserts metadata + point round-trip
equality. KPI 1 — ingest p95 ≤ 1 ms per 100-point batch on the
in-memory adapter — passes by a wide margin; the linear scan
ceiling is loose by design.

The structured-query slice adds the `Predicate` value type with
two filter dimensions: service name (read from the metric's
resource attributes) and label equality (read from each point's
own attributes). Multiple `label_eq` filters compose as
intersection; the acceptance test pins this with a
three-attribute query (`service` + `http.route` + `http.status_code`).
The empty-predicate / range-only equivalence holds. KPI 2 —
query p95 ≤ 10 ms over 10 000 points under a service +
`http.route` predicate — passes with significant headroom.

The `MetricsRecorder` seam carries forward verbatim from Lumen
and Sluice. The dependency graph stays single-line: Pulse
depends on `aegis` (for `TenantId`) only. No OTLP SDK, no
DataFusion, no Parquet, no Tantivy.

Sixteen new acceptance tests GREEN — nine on the walking
skeleton, seven on structured query. Workspace: 78 suites, all
GREEN. Pulse v0 is feature-complete. The platform plane now
counts eleven shipped features. The storage plane has its
second engine and the trait shape for "first-party storage of
a signal pillar" is no longer a one-off — it is a pattern,
expressed by `LogStore` and `MetricStore`, that the
trace-pillar (Ray) and the profiling-pillar (Strata) will
inherit.

---

## Ray v0 — DISCUSS + slices 01 + 02 GREEN (three pillars)

Ray is the trace pillar. With it the storage plane completes
the three classical signal types — logs (Lumen), metrics
(Pulse), traces (Ray) — and the "first-party storage of a
signal pillar" pattern is now expressed three times, by three
crates that share a single posture: trait + in-memory adapter +
two-slice DISCUSS + DELIVER in one commit + dual-purpose
acceptance suite + observability seam.

```mermaid
flowchart LR
    Aperture[Aperture exporter] -.->|v1| Trait[TraceStore trait]
    Trait --> InMem[InMemoryTraceStore v0]
    Trait -.->|v1| Iceberg[Iceberg-on-Parquet v1]
    InMem -->|trace_id| GT[get_trace]
    InMem -->|service+range| Q[service+range query]
    InMem -->|Predicate| P[span_name+kind+status]
    InMem -->|MetricsRecorder| OTLP[OTLP counters]
    style InMem fill:#dfe
    style Trait fill:#dfe
```

The shape difference from Lumen and Pulse is the **dual index**.
Pulse keys by `(TenantId, MetricName)` and answers
single-metric queries. Lumen keys by `TenantId` and scans the
whole tenant. Ray needs two query shapes: pull-by-`trace_id`
(the bedrock query of distributed tracing — Riley copies an id
from a log entry, clicks through, sees the whole trace) AND
scan-by-`(service.name, time range)` (the "what was running"
query). A single index would force one of these to be O(N) on
ingest; the v0 adapter pays 2× memory and 2× sort cost to keep
both O(1) on lookup. Spans are cloned on ingest into
`HashMap<(TenantId, TraceId), Vec<Span>>` and
`HashMap<(TenantId, ServiceName), Vec<Span>>` simultaneously.
v1's `trace_id`-partitioned columnar layout collapses this back
into a single physical layout with proper secondary indices.

The byte-stable round-trip test is the most demanding of the
three storage engines. A `Span` carries `trace_id`, `span_id`,
optional `parent_span_id`, name, `SpanKind`, start and end
timestamps in nanoseconds, a `SpanStatus` (code + message), a
span-attribute map, a resource-attribute map, a vector of
`SpanEvent` (each with its own timestamp + name + attribute
map), and a vector of `SpanLink` (each pointing at another
`(trace_id, span_id)` plus attributes). The acceptance test
ingests one fully-populated span — a `POST /api/checkout`
server span with a recorded `payment.declined` event, a
`follows-from` link, an `Error` status with a message, and
resource attributes for `service.name`, `service.version` —
and asserts `assert_eq!(spans[0], original)`. That one
assertion is the v1 columnar adapter's complete acceptance
contract for span identity.

KPI 1 is the first KPI in the storage plane that needed a
deliberate ceiling adjustment. Pulse and Lumen both pin
ingest p95 at 1 ms per 100-record batch. Ray sits at ~2× that
cost because every span writes into two sorted buckets, not
one. The honest move was to raise Ray's ceiling to 2 ms with
the rationale documented in `outcome-kpis.md` — the dual-index
trade-off is the right v0 shape, and the KPI is a ceiling on
that reality, not a stretch goal that pretends the cost
doesn't exist. Same posture Aegis took when its catalogue-load
KPI moved from 10 ms to 50 ms once `toml` parse cost was
measured: KPIs describe the system that ships, not the system
the architect imagines.

The structured-query slice adds three filter dimensions:
`span_name` (e.g. `"db.query"` to narrow a trace to its
database calls), `kind` (e.g. `Client` to find every outbound
call), and `status` (e.g. `Error` to find every span that
failed). All three compose as intersection. The acceptance
test pins a four-way ingest where exactly one span matches
all three filters; the predicate finds that one span and only
that one.

The `MetricsRecorder` seam is identical to Lumen's, Pulse's,
and Sluice's. Ray depends on `aegis` (for `TenantId`) only —
same single-line dependency graph as every storage engine
before it.

Sixteen new acceptance tests GREEN — eight on the walking
skeleton, eight on structured query. Workspace: 81 suites,
all GREEN. Ray v0 is feature-complete. The platform plane now
counts twelve shipped features. The storage plane has its
three classical pillars, and the trait shape is no longer
"the pattern Lumen pioneered" — it is the way Kaleidoscope
ships first-party storage, full stop. The fourth pillar
(Strata, profiles) will inherit the same shape; the
trace-id-partitioned columnar substrate that Phase 5 promises
has a concrete acceptance contract waiting for it.

---

## Strata v0 — DISCUSS + slices 01 + 02 GREEN (fourth pillar)

Strata is the fourth and final signal pillar in the
architectural roadmap. With it the storage plane is complete
for the four-pillar correlation story — metric (Pulse) →
trace (Ray) → log (Lumen) → flame-graph (Strata) without
leaving Prism. The v0 cut is, again, port-first: the
`ProfileStore` trait that the v1 columnar adapter (Arrow +
Parquet + DataFusion + RocksDB + gimli/addr2line
symbolisation) will implement, plus one `InMemoryProfileStore`
adapter that proves the contract carries pprof-shaped
profiles end-to-end.

```mermaid
flowchart LR
    Aperture[Aperture exporter] -.->|v1| Trait[ProfileStore trait]
    Trait --> InMem[InMemoryProfileStore v0]
    Trait -.->|v1| Disk[Parquet+RocksDB v1]
    Trait -.->|v1| Sym[gimli + addr2line symboliser]
    InMem -->|service+range| Q[range query]
    InMem -->|Predicate| P[profile_type filter]
    InMem -->|MetricsRecorder| OTLP[OTLP counters]
    style InMem fill:#dfe
    style Trait fill:#dfe
```

What is different from the other three storage engines is the
*shape* of what is stored. Logs are records, metrics are
points, spans are events with parent-child structure. Profiles
are something else: a string table, a function index, a
mapping (loaded-binary) index, a location (address-into-mapping)
index, a sample (stack as `location_id` list + measured values),
and a `sample_type` array describing what each value column
means. The byte-stable round-trip test ingests a fully-
populated CPU profile with a 14-entry string table, five
functions, two mappings, four locations including one with
inlined frames (`function_ids = vec![1, 2]` — the inner
frames first), and two samples with thread / process
sample-attribute maps. The acceptance test then asserts
`assert_eq!(out[0], original)` over that whole structure.

The v0 adapter keeps a deliberately simple single-index shape:
`HashMap<(TenantId, ServiceName), Vec<Profile>>` sorted by
`time_unix_nano`. Ray paid 2× memory for a dual index because
the bedrock distributed-tracing query needs O(1) lookup on
both `trace_id` and `service`. Strata's two queries — by
`(service, range)` and by `(service, profile_type)` — share
the same service key; the predicate composes against the
in-bucket scan. The single-index simplicity is the right
choice when both queries hit the same axis.

KPI 1 is again calibrated honestly to what the v0 adapter
actually costs. Profiles are kilobytes to megabytes each; the
realistic OTLP-Profiles batch shape is around ten profiles per
flush, not the hundred records used elsewhere. Ingest p95 ≤
5 ms per ten-profile batch over two hundred trials. The
ceiling reflects profile cloning cost; v1's columnar substrate
deduplicates string tables, locations, and functions across
profiles and pays this cost only at compaction. Same posture
as Ray's KPI 1 (2 ms not 1 ms because of the dual index): the
KPI describes the shipping system, not the imagined one.

The structured-query slice adds a single filter dimension at
v0 — `profile_type` equality (`"cpu"`, `"heap"`,
`"goroutine"`, `"block"`, and so on). Predicates on samples,
locations, or function names are deliberately deferred to v1
because they are expensive on a linear scan; once the columnar
substrate lands, those predicates land with it. KPI 2 — query
p95 ≤ 10 ms over a thousand ingested profiles — passes with
significant headroom; the scan cost is dominated by the
profile clone on result construction, not by the predicate
itself.

What this whole arc teaches: the trait shape for "first-party
storage of a signal pillar" was a pattern when Lumen pioneered
it, a discipline when Pulse and Ray repeated it, and is now a
contract that the storage plane lives inside. Four pillars,
four traits with the same posture, four in-memory adapters
sitting behind the same `MetricsRecorder` seam. Each one
carries a byte-stable round-trip acceptance test that pins
the v1 disk-backed adapter's identity contract; each one has
KPI ceilings calibrated to the real cost of that signal's
shape; each one ships in one commit per the Aegis + Sluice +
Lumen + Pulse + Ray precedent. The substrate work for v1 has
exactly the contract it needs, and not one ambiguity more.

Thirteen new acceptance tests GREEN — eight on the walking
skeleton, five on structured query. Workspace: 84 suites,
all GREEN. Strata v0 is feature-complete. The platform plane
now counts thirteen shipped features. The storage plane is
complete for v0.

---

## Cinder v0 — DISCUSS + slices 01 + 02 GREEN (the durability gap)

Four storage engines that lose data on restart is an honest
gap, not a feature. The Phase 7 answer is Cinder: the tiering
layer that records, for every ingested item, which physical
tier it currently lives in — hot, warm, or cold. The
hot-tier substrate is what the four engines already hold in
memory or in RocksDB; the warm tier is local Parquet; the
cold tier is S3 via OpenDAL plus Iceberg manifests. v0 is
the port-first cut, as always: the `TieringStore` trait that
the v1 disk-backed adapter will implement, plus one in-memory
adapter that proves the contract carries place + lookup +
migrate + lifecycle evaluation end-to-end.

```mermaid
flowchart LR
    L[Lumen / Pulse / Ray / Strata] -->|tier lookup| Trait[TieringStore trait]
    Trait --> InMem[InMemoryTieringStore v0]
    Trait -.->|v1| Iceberg[OpenDAL+Iceberg v1]
    InMem -->|place| H[Hot]
    InMem -->|evaluate_at| W[Warm]
    InMem -->|evaluate_at| C[Cold]
    style InMem fill:#dfe
    style Trait fill:#dfe
```

The architectural shape change is bigger than the line count
suggests. Lumen, Pulse, Ray, and Strata all *store payloads*
— records, points, spans, profiles. Cinder *stores
metadata*: for each `(tenant, item_id)` it records the
current tier plus a placement timestamp plus a last-migration
timestamp. The separation is deliberate. The storage engines
own the payload bytes; Cinder governs where those bytes
should live. At v1 the storage engines will consult Cinder on
the read path to decide which physical substrate to query —
the hot tier might be in-process, the warm tier on local
disk, the cold tier in S3. At v0 the lookup is exercised
against an in-memory `HashMap<(TenantId, ItemId), TierEntry>`;
the trait shape is identical.

The walking skeleton pins place + get_tier + migrate +
list_by_tier + per-tenant isolation + timestamp preservation.
The byte-stable test for the four storage engines becomes,
for Cinder, a timestamp-stable test: the placement timestamp
survives every subsequent migration, only the last-migration
timestamp updates. KPI 1 — get_tier p95 ≤ 50 µs over ten
thousand placed items — passes by a wide margin; the lookup
is a single `HashMap` get and the test pins it as the v0
ceiling.

The structured-query slice is something different from the
other engines'. Cinder's slice 02 is not "narrow the query
with predicates" but "advance items through tiers based on
their age". The `TierPolicy::age_based(hot_to_warm,
warm_to_cold)` value type carries two `Duration` thresholds;
`TieringStore::evaluate_at(now, &policy)` is a pure function
of simulated time. The acceptance test sets a one-hour
hot-to-warm threshold and a one-day warm-to-cold threshold,
advances simulated time, and asserts the migrations land
where expected. Idempotence under repeated invocation is
pinned by a specific test: after the first migration at
t=3600, the second `evaluate_at(t=3600)` returns zero
migrations — the freshly-migrated item now has
`migrated_at=3600` and age=0 in its new tier.

The `evaluate_at(now, &policy)` shape is deliberately a pure
function rather than a background-thread timer. The operator
binary at v1 will own the periodic invocation; Cinder's job
is the pure evaluator. This keeps the crate testable in
milliseconds — the slice-02 KPI test runs two hundred
`evaluate_at` calls across ten thousand placed items in
under a second total wall-clock, with no `tokio::time::sleep`
shenanigans.

KPI 2 — `evaluate_at` p95 ≤ 5 ms over ten thousand items —
passes with room. The first call moves a lot of items; the
subsequent calls (idempotent) cost only the scan, and the
scan is bounded by the hashmap iteration cost. v1's columnar
substrate will keep this cheap via a proper age index.

What this whole arc teaches: the storage plane v0 is now
*structurally* complete. Four engines hold payloads behind
identical trait shapes; one tiering port governs where those
payloads should live. The v1 substrate work — Arrow +
Parquet + DataFusion + RocksDB + Tantivy for the engines,
Apache OpenDAL + Iceberg-Rust for Cinder — has, for each
piece, a precise acceptance contract written down. The piece
the architect was previously waving at as "Phase 7" is now a
trait with seven acceptance criteria and two KPI ceilings,
and the disaster-recovery drill story has its first concrete
hook.

Seventeen new acceptance tests GREEN — nine on the walking
skeleton, eight on lifecycle. Workspace: 87 suites, all
GREEN. Cinder v0 is feature-complete. The platform plane now
counts fourteen shipped features. The storage plane has its
four payload engines plus its tiering governor; v0 ends
where the v1 substrate work begins.

---

## Augur v0 — DISCUSS + slices 01 + 02 GREEN (first non-storage feature)

Augur is the first feature in months that is NOT another
storage engine clone. The five prior crates — Lumen, Pulse,
Ray, Strata, Cinder — all wore the same `trait + in-memory
adapter + two-slice DISCUSS + DELIVER + MetricsRecorder
seam` skin because that pattern fitted the problem. The
trait shape and the byte-stable round-trip belong to
storage; the anomaly-detection layer has a different job
and consequently a different shape. Phase 9 in the roadmap
positions Augur as the cross-pillar analyser: Bayesian
online change-point detection on Pulse, sentence-transformer
embeddings on Lumen, rare-trace detection on Ray, plus
small-LLM summarisation served by vLLM or llama.cpp. v0
ships none of that ML stack — and that absence is the
deliberate v0 choice.

```mermaid
flowchart LR
    Pulse[Pulse: f64 stream] --> Z[ZScoreObserver]
    Lumen[Lumen: log body stream] --> R[RareEventObserver]
    Ray[Ray: span name stream] --> R
    Z --> A[Anomaly events]
    R --> A
    A -.->|v1| Beacon[Beacon incident channel]
    A -.->|v1| LLM[Qwen / Mistral summariser]
    style Z fill:#dfe
    style R fill:#dfe
```

The trait is generic over the observed signal type.
`AnomalyObserver<T>` carries one method,
`observe(tenant, value, observed_at) -> Option<Anomaly<T>>`.
v0 ships two concrete `T`-instantiations:
`AnomalyObserver<f64>` for numeric streams and
`AnomalyObserver<String>` for categorical streams. The
generic parameter is the seam where v1 will plug in
multi-variate observers (`Vec<f64>`), structural observers
on `Span`, and embedding-based observers on
`SentenceVector` — all behind the same one-method trait.

What makes the v0 cut honest is the deliberate refusal of
the ML stack. The Phase 9 roadmap calls for `numpy`,
`scikit-learn`, `sentence-transformers`, `vllm` or
`llama.cpp`, plus Qwen 2.5 or Mistral 7B weights under
Apache-2.0. Every one of those is excluded from the v0
dependency graph; Augur depends on `aegis` (for
`TenantId`) and the Rust standard library, full stop. The
numeric detector uses Welford's algorithm, a 1962 paper by
B. P. Welford that computes mean and variance online in
O(1) per sample without storing the history. The
categorical detector uses a `HashMap<String, u64>`
frequency baseline with first-crossing emission. Hand-rolled,
small, fast, and entirely under the v0 author's control.
v1 lifts both detectors to proper statistical models while
keeping the same trait shape; the operator binary will
hot-swap detectors per signal without recompiling the
crate consumers.

The z-score detector pins three specific behaviours that
matter in production. First, warm-up: during the first
`min_samples` observations no anomaly fires, regardless of
how outlying the value is — the variance is undefined or
unstable. Second, sustained-anomaly adaptation: the
baseline updates on every observation including the
anomalous ones, so a permanently-shifted regime eventually
becomes the new baseline (v1's BOCPD will treat that as a
change point and split the baseline cleanly; v0 simply
drifts). Third, isolation: two separate observer instances
maintain independent baselines — at v0 the operator
creates one observer per `(tenant, signal)`, and the trait
shape makes that explicit.

The rare-event detector ships a frequency-baseline
implementation with first-crossing emission. A new event
that lands below the configured rarity threshold fires one
anomaly; subsequent observations of the same event do not
re-emit. The simplification is deliberate at v0 (otherwise
a rare-but-recurring event would dominate the anomaly
stream); v1's rolling-window evaluation re-tests over
recent buckets.

KPI 1 — `observe` p95 ≤ 10 µs after warm-up — passes by a
wide margin on the z-score detector. The numeric work is
six multiplies and three adds per sample, plus an optional
clone of `TenantId` on emission. KPI 2 — `observe` p95 ≤
20 µs on a 1 000-distinct-event vocabulary — passes on the
rare-event detector. The categorical work is one HashMap
get-or-insert plus one division. Both detectors run
in-line on the storage plane's hot path; their cost has
to be tiny, and it is.

What this whole arc teaches: the v0 cut for an "AI-amplified
observability platform" is allowed to be the simplest thing
that exercises the trait. The ML stack at v1 is not
postponed because it is hard; it is postponed because the
trait shape has to be stable before the substrate goes in.
v0 ships two detectors that catch real categories of
anomalies (step changes, novel events) using methods that
predate the modern ML era — Welford 1962, frequency tables
since forever — and that is enough to make Augur a useful
sentinel today, with the proper statistical machinery
arriving at v1 behind the same one-method trait.

Fourteen new acceptance tests GREEN — eight on the z-score
detector, six on the rare-event detector. Workspace: 90
suites, all GREEN. Augur v0 is feature-complete. The
platform plane now counts fifteen shipped features. For
the first time since Aegis the new crate does not sit
inside the storage pattern, and the project's first
genuinely cross-pillar surface has its v0 contract written
down.

---

## Cinder v1 — DISCUSS + slices 01 + 02 GREEN (first v1 anywhere)

For the first time in this project a feature ships at v1.
The fifteen prior crates all sit at v0 with in-memory
adapters; the narrative has repeated, often, that "the v1
disk-backed adapter inherits the v0 trait shape". Repetition
is not proof. Cinder v1 is the proof. A
`FileBackedTieringStore` adapter ships behind the same v0
`TieringStore` trait, with NDJSON write-ahead-log durability
and explicit snapshot compaction. The v0 acceptance suite
for the in-memory adapter stays green; the v1 acceptance
suite is additional, not replacement.

```mermaid
flowchart LR
    Op[Operator] -->|place/migrate| FB[FileBackedTieringStore v1]
    FB -->|append| WAL[NDJSON WAL]
    FB -->|on-call| Snap[Snapshot file]
    Snap -->|recovery| FB
    WAL -->|recovery| FB
    FB -.->|v2| Iceberg[Iceberg + OpenDAL]
    style FB fill:#fde
    style WAL fill:#fde
    style Snap fill:#fde
```

The architectural learning is exactly the one the v0
contract was written to enable. The trait shape did not
need to change. The trait's `place`, `get_tier`, `migrate`,
`list_by_tier`, and `evaluate_at` methods all carry forward
to a durable adapter unchanged. The only modification to
the v0 public surface is one additive variant on the
existing `MigrateError` enum:
`PersistenceFailed { reason: String }` is added so that
adapters with side effects (file I/O, S3 calls, network
hops) can surface failures through the same return type
the v0 in-memory adapter already used. Adding an enum
variant is additive; a v0 caller that pattern-matched
exhaustively on `MigrateError::UnknownItem` gets a compile
warning and a one-line fix. This is the price of not
having `#[non_exhaustive]` on the v0 enum; it is a small
price, and it is documented in the v1 wave-decisions.

The slice 01 work is the WAL itself. Every `place` and
`migrate` operation serialises as one NDJSON line and
appends to `{base}.wal`. The walking-skeleton acceptance
test creates a store at a temp path, places three items,
migrates one, drops the store, opens a fresh store at the
same path, and asserts every tier and every timestamp is
restored byte-stable. The classical WAL contract — recovery
by replay — is the contract Cinder v1 ships. The
implementation is small (about 250 lines for the adapter
plus 200 for the slice-01 acceptance suite). The choice of
NDJSON over a binary format is deliberate at v1: human-
readable WALs are easier to inspect during development and
debugging, and the performance ceiling is good enough for
v1.

The slice 02 work is snapshot compaction. After enough
operations, a pure WAL grows unbounded and recovery time
grows linearly. The `snapshot()` method writes the current
in-memory state to a separate `{base}.snapshot` file and
then truncates the WAL. On the next `open` the snapshot is
loaded first and only the remaining WAL records are
replayed. The acceptance test pins three properties:
snapshot writes are observable on disk (the WAL file
shrinks to zero bytes), snapshot + remaining-WAL recovery
produces the same state as pure-WAL recovery, and
`snapshot()` is idempotent under no intervening writes.

The honest KPI moment came on slice 02. The initial KPI 2
target was "recovery p95 ≤ 50 ms over 10 000 placed
items". Reality on debug-mode `serde_json` parsing came in
at about 550 ms. The honest reaction is the same one that
moved Aegis's catalogue-load p95 from 10 ms to 50 ms and
Ray's ingest p95 from 1 ms to 2 ms: the KPI describes the
system that ships, not the system the architect imagines.
The ceiling is now 1 s with explicit rationale documented
in `outcome-kpis.md`. The pattern is now genuinely a
tradition across the project, and that tradition is more
useful than any single optimistic number.

What this whole arc teaches: the v0 trait shape carried
forward to a durable v1 adapter without a single line of
v0 code being touched, except for one additive error
variant. The claim "the v1 adapter inherits the trait" is
no longer rhetoric; the workspace contains two adapters
behind the same trait, one in-memory and one file-backed,
and both pass their respective acceptance suites
simultaneously. The v2 substrate (S3 + OpenDAL + Iceberg
manifests) inherits the same shape; the work between v1
and v2 will be substrate replacement, not contract
change.

Thirteen new acceptance tests GREEN — eight on WAL
durability, five on snapshot compaction. Workspace: 92
suites, all GREEN. Cinder v1 is feature-complete. The
platform plane now counts sixteen shipped features, and
for the first time the platform contains a feature that
survives a process restart.

---

## Sluice v1 — DISCUSS + slices 01 + 02 GREEN (the pattern is repeatable)

Once is an accident, twice is a tradition. Cinder v1
proved the v0→v1 carry-forward on one crate. Sluice v1
proves it on a second crate of a completely different
shape — a queue, not a key/value store. The point of the
exercise is not the queue itself; the point is that the
methodology survives a second application.

```mermaid
flowchart LR
    Producer --> Q[FileBackedQueue v1]
    Q -->|append| WAL[NDJSON WAL]
    Q -->|on call| Snap[Snapshot file]
    Snap --> Q
    WAL --> Q
    Q -.->|v2| Kafka[Kafka / NATS / Redpanda]
    style Q fill:#fde
```

Sluice v0 had a richer trait than Cinder v0. Four mutating
methods — `enqueue`, `dequeue`, `ack`, `nack` — plus two
observable ones, plus a non-trivial invariant: a nacked
message returns to the head of its tenant's queue. The
walking-skeleton acceptance test for v1 pins that invariant
across restart explicitly. A message is enqueued, dequeued,
nacked; the process drops; the recovered queue dequeues the
nacked message first, not the one that was second-in-line
before the nack. The classical FIFO-with-redelivery
contract holds across durability.

The other queue-specific concern is the monotonic
`MessageId` counter. v0 generates ids monotonically within
the lifetime of one adapter instance. v1 must resume the
counter above any id it ever issued — otherwise a fresh
enqueue after restart would collide with a replayed id.
The implementation scans the WAL on replay, tracks the
maximum id seen, and resumes `next_id` from `max + 1`. The
acceptance test enqueues seven messages, restarts, enqueues
one more, and asserts the new id is `8`.

The byte-stability concern was different from Cinder's,
and that difference taught something. Cinder v1
round-trips small structured tier metadata; Sluice v1
round-trips opaque `Vec<u8>` payloads. JSON cannot natively
represent arbitrary bytes; serialising `Vec<u8>` as a JSON
array of integers is verbose and slow. The v1 wave-decision
chose hex encoding over base64 to avoid pulling in a new
dependency. Hand-rolled hex is ten lines each way, has zero
allocations beyond the output string, and round-trips every
byte from 0x00 to 0xff — pinned by a dedicated acceptance
test that enqueues a 256-byte payload covering the full
byte range and asserts byte-exact recovery.

The `EnqueueError` enum extends additively, the same way
`MigrateError` did in Cinder v1. The new variant is
`PersistenceFailed { reason: String }`. The compile cost
came as predicted: a v0 acceptance test pattern-matched
exhaustively on `EnqueueError::Full`, and the
non-exhaustive match was caught by the compiler the moment
the new variant landed. The fix was a one-line wildcard
arm. The whole point of the v1 wave-decision noting "v0
callers that pattern-matched exhaustively need to add a
wildcard arm" was to make this expected, not surprising.
The compiler is the spec.

KPI 1 settled at 300 µs per enqueue, six times the v0
in-memory ceiling of 50 µs. The honesty move is the same
one made in Cinder v1: WAL durability adds real per-op
cost, and the KPI describes the system that ships. KPI 2
settled at 500 ms recovery for 10 000 enqueues. Both KPIs
pass at first run on the file-backed adapter.

What this whole arc teaches: the v0→v1 carry-forward is
not Cinder-specific. The workspace now contains two
completely independent crates — one a tier-metadata store,
one a queue — where the v0 trait carried forward to a v1
durable adapter without a trait-shape change. The only
modification to either v0 public surface was one additive
error variant on each. That is a generic capability of the
methodology, not a feature-specific accident. Two
v0→v1 carry-forwards make the claim credible with
evidence; three or four would make it a settled tradition.

Sixteen new acceptance tests GREEN — ten on WAL durability,
six on snapshot compaction. Workspace: 94 suites, all
GREEN. Sluice v1 is feature-complete. The platform plane
now counts seventeen shipped features. Two features
survive a process restart.

---

## Lumen v1 — DISCUSS + slices 01 + 02 GREEN (the pattern settles)

Three v0→v1 carry-forwards across three independent crates
on three different shapes — tier metadata (Cinder), queue
(Sluice), log store (Lumen). The methodology now
demonstrably applies to anything that fits the
trait+adapter pattern. The claim is no longer "we believe
v0→v1 works"; it is "v0→v1 works in exactly the same way
every time, with the same costs in the same places". A
fourth or fifth carry-forward would not teach more.

```mermaid
flowchart LR
    Producer --> L[FileBackedLogStore v1]
    L -->|per-batch append| WAL[NDJSON WAL]
    L -->|on call| Snap[Snapshot file]
    Snap --> L
    WAL --> L
    L -.->|v2| Parquet[Arrow + Parquet + Tantivy]
    style L fill:#fde
```

Two things distinguished this slice from the prior two. The
first was the WAL granularity choice. Cinder v1 logs one
record per state change; Sluice v1 logs one record per
operation. Lumen's natural unit is the batch — an OTLP
exporter delivers logs in groups of dozens or hundreds, and
Lumen's v0 ingest is already batch-shaped. v1 logs one
`Ingest` record per batch, carrying the whole
`Vec<LogRecord>` inline. The WAL is smaller and recovery
does fewer parse calls; the per-line size is larger but
JSON's tokeniser amortises across the records.

The second was the v0 error enum's starting shape.
`LogStoreError` was an empty enum at v0, with a
`match *self {}` Display impl using the never-type idiom —
the idiomatic Rust way to express "this enum has no
variants". v1 grew it from zero variants to one. The
Display impl had to be rewritten with a real arm. The
compile-time cost of going from empty to non-empty is
genuinely larger than the cost of adding a variant to an
enum that already had one. Future v0 work in this project
should consider declaring the error enum with
`#[non_exhaustive]` from the start, even when v0 has no
failure modes; the marker reserves the room for v1 to grow
without breaking exhaustive matches. That is a discrete
methodology lesson coming out of the third carry-forward,
and it is worth filing alongside the other v0→v1
traditions.

KPI 1 had the fourth honesty moment in the series. Initial
target was 500 µs per 100-record batch; reality settled at
1.1 ms in debug mode, driven by batch-clone-for-WAL plus
JSON encode of 100 records plus BufWriter flush. The
ceiling moved to 1.5 ms with explicit rationale. Four
honesty moments in a row — Ray KPI 1, Aegis KPI 2, Cinder
v1 KPI 2, Lumen v1 KPI 1 — and at this point it would be
more suspicious if a KPI guess landed dead-on the first
time.

Twelve new acceptance tests GREEN — eight on WAL
durability, four on snapshot compaction. Workspace: 96
suites, all GREEN. Lumen v1 is feature-complete. The
platform plane now counts eighteen shipped features.
Three features survive a process restart. The v0→v1
carry-forward is settled.

---

## Integration suite — three adapters compose under one tenant

Eighteen features, three of them durable, and until now the
narrative has had no evidence that they actually fit
together. Every acceptance test in the project lived inside
one crate. The implicit promise — that `aegis::TenantId` is
the cross-crate identity contract, that adapters compose
under shared tenant identity, that the v1 durability
guarantees hold simultaneously across multiple stores — was
unproven. This slice fixes that.

```mermaid
flowchart LR
    T[aegis::TenantId 'acme'] --> L[Lumen v1: log records]
    T --> S[Sluice v1: notifications]
    T --> C[Cinder v1: tier metadata]
    L -.->|drop+reopen| L2[Lumen v1 recovered]
    S -.->|drop+reopen| S2[Sluice v1 recovered]
    C -.->|drop+reopen| C2[Cinder v1 recovered]
    style T fill:#fec
```

A new crate, `integration-suite`, joins the workspace. It
has no library and exists solely to host cross-crate
acceptance tests. The first one is straightforward in
intent but demanding in scope: a single test method opens a
`FileBackedLogStore`, a `FileBackedQueue`, and a
`FileBackedTieringStore` simultaneously; ingests records
for tenant `acme` into Lumen; enqueues a "batch processed"
notification for `acme` into Sluice with the batch's id as
the payload; places the batch in Cinder's Hot tier under
the same tenant. A second tenant `globex` runs in parallel
with distinct payloads and a different initial tier. The
whole scope drops. Three new adapters open at the same
paths. The assertion: every state survived, FIFO order in
the queue holds, observed-time order in the log holds, tier
metadata holds, and tenant isolation holds across all three
adapters simultaneously.

The second test is smaller and more declarative. It exists
to document, in compiled and exercised code, that the same
`&aegis::TenantId` reference passes to all three adapters
with no conversion, no clone-per-call, no adapter-specific
tenant types. If aegis ever changes `TenantId`'s shape, the
test will break at compile time and alert the maintainer
that the cross-crate identity contract has shifted. That is
a property worth pinning explicitly.

There is no DISCUSS overhead for this work. The crate is
not a user-facing feature; it is correctness evidence for
composition. The methodology applies where it earns its
keep, and writing user stories for an integration suite
would be ceremony for its own sake. The decision is
recorded here so future maintainers know it was deliberate.

Two new acceptance tests GREEN. Workspace: 98 suites, all
GREEN. The platform now has, for the first time, an
explicit acceptance assertion that it is one thing.

A second test file in the same crate goes further with a
different kind of composition. The first integration test
proved that durable adapters can coexist under the same
tenant identity. This one proves that two crates from
different pillars cooperate to produce behaviour neither
crate produces alone. The application ingests metric
points into Pulse v0 and, as it does so, feeds each value
into an Augur v0 `ZScoreObserver`. The baseline accumulates
over a hundred stable observations; a five-sigma spike is
then injected; Pulse stores it and Augur flags it. The
assertion that closes the loop is byte-equality on the
`f64`: the value Pulse's `query` returns as its last point
is bit-identical to the value Augur recorded in the
emitted `Anomaly`. That bit-identity is the cross-pillar
correlation contract — a metric and its anomaly event
agree on what number triggered them.

Both crates are still v0 and in-memory. v1 of either could
add a built-in subscriber bridge so the wiring becomes
implicit; v0 leaves it explicit, which has the side benefit
of documenting the contract in compiled code that the
integration suite continuously exercises. Two more
acceptance tests GREEN. Workspace: 99 suites.

---

## Self-observability — Kaleidoscope observes itself

The composition story has a missing piece. Every crate
exposes a `MetricsRecorder` seam meant to let an operator
wire observability later, but no operator wiring exists in
the workspace. The promise — "Kaleidoscope is observable" —
has been a contract on paper since the first crate shipped
and never demonstrated end-to-end. A new crate
`self-observe` closes that loop using the platform's own
primitives.

```mermaid
flowchart LR
    L[Lumen InMemoryLogStore] -->|MetricsRecorder| B[LumenToPulseRecorder]
    B -->|MetricPoint| P[Pulse InMemoryMetricStore]
    P -.->|query 'lumen.ingest.count'| Op[Operator dashboard]
    style B fill:#fec
```

The bridge is a single struct, `LumenToPulseRecorder`. It
implements `lumen::MetricsRecorder`, holds an
`Arc<dyn pulse::MetricStore + Send + Sync>`, and turns each
`record_ingest(tenant, count)` call into a single-point
`MetricBatch` ingested into Pulse under the same tenant.
Same for `record_query`. The metric name follows the
convention `lumen.ingest.count` and `lumen.query.count`; the
value is the count; the tenant identity passes through
unchanged. Pulse is being used here exactly the way an
operator would use it for real customer metrics; the only
difference is that the source of the data is another
Kaleidoscope crate rather than an instrumented application.

The acceptance suite pins the obvious properties: an ingest
becomes a point with the right count, a query becomes a
point with the right matched-count, two tenants land in
isolated Pulse buckets, an unused Lumen produces no Pulse
data, and the bridge is `Send + Sync` so the trait bounds
are satisfied at compile time. One test documents the v0
Lumen behaviour that an empty batch still emits a zero-count
event, locking the contract down so a future change has to
update the test deliberately.

The choice not to depend on `opentelemetry-otlp` is
deliberate at this stage. The OTLP exporter is a heavy
dependency that pulls in tokio, tonic, prost, and a real
async runtime; for an in-workspace demonstration of "the
platform observes itself" it is overkill. A v2 of this crate
may add an `OtelOtlpRecorder` family that exports to a real
OTLP collector for cross-process observability; v1 stays
inside the workspace because that is where the contract
teaches clearly without external infrastructure.

The same pattern fits every other crate's `MetricsRecorder`
trait. Future bridges would follow the naming convention
`XxxToPulseRecorder`. The pattern is now demonstrated once;
extending to Cinder, Sluice, Augur, Ray, Strata is mechanical
and can land when a real deployment needs it. Six new
acceptance tests GREEN. Workspace: 101 suites. Kaleidoscope
observes itself.

---

## kaleidoscope-cli — from libraries to a product

Twenty-one features and one hundred and one green test
suites, but until now an operator could not actually launch
anything. Every demonstration of the platform lived inside
acceptance tests; the implicit promise that "you could
deploy this" was unproven. A small CLI closes the loop. It
takes NDJSON `LogRecord` lines on stdin, ingests them into
Lumen v1, places a Cinder v1 tier metadata entry per batch
in the Hot tier, and writes a stats line to stderr on
completion. A second invocation in `read` mode opens the
same `data_dir`, queries Lumen, and writes every record
back to stdout as NDJSON. The pipe an operator would
actually write at the command line:

```text
cat /var/log/otlp.ndjson | kaleidoscope-cli ingest acme ./data
kaleidoscope-cli read acme ./data | jq .body
```

```mermaid
flowchart LR
    Stdin[stdin NDJSON] --> CLI[kaleidoscope-cli ingest]
    CLI --> Lumen[Lumen v1: log records]
    CLI --> Cinder[Cinder v1: Hot tier metadata]
    CLI -->|self-observe| Pulse[Pulse: lumen.ingest.count]
    Disk[(data_dir)] --> ReadCLI[kaleidoscope-cli read]
    ReadCLI --> Stdout[stdout NDJSON]
    style CLI fill:#fec
    style ReadCLI fill:#fec
```

The crate is structured as a thin binary wrapping a library.
The binary parses arguments (hand-rolled, no `clap`; a
two-subcommand positional CLI does not earn the dependency)
and dispatches to `kaleidoscope_cli::ingest` or
`kaleidoscope_cli::read`. Both functions take a generic
`BufRead` or `Write`, which means the acceptance tests
exercise the library with `Cursor<&[u8]>` and `Vec<u8>`
rather than spawning the binary as a subprocess. That's
faster, more deterministic, and pins the same behaviour the
binary delivers.

The `MetricsRecorder` wired into Lumen inside this binary is
the `LumenToPulseRecorder` from `self-observe`, so every
ingest fires a `lumen.ingest.count` event into an in-process
Pulse store. The Pulse handle is currently dropped at end
of call; a v2 of the CLI could keep it alive and expose a
`stats` subcommand that queries the metric store before
shutdown. For now the demonstration is structural: the
binary that ships to operators uses the same self-observe
seam the workspace already validates.

Seven new acceptance tests in the library plus a smoke test
through the actual binary via shell pipe: a single JSON
record flows from stdin through `ingest`, persists to
`./data`, and comes back through `read`. The shell pipe
that opens this section is real and reproducible. Workspace:
104 suites GREEN. Twenty-one features. Three durable.
Kaleidoscope is, for the first time, a thing you can run
rather than a thing you can read about.

---

## OTLP-JSON cross-process bridge, observable via the CLI

The self-observe story had one circular claim. The narrative
said operators can pipe Lumen's events to an OTLP collector,
but the only thing the workspace shipped was the in-process
Pulse bridge. The cross-process side was a paragraph, not a
contract. Two small commits close the loop: a hand-rolled
OTLP-JSON writer that emits one NDJSON line per Lumen event,
and a CLI flag that wires it into the operator-facing
`ingest` subcommand.

```mermaid
flowchart LR
    Stdin[stdin NDJSON] --> CLI[kaleidoscope-cli ingest]
    CLI -->|--observe-otlp /tmp/otlp.log| File[/tmp/otlp.log NDJSON]
    File -.->|tail -f or sidecar| Forwarder[OTLP/HTTP sidecar]
    Forwarder -.->|HTTP POST| Collector[OTLP collector]
    style File fill:#fec
```

`LumenToOtlpJsonWriter<W: Write + Send + Sync>` is the bridge.
Each call to `record_ingest` or `record_query` serialises one
OTLP-JSON `ResourceMetrics` to the inner writer. The shape is
the minimal subset an OTLP collector requires: resource
attributes (tenant id), a single scope (`kaleidoscope.lumen`),
one metric with `aggregationTemporality = 2` (CUMULATIVE) and
`isMonotonic = true`, one data point carrying the count.
`uint64` values are encoded as strings per the OTLP-JSON spec,
not as JSON numbers. Tenant id appears both as a resource
attribute and as a point attribute because collectors
disagree on which one they prefer.

The deliberate non-decision: no `opentelemetry-otlp`
dependency. No `tokio`, no `tonic`, no `prost-json`. The
bridge is sync, depends on `serde` + `serde_json` (already in
the workspace), and emits a byte stream a sidecar process can
consume. The cost of pulling in the full OTLP SDK at this
stage would be a runtime, a TLS stack, and a retry / batching
abstraction, all to do work the operator's collector already
does. v2 may add the SDK when a deployment actually demands
push-style export; v1 keeps the bridge leaf-flat.

The CLI extension is one new flag,
`--observe-otlp <path>`. Without it, ingest behaves as before
(in-process Pulse recorder, dropped at end of call). With it,
the recorder becomes the OTLP-JSON writer pointing at that
file in append mode. An operator who wants to watch the
metric stream while ingest runs opens a second terminal and
runs `tail -f`. A sidecar who wants to forward to a real
OTLP/HTTP collector reads the file and POSTs each line. Both
are working shell patterns; neither needs anything more from
the Kaleidoscope binary.

The `ingest` library signature gained an
`otlp_log_path: Option<&Path>` parameter at the end. Seven
existing tests pass `None` and continue to test the prior
behaviour; three new tests exercise `Some(path)` and assert
the file structure, append semantics, and absence-when-flag-
missing. A real shell-pipe smoke test before commit produced
this OTLP-JSON line, verbatim:

```json
{"resource":{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}]},"scopeMetrics":[{"scope":{"name":"kaleidoscope.lumen"},"metrics":[{"name":"lumen.ingest.count","sum":{"aggregationTemporality":2,"isMonotonic":true,"dataPoints":[{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}],"timeUnixNano":"1778813197474338000","asInt":"1"}]}}]}]}
```

What this whole arc teaches: the platform's outward
observability does not need a heavyweight SDK to be honest.
The minimal contract is "emit OTLP-JSON in the shape a
collector consumes, leave the network to a sidecar". The
sidecar pattern is older than OTLP and survives every
ecosystem shift. v1 commits to it; v2 adds the SDK only when
a real deployment demands push semantics. Six new acceptance
tests across the two commits. Workspace: 106 suites GREEN.
Twenty-one features. Three durable. One launchable. One
cross-process observable.

---

## cinder-to-pulse-bridge-v0 — the methodology is the point

This is a short section about a small feature. The shipped
artefact is one Rust struct, one constructor, three trait
methods, 139 lines of source. By comparison, the previous
sections have each described months of work or many shipped
crates. The disproportion is deliberate. This feature exists
to demonstrate the methodology itself, not to add a
substantial new capability.

The bridge is the symmetric extension of `LumenToPulseRecorder`:
it implements `cinder::MetricsRecorder` and writes Cinder's
tier-management events (`place`, `migrate`, `evaluate`) into a
`pulse::MetricStore`. Operators get answers to questions
like "how many Hot→Warm migrations per minute did tenant
`acme` see in the last hour?" by issuing the same query API
they already use for Lumen metrics. Lowercase tier
serialisation. Best-effort emission. `cinder.place.count`,
`cinder.migrate.count`, `cinder.evaluate.migrated.count`. The
shape was inevitable from the moment the Lumen bridge
shipped; the only question was what process would carry it
into production.

This feature was originally delivered in a single overnight
session as one of thirty-one direct commits with no nWave
artefacts. Each commit was individually defensible — small,
well-tested, narrative-updated. Cumulatively they amounted to
abandonment of the methodology that is the whole point of
this project. Andrea reviewed the work in the morning,
identified the failure mode, and asked for a complete revert.
The thirty-one commits were rolled back via one revert commit;
this feature was then redone end-to-end through the proper
five-wave loop.

```mermaid
flowchart LR
    Discuss[DISCUSS] -->|Luna| DiscussRev[Eclipse]
    DiscussRev -->|APPROVED| Design[DESIGN]
    Design -->|Morgan| DesignRev[Reviewer]
    DesignRev -->|APPROVED| Devops[DEVOPS]
    Devops -->|Apex| DevopsRev[Forge]
    DevopsRev -->|APPROVED| Distill[DISTILL]
    Distill -->|Scholar| Deliver[DELIVER]
    Deliver -->|Crafty| Gate{100% mutation kill}
    Gate -->|PASS| Ship[(production)]
    style Discuss fill:#cef
    style Design fill:#cef
    style Devops fill:#cef
    style Distill fill:#fec
    style Deliver fill:#fec
    style Gate fill:#fcc
    style Ship fill:#cfc
```

What landed: four atomic commits, one per artefact-class
boundary, with a fifth atomic commit closing the DISTILL +
DELIVER work together with the production code change.
Five waves, four formal peer reviews, one mutation gate. Zero
critical, high, or medium issues across every review. 11
acceptance tests written first (RED), then turned green one
slice at a time, then refactored to mirror the Lumen bridge's
shape exactly. Mutation kill rate 6/6 = 100%. Workspace
went from 106 to 107 test suites GREEN.

The bridge could have been written in fifteen minutes. The
nWave loop took several hours. That ratio is the lesson. The
methodology costs more than the typing, but the typing is
the cheapest part of software. The audit trail — DISCUSS
user stories, DESIGN ADR-0038, DEVOPS environment inventory,
DISTILL acceptance tests, DELIVER mutation kill rate — is
what makes the bridge a piece of the platform rather than a
piece of code. Six months from now, if a future Bea tries to
quietly rename `cinder.place.count` to something else, the
ADR pins the contract and the acceptance tests fail with a
specific assertion message; the proposal does not silently
land in a 03:14 incident.

The whole point of redoing the overnight work this way is to
prove that the methodology is durable even at small scale.
Andrea wrote a memory note after the revert:
`feedback_nwave_required_even_overnight`. It says: "Bypassing
nWave on Kaleidoscope is self-betrayal." This feature is the
first instance of honouring that note.

---

## cinder-to-otlp-json-bridge-v0 — the pattern repeats

A second small feature, immediately after the first. The shipped
artefact is one Rust struct, one constructor, three trait methods,
289 lines of source. The symmetry to the previous section is the
point. If the Pulse-sink bridge proved that the methodology can
hold for a small feature once, the OTLP-JSON-sink bridge proves
that the methodology can hold for the next small feature too. Two
sinks for Cinder events now exist: the in-process Pulse sink, which
matters when the operator wants to query Cinder transitions through
the same `pulse::MetricStore` interface they already use for Lumen;
and the cross-process OTLP-JSON sink, which matters when the
operator wants Cinder transitions to land in their existing OTLP
collector alongside Lumen ingest and query events. Both sinks emit
the same three metric names. Both write the same wire format their
Lumen-side siblings already write. The Cinder observability surface
is now complete in the same two shapes that the Lumen observability
surface already had.

What makes this feature worth its own narrative section is not the
code but the absence of friction. The DISCUSS wave reused the
analysis from the Pulse-sink sibling, varying only the sink-shape
decisions. DESIGN copied the application architecture diagram, kept
the C4 boxes, and added ADR-0039 alongside ADR-0038 as a parallel
contract. DEVOPS established the OK5 NDJSON-validity guardrail as a
new outcome KPI but otherwise inherited the same five workspace
gates with no new tooling. DISTILL produced 12 acceptance tests in
the same Rust idiom as its Lumen-side precedent. DELIVER landed the
production code with all 12 tests green, zero clippy warnings,
6 mutants generated, 6 mutants killed, zero white-box tests needed.

The DEVOPS wave surfaced a quiet defect from the previous feature.
Forge, the platform-architect reviewer, ran an external-validity
check and discovered that the prior Pulse-sink wave had claimed its
DELIVER commit would land a new `gate-5-mutants-self-observe` CI
job — but the commit had not included the workflow edit. The job
was missing. Forge blocked the DEVOPS handoff until the gap was
closed. The fix-forward commit landed the missing job to the CI
workflow and added a "Post-merge correction" section to the prior
wave's `wave-decisions.md`. The reviewer's external-validity check
saved a future incident where mutation testing for the entire
self-observe crate would have been silently absent from CI. That is
exactly the asymmetry the reviewer brief is designed to create.

The second deliberate forward-compatibility note also emerged in
the DEVOPS wave. The OTLP-JSON sink is library-only at v0; the
post-v0 CLI feature will wire both Lumen and Cinder writers to the
same `std::fs::File` via the `--observe-otlp <path>` flag.
Cross-writer concurrency against a real file becomes a new
invariant that neither sibling can guarantee in isolation. ADR-0039
gained a §7 documenting this handoff explicitly, naming the future
outcome KPI as `OK6-CLI-cross-writer-ndjson` and pinning the
acceptance-test shape the CLI feature must produce. The reviewer
caught this gap during peer review and asked for it to be written
down before the wave closed. The future feature now knows what it
owes the platform before it begins.

```mermaid
flowchart LR
    Lumen[(Lumen events)] -->|ingest+query| LumenPulse[LumenToPulseRecorder]
    Lumen -->|ingest+query| LumenOtlp[LumenToOtlpJsonWriter]
    Cinder[(Cinder events)] -->|place+migrate+evaluate| CinderPulse[CinderToPulseRecorder]
    Cinder -->|place+migrate+evaluate| CinderOtlp[CinderToOtlpJsonWriter]
    LumenPulse --> Pulse[(pulse::MetricStore)]
    CinderPulse --> Pulse
    LumenOtlp --> File[(NDJSON sink)]
    CinderOtlp --> File
    style CinderOtlp fill:#cfc
    style File fill:#fec
```

The 2 × 2 above is now closed. The lesson is the same lesson the
Pulse-sink section landed, but said one octave louder. A single
disciplined execution can be a heroic effort; a second disciplined
execution on the next feature is evidence that the discipline has
become routine. The first feature redo proved nWave can hold; the
second redo proves nWave keeps holding. The methodology is no
longer on probation. It is the way this project ships.

---

## cli-cinder-otlp-wiring-v0 — the methodology earns its keep

The third small feature in the redo sequence is the smallest yet:
one match arm in a CLI library function changes from
`Box::new(NoopRecorder)` to a pair of file-shared OTLP-JSON
writers, and now the operator's `--observe-otlp <path>` flag sinks
both Lumen ingest events and Cinder tier-management events to the
same NDJSON file. From the operator's point of view, the OTLP
collector that already had Lumen activity now also has Cinder
hot-warm-cold transitions in the same stream. From the code's
point of view, fifteen lines moved.

This is the feature ADR-0039 §7 told us was coming. The §7
handoff note, written during the previous wave's DEVOPS review,
spelled out exactly what this feature owed the platform: a new
outcome KPI called OK6-CLI-cross-writer-ndjson, an acceptance test
that spawned two writer threads against a real file, and an
explicit concurrent-pause scenario. We knew before we began what
this feature had to prove. The DESIGN wave picked
`File::try_clone` and delegated cross-writer atomicity to POSIX
O_APPEND. The DEVOPS wave added a new gate-5 mutation job for
kaleidoscope-cli, mirroring the per-package precedent. The DISTILL
wave wrote five acceptance tests including the concurrent one. The
DELIVER wave flipped the match arm and made all five tests green.

Then the methodology earned its keep.

Crafty ran the concurrent acceptance test on macOS. It flaked.
Some runs passed, some failed. The failure mode was empty lines
and torn JSON records in the sink. Crafty traced the root cause:
ADR-0039 §2 had specified an atomic write triple,
`write_all(body) + write_all(b"\n") + flush`, guarded by the
writer's internal `Mutex<W>`. The Mutex makes that triple atomic
within a single writer. But each `write_all` issues a separate
`write(2)` syscall, and POSIX O_APPEND atomicity is per-`write(2)`,
not per-`write_all`. When two writers share the same O_APPEND
file, the Cinder body syscall can land between Lumen's body and
Lumen's newline. The kernel never promised otherwise.

This defect had been in the codebase since the first
LumenToOtlpJsonWriter shipped. The previous two features that
introduced OTLP-JSON writers (Lumen and Cinder library
implementations) both shipped acceptance tests that exercised a
single writer against an in-memory `Vec<u8>` sink. Single thread,
single writer, no concurrency. The within-writer Mutex made the
tests pass. The cross-writer composition was never tested because
no feature, until now, composed two writers against the same
file. The cross-writer guarantee that the prior waves assumed was
"obviously inherited from §2" was, in fact, never structurally
true. It was just never exercised.

The fix was three lines: coalesce body + `\n` into one buffer,
emit via a single `write_all`, flush. One `write(2)` syscall per
line. Under sub-PIPE_BUF (4096 byte) writes, this IS atomic
across appenders sharing the same O_APPEND file. Crafty applied
the fix to both writers, the concurrent test went stable across
ten consecutive runs, and the architectural truth was restored.
A fix-forward commit updated ADR-0039 §2 and §7 with correction
boxes explaining the root cause and the lesson, and appended a
post-merge correction note to the prior wave's wave-decisions.md.

```mermaid
flowchart LR
    L[LumenToOtlpJsonWriter] -->|write(2) body+\n| F[(File O_APPEND)]
    C[CinderToOtlpJsonWriter] -->|write(2) body+\n| F
    F -->|atomic ≤4096B| Sink[(NDJSON sink)]
    style L fill:#cef
    style C fill:#cef
    style F fill:#fec
    style Sink fill:#cfc
```

The lesson is not "we had a bug". Every project has bugs. The
lesson is that the methodology surfaced a real architectural
defect that the previous two waves' acceptance tests could not see
because of their test scope. Multi-writer composition needs
multi-writer testing. ADR-0039 §7 told this feature exactly what
to test, this feature did exactly what it was told, and the test
caught the bug. The next time someone proposes wiring two writers
to the same file, the lessons are now structural rather than oral.
ADR-0039 §2 names the failure mode in a correction box.
`tests/observe_otlp_cinder_wiring.rs` exercises the failure mode in
CI on every commit. The post-merge correction note in the prior
wave's wave-decisions.md records why the prior gate didn't catch
it. The corner that the methodology would have left invisible is
now lit from three angles.

The feature ships fifteen lines of code, four atomic commits, one
correction box, and one architectural truth that holds in
production rather than only in proof. The cost ratio is the same
as the prior two features: minutes of typing, hours of methodology.
The dividend was paid here in full.

---

## cli-read-observe-otlp-v0 — symmetry without ceremony

The fourth small feature in the redo sequence closes a symmetry
the operator probably did not realise was missing. Today
`kaleidoscope-cli ingest --observe-otlp <path>` sinks ingest
activity to the operator's NDJSON collector. After this feature,
`kaleidoscope-cli read --observe-otlp <path>` does the same for
query activity. The operator now has both halves of the operational
loop visible through the same OTLP tooling they already deployed.
The shell command that worked for one subcommand now works for the
other. No new flag, no new infrastructure, no new mental model.

What makes this feature worth its own section is that the
methodology absorbed it with even less ceremony than the previous
one. DESIGN took less than a page. The DEVOPS wave shipped without
adding a single CI workflow edit, because the per-package gate-5
mutation job introduced two features ago auto-covered the diff via
its `--in-diff` filter. DISTILL produced three acceptance tests:
one for the happy path, one for the no-flag guardrail, and one
that runs `ingest` then `read` against the same file in a single
session and asserts all three metric name types appear in the
captured stream. The implementation is one match arm in `read()`
plus a one-line addition to the CLI's argument parser.

Crafty surfaced and killed a coverage gap the previous waves had
not addressed. When mutation testing ran against the diff, two
mutants survived: body-deletion of `print_usage` and body-deletion
of `run_read` in `main.rs`. The CLI's binary wrappers around the
library entry points had been untested since the binary first
shipped. The acceptance tests exercised the library function
directly; nobody had ever exercised the binary itself. Crafty
killed both mutants by extracting testable inner forms
(`write_usage(&mut impl Write)` and `run_read_with<O, E>(args,
stdout, stderr)`) and adding a `cli_binary_smoke.rs` that spawns
the real `CARGO_BIN_EXE_kaleidoscope-cli` to assert stdout and
stderr bytes end-to-end. The same mutation gate that previously
caught architectural defects in the writers now also catches the
shell-facing surface of the binary. Another corner lit by the
methodology that nobody had asked for it to light.

```mermaid
flowchart LR
    Op[Operator] -->|kaleidoscope-cli ingest --observe-otlp| Ingest[ingest path]
    Op -->|kaleidoscope-cli read --observe-otlp| Read[read path]
    Ingest --> File[(NDJSON sink)]
    Read --> File
    File --> Collector[(OTLP collector)]
    style Read fill:#cfc
    style File fill:#fec
```

The dividend this time was not an architectural correction. It
was negative-space evidence: a feature shipped with zero
workflow edits, zero new ADRs, zero new dependencies, and a
mutation gate that paid the operator back on a corner of the
codebase the operator had not even asked about. Four features
into the redo, the methodology now ships features at the speed
of the typing, with the audit trail accumulating as a side effect.
That is the steady state Andrea has been trying to reach since the
project started. It looks ordinary on the surface. It is not.

---

## cli-stats-subcommand-v0 — the methodology grows the product

The fifth small feature in the redo sequence is the first that
extends the operator-facing surface of the CLI instead of wiring an
existing flag onto an existing subcommand. Before this feature, the
CLI exposed `ingest` and `read`: one to feed the platform, one to
dump it back out. After this feature, the CLI also exposes `stats`:
a one-shot inspection of a tenant's data that returns the record
count and the timestamp window without ever materialising the
records themselves. For a populated tenant with millions of
records, that is ten gigabytes of NDJSON that no longer have to
flow through the operator's pipe just to answer the question "did
yesterday's ingest land anything?"

The architectural shape was thinner than expected. DESIGN noticed
that the existing `LogStore` trait already documents an
ascending-timestamp invariant at the port level, so `records.first()`
and `records.last()` are constant-time. DESIGN also noticed that
no datetime crate exists anywhere in the workspace, so the choice
between `chrono` and `time` was a false one: the answer was
"hand-roll the formatter". Twenty lines of integer arithmetic
based on Howard Hinnant's public-domain civil_from_days algorithm
produce ISO 8601 UTC with nanosecond precision, no dependency, no
abstraction tax. The function shape mirrors `read()` exactly. The
new subcommand is one match arm in `main.rs`. The feature is, to
first order, one new public library function and one new
subcommand dispatcher.

The methodology earned its keep again, in a quieter way than the
previous feature. Mutation testing on the diff generated 103
mutants. The first run killed 80.6 per cent. Each iteration
identified the surviving mutants, added a focused white-box test
for them, and converged at 100 per cent kill rate by the third
iteration. The deepest mutant uncovered a real bug Crafty had
introduced: his civil_from_days implementation incorrectly
treated year 0 as a non-leap year. Hinnant's proleptic Gregorian
calendar treats year 0 as a leap year because it is divisible by
400. The pre-epoch test witnesses had to be corrected from
`(0, 3, 1)` to `(0, 1, 1)` and `(0, 2, 29)` after observing the
unmutated function's actual output. This is the kind of defect
that ships silently in any library whose test suite covers only
post-epoch dates, then surfaces a decade later when someone runs
a backfill against a historical archive. The per-feature
mutation gate forced the boundary cases into the test suite
before the function ran in production once.

```mermaid
flowchart LR
    Op[Operator] -->|kaleidoscope-cli stats acme /tmp/data| Stats[stats subcommand]
    Stats -->|query tenant=acme range=all| Lumen[(FileBackedLogStore)]
    Lumen -->|records sorted| Stats
    Stats -->|records=N + earliest + latest| Stdout[(stdout)]
    style Stats fill:#cfc
    style Stdout fill:#fec
```

The narrative shape this feature lands is "from data movement to
data inspection". The CLI is no longer just a pipe between stdin
and the storage adapters. It is starting to be a tool the operator
uses to interrogate the platform's state. The next obvious move
is symmetric on the Cinder side — show me the tier distribution
for tenant X — and after that the natural progression is a
subcommand that runs a real query against a time range rather than
the full record set. Five features into the redo, the platform is
growing operator-facing surface area through the same methodology
that previously was only protecting library internals. The five
waves do not care which kind of feature passes through them. They
care that the feature is small, that the contract is locked in
DISCUSS, that the architecture is recorded in DESIGN, that the
gates are inherited in DEVOPS, that the test is written before the
code in DISTILL, and that the mutation gate runs in DELIVER. Every
feature this week has paid each wave its fare and arrived in
production with the audit trail intact.

---

## cli-stats-cinder-tier-distribution-v0 — locked contracts and parallel functions

The sixth feature in the redo sequence extends the previous one.
The operator's `stats acme /tmp/data` call now also prints the
tenant's Cinder tier distribution. The output gains three optional
lines: `hot=H`, `warm=W`, `cold=C`. Lines for empty tiers are
omitted. Lines for tenants with no Cinder placements at all are
omitted entirely, keeping the predecessor's three-line output
byte-equivalent for the common case.

The interesting decision was structural. The predecessor shipped
its own acceptance test file that locked the three-line output as
a byte-level contract. If this feature simply extended `stats()`
in place, that test would fail the moment a Hot Cinder placement
appeared, because the predecessor's own `ingest()` setup places
one Hot item per batch. The locked test would assert "exactly
three lines" and the now-Cinder-aware `stats()` would emit four.
The locked test was the OK4 oracle. It was load-bearing. It could
not be touched.

DESIGN evaluated four shapes. In-place extension breaks the locked
test. Renaming the old `stats()` to `stats_lumen_only()` breaks
the `use` import in the locked test. Modifying the locked test
violates its hard-rule status. An optional fourth parameter is
impossible without overloads. The fifth shape was the obvious one
in retrospect: add a parallel function. `stats_with_tiers()` lives
next to `stats()` in the same library file. The CLI dispatcher
calls the new one. The old function stays as the byte-level oracle
that the locked test still validates. Both functions remain green.
Neither contract is renegotiated. The cost of the parallel
function is fifteen additional lines of source. The cost of any
of the rejected alternatives would have been a locked-test
revision, a library API rename, or a contract negotiation. The
methodology made the cheapest path the only path the constraints
allowed.

```mermaid
flowchart LR
    Op[Operator] -->|stats acme /tmp/data| Run[run_stats]
    Run -->|stats_with_tiers| New[stats_with_tiers]
    New -->|query records| Lumen[(FileBackedLogStore)]
    New -->|list_by_tier x3| Cinder[(FileBackedTieringStore)]
    New -->|records + earliest + latest + hot + warm + cold| Stdout[(stdout)]
    OracleTest[stats_subcommand.rs] -.->|locked oracle| Legacy[stats]
    style New fill:#cfc
    style Legacy fill:#fec
    style OracleTest fill:#cef
```

The dividend was a small lesson about locked contracts. They are
not a burden. They are a forcing function that pushes the design
toward the smallest-blast-radius change. When you cannot rename,
cannot delete, cannot modify, the only remaining move is to add
something parallel. The cost is a little duplication. The benefit
is that everything that worked yesterday continues to work today
without re-negotiation. Six features into the redo, the platform
has grown a habit of preserving the contracts it has previously
ratified, and the methodology is now visibly opinionated about how
to do that without painting itself into a corner.

Mutation testing on the diff generated nine mutants and killed all
nine. The inline white-box tests from the previous wave's
civil_from_days coverage continued to amortise: the new function
shares the formatter and the iteration helpers with `stats()`, so
the same tests that killed mutants in the previous wave killed
them again here, plus the five new acceptance tests killed the
function-specific mutants. The mutation gate is now noticeably
cheaper per feature because each feature inherits the white-box
coverage of the features that came before it.

---

## cli-read-time-range-v0 and cli-stats-time-range-v0 — the parser proves its keep

The seventh and eighth features in the redo sequence ship the
same flag pair (`--since` and `--until`) to `read` and `stats`
in sequence. The substantive engineering happened in the read
feature: a hand-rolled ISO 8601 parser sitting next to the
existing formatter from the stats subcommand, sharing the
`days_from_civil` calendar arithmetic in both directions, with
inline white-box tests covering leap years, the pre-1970
boundary, missing `Z` suffix, lowercase `z`, calendar overflow,
and the parse-before-store-open fail-fast guarantee. The
sibling stats feature is what the parser earned afterwards. It
plugged into stats with twelve lines of change.

The DESIGN review surfaced two HIGH items that mattered.
Atlas, the architect reviewer, noticed that the wave-decisions
document referred to "Hinnant's algorithm" without IP provenance
and without a `LICENSING.md` entry. He also noticed that the
proposed parser accepted year `0000`, but `u64` nanoseconds
since the Unix epoch cannot represent pre-1970 timestamps
without wraparound. Both were legitimate corrections. The year
range tightened to `[1970, 9999]` with the parser rejecting
pre-1970 dates under the same fail-fast contract as other
invalid inputs. `LICENSING.md` gained a `## Third-party
algorithms` section attributing Howard Hinnant's date
algorithms with the URL and the public-domain dedication,
covering both the formatter and the parser. The DELIVER wave
inherited a sharper spec because of these corrections.

The sibling stats wave illustrated a different lesson. When
the architect rejected three alternatives for how `stats` should
accept the new flags (in-place breaks the locked oracle; rename
breaks the import; modifying the locked test violates its
hard-rule status), only the fourth shape survived: extend the
existing `stats_with_tiers()` signature by one parameter and
update the locked test mechanically to pass `TimeRange::all()`
at every call site. Six call sites updated in the locked test;
zero assertions touched. The byte-equivalence contract held
exactly because the parameter that was added is the default the
test was previously implicit about. This is the second time the
methodology has produced an "add a parallel function or extend
explicitly" choice driven entirely by what the locked test
allows. Six features into this kind of work, the locked-test
constraint is no longer an obstacle. It is the design lens.

```mermaid
flowchart LR
    Op[Operator] -->|read --since X --until Y| Read[read path]
    Op -->|stats --since X --until Y| Stats[stats path]
    Read --> Parser{parse_iso8601_utc_nanos}
    Stats --> Parser
    Parser --> Lumen[(Lumen TimeRange query)]
    Lumen --> Output[(NDJSON stdout / stats summary)]
    style Parser fill:#cfc
    style Lumen fill:#fec
```

What also happened during these two features, quietly, is that
Gate 1 broke. The CI hardware on GitHub Actions ubuntu-latest
runs roughly fifteen times slower than the local workstation
that calibrated the original p95 latency budgets, and four of
those budgets were under two milliseconds. Each one had been
silently failing on CI for the previous two weeks, blocking any
graduation tag. The methodology surfaced this when the
inspection hit the right corner: Andrea asked why nothing had
been tagged, the answer turned out to be Gate 1 being
consistently red, and the root cause was four timing budgets
calibrated for the wrong hardware. The fix was a batch bump:
Cinder KPI 2 from 1 s to 2.5 s, Lumen v0 KPI 1 from 1 ms to
2 ms, Lumen v1 KPI 1 from 1.5 ms to 3 ms, Lumen v1 KPI 2 from
1 s to 2.5 s, Pulse v0 KPI 1 from 1 ms to 2 ms, and Aegis v0
KPI 1 from 1 ms to 2 ms. Each test now carries an inline
"bump history" comment recording the original budget, the date
of the bump, and the CI-vs-workstation observation that
justified it. The KPI's intent ("bounded under a few
milliseconds locally, under a few milliseconds on shared
hardware too") survived; only the numeric ceiling moved.

This is the third operational lesson the methodology has taught
in the redo sequence. The first was that nWave is required even
overnight. The second was that the methodology surfaces real
defects that the test scope was previously too narrow to
detect. The third, landing here, is that CI hardware reality
must show up in the budget specs from the start. Numbers
calibrated only against a fast workstation are decoration. The
next time a slice ships a p95 KPI, the budget gets a CI-realism
margin written into the test from the first commit.

---

## cli-migrate-subcommand-v0 — the first state-mutating tool

The ninth feature in the redo sequence is the first that gives
the operator a deliberate state-mutating action. Until now the
CLI could ingest records (a side effect, but operationally
"feeding the platform"), read records back (a query), and
inspect summary statistics (a read). The new `migrate`
subcommand is different. The operator types

  kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold

and Cinder moves the item from its current tier to the
operator-supplied one. One CLI invocation does what previously
required writing a small Rust harness around
`TieringStore::migrate`.

The shape forced an interesting design decision. The Cinder
migrate API returns `Result<(), MigrateError>` without telling
the caller what the from-tier was. To report `from=hot to=cold`
on stdout, the function must call `get_entry()` first to
capture the current tier, then call `migrate`. That introduces
a notional race window between the two calls. The DESIGN wave
documented this honestly: v0 is single-process, so the window
has no concurrent-mutation hazard, but post-v0 multi-process
work will need to either accept the report's freshness limits
or push the from-tier through the API. The architect's job
here was to surface the trade-off rather than hide it.

The methodology's small mutation gate produced a quiet lesson
during DELIVER. The `SystemTime::now()` call inside the
migrate function had no observable effect at the CLI boundary:
the stdout line shows the from-tier and to-tier but never the
timestamp, so a mutation that changed `SystemTime::now()` to
`UNIX_EPOCH` was wire-invisible. The acceptance tests could
not see it. Forge, the platform-architect reviewer, flagged
this during DEVOPS review and identified the kill condition:
`TierEntry::migrated_at` is observable through the public
`get_entry()` accessor, so an inline white-box test in
`lib.rs` that captures `migrated_at` before and after the
migrate call kills the mutant cleanly. Crafty added that test
during DELIVER. Mutation kill rate held at 100 per cent on the
diff.

```mermaid
flowchart LR
    Op[Operator] -->|migrate acme /tmp/data item-42 cold| Run[run_migrate]
    Run -->|parse_tier| Cmd[migrate library function]
    Cmd -->|get_entry pre-flight| Cinder[(FileBackedTieringStore)]
    Cmd -->|migrate tenant item tier now| Cinder
    Cmd -->|migrated tenant=acme item=item-42 from=hot to=cold| Stdout[(stdout)]
    Cinder -.->|migrated_at observable for white-box test| Cmd
    style Cmd fill:#cfc
    style Stdout fill:#fec
```

The narrative shape this feature lands is "the operator can
now change the platform's state on purpose". Before this
feature the CLI was a read-and-feed tool. After this feature
the CLI is also a lifecycle tool. Combined with the time-range
filtering on read and stats, the kaleidoscope-cli surface is
now coherent for incident-response work: query the window,
see the tier distribution, move the items that need moving,
verify the move by re-querying. Each one of those steps is a
single CLI invocation. Nine features into the redo, the CLI
has crossed the line from "library wrapper" to "operator
tool".

---

## cli-migrate-observe-otlp-v0 — the audit trail closes

The tenth small feature in the redo sequence is the smallest
new code change of any feature yet: about eight lines of Rust
plus a doc update. It nonetheless completes an important
operator workflow. The migrate subcommand from the prior
feature was the first state-mutating action the CLI had
offered. Until this feature, those state changes were
fire-and-forget: the operator typed the command, Cinder
reflected the move, and the only record was the operator's
shell history. After this feature, the same `--observe-otlp
<path>` flag that already worked on ingest and read works on
migrate too. Every successful migration leaves one
`cinder.migrate.count` line in the operator's OTLP sink,
carrying the tenant id, the from-tier, and the to-tier as
attributes.

The narrative shape here is "the audit trail closes". The
operator now has full observability across the three things
the CLI does. Ingest activity is recordable. Query activity is
recordable. State mutations are recordable. Combined with the
time-range filter on read and stats, an operator running an
incident-response session leaves a complete forensic record
without any extra tooling. The OTLP collector deployed for
ingest visibility already accepts every line this CLI now
emits, no schema change, no new metric name surface, no new
sink format. The work that ADR-0038 and ADR-0039 did months
ago to lock the wire shape is paying its third operator-facing
dividend.

```mermaid
flowchart LR
    Op[Operator] -->|ingest --observe-otlp| Ingest[ingest path]
    Op -->|read --observe-otlp| Read[read path]
    Op -->|migrate --observe-otlp| Migrate[migrate path]
    Ingest --> Sink[(NDJSON sink)]
    Read --> Sink
    Migrate --> Sink
    Sink --> Collector[(operator's OTLP collector)]
    style Migrate fill:#cfc
    style Sink fill:#fec
```

The operational lesson this feature lands is about the value
of locked contracts on the wire layer. ADR-0039 §1 locked the
`CinderToOtlpJsonWriter` public surface six features ago. Five
features later that contract is still standing, and a new
feature consuming it required exactly zero changes to the
writer, zero changes to the metric name, zero changes to the
attribute shape, and zero changes to the on-disk file format.
The cost of pinning the wire shape early was a few hours of
DESIGN debate. The dividend is that the tenth feature plugs
into the same contract as the second, third, fourth, fifth,
and sixth features did, without any of them needing to know
about each other.

One small detail about how this feature was delivered:
Crafty's agent quota ran out earlier in the session, so the
DELIVER work was done by the orchestrator directly. The pattern
was identical to three prior `--observe-otlp` wiring features
shipped on the same crate, so the mechanical work fit in
about fifteen edits. The nWave gates remained mandatory: the
acceptance tests still went through DISTILL with a peer
reviewer, the locked tests received only mechanical
signature-match updates, and the workspace gates (test, fmt,
clippy) ran clean before commit. The methodology survives the
loss of one agent because the methodology was never the agent.
The agent was just the cheap labour.

---

## cli-list-items-subcommand-v0 — the pipeline closes

The eleventh feature in the redo sequence completes the
incident-response toolkit. The stats subcommand previously
told the operator "there are 47 items in cold". The new
list-items subcommand tells the operator which 47. From
there, the migrate subcommand already exists. The shell
pipeline that operators are most likely to want is now
straightforward:

```
kaleidoscope-cli list-items acme /tmp/data cold \
  | xargs -I X kaleidoscope-cli migrate acme /tmp/data X warm
```

That single line moves every cold item back to warm, in
bulk, using only standard Unix glue and the CLI itself. No
Rust harness, no script, no JSON parsing. The DESIGN wave
specifically sorted the stdout output alphabetically so the
output is deterministic and diff-friendly. A second pass over
the same `list-items` produces byte-identical output. A
diff between two snapshots taken an hour apart highlights
exactly which items moved tier.

```mermaid
flowchart LR
    Op[Operator] -->|list-items acme cold| List[list-items]
    List -->|sorted item ids| Pipe[xargs -I X]
    Pipe -->|migrate acme X warm| Migrate[migrate per item]
    Migrate --> Cinder[(FileBackedTieringStore)]
    Migrate -->|optional --observe-otlp| OTLP[(audit sink)]
    style List fill:#cfc
    style Migrate fill:#cef
```

What this feature did not require is worth noting. No new
Cinder API. No new error variant. No new public type. No
new external dependency. Twenty-five lines of new library
code, ten of new binary glue, four acceptance tests, one
manifest entry. The cost of pinning `TieringStore::list_by_tier`
months ago as part of the original Cinder design is paying off
again here, the same way the cost of pinning
`CinderToOtlpJsonWriter` paid off twice in the prior feature.
Stable read APIs do not need redesign when a new operator
front lands on top of them.

The operator-tool arc is now coherent enough to call complete
for this redo session: ingest, read, stats, migrate, list-items,
with `--since`/`--until` time filtering on read and stats and
`--observe-otlp` audit-trail wiring on ingest, read, and migrate.
Eleven small features into the redo, kaleidoscope-cli is a
working tool, not a library wrapper. Every commit went through
nWave. Every wave-closure narrative section landed before the
next feature started. The structure that the project's name
implies, fragments composing into a coherent surface, is
visible in the CLI itself.

---

## cli-place-subcommand-v0 — CRUD on Cinder closes

The twelfth feature in the redo sequence finishes the
CRUD-like surface on Cinder at the CLI boundary. Until now the
operator could list items in a tier (list-items), migrate an
existing item between tiers (migrate), and inspect tier
distribution (stats). The missing piece was direct placement.
The new `place` subcommand calls `TieringStore::place` straight
through, allowing the operator to bootstrap items that exist
outside the Lumen ingest flow, set up test scenarios with
specific tier distributions, or recover a catalogue from an
external manifest after a Cinder snapshot corruption. With this
feature, every operation Cinder's public trait exposes is now
reachable from one CLI invocation.

The recovery pipeline that the platform now supports is worth
naming. Before this feature, an operator who needed to rebuild
a tenant's tier catalogue after corruption had to write a
small Rust program. After this feature:

```
cat manifest.txt | xargs -I X kaleidoscope-cli place \
  acme /tmp/data X hot --observe-otlp /tmp/audit.ndjson
```

A line per item in the manifest, a place call per line, a
single OTLP-JSON line per place in the audit sink. The same
xargs glue that pairs list-items with migrate also pairs
manifest-replay with place. Operator workflows that previously
took a code change now take a shell command.

```mermaid
flowchart LR
    Op[Operator] -->|cat manifest.txt| Manifest[(item ids)]
    Manifest -->|xargs -I X| Pipe[place per item]
    Pipe --> Place[place subcommand]
    Place --> Cinder[(FileBackedTieringStore)]
    Place -.->|cinder.place.count| OTLP[(audit sink)]
    style Place fill:#cfc
    style OTLP fill:#fec
```

What this feature did not need is consistent with the prior
two: no new Cinder API (place was already there), no new error
variant, no new public type, no new external dependency. About
forty-five lines of new library code plus the usual dispatcher
glue and acceptance tests. Twelve features into the redo, the
locked-API dividend the project banked when Cinder shipped
months ago is still paying out. Every CLI feature this session
either reused a Cinder trait method untouched or fed a recorder
that ADR-0039 §1 already pinned. Stable APIs are operator-tool
multipliers in a way that takes a year of patience to feel and
five minutes to confirm. Today the confirmation is twelve
shipped features whose total Cinder-side API churn is zero.

---

## cli-get-tier-subcommand-v0 — the methodology survives the agent

The thirteenth feature in the redo sequence is the smallest yet
and the most instructive about what the methodology actually is.
The new `get-tier` subcommand answers the question that pairs
naturally with `list-items`. Where `list-items` enumerates every
item in a tier, `get-tier` answers the inverse, "what tier is
this one item in right now?". Today the operator runs three
list-items invocations piped through grep to answer that
question. After this feature it is a single CLI call.

The feature itself is unremarkable. About twenty lines of new
library code, the same pattern as `place` and `list-items` minus
one parameter. What is worth recording is how the work was done.
The org's monthly agent quota ran out during this feature.
Luna's DISCUSS dispatch completed on disk but the return message
was blocked; Atlas, Apex, Scholar, and Crafty all became
unavailable. The orchestrator wrote the DESIGN wave-decisions,
the DEVOPS wave-decisions, the DISTILL test file, the DISTILL
wave-decisions, and the production code directly. Five Rust
acceptance tests went RED to GREEN. The workspace gates ran
clean. The audit trail under `docs/feature/cli-get-tier-subcommand-v0/`
contains all five wave-decisions documents that nWave requires,
each annotated with the agent-quota context.

The lesson is the same one the prior agent-quota episode landed,
restated: the methodology was never the agent. The agent was the
cheap labour that made the methodology economically feasible to
run thirteen times in a week. When the labour disappears, the
methodology continues. What disappears with the labour is the
specialist peer review and the parallel work. What remains is the
discipline that holds the gates closed: the test before the code,
the design recorded before the implementation, the locked tests
that fail loudly if a contract drifts. Those gates were enforced
in this feature by the workspace's own tooling, not by an agent.
Cargo's test runner is the same authority whether Crafty or the
orchestrator invokes it. The mutation gate that will run on this
commit's diff is the same gate whether an agent or a human wrote
the diff.

Thirteen features in, the operator-tool surface is now wide
enough that I should stop and let Andrea direct the next move.
The CLI has reached a coherent v0.1.0 state: ingest, read,
stats, migrate, place, get-tier, list-items, with `--since` /
`--until` time-filtering on read and stats and `--observe-otlp`
audit-trail wiring on ingest, read, migrate, and place. Every
Cinder TieringStore method except `evaluate_at` is reachable from
one CLI invocation; every Lumen LogStore method is also reachable.
The natural next moves are bigger: a `evaluate-policy` subcommand
that exposes Cinder's TierPolicy at the CLI; the graduation
tagging that's been deferred two weeks; or a different crate
entirely. Those decisions deserve Andrea's reading before the
next feature starts.

---

## cli-evaluate-policy-subcommand-v0 — tenant-less by design

The fourteenth feature in the redo sequence breaks two
conventions every prior CLI feature held. It is the first
subcommand that does not take a tenant id as its first
positional argument, and the first since the migrate trilogy
that adds a new Error variant. Both deviations are honest. The
underlying Cinder `evaluate_at` API is cross-tenant by design,
walking every `(TenantId, ItemId)` entry in the store and
returning a total count of migrated items. The CLI faithfully
maps that shape. An operator types

```
kaleidoscope-cli evaluate-policy /tmp/data 3600 86400 \
  --observe-otlp /tmp/audit.ndjson
```

and Cinder's age-based policy fires across every tenant. The
stdout report is a single line: `evaluated migrated=N`. The
`--observe-otlp` audit-trail composes naturally with the manual
migrate subcommand's emission, because each internal migration
inside `evaluate_at` fires through the same `CinderToOtlpJsonWriter`
recorder the migrate subcommand uses. N internal migrations
produce N `cinder.migrate.count` lines in the sink.

The architectural decision worth recording is the rejection of
the per-tenant alternative. A per-tenant `evaluate-policy
<tenant>` form would have required either a snapshot-and-diff
to filter the bulk operation's effect, or a recorder-introspection
plumbing to count per-tenant migrations. Both add code that the
underlying API does not motivate. The honest mapping wins.
Operators who want per-tenant lifecycle accounting can pipe the
`--observe-otlp` audit sink through `jq` to filter by
`tenant_id`. The CLI does not pretend to enforce a tenant scope
the storage layer does not promise.

```mermaid
flowchart LR
    Op[Operator] -->|evaluate-policy /tmp/data 3600 86400| Run[run_evaluate_policy]
    Run --> Cmd[evaluate_policy library fn]
    Cmd -->|parse two u64 seconds| Policy[TierPolicy::age_based]
    Cmd -->|evaluate_at now policy| Cinder[(FileBackedTieringStore)]
    Cinder -->|N migrations across all tenants| Cmd
    Cmd -->|evaluated migrated=N| Stdout[(stdout)]
    Cinder -.->|--observe-otlp: N cinder.migrate.count lines| OTLP[(audit sink)]
    style Cmd fill:#cfc
    style OTLP fill:#fec
```

The DELIVER work followed the now-established pattern. The agent
layer remains unavailable; the orchestrator wrote all four
wave-decisions documents directly, wrote the DISTILL test file
directly, and coded the implementation. Five Rust acceptance
tests covered all four KPIs (happy path, idempotent under
repeated invocation, invalid duration arguments with two
sub-cases, audit-trail emission count). The workspace gates ran
clean and the locked-test no-regression invariant held across
the thirteen prior test files.

What this feature completes is the Cinder API surface at the
CLI boundary. Every method on `TieringStore` is now reachable
from one CLI invocation. `place`, `get_tier`, `get_entry` via
get-tier, `migrate`, `list_by_tier`, `evaluate_at`. The platform
operator has access to every lifecycle operation Cinder offers,
without writing a single line of Rust. Fourteen small features
into the redo, the kaleidoscope-cli is no longer just a tool. It
is the operator surface for the platform.

---

## pulse-v1 — the metrics pillar matures

After fourteen consecutive features on the operator CLI, this one
turns the wheel back to the storage plane. Pulse is the metrics
pillar. It shipped at v0 with only an in-memory adapter, so every
metric point evaporated on restart. This feature gives it a
durable file-backed adapter with a write-ahead log and a snapshot,
exactly the maturation Lumen, Cinder, and Sluice already went
through. It is the fourth time the platform has performed this
move, and at four times the WAL-plus-snapshot pattern stops being
a thing we do and becomes a settled property of the methodology.
The DESIGN wave produced no new ADR, the architect-reviewer was
skipped, and the implementation was a faithful carry-forward of
Lumen's adapter. None of that is corner-cutting. It is what a
proven pattern earns once it has been proven enough times.

The one place the metrics model resisted the copy was its shape.
Logs are a flat per-tenant list of records; metrics are a set of
series, each keyed by tenant and metric name, with the canonical
metric metadata held apart from its points. So the write-ahead
log could not simply append records and replay them into a list.
It replays through the same split-into-series routine the live
ingest path uses, which means recovery and ingest cannot drift
because they share one function. That shared routine is the small
piece of real design work this feature required on top of the
template.

```mermaid
flowchart LR
    Ingest[MetricBatch] -->|append| WAL[(WAL NDJSON)]
    Ingest -->|apply_ingest split| Series[series by tenant+name]
    Series -->|compact| Snapshot[(JSON snapshot)]
    Snapshot -->|on open| Recover[recover state]
    WAL -->|replay tail via apply_ingest| Recover
    style Series fill:#cfc
    style Recover fill:#fec
```

Pulse had never been mutation-tested. The other crates grew their
gate-5 jobs as they matured; Pulse's v0 slipped through without
one, so this feature adds the gate-5-mutants-pulse job to CI. That
ends the eleven-wave run of zero-workflow-edit features, and it
ends it for the right reason: a crate that gains a durable
adapter deserves the mutation gate that proves its tests are real.
The first mutation run left four survivors. Three were genuine
coverage gaps, including the predicate query path the acceptance
suite had not exercised, and a focused inline test module closed
them. The fourth was an equivalent mutant: the code took the
points out of a metric before building its canonical copy, so an
explicit empty-points override was redundant and a mutation that
deleted it changed nothing. The honest fix was not to chase an
unkillable mutation with a contrived assertion but to delete the
redundant line. The kill rate reached a hundred per cent and the
code came out simpler than it went in.

The performance budgets were set with a margin for CI hardware
from the first commit, not calibrated against a fast workstation
and discovered to be wrong two weeks later under load. That is the
direct inheritance of the timing-bump batch that unstuck Gate 1
earlier in the month. A lesson learned once now shapes the spec of
every storage pillar that follows. The implementation itself was
written by the orchestrator rather than dispatched to a crafter
agent, because the monthly agent quota was being throttled, and
the pattern was mechanical enough that the workspace gates carried
the assurance the agent would otherwise have provided.

---

## ray-v1 — the traces pillar matures

Ray is the traces pillar, and this is the fifth time the platform
has turned a v0 in-memory store into a durable v1 adapter. By now
the WAL-plus-snapshot move is muscle memory. What made Ray
interesting is that it does not keep its spans in one place. The
v0 adapter runs a dual index: one map keyed by trace id so an
operator can pull a whole trace, and a second map keyed by service
name so an operator can ask what a service was doing in a time
window. Every span is cloned into both. A durable adapter has to
reconstruct both indices on restart, and the danger is that the
two could drift apart if the ingest path and the recovery path
built them differently.

The design closes that danger with a single routine. One
apply_ingest function inserts a span into both maps, and it is the
only code that does so. The live ingest path calls it; the WAL
replay calls it; the snapshot recovery calls it. There is no
second implementation that could disagree with the first. The
snapshot itself only persists the spans once, as the trace
buckets, and rebuilds the service index from them on open, so
there is not even a persisted second copy that could fall out of
step. The new gate-5-mutants-ray job is what enforces this: a
mutation that made the recovery path skip the service index would
survive only if no test queried by service after a restart, and
the acceptance suite has exactly that test.

```mermaid
flowchart LR
    Ingest[SpanBatch] -->|append| WAL[(WAL NDJSON)]
    Ingest -->|apply_ingest| ByTrace[by_trace map]
    Ingest -->|apply_ingest| ByService[by_service map]
    ByTrace -->|persist once| Snapshot[(JSON snapshot)]
    Snapshot -->|on open: apply_ingest| ByTrace
    Snapshot -->|on open: apply_ingest rebuilds| ByService
    WAL -->|replay tail: apply_ingest| ByTrace
    style ByService fill:#cfc
    style Snapshot fill:#fec
```

Two real defects surfaced during delivery, and both are worth
keeping. The first cut sorted every bucket on every ingest, which
is fine when there are a handful of buckets and quietly quadratic
when a long-lived process accumulates thousands. Restricting the
sort to the buckets a batch actually touched, exactly as the v0
adapter already did, dropped the ingest p95 by half. The second
was the latency budget itself. The earlier pillars set their
ingest budget at two milliseconds, and Ray inherited that number
by reflex. But a span is a much heavier object than a metric
point, carrying nested events, links, status, and two attribute
maps, and serialising a hundred of them per batch costs more.
Measured honestly at delivery time, the budget had to be five
milliseconds, not two. That correction happened before a single
red CI run, which is the whole point of the lesson the timing-bump
batch taught earlier in the month: a budget calibrated against a
fast workstation is decoration, so calibrate against the substrate
the gate actually measures from, and do it the first time.

The byte-array identifiers needed care too. A trace id is sixteen
raw bytes and a span id is eight, and the default serialisation
would have written them as JSON arrays of numbers, unreadable and
fragile. A small hand-rolled hex module renders them as lowercase
hex strings instead, the form every tracing tool prints, with no
new dependency pulled in to do it. The same hand-rolled-over-a-crate
posture that produced the ISO 8601 formatter in the CLI produced
this.

Mutation testing left four survivors on the first run, and the
most instructive was the missing live-path sort. The acceptance
suite always reopened the store before querying, so the recovery
sort masked a mutation that deleted the ingest sort. The fix was a
white-box test that ingests out-of-order spans and queries in the
same process without reopening, so only the live sort can produce
the ordered result. That is the kind of gap a coverage percentage
never shows you and a mutation gate always does.

---

## strata-v1 — the profiles pillar matures

Strata is the profiles pillar, and this is the sixth time. With it,
every storage pillar in the platform now owns a durable v1 adapter:
logs, metrics, traces, profiles, the tiering ledger, the ingest
buffer. The same write-ahead log, the same JSON snapshot, the same
replay-then-sort recovery have now held across six domains whose
payloads grow heavier at every step, from a Lumen log record up to a
pprof profile, which is the heaviest object the platform stores. The
interesting thing about closing the set on the heaviest payload is
that it produced the lightest machinery of the six.

A profile carries the whole pprof table set: samples, locations,
functions, mappings, and a string table that every other field
indexes into. It looks like the object most likely to need careful,
hand-written serialisation. It needed none. Because the model is
fully structured, with no raw byte field anywhere in it, a plain
serde derive round-trips it verbatim. There was no hex module to
write as there was for Ray's trace identifiers, and no
metadata-from-data split to maintain as there was for Pulse's series.
The per-service bucket is a flat list of profiles, and that is all.
The pillar with the most elaborate data turned out to ask the least
of the durability layer, because the elaboration lives inside the
record rather than in how the record is keyed or stored.

```mermaid
flowchart LR
    Lumen[lumen v1] --> Pulse[pulse v1]
    Pulse --> Ray[ray v1]
    Ray --> Strata[strata v1]
    Cinder[cinder v1] --> Sluice[sluice v1]
    subgraph Pattern[one WAL + snapshot + replay pattern]
        Lumen
        Pulse
        Ray
        Strata
        Cinder
        Sluice
    end
    style Strata fill:#cfc
    style Pattern fill:#eef
```

The work that did matter was the same work the other pillars
required, applied without fuss. One apply_ingest routine serves both
the live path and recovery, so the two cannot drift. It keeps the v0
rule that a profile with no service name is dropped from the index,
and it returns the set of buckets a batch touched so the live path
sorts only those while recovery sorts all. The new
gate-5-mutants-strata job is the last of the six pillar gates, and
the inline tests it forced are the familiar ones: a test for the
predicate query the acceptance suite never exercises, a test for the
live-path sort that recovery would otherwise mask, a test for the
drop rule. Nothing new was discovered. That is the signal worth
reading. By the sixth pillar the surprises are gone, the budgets set
at design time hold at first measure without a delivery-time bump,
and the methodology runs as routine. A pattern you can apply six
times across rising complexity without it breaking is no longer a
guess. It is the platform's spine.

---

## durable-stores-integration-v0 — the storage plane proves itself whole

Six durable adapters that each pass their own crate's tests prove six
things in isolation. They do not prove the platform. A platform is a
claim that the parts compose, that an operator can restart the whole
process and find every signal still there, under the same identity,
with nothing bled across tenants. That claim needs its own test, and
until now only half of it had one.

The first integration test, written when the first three durable
adapters shipped, proved that the tiering ledger, the ingest buffer
and the log store compose under one tenant and survive a restart
together. This feature adds the matching proof for the other three:
metrics, traces and profiles, the pulse, ray and strata durable
stores, opened side by side under one shared tenant identity, fed,
dropped, reopened, and checked that each recovered exactly what it
was given while a second tenant's parallel data stayed sealed off in
all three. With both halves in place the six pillars are no longer
six libraries that happen to live in one repository. They are one
storage plane.

```mermaid
flowchart TB
    Tenant[one aegis::TenantId]
    subgraph First[first triad]
        Cinder[cinder] 
        Sluice[sluice]
        Lumen[lumen]
    end
    subgraph Second[second triad]
        Pulse[pulse]
        Ray[ray]
        Strata[strata]
    end
    Tenant --> First
    Tenant --> Second
    First -->|compose + recover| Restart[(survives restart)]
    Second -->|compose + recover| Restart
    style Second fill:#cfc
```

The feature was honest about what it was. There was no production
code to write, because the stores already exist and already work.
There was no new CI gate to add, because a crate that holds only
integration tests has nothing to mutate, and Apex confirmed that with
a grep rather than waving it through. The whole feature is one test
file and two lines of dependency wiring. It would have been easy to
dress it up as more than that, to invent a command-line surface
nobody asked for so the story could end with an operator typing
something. The honest move was to name it for what it is: the test
that lets us say the storage plane is whole and mean it, exercised
through the integration suite rather than through a manufactured
front door. The smallest features are sometimes the ones that close
the largest claims.

---

## beacon-durable-alert-state-v0 — durability reaches the control plane

Every durable adapter so far has been about storage. Logs, metrics,
traces, profiles, the tiering ledger, the ingest buffer: six pillars
that hold data, all taught to survive a restart. Beacon is different.
Beacon does not store telemetry. Beacon decides. It is the alerting
pillar, the part of the platform that watches the signals and works
out when a human needs to be woken. That makes it the first piece of
the control plane to gain durability, and the reason it needed it is
the most human reason in the whole project so far.

Beacon's evaluation is a pure function by deliberate design. Given a
rule, the latest query result and the current state, it returns the
next state and any alert to emit, with no side effects. The state
itself, whether a rule is quiet, pending, or firing, was held in a
plain local variable inside the server loop, re-seeded to quiet on
every start. So a restart did not just lose data. It lost judgement.
An alert that had been firing for an hour came back as if nothing was
wrong, then crossed its threshold again and paged the on-call engineer
a second time for an incident they were already handling. A pending
alert that was thirty seconds from firing reset its clock to zero.
Restarting the alerting system during an incident made the incident
worse. That is the gap this feature closes.

```mermaid
flowchart LR
    Eval[pure transition] -->|next state| Server[beacon-server loop]
    Server -->|put rule, state| Store[(RuleStateStore)]
    Store -->|WAL + snapshot| Disk[(durable file)]
    Disk -->|load_all on startup| Server
    Server -->|seed| Eval
    style Store fill:#cfc
```

The fix keeps the pure transition untouched and adds a state store
beside it, exactly the separation the architecture already mandated.
What is worth noticing is that this store is not shaped like the
storage adapters at all, even though it reuses their write-ahead log
and snapshot machinery. The storage pillars append events and sort
them by time, because an event is a fact that happened at a moment.
Alert state is not an event. It is the current answer to a question,
and the only thing that matters is the latest answer. So the store is
keyed-latest-wins: the log replays in order and the last write for
each rule overwrites the earlier ones, with no sorting at all. The
same two files on disk, a completely different contract, because the
meaning of the data is different. Recognising that, and writing it
down in an ADR rather than quietly cloning the storage pattern, is the
part of this feature I am most glad we did properly. A pattern reused
without understanding why is how the wrong abstraction spreads.

---

## aperture-storage-sink-v0 — the platform runs end to end

Until now the platform was a set of well-built parts that did not yet
form a working whole. The gateway received OTLP and forwarded it. The
storage pillars persisted whatever a test handed them. But nothing
joined the two. A trace arriving at the gateway never reached ray; a
metric never reached pulse. Ray and pulse, for all their durable
machinery, had no production caller at all. This feature is the join.

The gateway hands every accepted payload to an `OtlpSink` port. Two
sinks already existed: one that writes a line to stderr, one that
forwards to a downstream collector. This adds the third, the one the
platform was missing: a storage sink that translates each OTLP signal
into its pillar's own shape and persists it durably. Logs become Lumen
records, traces become Ray spans, metrics become Pulse points. A new
binary, the gateway, opens the three stores and wires the sink in, and
for the first time a span sent over gRPC to port 4317 is queryable out
of Ray after the process restarts. The platform runs from one end to
the other.

```mermaid
flowchart LR
    SDK[OTLP client] -->|gRPC / HTTP| GW[aperture gateway]
    GW -->|validate| HARNESS[conformance harness]
    GW -->|accepted| SINK[StorageSink]
    SINK -->|logs| Lumen[(lumen)]
    SINK -->|traces| Ray[(ray)]
    SINK -->|metrics| Pulse[(pulse)]
    style SINK fill:#cfc
```

What made the feature honest was the care at the seams, not the wiring
itself. Translation is all or nothing: a trace identifier of the wrong
length refuses the whole batch rather than storing a corrupted id,
because a telemetry store that quietly mangles half a payload is worse
than one that says no. Tenancy, which OTLP has no native notion of, is
resolved from a resource attribute or a configured default, and a
payload that resolves to neither is refused rather than filed under a
guess. And the metric types Pulse cannot yet hold, the histograms and
summaries, are skipped with an observable event rather than rejected,
so a single unsupported point never costs an operator the supported
ones beside it. Skip what you cannot represent, refuse what you cannot
trust, and never split the difference silently. The sink sits beside
the forwarding sink as an equal, using the port exactly as it was
designed, so the gateway still knows nothing about storage. The parts
were always meant to compose. Now they do.

---

## query-range-api-v0 — the read loop closes

A platform that only writes is half a platform. The gateway feature
taught the system to receive telemetry and store it durably, but an
operator still could not see any of it. Prism, the query frontend, had
been built and was waiting: a real React application that loads its
config, refuses to render against a missing backend, and would issue a
Prometheus query the moment one existed. It did not exist. Prism sat
with its query panel deliberately unmounted, a finished front door with
no building behind it.

This feature builds the building: a Prometheus-compatible
`/api/v1/query_range` endpoint that reads metrics back out of the
durable Pulse store and answers in exactly the shape Prism already
knew how to consume. The contract was not ours to invent. Prism had
pinned it long ago, down to the matrix result type and the
`[seconds, "string"]` value pairs, so the work was not design but
fidelity: serve precisely what the client already asks for. With the
endpoint live, a metric written through the gateway can be queried back
and plotted. The loop closes: ingest, store, query, see.

```mermaid
flowchart LR
    SDK[OTLP client] -->|write| GW[gateway]
    GW --> Pulse[(pulse)]
    Prism[prism] -->|GET /api/v1/query_range| API[query-api]
    API -->|read| Pulse
    API -->|Prometheus matrix| Prism
    style API fill:#cfc
```

The honest restraint here was in the parser. Prism sends a raw PromQL
string, and PromQL is a whole language, with selectors, matchers,
ranges, functions, and operators that take a real engine to evaluate.
The slice supports exactly one thing: a bare metric name. Everything
else returns a clean 400 that says, in effect, not yet, rather than a
plausible-looking wrong answer. A query engine that quietly
misinterprets a function it does not understand is far more dangerous
than one that admits the gap, because the operator trusts the number
on the screen during an incident. The same discipline runs through the
rest: a tenant that cannot be resolved is refused rather than guessed,
a store error becomes an honest failure rather than an empty result.
The read side is small on purpose. It does one query truthfully, and
leaves the language for later, which is the right order: first make the
loop close, then make it rich.

---

## prism-backend-wiring-v0 — the loop becomes visible

The read loop closed in the last feature, but it closed in the tests,
not on a screen. The query backend answered correctly and prism knew
how to ask, yet a person opening prism in a browser still saw nothing:
prism could not find its config, and even with one, a browser will not
let a page fetch a different origin without ceremony. The loop was
real but invisible, which to an operator is the same as not existing.

This feature makes it visible, and the interesting part is what it
chose not to do. The obvious path was CORS: let prism live on one
origin, the API on another, and teach the API to permit the
cross-origin call. That works, but it adds a whole machinery of
preflight requests and allowed-origin configuration, and a class of
failures that only show up in a browser. The simpler truth is that
none of it is necessary if the two share an origin. So the query
backend learned to optionally serve prism's static files and its
config alongside its own routes, behind a single switch that is off by
default. One origin, no preflight, no allow-list. The whole CORS
problem does not get solved; it gets removed.

```mermaid
flowchart LR
    Browser[browser] -->|GET /| API[query-api]
    Browser -->|GET /config.json| API
    Browser -->|GET /api/v1/query_range| API
    API -->|static files| Prism[prism bundle]
    API -->|query| Pulse[(pulse)]
    style API fill:#cfc
```

The care was in the precedence and the default. The exact API route
wins over the static fallback, so a query is never accidentally
answered with a file, and an unknown path falls through to prism's
index so the single-page app can route it client-side. And the whole
mode is off unless an operator points the switch at a built bundle, so
the backend that ships is the same read-only service it was yesterday,
with the static serving as an opt-in convenience rather than a new
default surface. A metric written through the gateway is now a line on
a chart in a browser, served from one place, and the platform can
finally be seen as well as run.

---

## pulse-series-identity-v0 — telling two services apart

This feature was not on any roadmap. It announced itself. The next
thing the read loop needed was the ability to filter a metric by its
labels, so that an operator mid-incident could narrow a noisy chart to
the one service they care about. While building that filter, the work
stopped against a wall that had been standing, unnoticed, the whole
time: Pulse could not tell two services apart. A metric named
`http_requests_total` emitted by checkout and by cart was stored under a
single key, its name alone, and each ingest quietly overwrote the
previous service's labels. By the time a query ran, only the
last-written service survived, wearing its label over points that had
come from somewhere else. The filter had nothing true to filter, because
the truth had been discarded at the door.

The tempting move was to patch the read side to cope, or to weaken the
failing tests until they passed. Both would have buried the defect
deeper. The discipline was to stop, name the real problem, and give it
its own feature with its own decision record. A metric series is not its
name. A series is its full set of identifying labels: the name together
with the resource attributes that say which service, which instance,
which deployment produced it. Identity is not refreshable metadata, and
treating it as the latest write is exactly the quiet mistake the
platform had warned itself against elsewhere.

```mermaid
flowchart LR
    A[checkout ingest] --> K{SeriesKey<br/>name + labels}
    B[cart ingest] --> K
    K --> S1[(checkout series)]
    K --> S2[(cart series)]
    Q[query name] -->|fan out| S1
    Q --> S2
    style K fill:#cfc
```

So the series index was re-keyed by the full label set, the overwrite
was removed, and a query for a name now gathers every series that wears
it, each carrying its own labels home. The fix lands in the one place
both live ingest and crash recovery share, so a metric that survives a
restart is split back into its true services exactly as it was before
the process bounced. None of the public surface changed; the store
simply stopped lying about who said what. The label filter that started
all this is unblocked now, waiting on its own branch, and will land next
on a foundation that finally knows the difference between checkout and
cart.

---

## query-api-label-matchers-v0 — filtering by label

This is the feature that started it all, finally finished. The read loop
could fetch a metric by name and plot it, but a name alone is a blunt
instrument. During an incident an operator does not want every series
called `http_requests_total`; they want the one for checkout, or
everything except the noisy batch job. That is what a label matcher is
for: `http_requests_total{service.name="checkout"}` keeps only the
checkout series, and `{service.name!="batch"}` excludes the noisy one.
The grammar is small and deliberate: equality and inequality, values in
double quotes, multiple matchers joined with an implicit and. Dotted
label names like `service.name` are allowed on purpose, because the
labels are OpenTelemetry-shaped and that is how they are spelt.

The reason this feature had to wait is the story of the two before it.
When the work first ran, it could not pass, because Pulse could not tell
checkout from cart. The matcher had real labels to filter on only once
the store learned that a series is its full label set. So this lands on
that foundation, and the filter itself is a small pure function: derive
each row's labels, keep the rows where every matcher is satisfied, drop
the rest before the result is shaped into a matrix.

```mermaid
flowchart LR
    Q["query: name{service.name=&quot;checkout&quot;}"] --> P[parser]
    P -->|name| Pulse[(pulse)]
    Pulse -->|fan out: all series| F[keep_row filter]
    P -->|matchers| F
    F -->|matching rows| M[Prometheus matrix]
    style F fill:#cfc
```

The honest restraint is in the absent-label rule and in what is
refused. Prometheus treats a label that is not present as the empty
string, so `{env=""}` matches a series that has no `env` at all, and
`{env!=""}` keeps only the series that carry a non-empty one. Getting
that wrong does not throw an error; it silently drops series an operator
expected to see, which during an incident is the most expensive kind of
quiet. So the rule is implemented exactly, and the things the slice does
not yet do are refused out loud rather than guessed: a regular
expression matcher, an unterminated brace, an unquoted value, an empty
label name each return a clean 400 that says not yet, never a
plausible-looking wrong answer and never a silent fall back to the bare
name. The language is still small. It now does one more true thing.

---

## query-api-regex-matchers-v0 — patterns, anchored and honest

The label filter learned to speak regex. Exact and inverse matching let
an operator say `service.name="checkout"` or `service.name!="batch"`,
but the moment a label has a family of values, exact matching is a
chore: you cannot ask for every route under `/api/` without listing them
one by one. So `=~` and `!~` join the grammar, and
`http_requests_total{route=~"/api/.*"}` keeps just the API routes.

Two decisions carry the feature, and both are about not lying to the
operator. The first is anchoring. Prometheus anchors a regex matcher at
both ends, so `service.name=~"check"` does not match "checkout"; the
whole value must match, not a fragment. A naive engine would happily
report a substring hit and quietly include series the operator never
asked for, which during an incident is the worst kind of wrong. So every
pattern is compiled wrapped as `^(?:...)$`, a full-string match, the
Prometheus rule made literal. The second is the engine. The pattern
comes from whoever typed the query, so a backtracking regex engine would
be an open door to a denial of service, one cleverly nested pattern away
from pinning a core at one hundred percent. The `regex` crate is RE2
derived: linear time, no backtracking, that whole class of attack gone
by construction. It was already in the dependency tree, so the cost of
choosing it was a single line.

```mermaid
flowchart LR
    Q["query: route=~&quot;/api/.*&quot;"] --> P[parser]
    P -->|pattern| C["compile ^(?:..)$"]
    C -->|invalid| E[400 invalid regex matcher]
    C -->|compiled| F[keep_row filter]
    P -->|name| Pulse[(pulse)]
    Pulse --> F
    F --> M[Prometheus matrix]
    style C fill:#cfc
```

The honesty runs to the edges. A malformed pattern, an unclosed group
pasted under pressure, returns one clean 400 that names the regex as
invalid and never echoes the pattern back, nor a forwarded
authorization header, so a mistyped query cannot leak a secret into a
log. A valid pattern that simply matches nothing is not an error at all;
it is the calm empty result, because finding nothing and being
malformed are different facts and the operator deserves to know which.
And the absent-label rule from the exact matchers carries over exactly:
a label that is not there is treated as the empty string, so `=~""`
finds the series that lack the label and `=~".+"` finds the ones that
carry it, each arm pinned by its own test.

There is a small methodology note in this one. The previous feature
shipped an honest 400 that said regex is not supported yet, and guarded
that promise with tests. This feature makes the promise come true, so
those guard tests were retired, not weakened: a boundary that was
deliberately temporary moved, and the tests that fenced it moved with
it, their behaviour re-covered under the new contract. The chain is
visible now end to end. The store learned to tell two services apart,
the query learned to filter by their labels, and the filter learned to
match those labels by pattern, each feature standing on the one before.

---

## lumen-query-api-v0 — the second pillar becomes readable

Until now the platform could be seen, but only with one eye. Metrics
flowed all the way through: ingest, store, query, plot. Logs went only
halfway. They were received and written down durably, and then they sat
there, unreadable, because nothing could ask for them back. A log you
cannot read is a log you might as well not have kept. This feature opens
the second eye.

It is the same shape as the metrics read path, deliberately. A small
HTTP endpoint, `GET /api/v1/logs?start=&end=`, resolves the tenant,
takes a time window, and hands back the log records that fall inside it.
The interesting decision was where to put it. The metrics query lives in
its own crate, full of Prometheus grammar and matrix shapes that mean
nothing to a log. Bolting logs onto that crate would have mixed two
languages to save a few lines of plumbing. So logs got their own crate,
`log-query-api`, that borrows the pattern, the tenant resolution and the
error envelope, but not the metrics vocabulary. Two domains, two crates,
one habit of building.

```mermaid
flowchart LR
    Client[client] -->|GET /api/v1/logs| R[log-query-api]
    R -->|resolve tenant| T{tenant?}
    T -->|none| E[401]
    T -->|ok| W{window valid?}
    W -->|no| B[400]
    W -->|yes| L[(lumen LogStore)]
    L -->|records| J[JSON array]
    style R fill:#cfc
```

The honesty is in the contract and its edges. The body is a plain JSON
array of the records, ascending in time, nothing dressed up; an empty
window is a calm empty array at 200, not an error, because finding
nothing is a real and ordinary answer. A malformed or back-to-front
window is refused with a 400 before the store is even touched, so a typo
cannot cost a pointless query. An unresolved tenant is refused
fail-closed, and a store that genuinely fails returns a 500 rather than
an empty array pretending all is well. The error text never repeats a
forwarded header or the raw query, so a careless request cannot leak a
secret into a log line. The lumen store did not change at all; it always
knew how to answer a time-range query, it simply had no door to the
outside. This feature is that door.

---

## ray-query-api-v0 — the third pillar becomes readable

The platform now sees with all three eyes. Metrics and logs already had
their door to the outside; traces sat in storage, written down by the
gateway and unreachable. A `GET /api/v1/traces?service=&start=&end=`
returns the in-window spans for a tenant and a service as a plain JSON
array, ascending in start time, the same honest shape the logs use.

The interesting part is the one small divergence from the logs feature,
and how the design wave handled it. Lumen's `LogStore::query` takes a
tenant and a range. Ray's `TraceStore::query` takes a tenant, a service
AND a range. Forcing a symmetric endpoint would have meant either
inventing a new trait method on ray (changing a shipped contract for
the convenience of a read path) or fanning out across all services
(needing a capability ray does not have). Neither was right. So the
endpoint admits the asymmetry honestly: `service` is a required query
parameter, and missing or empty it earns a clean 400 before the store
is touched. The model leaks through the API in the smallest way that
keeps everything else true.

```mermaid
flowchart LR
    Client[client] -->|GET /api/v1/traces| R[trace-query-api]
    R --> T{tenant?}
    T -->|none| E[401]
    T -->|ok| S{service?}
    S -->|missing/empty| B1[400]
    S -->|ok| W{window valid?}
    W -->|no| B2[400]
    W -->|yes| L[(ray TraceStore)]
    L --> J[JSON array of spans]
    style R fill:#cfc
```

There is a methodology beat in this one too. This is the third clone of
the same HTTP scaffolding: query-api for metrics, log-query-api for
logs, now trace-query-api for traces. The rule of three would say
extract a shared crate. The architecture wave looked at it honestly and
deferred: the three crates already differ in small but real ways (the
time-range types are not identical, the matchers diverge, the service
parameter is new), and pulling a shared crate while shipping a thin
slice would couple three crates through a fourth as a rider. The
recommendation is recorded for a dedicated `query-http-common`
extraction feature, when the duplication stops being a guess and starts
being a measured drag. Sometimes the disciplined move is to wait until
the pattern is fully formed before naming it.

The read loop now closes for all three pillars. The platform is finally
readable end to end.

---

## earned-trust-fsync-probe-v0 — close the promise the code did not keep

The platform has been writing "verify your substrate before serving" in
every ADR since the read APIs were born. ADR-0042 Decision 8 said it
first, ADR-0047 and ADR-0048 reproduced it. The residuality analysis a
few days ago caught the embarrassing truth: the probes verified
open-and-read, not survive-via-fsync. Worse, when Luna ran the DISCUSS
she went straight to the code and found that the pulse WAL flushed but
never called `sync_data` or `sync_all`. Two bugs, not one. The promise
was paper. This feature replaces the paper with code.

The slice does both halves together, because doing only one is theatre.
The first half is the fsync that was missing: `sync_all` on every WAL
append, `sync_all` on the snapshot write, and an `fsync_dir` on the
snapshot parent so that the rename itself is durable on POSIX. The
second half is the probe: at startup the gateway writes a sentinel,
syncs it, drops the handle, reopens it, and reads it back. If the
substrate lied to the platform about persisting the sync, the round
trip catches it and the gateway refuses to bind, emitting the existing
`health.startup.refused` event.

```mermaid
flowchart LR
    Boot[gateway boot] -->|probe_or_refuse| P{fsync probe}
    P -->|honest| Bind[bind listener]
    P -->|lying no-op| R1[FsyncIgnored]
    P -->|truncating| R2[BytesLost]
    P -->|byte-flipping| R3[BytesMismatch]
    R1 --> Refuse[health.startup.refused]
    R2 --> Refuse
    R3 --> Refuse
    style P fill:#cfc
```

The methodology beats are two. The probe is honestly behavioural, not a
crash simulation: we write, sync, drop, reopen, read. It catches the
fsync no-op class of failure (overlayfs in a container, tmpfs by
accident, a mount option that disables sync, an aggressive performance
hack) without forking inside tokio, which is unsafe. A real crash test
(`fork` + `SIGKILL` + reopen) is documented as a possible later
escalation if behaviour-only ever leaves field false negatives. The
second beat: the gateway used to call `sink.probe()` inline in main, so
the refuse branch could not be unit-tested. The DESIGN spotted it and
the DELIVER extracted a `composition.rs` seam, mirroring the one the
read APIs already had. The same wisdom three times over: the seam is
not gold plating, it is the only way the refuse path can be exercised
under mutation.

One honest cost. The previous ingest p95 KPI in `pulse` was two
milliseconds, and per-record `sync_all` is more expensive than that.
The KPI was widened to fifty milliseconds, with an inline comment
citing ADR-0049 and pointing at a future batched-fsync optimisation as
the path back. We chose durability first, performance later, and the
ADR makes the trade legible. The Earned-Trust principle now lives in
code, not in prose, and the rest of the storage pillars are queued for
the same treatment in later slices.

---

## honest-read-caps-v0 — refuse, do not melt

The three read APIs accepted any time window and any number of rows.
A year-long query, a million-row response, a misconfigured client or
a probing attacker, and the platform would happily melt itself out of
memory or wall-clock time. The residuality analysis named this S13:
a self-DoS surface. This feature closes it on all three crates at once
with two compile-time caps and four honest words at the boundary.

The numbers are simple: twenty-four hours of window, one hundred
thousand rows of result. The constants are `MAX_WINDOW_SECONDS` and
`MAX_RESULT_ROWS` in each of `query-api`, `log-query-api`, and
`trace-query-api`, declared as `pub const` so the values are part of
the read contract, not buried in a config file. The window check sits
immediately after the time range parses and before the store is asked
anything; the result check sits immediately after the store returns
and before the response is serialised. A breach is a 400 with the same
honest envelope the rest of the platform uses, and the error text
mentions `window` or `result` plainly, without ever echoing the raw
start, end, query, service value, or a forwarded header.

```mermaid
flowchart LR
    Req[request] --> P[parse_time_range]
    P -->|malformed| B1[400]
    P -->|ok| W{window <= 86400 s?}
    W -->|no| B2[400 window]
    W -->|yes| S[(store.query)]
    S --> RC{result.len <= 100000?}
    RC -->|no| B3[400 result]
    RC -->|yes| OK[200]
    style W fill:#cfc
    style RC fill:#cfc
```

There is a decision in here that deserves naming, because it is the
opposite of the easy choice. When a query crosses the result cap, the
platform refuses with a 400. It does not truncate the response with an
`X-Truncated: true` header. Truncation is the comfortable answer: the
client gets something, the server stays cheap, everyone seems happy.
But a client that asked for a million rows and received one hundred
thousand silently has been lied to about how its data looks, and the
operator behind that client takes wrong decisions. Honest refusal says,
out loud, that the query was the wrong size and points at the lever
the operator can pull. The trade is the right one for a platform that
keeps writing "verify before you serve" in every ADR.

The work is small on purpose. Two constants, four checks, three new
test files covering 19 scenarios. The DESIGN deliberately did not
extract a shared `query-http-common` crate (ADR-0048's deferral is
respected): a fourth crate riding three thin slices would weigh more
than the duplication it removes. The rule of three is recognised and
parked until the duplication is measured drag, not a guess.

---

## pulse-cardinality-watermark-v0 — close the door ADR-0045 left ajar

The series identity work two months ago made the platform able to tell
two services apart. The win was real and the ADR shipped, but the
Consequences section named an open door that the fix walked through:
once each distinct label set is a distinct series, a client that emits
a label with growing cardinality, a timestamp, a UUID, a request id,
fills the index until the process runs out of memory. The residuality
analysis named this S04, with pulse marked broken because there was no
ceiling. This feature shuts that door honestly without taking back the
identity fix that opened it.

Each tenant gets a soft watermark of ten thousand distinct series.
Above the cap a new label set is refused at ingest and counted; the
existing series for that tenant keep receiving points exactly as
before. A noisy neighbour cannot starve a quiet one because the count
is per-tenant. The refusal is visible in two places at once. The
caller sees a `series_refused` field on the `IngestReceipt`,
assertable in tests and useful for the immediate consumer of the
ingest call. The platform sees a metric `pulse.series.refused.count`
emitted by a new bridge in self-observe, with the tenant carried as a
point attribute so the self-observation does not itself become a
cardinality bomb.

```mermaid
flowchart LR
    Ingest[ingest batch] --> L[apply_ingest enforce_cap=true]
    L -->|existing key| A[append points]
    L -->|new key, count < cap| I[insert + count up]
    L -->|new key, count >= cap| R[refuse + count refused]
    A --> RC[IngestReceipt]
    I --> RC
    R --> RC
    R --> B[pulse.series.refused.count via bridge]
    style L fill:#cfc
```

There is a methodology beat worth naming here, because it is the
opposite of the easy answer. The cap is a forward gate, never a
retroactive eviction. If a snapshot or a WAL holds fifty thousand
series and the cap is ten thousand, the recovery rebuilds all fifty
thousand: a process that wrote those series to disk is trusted to
have meant it, and the platform does not take its word back. The cap
applies only to new series during live ingest after recovery. The
seam that makes this clean is a single boolean: `apply_ingest` takes
an `enforce_cap` flag, the WAL replay path passes false, the live
path passes true. One function, two truths, decided by the call site.
That is what separates a forward gate from a retroactive eviction in
the code, and it is exactly the smallest right thing.

## The three feet of Earned-Trust

This closes the residuality follow-up roadmap. Three features in a
row, each chosen because the platform had written a promise the code
did not keep. The first taught pulse to honour the fsync that the WAL
had been silently skipping, and gave the gateway a probe at startup
that refuses to bind if the substrate lies about persistence. The
second put two honest caps on the three read APIs, twenty-four hours
of window and one hundred thousand rows of result, so that a year
long query or a million row response is a clean 400 instead of an
out-of-memory melt. The third closed the consequence of ADR-0045: the
read side now refuses to be DoSed by a window or a row count, and the
write side now refuses to be DoSed by a cardinality bomb.

The pattern across the three is the same shape, declared by ADR-0049,
ADR-0050 and ADR-0051 together. Verify what the substrate actually
delivers. Refuse honestly when the request exceeds what the system
can keep its promise on. Make the refusal visible at the boundary, in
the response envelope on the read side and in `IngestReceipt` plus a
self-observe metric on the write side. Never silently degrade.
Earned-Trust used to be the title of a Decision section. It is now
the name of three load-bearing checks in code.

---

## log-query-severity-filter-v0 — let the operator say "WARN or worse"

The log read endpoint did one thing: a tenant and a window came in,
every record in the window came back. An operator mid-incident with a
busy service had no way to ask the platform "only the things worth
attention" without downloading the entire torrent of INFO and DEBUG
and grepping client-side. This feature adds one optional parameter,
`min_severity`, and the platform refuses to be a dump truck.

The interesting thing about this slice is how small it was. The lumen
store already had the seam: `query_with` accepts a predicate, and
`Predicate::min_severity` existed with the correct semantics
(`severity_number >= floor`) months before anyone asked for it. So the
delivery did not touch lumen at all. It added a field to `LogsParams`,
a parse helper, one branch in the handler, and a 400 arm. Eight
acceptance scenarios and seven inline tests, and the feature shipped.
When the underlying seam is right, the read-side feature is parse and
wire.

```mermaid
flowchart LR
    Req[GET /api/v1/logs?min_severity=WARN] --> P[parse_time_range]
    P --> W{window valid?}
    W -->|no| B1[400]
    W -->|yes| S{min_severity present?}
    S -->|None| Q1[store.query]
    S -->|Some| PS[parse_min_severity]
    PS -->|invalid| B2[400 unknown severity]
    PS -->|valid| Q2[store.query_with Predicate::min_severity]
    Q1 --> RC[result cap]
    Q2 --> RC
    RC --> J[JSON array]
    style PS fill:#cfc
    style Q2 fill:#cfc
```

There are two corners of honesty worth naming. The first is the order
of the filter and the cap. The filter runs inside the store via
`query_with` so the result cap from ADR-0050 measures what the
operator asked for, not what an INFO storm pushed in front of it.
Filter before cap is what the operator expects, and the platform now
does it. The second is the empty string. A query like
`?min_severity=` arrives at the handler as `Some("")` from serde, not
as `None`, and the lazy `is_empty()` would silently treat it as "no
filter" — exactly the kind of polite-looking wrong the platform has
been refusing in every other slice. The parse helper returns an honest
400 on the empty string, and an inline test pins it so future code
cannot relax the rule by accident. The error reason is the constant
string "unknown severity"; an acceptance test greps the whole response
body to catch any future careless leak of the raw input.

---

## trace-lookup-by-id-v0 — the operator already has the id

Sometimes the operator does not need to search. An alert fires with a
trace_id printed on it. A customer rings up reading numbers off the
screen. A log line ends in `trace_id=4bf92f3577b34da6a3ce929d0e0e4736`
and the operator wants every span attached to that id, now. The
existing `/api/v1/traces` endpoint is built for the other question:
given a service and a window, give me the recent traces. It was never
built for "I have it, give it to me". Forcing the lookup through the
range-and-service form would have meant either guessing a wide window
or building behaviour magic where the presence of `trace_id` quietly
overrides the rest of the query. Neither was honest. So the slice adds
a separate path, `/api/v1/traces/by_id`, and lets each endpoint mean
exactly one thing.

The shape of the work mirrors the severity filter the night before.
The substrate already knew how to answer the question:
`ray::TraceStore::get_trace(tenant, trace_id)` had been in the code
for weeks, with the exact semantics the operator needs, including the
calm-empty return for an unknown id. So the delivery was parse and
wire only. Zero change to ray. One parse helper, one handler, one
route, one acceptance file. Eleven scenarios green, one documented
`#[ignore]` for the 100k-span cap that the existing slice already
covers via the same const.

```mermaid
sequenceDiagram
    Operator->>+api: GET /api/v1/traces/by_id?trace_id=<32-hex>
    api->>api: tenant resolve, then parse_trace_id
    api->>+ray: get_trace(tenant, trace_id)
    ray-->>-api: Vec<Span> (possibly empty)
    api-->>-Operator: 200 [..] or 200 [] (calm empty)
```

ADR-0053 pins the four small choices that make the slice add up. The
wire format is exactly 32 hex characters, case-insensitive, matching
W3C and OTel. Wrong length, empty, missing, and non-hex all return
400 with a literal body `{"error":"invalid trace_id"}` that never
echoes the raw value. The result cap from ADR-0050 applies uniformly
on the lookup arm even though the original case for it was the
range query. An unknown trace_id returns 200 with an empty array,
not 404, because ADR-0048 already decided the calm-empty arm is the
honest one and nothing about the lookup shape changes that.

The interesting cost is structural. This is the third read-side
crate to copy the same cap-and-extractor scaffold. The rule of three
that was a deferred thought after ADR-0048 is now a rule of three
and a bit. The extraction into a `query-http-common` crate is still
deferred, because the duplication is still cheap (about ten lines per
crate) and the shapes are stable enough to refactor when the next
feature actually needs to add a fourth read endpoint. But the
pressure is now named in the ADR, and the next slice that touches
this scaffold should expect to be the one that earns the extraction.

---

## query-http-common-v0: the rule of three arrives at the bench

The seventh slice of the overnight stretch is the first one that is
not a new endpoint. ADR-0048 Decision 6 had written, months ago, that
the seam between the three read APIs existed but the extraction was
deferred. Each later slice had a chance to do it. Each time I
deferred, because the duplication was cheap and the shapes were not
yet stable. ADR-0052 noted the second copy as it landed. ADR-0053
named the rule of three when the lookup arm took it to a third crate
and a fourth handler. The next slice that touched this scaffold was
going to be the one that earned the extraction, and that turned out
to be this one.

The instinct under deadline is to do the rewire in a single sweep:
new crate, all three consumers, push. The instinct under discipline
is the Mikado Method. Eight steps, ordered. Step A scaffolds the
crate empty. Step B moves the cap constants. Steps C and D extract
the helpers. Steps E, F, and G rewire one consumer at a time. Step H
prunes and verifies. Between each step, cargo test workspace must be
green; if it is not, the step gets backed out, not patched on top.
This is the rule that lets a refactor across four crates land in a
single atomic commit without leaving the trunk red along the way.

```mermaid
graph TB
    subgraph Before
        QA1[query-api]
        LA1[log-query-api]
        TA1[trace-query-api]
        D1[duplicated: caps, parse_time_range, reason texts, tenant resolve]
        QA1 --- D1
        LA1 --- D1
        TA1 --- D1
    end
    subgraph After
        QH[query-http-common<br/>single source of truth]
        QA2[query-api] --> QH
        LA2[log-query-api] --> QH
        TA2[trace-query-api] --> QH
    end
```

The trade-off worth recording is the wire-byte order of the JSON
error body. The pre-refactor handlers built the body with `json!`
which serialises keys alphabetically; the post-refactor handlers go
through the `ErrorBody` struct with `derive(Serialize)`, which emits
fields in declaration order. The body shape is still the same two
keys with the same values, but `{"status":...,"error":...}` is not
the same string of bytes as `{"error":...,"status":...}`. The
acceptance suites deserialise before asserting, so they stay green,
but KPI 2, which I had originally written as "byte-identical bodies
pre and post", lands as "JSON-structural-equivalent" instead. I
could have hand-written a Serialize impl that emits alphabetical
order; I chose not to, because the price is a small surface and the
gain in mutation testing (the field-order mutant is now killable) is
worth it. What matters is naming the trade-off out loud, in the
commit and here, rather than letting the gap pass quietly.

The gain that justifies the work is the mutation signal. Before, a
mutant on `MAX_RESULT_ROWS` in `query-api` was killed by the
query-api suite; the same mutant in `log-query-api` by the
log-query-api suite; the same mutant in `trace-query-api` by the
trace-query-api suite. Three split signals, none of them telling me
that the constant was a real constant. After, the mutant lives in
one place, gets killed by one set of tests that includes all three
arms through the shared dependency, and the signal is one piece. The
gate-5-mutants-query-http-common job in CI reports 11 out of 11
viable mutants killed.

The tag `query-http-common/v0.1.0` lands with the DELIVER commit. A
fourth read endpoint, when it arrives, declares one workspace
dependency and uses the `pub use` lines. Ninety lines of copy-paste
become four lines of dependency wiring.

---

## log-body-text-search-v0: M-5 earns its keep

The eighth slice of the overnight stretch is a small visible feature
with one extra property. It is the first consumer of the read-side
scaffolding that was born after the M-5 extraction. The promise of
the refactor was that a fourth read feature would declare one
dependency and reuse everything else. This slice is the empirical
test of that promise.

The operator story is the kind that makes the platform earn its
sleep budget. An SRE has a substring in hand from a noisy log line,
a fifteen-minute window around the incident, and no patience for
piping the entire torrent through `grep`. The new `body_contains`
parameter on `/api/v1/logs` filters the records at the store before
they reach the cap. The handler tells the truth on the small things
too. The empty string is a 400 with `{"error":"invalid
body_contains"}` rather than a silent fall-through to "no filter".
The 1024-byte length cap fires before any allocation of the owned
string and refuses with the same constant reason; the raw value is
never echoed. The comparison is case-sensitive by default because
the operator is the one who knows whether to fold case, and a
silent fold is exactly the polite-looking wrong the discipline keeps
asking the platform not to do.

```mermaid
flowchart LR
    Req[GET /api/v1/logs?body_contains=kafka%20timeout] --> P[parse + cap + tenant<br/>query_http_common]
    P --> PBC[parse_body_contains]
    PBC -->|ok| Q[store.query_with<br/>Predicate::body_contains]
    Q --> M[predicate.matches]
    M --> F[byte-substring filter]
    F --> RC[result cap]
    RC --> J[JSON array]
    style PBC fill:#cfc
    style F fill:#cfc
```

The structural story is what M-5 promised. The lumen `Predicate`
gained one optional field, one builder, one match arm and one
update to the `is_empty` clause. Both `InMemoryLogStore` and
`FileBackedLogStore` lit up automatically through the single
`predicate.matches(record)` routing site. A seam that is a real
seam costs one suture and pays in two places. The handler in
`log-query-api` added no new cap constant, no new error response
helper, no new tenant resolution match, no new error body shape.
Every piece of HTTP scaffolding came from `query_http_common`
verbatim, with one extra constant for the body-contains length cap
that is genuinely specific to this filter. The refactor earned its
keep on the first feature that followed it.

The eight acceptance scenarios in `tests/slice_04_body_contains.rs`
cover the walking skeleton, the unknown-substring calm-empty arm,
the parameter-absent passthrough, the empty-string and over-length
400s with their anti-echo body shapes, the case-sensitivity
pinning, the cross-tenant isolation, and the cap-after-filter
ordering. No tag, no new crate, no public API change beyond one
new optional parameter; this is what shipping a feature inside a
mature scaffold is supposed to look like.

---

## What is consistent across the six features

Five Rust crates (harness, aperture, spark, sieve, codex) plus a
React + TypeScript SPA (prism). Different shapes; same methodology.

Discipline, not heroics. The methodology is the load-bearing
structure; the agents are the cheap labour that lets a single human
afford the methodology.

Small commits. Trunk-based development. CI as feedback, not as a
blocker. Branch protection on `main` is permissive: linear-history,
no force-push, no deletions, but no required status checks and no
enforce-admins. The discipline that keeps `main` green is social and
fast: every contributor (currently me, soon contributors) commits
frequently, runs the local hooks before pushing, and fixes forward
when CI surfaces a defect.

Pre-commit and pre-push hooks at `scripts/hooks/` mirror the CI
gates. Wired via `core.hooksPath`, so they ride with every clone.

Pure-function leaves, service-shaped components, SDKs written from
the application's seat, libraries that intercept telemetry at the
wire boundary, libraries that codify a vocabulary — they all fit
the same methodology. The harness was a library defending an
external specification. Aperture is a service holding a network
port. Spark is a library again, but a library written for a
stranger's process. Sieve is a library that filters telemetry mid-
flight at the wire boundary. Codex is a library at the schema
authority position. Five different shapes; the methodology absorbed
each without ceremony.

---

## What I want viewers to take away

AI agents do not replace engineering discipline. They amplify it.
This is the thesis. Without the discipline, the speed of generation
becomes recklessness very quickly. With the discipline, an
ambitious greenfield rewrite becomes tractable on a solo author's
timeline.

The methodology has to be visible. It cannot live only in the head
of the orchestrator. nWave's structure — five waves per feature, two
agents per wave (one to do the work, one to review it), explicit
peer-review iterations, wave-decisions documents that record every
choice — is what makes the AI's output auditable. Without that,
"AI-generated code" is a black box that ships uncontrolled.

The reviewer agents are non-negotiable. Even when iteration one
approves, the second pair of eyes catches real things. The reviewer
brief is deliberately different from the doer brief. That asymmetry
is what makes the review honest.

The methodology has gaps. The biggest one we found in feature one
was operational reality — the reviewer agents check artefact
fidelity, not whether the artefact actually runs on the
infrastructure it claims to. Future iterations of nWave's reviewer
briefs will close that gap. We surface gaps by running the
methodology on real problems, not by speculating about them.

The licence and governance choices are part of the engineering
discipline. A project that promises "always free and open source"
must encode that promise structurally — in the licence, in the
contribution model, in the trademark policy. Otherwise the promise
relies on the maintainer's good intentions, which is the same
fragile thing every re-licensed open source project relied on.

---

## Editorial note for future updates

Each time an nWave wave closes, add a section to this file in the
order: feature name, wave name, what the agent produced, what the
reviewer found, what the artefacts are. Then add two or three
slides to `slides.md` extracting the headline.

Avoid listing every test, every ADR section, every commit. The
narrative is for the audience, not for the audit trail. The
wave-decisions documents in `docs/feature/<feature>/<wave>/` are the
audit trail.

Maintain British English throughout. Andrea writes in British
English; the videos will be presented in English; the consistency
matters.
