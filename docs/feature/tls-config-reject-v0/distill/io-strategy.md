# I/O Strategy — tls-config-reject-v0 (DISTILL)

## Adapter inventory

This feature adds **no new driven adapter**. It is one config-validation invariant on the
already-built `aperture` binary. The relevant boundaries:

| Boundary | Adapter | New? | I/O tier in this suite |
|----------|---------|------|------------------------|
| Config load (TOML → typed) | figment `Toml::string` / `Toml::file` | no (shipped, ADR-0008) | real (string parse in-process; file in binary subprocess) |
| Process entry | `aperture` binary `main()` | no (shipped) | **real** — subprocess via `CARGO_BIN_EXE_aperture` |
| Listener bind | `compose::spawn_grpc` / `spawn_http` | no (shipped) | exercised by negative controls (real ephemeral bind); UNREACHABLE on refusal |

Because no new adapter is introduced, there is no new `@infrastructure-failure` /
InMemory-double surface to author. The "infrastructure failure" class for this feature
*is* the operator's malformed-intent config (a requested-but-unimplemented knob), which is
covered as the refusal path itself.

## Real-I/O coverage (`@real-io`)

The DEVOPS WS strategy is SLIM with a **real binary subprocess** proving surface
(`environments.yaml` proving_tests.refusal_path). Per the mandate to exercise REAL I/O for
the operator entry point:

- The 3 binary refusal tests (`ac{1,2,3}_*_binary_*`) spawn the **real `aperture` binary**
  built by Cargo, write a **real temp TOML file**, run the real process to completion, and
  assert against its **real exit code + real stderr bytes + real TCP connect-refused**.
  These are tagged `@real-io` in the file header rationale and prove the wiring
  (`main.rs` → `from_toml_path` → `into_config` → exit-2 arm) that no in-memory seam test
  can catch (path resolution, the pre-subscriber `eprintln!` window, exit-code mapping).

This satisfies "for every operator entry point, at least one scenario exercises real I/O".

## Port-collision safety (DEVOPS D3 / D4)

| Path | Ports used | Collision risk | Why safe |
|------|-----------|----------------|----------|
| Refusal (binary) | default 4317/4318 | **none** | the refusal NEVER constructs a `Config` and NEVER enters the bind path (`compose.rs:164,182`). Nothing binds, so parallel refusal tests against the defaults cannot collide — there is no listener to collide with. This is the structurally-safe surface and carries the suite's weight. |
| Positive bind (negative controls, AC-5/6) | ephemeral `127.0.0.1:0` | **none** | uses the `grpc_bind_addr`/`http_bind_addr` ephemeral override (`config/mod.rs:226-236`), exactly as `slice_07_tls_schema_knob.rs:42-45` does. OS assigns a free port; binds collision-free in the parallel hook. |

**No default-port (4317/4318) POSITIVE-bind test is added** to the parallel suite, per the
explicit DEVOPS D4 guidance and the gateway's prior discipline (it kept a clean-start
binary test `#[ignore]`d for exactly this reason). The refusal binary tests are
collision-safe precisely because they never bind.

## Determinism

Every assertion is one of: an **exit code** (`output.status.code()`), a **stderr string
grep** (`stderr.contains(...)`), a **`Result::is_err`/`is_ok`** at the seam, a **bound-port
check** (`handle.grpc_addr().port() != 0`), or a **TCP connect-refused** check. There is
**no wall-clock, no p95, no timing threshold** — so this feature is structurally immune to
the lumen/pulse p95 overnight-flake class documented in project memory. The one bounded
timeout (`connect_timeout(250ms)`) gates a connect that is *expected to be refused
immediately* (no listener) — it is a failure-fast bound, not a latency assertion.

## Pre-subscriber `eprintln!` window (carried from DEVOPS, DELIVER-owned)

When `--config <path>` is given, `main.rs` catches the loader `ConfigError` **before**
`install_subscriber` runs and prints via `eprintln!` (`main.rs:33-39`). The binary tests
therefore grep a **structured-shape line on stderr** carrying `event=config_validation_failed`
+ the named knob, NOT necessarily a subscriber-emitted JSON record. The acceptance
observable is fixed by ADR-0061 to: (a) exit 2, (b) the line carries
`event=config_validation_failed`, (c) it names the knob, (d) no listener bound. Whether
DELIVER emits JSON-via-subscriber or a structured `eprintln!` in that window is a DELIVER
mechanism choice; DISTILL asserts only the observable. The tests grep for the literal
`config_validation_failed` substring and the knob name, which both mechanisms satisfy.
