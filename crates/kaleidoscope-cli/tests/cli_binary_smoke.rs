// Kaleidoscope CLI — binary-level smoke tests
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

//! # Binary smoke tests
//!
//! The library acceptance suite (`observe_otlp_*`, `ingest_and_read_*`)
//! drives `kaleidoscope_cli::{ingest, read}` directly. That leaves the
//! thin glue in `src/main.rs` (`print_usage`, `run_read`) unobserved
//! by library-level tests, which lets `cargo mutants` survive trivial
//! body-deletion mutants in those wrappers.
//!
//! This file spawns the actual binary built by Cargo
//! (`CARGO_BIN_EXE_kaleidoscope-cli`) and asserts on its observable
//! behaviour — stdout, stderr, exit code, on-disk side effects. Both
//! goals are served: the binary contract is verified end-to-end, and
//! the two `main.rs` mutants that the library tests cannot reach are
//! killed.
//!
//! These are NOT acceptance tests for any feature US-NN; they are
//! mutation-coverage probes for binary-level seams.

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::UNIX_EPOCH;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kaleidoscope-cli")
}

fn temp_root(name: &str) -> PathBuf {
    let mut p = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    p.push(format!(
        "kal-cli-bin-{name}-{pid}-{nanos}",
        pid = std::process::id()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn binary_with_no_args_prints_usage_to_stderr() {
    // Kills `replace print_usage with ()` in src/main.rs. The mutant
    // turns the function body into a no-op, so stderr becomes empty
    // and this assertion fails. The library-level tests cannot reach
    // `print_usage` because they bypass `fn main`.
    let output = Command::new(bin())
        .output()
        .expect("spawn kaleidoscope-cli with no args");
    assert!(
        output.status.success(),
        "no-arg invocation exits 0 (help path)"
    );
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("kaleidoscope-cli ingest"),
        "stderr usage mentions ingest subcommand: {stderr:?}"
    );
    assert!(
        stderr.contains("kaleidoscope-cli read"),
        "stderr usage mentions read subcommand: {stderr:?}"
    );
}

#[test]
fn binary_read_subcommand_writes_records_to_stdout() {
    // Kills `replace run_read -> Ok(())` in src/main.rs. The mutant
    // skips the entire `run_read` body, so the binary emits no stdout
    // bytes and no stderr summary even though the data dir contains
    // one ingested record. Asserting on the stdout/stderr bytes flips
    // this mutant red.
    let root = temp_root("read_subcommand");
    let data = root.join("data");

    // Pre-ingest one record via the binary's `ingest` subcommand so
    // the data dir has something to read back. Using the binary here
    // also exercises the `run_ingest` glue (defence-in-depth: future
    // mutants on that wrapper would also be caught).
    let mut child = Command::new(bin())
        .arg("ingest")
        .arg("acme")
        .arg(&data)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ingest");
    let line = "{\"observed_time_unix_nano\":1,\"severity_number\":9,\"severity_text\":\"INFO\",\"body\":\"hi\",\"attributes\":{},\"resource_attributes\":{},\"trace_id\":null,\"span_id\":null}\n";
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(line.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let ingest_out = child.wait_with_output().expect("wait ingest");
    assert!(ingest_out.status.success(), "seed ingest succeeds");

    // Now drive `read` and assert on stdout (the records as NDJSON)
    // and stderr (the `read ok: records=1` summary line).
    let read_out = Command::new(bin())
        .arg("read")
        .arg("acme")
        .arg(&data)
        .output()
        .expect("spawn read");
    assert!(read_out.status.success(), "read exits 0");
    let stdout = String::from_utf8(read_out.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("\"body\":\"hi\""),
        "stdout contains the seeded record: {stdout:?}"
    );
    let stderr = String::from_utf8(read_out.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("read ok: records=1"),
        "stderr summary line present: {stderr:?}"
    );

    let _ = fs::remove_dir_all(&root);
}
