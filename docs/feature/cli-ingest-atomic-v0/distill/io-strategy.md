# I/O Strategy — `cli-ingest-atomic-v0` / DISTILL

## Strategy: real local I/O, in-process driving port (@real-io)

| Aspect | Choice | Rationale |
|--------|--------|-----------|
| Driving port | `kaleidoscope_cli::ingest(...)` library call with a `Cursor`-backed reader | In-process equivalent of `kaleidoscope-cli ingest <tenant> <data_dir>` with stdin pipe (ADR-0064 DD-6); deterministic, no subprocess |
| Store under test | REAL `FileBackedLogStore` (Lumen) + REAL `FileBackedTieringStore` (Cinder) on a per-test tmp `data_dir` | The commit discipline being tested is exactly the on-disk commit; an InMemory double would hide it |
| Count read-back | `read(tenant, data_dir, sink, None, TimeRange::all())` against the SAME `data_dir` | Shipped read surface, in-process equivalent of `kaleidoscope-cli read`/`stats records=N`; no private-file inspection |
| Isolation | unique tmp dir per test (`temp_data_dir(name)` = `temp_dir()/kal-cli-<name>-<pid>-<nanos>`), `cleanup` at end | No cross-test bleed; matches the existing roundtrip harness exactly |
| Observables | typed `Result` + committed count | The two operator-visible facts: did it fail (and name the line), and what is the store count after |

## No subprocess, no signals, no timing

Explicitly EXCLUDED (per DEVOPS A2):

- **No subprocess** — the binary is exercised through its library, not by
  spawning a process and piping stdin. Simpler and deterministic.
- **No signals** — unlike the beacon SIGHUP tests, there is no signal
  handling on this path.
- **No wall-clock / p95 / sleep / concurrency** — there is no timing
  assertion anywhere, so this sidesteps the overnight p95 flake class
  (`project_p95_wallclock_flakes_overnight`) entirely.

## Determinism

Every assertion is a typed `Result` value plus a committed-state COUNT read
back. The same `cargo test --workspace --all-targets --locked` invocation
runs the suite identically in the local pre-commit hook (Step 4) and in CI
gate-1-test. Fully deterministic, no flake surface.

## Environments (DEVOPS environments.yaml: `clean` + `ci`)

The slim DEVOPS environment set is `clean` (local) and `ci`. Both run the
identical `cargo test --workspace --all-targets --locked` command against
the same real file-backed adapters on a tmp dir; there is no
environment-specific precondition to parametrise (no env var, no external
service, no deploy target). Mandate 4 (pure-function-extraction before
fixtures) is satisfied trivially: there is no fixture matrix — the single
real-adapter path is the only path, parametrised by nothing.

## Mandate 4 note (pure function extraction)

The behaviour under test (validate-all-then-commit ordering) is a property
of the `ingest` orchestration, not extractable as a standalone pure function
without inventing a seam DESIGN did not call for (ADR-0064 keeps it as a
single re-ordered free function). The acceptance tests therefore exercise
the orchestration end-to-end through the driving port with the real adapter
— the correct level for an all-or-nothing COMMIT contract, which is
inherently about the I/O boundary. No fixture parametrisation exists to
push down; the mandate is vacuously satisfied.
