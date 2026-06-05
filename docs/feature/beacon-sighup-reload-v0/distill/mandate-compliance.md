# Mandate Compliance — beacon-sighup-reload-v0 (DISTILL)

British English throughout, no em dashes.

Evidence that the four acceptance-design mandates hold for
`crates/beacon-server/tests/sighup_reload.rs`.

## CM-A — Hexagonal boundary (driving port only)

The test imports NO internal beacon component (loader, state machine,
inhibition resolver, store). Its only imports are:

```
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use rustix::process::{kill_process, Pid, Signal};   // safe POSIX signal
use serde_json::Value;                              // parse sink POST body
use wiremock::matchers::{method, path};             // mock backend + sink
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
```

The system under test is reached only as `CARGO_BIN_EXE_beacon-server` (the
deployed binary) + a POSIX signal. There is no `use beacon::...` or
`use beacon_server::...` of an internal function. The driving port is the
signal; the observables are the sink and the stderr events. **PASS.**

## CM-B — Business language

Test fn names and assertion messages speak the operator's domain:
"added rule begins firing after sighup without restart", "malformed reload
keeps previous catalogue and does not crash", "surviving firing rule does
not repage", "daemon keeps alerting". The few unavoidable mechanism terms
(`beacon.reload.succeeded`, `webhook`, `stderr`) are the **operator-visible
contract identifiers** named in ADR-0063 "Observables", not test-internal
jargon: an operator reads exactly these event names in the logs. No HTTP
status code, no DB term, no private-field name leaks into an assertion
message. **PASS.**

## CM-C — Complete user journey with business value

Every scenario runs the full operator journey: a precondition (a live
catalogue, often already Firing) -> the operator action (edit + `kill -HUP`)
-> the observable outcome (the new alert fires / the daemon stays up and
keeps alerting / no re-page) -> the business value (apply an edit without a
restart; survive a fat-fingered edit mid-incident). No scenario tests an
isolated technical operation. The walking skeleton
(`added_rule_begins_firing_after_sighup_without_restart`) is demo-able to a
stakeholder verbatim. **PASS.**

## CM-D — Pure function extraction before fixtures

No business logic lives in the test; the pure evaluation core
(`transition` / `evaluate_once`) is already unit-tested in `beacon` and
`smoke.rs` and is untouched (ADR-0037). The test parametrises no environment
fixture matrix: one substrate (real Unix process + tmp dir + mock HTTP),
identical on Linux CI and macOS local. The impure reload orchestration is
exercised only through the driving port. Mandate 4 holds vacuously: nothing
to extract, no fixture matrix to minimise. **PASS.**

## Critique-dimension self-check (Dims 1-9)

| Dim | Check | Result |
|---|---|---|
| 1 Happy-path bias | error/safety-negative ratio | 5 of 9 = 56% (>= 40%). PASS |
| 2 GWT compliance | each scenario one precondition, one signal action, observable Then | PASS |
| 3 Business-language purity | no DB/HTTP-status/private-field terms in assertions; only the ADR-named event identifiers | PASS |
| 4 Coverage completeness | all 10 AC + all 10 DISCUSS UAT scenarios mapped (ac-coverage.md) | PASS |
| 5 WS user-centricity | scenario 1 titled as an operator goal; Then = operator observations (new alert fires, same process) | PASS |
| 6 Priority validation | US-02 (the load-bearing negative) gets 5 scenarios incl. both carryover paths; the largest risk is most-covered | PASS |
| 7 Observable-behaviour assertions | every Then asserts a sink POST, a named stderr event, process liveness, or `started_at`; none asserts internal/private state or a call count | PASS |
| 8 Traceability | every test carries an inline `(US-0x)` tag-comment; Check A maps US-01 + US-02 to >=1 scenario each | PASS |
| 9 WS boundary proof | Strategy C declared in wave-decisions; WS uses real binary + real signal + real I/O (no `@in-memory`); the sole driven adapter surface (signal + sink + backend) has real-I/O coverage | PASS |

### Dim 8 Check B — environment-to-scenario

`devops/environments.yaml` defines `clean` + `ci`, one substrate. Every
scenario constructs its own fresh writable tmp rules dir (the `clean`
precondition) and runs identically under `ci`. No environment lacks a
matching scenario. **PASS.**

## RED-not-BROKEN + No-Fixture-Theater

- Tests compile against the existing public surface only; no
  not-yet-existing symbol is referenced.
- All 9 are `#[ignore]`d; `cargo test --workspace` is green at the commit.
- Behavioural RED proven: scenario 1 run with `--ignored` FAILS at the
  `beacon.reload.succeeded` assertion (the event does not exist today),
  not at compile or setup. The Given steps set only preconditions (initial
  catalogue + the edit); no fixture supplies the expected firing or event,
  so the test CANNOT pass without the DELIVER handler. This is a valid
  outer-loop RED, not Fixture Theater. **PASS.**
