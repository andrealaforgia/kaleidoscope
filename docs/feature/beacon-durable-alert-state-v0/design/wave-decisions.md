# Design Wave Decisions — beacon-durable-alert-state-v0

DESIGN wave (nw-solution-architect / Morgan). Interaction mode:
**propose**. Backend feature, application scope, Kaleidoscope Rust
workspace. British English, no em dashes.

This extends the durable-adapter pattern (trait + InMemory v0 +
FileBacked v1 with WAL + snapshot + recovery) established by the six
storage pillars (cinder, sluice, lumen, pulse, ray, strata) to
beacon's per-rule alert state, WITHOUT violating ADR-0037 (beacon's
`transition` stays a pure, total, side-effect-free function). The
state-holding gains durability; the transition logic does not move.

## Mandatory reads checklist

- [x] `crates/beacon/src/lib.rs` — module layout, public surface.
- [x] `crates/beacon/src/state_machine.rs` — `RuleState`, `transition`.
- [x] `crates/beacon/src/types.rs` — `Rule.name` identity (~line 63).
- [x] `crates/beacon-server/src/main.rs` — `run_rule` (~line 146).
- [x] `crates/beacon-server/src/lib.rs` — orchestrator primitives.
- [x] `crates/strata/src/store.rs` — trait + InMemory shape precedent.
- [x] `crates/strata/src/file_backed.rs` — FileBacked WAL+snapshot+recovery.
- [x] `docs/feature/.../discuss/wave-decisions.md`, `user-stories.md`,
      `outcome-kpis.md`, story-map and journey artifacts.
- [x] `docs/product/architecture/adr-0037*` — pure-transition mandate.
- [x] Scanned `docs/product/architecture/` — highest ADR is **0039**,
      next free number is **0040**.

## Resolved serialisation risk (confirm)

The DISCUSS wave flagged "RuleState not currently Serialize/Deserialize"
as a Medium risk and raised a `SystemTime` vs `Instant` concern. Both
are now **RESOLVED and confirmed against the source**:

- `RuleState` (`crates/beacon/src/state_machine.rs`, lines 40-54) is
  `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` with variants
  `Inactive`, `Pending { since: SystemTime }`, `Firing { since:
  SystemTime }`. It uses `SystemTime`, **not** `Instant`.
- `serde` serialises `SystemTime` natively (as a duration since
  `UNIX_EPOCH`). A **plain** `#[derive(Serialize, Deserialize)]` on
  `RuleState` therefore round-trips faithfully. No custom time
  conversion, no `Instant`-to-wall-clock bridge, no newtype wrapper is
  needed. The `Instant` problem that would have forced a custom
  conversion **does not exist here.**

## Reuse Analysis (MANDATORY)

The orchestrator's instruction names strata; the DISCUSS docs name
lumen. Both are pillars of the identical FileBacked shape. The concrete
precedent read for this design is `strata`; lumen is its equivalent.

| Candidate | Location | Verdict | Rationale |
|-----------|----------|---------|-----------|
| `RuleStateStore` (the new port) | does not exist | **CREATE NEW** | State is a local `let mut state` in `run_rule` today; there is no store to extend. Justified create-new for the port itself. |
| WAL + snapshot + recovery machinery | `crates/strata/src/file_backed.rs` (and 5 sibling pillars) | **EXTEND PATTERN (reuse shape)** | `open()` recovers snapshot then replays WAL; `snapshot()` truncates WAL; append-on-write; additive `PersistenceFailed` error variant. We reuse this proven shape verbatim, changing only the replay semantics (keyed-latest-wins, see DD4). We do NOT duplicate a specific pillar; we apply the established pattern. |
| `PersistenceFailed { reason: String }` error variant | every pillar's `*StoreError` | **REUSE PATTERN** | Mirror the additive single-variant shape the in-memory adapter never returns (DD6). |
| In-memory test seam | `InMemoryProfileStore` (and siblings) | **REUSE PATTERN** | `InMemoryRuleStateStore` mirrors the v0 seam: behaviour-preserving, loses state on restart, used as the fast unit-test double. |
| pure `transition` | `crates/beacon/src/state_machine.rs` | **DO NOT TOUCH** | ADR-0037. The store sits beside it; no transition logic moves into the store. |

No existing rule-state store exists, so the port is a justified
CREATE NEW; the durability internals are a REUSE of the established
pillar pattern.

## Decisions (DD1..DD8)

### DD1 — where the `RuleStateStore` trait lives

A **new module `state_store` inside the `beacon` crate**, beside the
pure `transition` in `state_machine`. Confirmed current beacon module
layout (`crates/beacon/src/lib.rs`): `inhibition`, `loader`, `sinks`,
`slo`, `state_machine`, `types`. Add `pub mod state_store;` and re-export
the trait, adapters, and error from `lib.rs` alongside the existing
`pub use crate::state_machine::{...}`.

Rationale: the store holds `RuleState`, a beacon domain type, so it
belongs in the beacon library, not beacon-server (the binary owns
wiring, not the port). It contains **no transition logic** — it only
loads, persists and recovers values — so ADR-0037 is preserved exactly
as the storage pillars preserve their record/predicate purity. Name
`state_store` (not bare `store`) because beacon already has several
single-word modules and `state_store` reads unambiguously next to
`state_machine`.

### DD2 — store key and map shape

Key directly on **`String`** (the rule name), not a `RuleId` newtype.
`beacon::Rule.name` (`types.rs` line 65: `pub name: String`) is already
the stable rule identity used everywhere (sink construction, inhibition
resolver, logging). Map shape:

```text
HashMap<String, RuleState>
```

Rationale: a `RuleId(String)` newtype would earn its place only if there
were a competing string key to disambiguate, or a validation invariant
to enforce at construction. There is neither — the rule name is the one
identity, already a `String`, already used as a map key by the
`InhibitionResolver`. Per principle 8 (simplest solution first) and the
orchestrator's "recommend the lighter option unless a newtype earns its
place", we key on `String`. (The C4 and ADR write `RuleId` only as a
documentation alias for "the rule name string".)

### DD3 — the trait API

Pinned by reading how `run_rule` uses the state. Today (`main.rs` lines
146, 168-173): seed once with `RuleState::Inactive`, then each cycle
read `state`, compute `(next, emission)`, and on `state != next` persist
the new value. The trait mirrors that exactly:

```text
pub trait RuleStateStore: Send + Sync {
    /// Recover every persisted rule state at startup. Called once by
    /// beacon-server before spawning per-rule loops. The InMemory
    /// adapter returns an empty map; the FileBacked adapter returns the
    /// recovered map (snapshot + WAL replay, keyed-latest-wins).
    fn load_all(&self) -> Result<HashMap<String, RuleState>, RuleStateStoreError>;

    /// Persist the latest state for one rule. Called by the per-rule
    /// loop only when state != next (latest-wins, DD4). The InMemory
    /// adapter updates its map; the FileBacked adapter appends a
    /// WalRecord::Put and updates its map.
    fn put(&self, rule_id: &str, state: RuleState) -> Result<(), RuleStateStoreError>;
}
```

`load_all` is the recovery entry point (called once at startup, returns
the whole map for seeding); `put` is the per-transition upsert. A
per-key `get(&rule_id)` is deliberately **not** in the trait: the
orchestrator recovers the full map once at startup and then holds each
rule's current state in its loop, so a hot-path `get` would be dead
surface. Keeping the trait at two methods is the minimum that satisfies
US-01/02/03 and matches the storage pillars' "one recover, one write"
posture.

### DD4 — KEYED-LATEST-WINS semantics (the important contrast)

This is the load-bearing difference from the storage pillars and the
reason an ADR is warranted.

- **Storage pillars (cinder..strata): append-and-sort log.** Each WAL
  record is an *event in a time series*. Recovery replays every record
  and **re-sorts** each bucket on `time_unix_nano` (see
  `file_backed.rs` lines 132-135). The value is one of many ordered
  events; order is part of the contract.
- **RuleStateStore: keyed-latest-wins map.** Each WAL record is
  `WalRecord::Put { rule_id, state }` with `#[serde(tag = "op")]`. The
  value **is** the current state, not an event in a series. Recovery
  replays Put records **in file order**, and for each `rule_id` the
  **last Put seen wins**; earlier Puts for the same key are simply
  overwritten. The snapshot is just the current `HashMap<String,
  RuleState>`. **There is no sort step and no time-ordering of values**,
  because a rule has exactly one current state, not a history.

This contrast is documented explicitly in the ADR and in
`application-architecture.md`. Implementation reuses the pillar's
`open`/`snapshot`/append skeleton but replaces "push then sort" with
"insert overwrite".

### DD5 — the two adapters

- **`InMemoryRuleStateStore`** (v0 test seam): a
  `Mutex<HashMap<String, RuleState>>`. `load_all` returns a clone of the
  map (empty on a fresh process, so restart still loses state — exactly
  US-01 scenario 3). `put` inserts/overwrites. Never returns
  `PersistenceFailed`. Mirrors `InMemoryProfileStore`.
- **`FileBackedRuleStateStore`** (v1 durable adapter): mirrors
  `strata::FileBackedProfileStore`. WAL is NDJSON (one
  `WalRecord::Put` per line, base path + `.wal`); snapshot is a single
  JSON object (base path + `.snapshot`) holding the current map.
  `open()` loads the snapshot then replays the WAL with
  **keyed-latest-wins** (DD4) — no sort. `snapshot()` flushes and
  truncates the WAL. `put()` appends one NDJSON line and updates the
  in-memory map.

### DD6 — error type

```text
pub enum RuleStateStoreError {
    PersistenceFailed { reason: String },
}
```

A single additive variant, `Display` + `std::error::Error`, mirroring
every pillar's `*StoreError`. The in-memory adapter never returns it; it
exists for the FileBacked adapter's WAL/snapshot IO and parse failures.
Corrupt-on-startup (US-02 scenario 3) surfaces as
`PersistenceFailed { reason: ... }` from `open()` so beacon-server can
refuse to start rather than silently reset (Earned Trust: a lying or
truncated state file is a probed failure, not an assumption).

### DD7 — serde derives on RuleState

Add **plain** `#[derive(Serialize, Deserialize)]` to `RuleState` in
`state_machine.rs`. Nothing else needs deriving: `SystemTime` serialises
natively (DD7 confirm above), the enum is `Copy`, and the variants carry
only `SystemTime`. `QueryOutcome` and `Emission` are **not** persisted
and need no derives. The derive is purely additive to the public
surface and does not alter `transition` (ADR-0037 untouched).

### DD8 — beacon-server wiring (the exact point)

Three edits, all in `crates/beacon-server`:

1. **Open the store at startup** in `main.rs`, after rules are loaded
   and validated (after line 81, the `has_any_rules` guard) and before
   the per-rule spawn loop (line 104). Construct the
   `FileBackedRuleStateStore` via `open(path)`, where `path` is
   **derived** from the existing rules-directory location (no new CLI
   flag — the no-new-CLI-surface constraint holds). On
   `Err(PersistenceFailed)`, log a clear operator-facing error and
   return a non-zero `ExitCode` (a new code, e.g. `5`) — never start
   with silently reset state (US-02 scenario 3). Wrap in `Arc` and share
   across tasks like `backend`/`resolver`.
2. **Recover once and seed per rule.** Call `store.load_all()` once,
   logging the recovery line `recovered alert state rules_recovered=N
   firing=F pending=P` (US-02 elevator pitch). Drop any recovered key
   whose rule no longer exists in `outcome.rules` and log the dropped
   name (US-02 scenario 4). Pass each rule's recovered state (default
   `RuleState::Inactive` if absent) into `run_rule`.
3. **Replace the local seed and persist on transition.** In `run_rule`
   (`main.rs` line 146), replace `let mut state = RuleState::Inactive;`
   with `let mut state = recovered_state;` (the seeded value). After
   `state = next;` (line 172), when `state != next`, call
   `store.put(&rule.name, next)` and on `Err` log a warn-level
   persistence failure (do not kill the rule loop — a transient WAL
   write failure should degrade to in-memory, not silence the alert).

The pure `transition`/`evaluate_once` call (line 168) is unchanged.

## Quality attributes (ISO 25010)

- **Reliability / recoverability (primary)**: the whole feature. WAL +
  snapshot + recovery; corrupt state refuses startup rather than
  silently resetting (US-02 sc.3). KPI 1 (100% state recovery) and
  KPI 2 (0 spurious re-fires) are the north star.
- **Performance efficiency**: persist p95 <= 2 ms, recover p95 <= 1.5 s
  on ubuntu-latest (KPI 3/4). Keyed-latest-wins recovery is O(records)
  with no sort, lighter than the pillars' append-and-sort.
- **Maintainability / testability**: ports-and-adapters — the
  `InMemoryRuleStateStore` is the fast unit double; the FileBacked
  adapter is integration-tested with real files. `transition` stays
  pure and property-testable (ADR-0037 guardrail).
- **Security**: no new external surface, no secrets in the store, no
  network. The WAL/snapshot files inherit the operator's filesystem
  permissions. No threat-model expansion.

## Earned Trust — probe responsibility (principle 12)

`FileBackedRuleStateStore` depends on the filesystem, which can lie
(Docker overlayfs `fsync` no-op, full disk, truncated writes). The
DESIGN mandates that the durable adapter demonstrate it can honour its
contract:

- `open()` recovering corrupt/truncated state returns
  `PersistenceFailed`, exercised by US-02 scenario 3 (the catalogued
  substrate lie: a snapshot truncated by a full disk).
- The composition root (beacon-server startup, DD8 step 1) follows
  **recover-then-refuse**: a failed `open()` aborts startup with a
  structured error and non-zero exit, never a silent reset. This is the
  beacon analogue of "wire then probe then use".
- The acceptance suite (DISTILL wave) catalogues the substrate lies
  (corrupt snapshot, truncated WAL line, future-dated `since`) as gold
  tests, mirroring the pillars' inline mutation-coverage tests.

This is a handoff note to acceptance-designer and software-crafter, not
a new probe API on the trait (the trait stays two methods, DD3).

## Handoff annotations

### To DISTILL (acceptance-designer)

- Behavioural ACs already enumerated in `user-stories.md`
  (US-01/02/03). The DESIGN adds no new operator surface, so the
  scenarios stand. Translate the three substrate-lie boundaries
  (corrupt snapshot, future-dated `since`, rule-removed-from-config)
  into gold acceptance tests.
- No external integrations in this feature (no third-party APIs, no
  network). **No contract tests apply.**

### To DEVOPS (platform-architect / Apex)

- **`gate-5-mutants-beacon` expectation.** `grep -c
  "gate-5-mutants-beacon" .github/workflows/ci.yml` = **0** today:
  beacon is **not** mutation-gated. This feature gives beacon **real
  durable logic** — a `FileBackedRuleStateStore` with WAL replay,
  keyed-latest-wins recovery, snapshot truncation, and an error path
  that refuses startup. That logic is exactly the kind ADR-0005 Gate 5
  (100% kill rate) exists to protect. We **annotate the expectation**
  that a `gate-5-mutants-beacon` CI job is now warranted, scoped to the
  new `state_store` / FileBacked source. The decision is **Apex's** in
  DEVOPS; this is a flag, not a mandate.
- **Latency budgets** to wire as regression signals: persist p95 > 2 ms
  or recover p95 > 1.5 s on ubuntu-latest (KPI 3/4).
- **Startup recovery log line** (`recovered alert state ...`) is a new
  operational signal worth surfacing.

## ADR verdict

**Write one: ADR-0040.** Two things justify it: (a) a persistence seam
is introduced into a crate that is pure-by-design (ADR-0037), and the
ADR records *why that does not violate the mandate*; (b) the
**keyed-latest-wins** WAL replay contract differs from every other
durable adapter in the platform (all six pillars are append-and-sort),
and a future reader copying a pillar's recovery code into beacon would
introduce a latent ordering bug. The ADR pins both. Written to
`docs/product/architecture/adr-0040-beacon-rule-state-store-seam.md`.

## brief.md

Not extended. `docs/product/architecture/brief.md` carries
platform-wide cross-cutting decisions; this is a per-feature seam that
reuses an already-recorded platform pattern (the FileBacked pillar
shape) and is fully captured by ADR-0040 plus this design wave. Adding a
feature-scoped section to the platform brief would dilute it. (Verified:
proportionate to the feature, per the brief.)
