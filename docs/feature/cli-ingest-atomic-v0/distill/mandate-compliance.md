# Mandate Compliance — `cli-ingest-atomic-v0` / DISTILL

Evidence that all four acceptance-design mandates pass, for the handoff to
DELIVER. (This project's acceptance idiom is Rust `#[test]` functions with
`// Given / // When / // Then` comment blocks, not Gherkin `.feature` files,
per US-01 System Constraints. The mandates apply to the test functions.)

## CM-A — Hexagonal boundary (driving port only)

`tests/ingest_atomic.rs` imports ONLY the driving port and domain types:

```
use aegis::TenantId;
use kaleidoscope_cli::{ingest, read, DEFAULT_BATCH_SIZE};
use lumen::{LogRecord, SeverityNumber, TimeRange};
```

- `kaleidoscope_cli::ingest` / `read` — the CLI driving port (entry point).
- `kaleidoscope_cli::{Error, IngestStats}` — public result/error types.
- `aegis::TenantId`, `lumen::{LogRecord, SeverityNumber, TimeRange}` —
  domain value types used to build inputs and express the read range.

Zero internal-component imports: NO `lumen::FileBackedLogStore`, NO
`cinder::FileBackedTieringStore`, NO `flush`, NO private helper. The store
is exercised INDIRECTLY through `ingest`/`read` exactly as an operator
reaches it. No Testing Theater. **PASS.**

## CM-B — Business language abstraction

The Given/When/Then comment blocks speak the operator's language: "a fresh
empty store", "Priya ingests the malformed file", "the store committed
NOTHING", "re-running a still-malformed input does not double-count". The
assertions check operator-observable facts — the typed error naming the bad
line, and the committed store COUNT read back through the read surface — not
internal state. A grep for `http|json payload|status code|database|REST|
controller|404|500` in the test logic returns only a URL in the licence
header and the phrase "safety net" — no technical jargon in any
Given/When/Then or assertion. **PASS.**

## CM-C — User journey completeness

Every test traces a complete operator journey with observable value, not an
isolated technical operation:

- AC1: operator ingests bad file -> sees it fail naming the line -> store
  count unchanged (the "is my store dirty?" decision).
- AC2: operator re-runs the bad file -> still fails, count still unchanged
  (the "can I just re-run this?" decision is safe).
- AC3: operator fixes the named line -> corrected file ingests exactly once
  (the "is it fully in now?" decision).
- AC4: operator ingests a clean file -> every record committed once, no
  regression.
- AC5: operator ingests a file whose first line is bad -> fails naming line
  1, nothing committed.

Each asserts a return value from the driving port (`Err(ParseRecord{line})`
/ `Ok(IngestStats{...})`) and/or the observable committed COUNT — never a
private field or a method-call count (Dimension 7 satisfied). **PASS.**

## CM-D — Pure function extraction before fixtures

There is NO fixture matrix to parametrise: the single real-adapter path
(real `FileBackedLogStore` + `FileBackedTieringStore` on a tmp dir) is the
only path, and it is parametrised by nothing (no env var, no external
service, no environment variant — DEVOPS environments are `clean` + `ci`
running the identical command). The all-or-nothing COMMIT contract is
inherently a property of the I/O boundary, so it is correctly tested through
the orchestration end-to-end rather than as an extracted pure function;
ADR-0064 deliberately keeps `ingest` as a single re-ordered free function
and introduces no new seam. Mandate vacuously satisfied — nothing to push
down. **PASS.**

## Walking-skeleton boundary (Dimensions 5 & 9)

This is a SLIM correctness wave on an already-shipped driving port, not a
greenfield feature — there is no NEW walking skeleton and no NEW driven
adapter. Every test runs against the REAL file-backed adapters with real
local I/O (`@real-io`), so the "if I deleted the real adapter, would this
still pass?" litmus FAILS (it could not pass — the count-readback needs the
real store). No `@in-memory` anywhere on the path. Adapter-integration
coverage is satisfied by the real-I/O store in all five tests. **PASS.**

## Error-path ratio (Dimension 1)

4 of 5 scenarios exercise the parse-error / failed-recovery path = 80% (>=
40% target). Proportionate for a single-failure-mode hardening wave. **PASS.**
