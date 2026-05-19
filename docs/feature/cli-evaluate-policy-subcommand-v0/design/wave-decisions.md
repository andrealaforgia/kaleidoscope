# DESIGN — cli-evaluate-policy-subcommand-v0

Author: orchestrator-direct (agent quota exhausted)
Date: 2026-05-19

## DD1 — Function shape

```rust
pub fn evaluate_policy(
    data_dir: &Path,
    hot_to_warm_secs: u64,
    warm_to_cold_secs: u64,
    mut writer: impl Write,
    otlp_log_path: Option<&Path>,
) -> Result<usize, Error>;
```

No tenant arg (per DISCUSS D5). Returns the count migrated; the
caller in main.rs emits the stdout line. Mirroring ingest's
"return stats, caller writes report" shape but with a single
value.

Actually correction: keep it consistent with the rest of the
file. The other subcommand functions write inside. So:

```rust
pub fn evaluate_policy(
    data_dir: &Path,
    hot_to_warm_secs: u64,
    warm_to_cold_secs: u64,
    mut writer: impl Write,
    otlp_log_path: Option<&Path>,
) -> Result<(), Error>;
```

Body builds `Duration::from_secs(hot_to_warm_secs)` +
`Duration::from_secs(warm_to_cold_secs)`, constructs
`TierPolicy::age_based(...)`, opens FileBackedTieringStore
(with NoopRecorder or CinderToOtlpJsonWriter per otlp_log_path),
calls `evaluate_at(SystemTime::now(), &policy)`, writes
`evaluated migrated=<N>\n` to writer.

## DD2 — Recorder construction

Same match arm as migrate/place. `Some(path)` →
OpenOptions+CinderToOtlpJsonWriter. `None` → NoopRecorder. The
bulk `evaluate_at` will internally call `record_migrate` for
each migrating entry; with CinderToOtlpJsonWriter, that means N
`cinder.migrate.count` lines per N internal migrations.

## DD3 — Duration parsing

`u64` positional args in main.rs. The boundary parser is
`str::parse::<u64>()` which rejects negative, non-numeric,
overflow. Errors lift into a new
`Error::InvalidDuration { value: String, secs_kind: &'static str }`
where `secs_kind` is `"hot_to_warm"` or `"warm_to_cold"` so the
stderr line names which arg was bad.

## DD4 — New Error variant

`Error::InvalidDuration { value: String, secs_kind: &'static str }`
is genuinely new. Display: `invalid duration "{value}" for
<secs_kind>_secs: expected non-negative integer seconds`. This
is the first new Error variant introduced since the migrate
feature added CinderMigrate and InvalidTier.

## DD5 — Reuse Analysis

| Construct | Source | Reuse |
|-----------|--------|-------|
| FileBackedTieringStore::open | cinder | REUSE |
| CinderRec / NoopRecorder | cinder | REUSE |
| CinderToOtlpJsonWriter | self_observe | REUSE |
| TierPolicy + age_based | cinder | REUSE (new use site) |
| TieringStore::evaluate_at | cinder | REUSE (new use site) |
| cinder_base | local | REUSE |
| Error::CinderOpen + From<io::Error> | local | REUSE |
| Error::InvalidDuration | local | CREATE NEW |

One new Error variant. No new public type beyond that. No new
external dependency.

## DD6 — Out of scope (DESIGN locks)

- No dry-run mode (Cinder API gives no preview hook).
- No per-tenant filter (DISCUSS D5).
- No floating-point durations (DISCUSS D6).
- No alternate policy shapes (TierPolicy::age_based only).

## No new ADR

Free-function pattern continues. The tenant-less shape is a
deviation from the CLI convention but not from the ADR-0001
library shape (which is "free functions returning Result<_,
Error>"). The convention drift is documented in DISCUSS D5; no
architectural recording warranted.

## DEVOPS handoff

- Paradigm: Rust idiomatic
- External integrations: NONE
- Dependency footprint: ZERO new external crates
- CI gates: 5 existing inherit; gate-5-mutants-kaleidoscope-cli
  auto-covers via --in-diff
- Workspace changes: one new [[test]] block
- Mutation scope: lib.rs (new evaluate_policy fn + new Error
  variant + Display) + main.rs (run_evaluate_policy +
  print_usage update), 100% kill rate target
