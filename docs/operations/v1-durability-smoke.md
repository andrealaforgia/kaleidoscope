# v1 durability smoke

A short operator-facing shell recipe that proves "v1 means
restart-safe" on disk, not just in the test suite. Run it
after building the release binary; if every phase passes, the
file-backed adapters behind `kaleidoscope-cli ingest` /
`read` / `compact` have done their job.

## What it does

The script `scripts/v1-durability-smoke.sh` walks the binary
through five phases against a single tenant (`acme`):

1. **Ingest** two NDJSON records.
2. **Read** them back from the same process state.
3. **Re-read** — a second invocation against the same data
   directory, which means a fresh `FileBackedLogStore::open`
   that has to replay the WAL.
4. **Compact** — `snapshot()` on both Lumen and Cinder.
   Verifies the WAL files truncate to zero bytes.
5. **Read again** — final invocation, now driven entirely
   from the snapshot (the WAL is empty).

If every phase returns the expected record count and the WAL
sizes drop to zero after compact, the script exits 0. Any
deviation exits 1 with a clear message naming the failed
phase.

## Why an operator script as well as a test suite

`cargo test -p kaleidoscope-cli` already exercises every
phase via library functions. The test suite is the canonical
proof — it runs in CI, and any regression breaks `main`.

This script is for the human who clones the repo for the
first time and wants to see the v1 property happen on their
own filesystem with the actual binary, not the test-runner
process. It is also the thing to point at when someone asks
"how do I know your durability claim is real?" — the answer
is "run this script and watch the WAL truncate".

The script also serves as documentation by example for the
operator-facing surface: every flag, every subcommand, every
typical workflow appears in one script that is short enough
to read in a single sitting.

## Usage

```bash
# Build the release binary first.
cargo build --release -p kaleidoscope-cli

# Run the smoke. Optional argument is the data directory;
# default is /tmp/kal-durability-smoke-<pid>.
scripts/v1-durability-smoke.sh

# Or pick your own directory so you can inspect the files
# after the script finishes:
scripts/v1-durability-smoke.sh /tmp/my-kal-test
ls /tmp/my-kal-test/   # lumen.wal, lumen.snapshot, cinder.wal, cinder.snapshot
```

## What "v1" guarantees

The six file-backed adapters that the script exercises
indirectly (via the CLI, which currently wires Lumen and
Cinder; Sluice is in the in-process path; Pulse, Ray, Strata
have their own v1 but are not yet on the CLI surface) all
share the same durability contract:

- Every `ingest` call appends one NDJSON line to a WAL file,
  flushed before the call returns. A crash between two
  ingests loses no committed data.
- `snapshot()` writes a JSON dump of the current state,
  flushes, then truncates the WAL. The next `open()` loads
  the snapshot then replays any subsequent WAL entries.
- Tenant identity (`aegis::TenantId`) is the partition key;
  one tenant's `ingest` cannot leak into another tenant's
  `read`.

The cross-crate integration tests
`v1_six_adapters_compose_under_restart` and
`v1_three_adapters_compose_under_restart` are the formal
proofs. This script is the operator-facing demonstration.

## What's NOT yet exercised by this script

`kaleidoscope-cli` does not yet have subcommands for the
other three durable adapters (Pulse v1, Ray v1, Strata v1).
Their v1 acceptance suites are in their own crates
(`crates/pulse/tests/v1_file_backed_metric_store.rs`,
`crates/ray/tests/v1_file_backed_trace_store.rs`,
`crates/strata/tests/v1_file_backed_profile_store.rs`); when
the CLI grows `ingest-metrics`, `ingest-spans`, and
`ingest-profiles` subcommands, this script will extend to
cover them too.
