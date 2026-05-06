//! Black-box CLI smoke test for the `aperture` binary.
//!
//! Spawns the binary built by Cargo (`CARGO_BIN_EXE_aperture`) and
//! asserts the exit-code contract documented in `src/main.rs`:
//!
//!   - exit 2 on argv parse error (unrecognised flag, missing path)
//!   - exit 2 on TOML loader error (file missing or malformed)
//!   - exit 0 on `--help`
//!
//! These three paths are the smallest set that pins `main()` against
//! the trivial mutation `fn main() -> ExitCode { Default::default() }`,
//! which would silently return ExitCode::SUCCESS regardless of input.
//! That mutation slipped past Gate 5 on commit `6b09c0d` because no
//! integration test exercised the binary's entry point. This file
//! closes that gap and was added in the same wave as the
//! `--config <path>` argv-wiring fix (post-merge correction recorded
//! in `docs/feature/aperture/deliver/slice-08-completion.md`).
//!
//! The "happy path" — running the binary against a valid config and
//! observing it bind sockets, drain on SIGTERM, and exit 0 — is
//! covered by the existing process-level integration tests under
//! `slice_*.rs` that drive the library's `aperture::run` entry point.
//! That coverage is already complete; this file's job is the
//! exit-code contract for the CLI surface alone, deliberately small
//! and deliberately fast.

use std::process::Command;

fn aperture_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aperture"))
}

#[test]
fn unrecognised_flag_exits_two() {
    let output = aperture_bin()
        .arg("--bogus")
        .output()
        .expect("spawn aperture");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("argv error"),
        "stderr should mention argv error; got: {stderr}"
    );
}

#[test]
fn config_flag_without_path_exits_two() {
    let output = aperture_bin()
        .arg("--config")
        .output()
        .expect("spawn aperture");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("argv error"),
        "stderr should mention argv error; got: {stderr}"
    );
}

#[test]
fn config_with_missing_path_exits_two_with_config_error() {
    let output = aperture_bin()
        .args(["--config", "/nonexistent/path/aperture.toml"])
        .output()
        .expect("spawn aperture");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("config error"),
        "stderr should mention config error; got: {stderr}"
    );
}

#[test]
fn help_flag_exits_zero_and_prints_usage() {
    let output = aperture_bin()
        .arg("--help")
        .output()
        .expect("spawn aperture");
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--config"),
        "help should mention --config; got: {stderr}"
    );
}
