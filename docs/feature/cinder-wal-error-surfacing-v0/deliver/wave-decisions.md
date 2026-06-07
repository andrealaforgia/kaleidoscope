# DELIVER — wave decisions (cinder-wal-error-surfacing-v0)

## Fix-forward correction (2026-06-07)

**Defect (Bea Verifier N30, msg 056).** The WS-B failure scenario
`place_onto_failing_disk_fails_loudly_and_is_not_durable` in
`crates/kaleidoscope-cli/tests/wal_error_surfacing_cli_skeleton.rs` injected
its disk fault by `chmod`-ing the seeded WAL file READ-ONLY
(`set_readonly(true)`) and asserting the next `place` surfaced
`persistence failed: io:` with a non-zero exit. ROOT BYPASSES the owner-write
permission bit (`CAP_DAC_OVERRIDE`): under a root test runner the append
SUCCEEDS, the binary exits 0, and the test FALSE-PASSES — it proves nothing on
the very runner most likely to be used in CI containers. The verifier confirmed
the fix is to convert the fault from a permission bit to a structural,
user-independent filesystem error.

**Correction.** The fault is now injected by replacing the WAL file with a
**directory** at the WAL path (`<cinder_base>.wal`). The binary opens the store
fresh on every invocation via `FileBackedTieringStore::open(...)`, whose
append-open is `OpenOptions::new().create(true).append(true).open(&wal_path)`
(`crates/cinder/src/file_backed.rs:188`). Opening a directory for byte-append
returns a real `io::Error` — EISDIR, "Is a directory (os error 21)" — for
**every** user, root included, because you cannot append bytes to a directory
inode. The kernel enforces this by inode type, not by permission bits, so the
test bites on any runner. Observed binary output:

```
kaleidoscope-cli: cinder open: persistence failed: io: Is a directory (os error 21)   (exit 1)
```

**Intent + assertions unchanged.** The scenario still asserts: (a) non-zero
exit, (b) stderr contains `persistence failed` AND `io:`, (c) no
`placed tenant=acme item=trade-002` line on stdout, and (d) the failed
placement is NOT durable (a follow-up `get-tier`, after the directory fault is
cleared, must not report `warm`). The substrate surfaces as `CinderOpen` rather
than `CinderPlace` — DWD-3 already records that the OBSERVABLE D2 contract (loud
non-zero exit, `persistence failed: io:`, nothing acked durable) holds either
way, so the operator-facing assertion set is identical.

**Falsifiability preserved.** The load-bearing assertion is on a loudly
surfaced `Err(PersistenceFailed)` (non-zero exit + the `persistence failed: io:`
stderr substring), not on "any error". A swallow-path regression — the
pre-feature bug where `place` ate the WAL error and exited 0 — flips the
`!place.status.success()` assertion RED. The fix does not weaken the test.

**Scope.** Test-only change (plus its doc comments). Production source under
`crates/cinder/src/` and `crates/kaleidoscope-cli/src/` is UNTOUCHED — so
per-feature mutation testing is N/A for the production surface (there is no
changed production code to mutate). No `chmod`/`set_readonly`/`set_permissions`/
`PermissionsExt`/`from_mode` machinery remains in the test. SEMVER: none
(test-only). Validated: `cargo test -p kaleidoscope-cli --test
wal_error_surfacing_cli_skeleton` GREEN (2/2), `cargo build --workspace` ok,
`cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets
-- -D warnings` clean, `cargo deny check` ok.

Reference: this correction is delivered as a single fix-forward commit on
`main` (see the commit that adds this note).
