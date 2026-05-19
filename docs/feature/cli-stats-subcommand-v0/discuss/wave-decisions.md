# Wave Decisions — `cli-stats-subcommand-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus decisions
recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| `feature_type` | `backend` | CLI plumbing + library function inside `kaleidoscope_cli`. No new UI; adds a third subcommand alongside `ingest` and `read`. |
| `walking_skeleton` | `no` | The CLI exists, `kaleidoscope-cli` already dispatches two subcommands, the `LogStore::query(tenant, TimeRange::all())` API already exists (`crates/lumen/src/store.rs:84`), and `LogRecord::observed_time_unix_nano` is the canonical sort key (`crates/lumen/src/record.rs:48`). This feature is a thin extension that adds one subcommand on top of an existing CLI substrate. |
| `research_depth` | `lightweight` | Single operator persona (Priya, inherited from the three prior `--observe-otlp` features). Single decision enabled per invocation ("is there data for this tenant, and across what time window?"). Output shape collapsed by precedent: stdout key=value lines, mirroring the `ingest ok: records=N batches=M tier_items=K` line `ingest` already writes to stderr (`crates/kaleidoscope-cli/src/main.rs:111-114`). |
| `jtbd_analysis` | `no` | The job is obvious and singular: an operator wants to inspect what is in a tenant's data directory without dumping every record through stdout. Persona inherited; forces are direct mirror-images of the operator's existing post-ingest smoke-test workflow. DIVERGE artifacts absent; absence recorded as a risk below. |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. The job statement is implicit and singular: an operator wants record count + earliest/latest timestamp for a tenant, in stdout, without dumping the whole record set. | DIVERGE skipped by Andrea's explicit instruction. The output shape (stdout key=value lines, one per stat) has exactly one reasonable shape that operators can `grep` and pipe to `awk` or `cut`. |
| No formal JTBD workshop | LOW. Persona, push (operator runs `read \| wc -l` today), pull (one-shot stats), anxiety (no risk of mutating data — pure query), habit (operator already runs `kaleidoscope-cli read ...` after ingests). | Persona + emotional-arc inherited from the four `--observe-otlp` reference features under `docs/feature/cli-*/discuss/`. |
| No standalone Three Amigos session | LOW. Reviewer pass at handoff time replaces the workshop. The shape is doubly constrained: by the existing positional argument convention (`<tenant> <data_dir>`) and by the existing `LogStore::query(tenant, TimeRange::all())` API that already returns the record set needed to compute count + min/max. | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D1: Scope is a new `stats` subcommand on `kaleidoscope-cli`

The change adds a third subcommand alongside `ingest` and `read`:

```text
kaleidoscope-cli stats <tenant_id> <data_dir>
```

Output goes to **stdout** (unlike `ingest`, which writes stats to
stderr; `stats` is the entire point of this subcommand so stdout is
correct — operators pipe stdout to `grep`, `awk`, `jq`, etc., and
machine-parsing the principal output of a subcommand off stderr is
unidiomatic). Output is simple key=value lines, one per stat, in a
deterministic order:

```text
records=N
earliest=<ISO8601>
latest=<ISO8601>
```

Empty-tenant case: see D5. ISO8601 format: see D6.

### D2: Out of scope — Cinder stats

Cinder is not consulted by this subcommand. The `LogStore` adapter
already exposes `query(tenant, TimeRange::all())` which is sufficient
to compute the v0 record count and the v0 time range from the Lumen
side. No Cinder-tier counts, no Cinder-place counts, no Cinder-hot/warm
distribution. A future `cli-stats-cinder-v0` feature can add per-tier
counts if the operator demand surfaces.

### D3: Out of scope — `--observe-otlp` wiring on `stats`

The `--observe-otlp` flag is NOT accepted by the `stats` subcommand in
this feature. The `stats()` call will internally drive one
`LogStore::query(...)` call (which produces a `record_query` metric
event if the recorder were wired to the OTLP writer); operators who
want OTLP visibility of stats queries get it for free if a follow-up
feature wires `--observe-otlp` here. In v0, `stats` constructs a
quiescent recorder (the same `LumenToPulseRecorder` over an in-process
Pulse store that `read` uses in its no-flag path,
`crates/kaleidoscope-cli/src/lib.rs:275-279`) so that no OTLP file is
created and the subcommand's only observable side effect is the
records it writes to stdout.

### D4: Out of scope — JSON/CSV output formats

No `--json`, no `--csv`, no `--format=...`. v0 ships exactly the
key=value text shape described in D1. Operators who want JSON can
construct it from the key=value lines with one `awk` invocation. The
machine-parseable contract becomes a v1 concern once the v0 output
shape proves it is the right thing to make machine-parseable.

### D5: Empty-tenant case — `records=0` and OMIT timestamp lines

When the tenant has zero records in `data_dir` (either the tenant has
never been ingested, or every record has been deleted, or the
`data_dir` is empty), the subcommand prints exactly one line:

```text
records=0
```

…and does NOT print `earliest=` or `latest=` lines. Rationale:

- The "earliest/latest of an empty set" is mathematically undefined;
  any sentinel value (`<none>`, `null`, `-`) invites operators to
  parse it as a real timestamp and silently get the wrong answer.
- Operators can disambiguate the empty case from the populated case by
  asserting `records=0` (or, more directly, by piping through `wc -l`
  on the stats output: 1 line = empty, 3 lines = populated).
- `grep` / `awk` over key=value lines naturally handles "key absent"
  as a falsy condition.
- Operationally, the empty case is the second-most-common case for a
  fresh tenant or a wrong `data_dir` typo; surfacing it as a
  one-line, unambiguous answer is the most valuable behaviour.

This is the cleaner of the two options the task brief offered. The
alternative (`earliest=<none>` / `latest=<none>`) is documented in the
slice brief as the rejected option.

Exit code is `0` for the empty case (it is not an error to query a
tenant with no data).

### D6: Timestamp format — ISO 8601 with `Z` (UTC) suffix, nanosecond precision

The `LogRecord::observed_time_unix_nano: u64` field
(`crates/lumen/src/record.rs:48`) stores nanoseconds since the Unix
epoch. The stats output renders timestamps as ISO 8601 UTC, e.g.:

```text
earliest=2026-05-18T08:23:04.123456789Z
latest=2026-05-19T17:42:11.987654321Z
```

Rationale:

- ISO 8601 with `Z` suffix is unambiguously UTC, parseable by every
  standard datetime library, and stable under lexicographic sort
  (so `sort` on the output preserves chronological order).
- Nanosecond precision is preserved because Kaleidoscope's
  log-record substrate stores nanoseconds and operators correlating
  events across systems may need sub-microsecond resolution.
- The exact formatting library choice (e.g. `chrono`, `time`,
  hand-rolled) is a DESIGN-wave concern. The wire-observable
  contract is the ISO 8601 UTC string with trailing `Z` and
  nanosecond precision.

If, during DESIGN, the chosen formatting library cannot preserve
nanoseconds without external dependencies the workspace doesn't
already pull in, DESIGN may downgrade to microsecond precision and
record the choice as an ADR addendum. The OK2 KPI is robust to that
downgrade as long as min and max remain in `min <= max` order and are
consistent with what the underlying records contain.

### D7: Out of scope — sorting, filtering, multi-tenant aggregates

- No `--sort-by-time`, `--severity-min=`, `--since=`, `--until=`.
  The whole point of v0 is the count + time range over the full
  record set; filtering belongs in a later feature.
- No `kaleidoscope-cli stats <data_dir>` (all-tenants form). v0
  requires a positional `<tenant_id>` exactly the way `ingest` and
  `read` do.
- No `--per-day`, `--per-hour` histograms. A histogram is a different
  feature, with a different output shape, and a different operator
  decision (capacity planning at higher granularity, vs the v0
  "first/last write" audit answer).

### D8: Library function shape — NAMED but NOT designed

Per the task brief: the library function is named
`kaleidoscope_cli::stats(tenant, data_dir, writer)` returning
`Result<(usize, Option<(SystemTime, SystemTime)>), Error>` or a
similar shape. The exact signature is locked by DESIGN
(`nw-solution-architect`). The wire-observable behaviour is what
matters for the DISCUSS wave's acceptance criteria; the signature is a
DESIGN concern.

A reasonable expectation (not a constraint on DESIGN): the function
internally constructs `FileBackedLogStore::open(lumen_base(data_dir),
recorder)` using the same `lumen_base` helper `read()` already uses
(`crates/kaleidoscope-cli/src/lib.rs:118-120`), calls
`lumen.query(tenant, TimeRange::all())`, computes
`records.len()` for the count and iterates once for min/max
`observed_time_unix_nano`, then formats and writes the key=value lines
to the supplied `writer`. The recorder construction can be the same
quiescent `LumenToPulseRecorder` pattern `read` uses in its no-flag
arm.

### D9: Acceptance test file location

New file:
`crates/kaleidoscope-cli/tests/stats_subcommand.rs`. Mirrors the
harness pattern in `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs`
and `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` (per the
predecessor feature). The `tenant`, `record`, `temp_root`, `cleanup`
helpers are duplicated inline at v0 — the rule-of-three extraction was
already deferred in `cli-read-observe-otlp-v0/discuss/wave-decisions.md`
D6 and the extraction remains a separate refactoring task.

### D10: SSOT journey and `jobs.yaml` are NOT modified in this wave

Same posture as the four reference features. The SSOT
operator-incident-response journey is incident-time focused; this
inspection subcommand serves the orthogonal "operator confirms data
landed and audits time window" workflow, which is operationally
nice-to-have but does not rise to the level of an SSOT journey
modification. The feature-local artefacts produced in this wave are
NOT promoted to `docs/product/journeys/` or `docs/product/jobs.yaml`.

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 1 story, 1 bounded context (`kaleidoscope-cli` crate), 1
modified file (`src/main.rs` — adds a third subcommand dispatch arm
and updates `print_usage`), 1 modified file (`src/lib.rs` — adds the
new `stats()` library function), 1 new test file
(`tests/stats_subcommand.rs`), 1 manifest line-level change
(`Cargo.toml`). Estimated effort: well under 1 day. PASSES the
right-sized gate. Strictly smaller than `cli-read-observe-otlp-v0`
because there is no OTLP wiring at all (no `--observe-otlp` flag, no
recorder branching) and no cross-subcommand symmetry KPI.

## Handoff

Next wave: DESIGN (`nw-solution-architect`). Inputs delivered:

- `user-stories.md`
- `outcome-kpis.md`
- `story-map.md`
- `slices/slice-01-stats-subcommand-emits-record-count-and-time-range.md`
- `dor-validation.md`
- `wave-decisions.md`

DESIGN wave's principal decision: lock the exact signature of
`kaleidoscope_cli::stats`. The most-likely-correct shape, per the task
brief, is `stats(tenant: &TenantId, data_dir: &Path, writer: impl
Write) -> Result<(usize, Option<(SystemTime, SystemTime)>), Error>` or
a structurally similar function returning `Result<StatsReport,
Error>`. Any signature that lets the `main.rs` `run_stats` dispatcher
forward stdout into the function and that produces the wire-observable
output described in OK1, OK2, OK3 is acceptable.
