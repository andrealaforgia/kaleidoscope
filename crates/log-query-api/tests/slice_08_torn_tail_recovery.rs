// Kaleidoscope log-query-api — slice 08 torn-tail recovery acceptance suite
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

//! Slice 08 — torn-tail recovery, end-to-end binary path
//! (wal-torn-tail-recovery-v0, US-01; verifier expectation D04).
//!
//! Feature: an operator restarts the lumen-backed `log-query-api` binary
//! against a crashed `pillar_root` whose WAL holds N durably acked records
//! followed by one torn final line. The binary opens
//! `FileBackedLogStore::open(pillar_root/lumen, ..)`, drops the torn tail,
//! recovers the intact prefix, binds its listener, and `GET /api/v1/logs`
//! serves exactly the N acked records. On the recovery it emits one
//! structured WARN `event="wal.recovery.torn_tail_dropped"` to stderr.
//!
//! ## Driving port (the headline, verifier D04)
//!
//! The operator's actual invocation path: the COMPILED `log-query-api`
//! binary launched as a child process, with a controlled environment, a
//! crashed `pillar_root` on a real tmp directory, and a real HTTP query
//! over the bound ephemeral port. This is the same subprocess + stderr +
//! HTTP shape the EDD verifier uses (slice 07 established it for the
//! tracing-subscriber feature). An in-process router test cannot observe
//! the binary's recovery-on-open or its process-global stderr WARN; only a
//! spawned process can. The store-reopen unit of the same behaviour lives
//! in `crates/lumen/tests/v1_slice_03_torn_tail_recovery.rs`.
//!
//! ## I-O strategy: C (real local I/O)
//!
//! Real WAL bytes on a real tmp `pillar_root`, real child process, real
//! TCP. No external services, no containers. See
//! `docs/feature/wal-torn-tail-recovery-v0/distill/wave-decisions.md`
//! DWD-1.
//!
//! ## RED-not-BROKEN posture (Mandate 7)
//!
//! Both scenarios are `#[ignore]`d until their DELIVER slice removes the
//! marker (Outside-In). They drive ONLY existing public surface: the
//! compiled binary (`CARGO_BIN_EXE_log-query-api`), `lumen::
//! FileBackedLogStore` for seeding, and on-disk WAL bytes. They COMPILE
//! against today's code with no scaffold. They are RED because (a) today's
//! `open` refuses a torn tail and the binary exits non-zero, never
//! binding, and (b) `query_http_common::init_tracing()` is a no-op
//! scaffold at DISTILL close so no WARN reaches stderr even once recovery
//! lands. DELIVER turns them GREEN one at a time; neither is BROKEN.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use aegis::TenantId;
use lumen::{FileBackedLogStore, LogBatch, LogRecord, LogStore, NoopRecorder, SeverityNumber};
use serde_json::Value;

const LUMEN_SUBDIR: &str = "lumen";

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(observed: u64, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    LogRecord {
        observed_time_unix_nano: observed,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

fn temp_pillar_root(test_name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("lqa-torn-tail-{test_name}-{pid}-{nanos}"));
    std::fs::create_dir_all(&path).expect("mkdir pillar_root");
    path
}

fn cleanup(pillar_root: &Path) {
    let _ = std::fs::remove_dir_all(pillar_root);
}

fn wal_path_of(base: &Path) -> PathBuf {
    let mut p = base.as_os_str().to_owned();
    p.push(".wal");
    PathBuf::from(p)
}

/// Seed a lumen store at `pillar_root/lumen` (the binary's
/// `LUMEN_SUBDIR`) with `n` acked single-record batches, then close it so
/// the WAL is flushed.
fn seed_acked_prefix(pillar_root: &Path, tenant_name: &str, n: u64) -> PathBuf {
    let base = pillar_root.join(LUMEN_SUBDIR);
    let store = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("seed open");
    for i in 0..n {
        store
            .ingest(
                &tenant(tenant_name),
                LogBatch::with_records(vec![record(100 + i, &format!("order {i}"))]),
            )
            .expect("seed ingest");
    }
    drop(store);
    base
}

/// Append a torn final line (partial JSON, NO trailing newline). Returns
/// its byte length.
fn append_torn_tail(base: &Path, torn: &str) -> usize {
    let wal = wal_path_of(base);
    let existing = std::fs::read_to_string(&wal).unwrap_or_default();
    std::fs::write(&wal, format!("{existing}{torn}")).expect("append torn tail");
    torn.len()
}

fn stderr_event_count(stderr: &str, event_name: &str) -> usize {
    stderr
        .lines()
        .filter(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .and_then(|v| {
                    v.get("event")
                        .and_then(|e| e.as_str())
                        .map(|e| e == event_name)
                })
                .unwrap_or(false)
        })
        .count()
}

fn stderr_event_value(stderr: &str, event_name: &str) -> Option<Value> {
    stderr.lines().find_map(|line| {
        let v: Value = serde_json::from_str(line).ok()?;
        if v.get("event").and_then(|e| e.as_str()) == Some(event_name) {
            Some(v)
        } else {
            None
        }
    })
}

/// Spawn the binary against `pillar_root` with `tenant_name`, drain stderr
/// on a dedicated thread under a wall-clock deadline (the slice-07 shape),
/// and stop as soon as `listener_bound` or `health.startup.refused`
/// appears. Returns `(bound_addr, captured_stderr, child)`.
fn spawn_until_settled(
    pillar_root: &Path,
    tenant_name: &str,
    timeout: Duration,
) -> (Option<String>, String, Child) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_log-query-api"))
        .env("KALEIDOSCOPE_PILLAR_ROOT", pillar_root)
        .env("KALEIDOSCOPE_LOG_QUERY_TENANT", tenant_name)
        .env("KALEIDOSCOPE_LOG_QUERY_ADDR", "127.0.0.1:0")
        .env("RUST_LOG", "info")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn log-query-api");
    let mut err = child.stderr.take().expect("child stderr piped");
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match err.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let deadline = Instant::now() + timeout;
    let mut stderr = String::new();
    let mut bound: Option<String> = None;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(remaining.min(Duration::from_millis(200))) {
            Ok(chunk) => stderr.push_str(&String::from_utf8_lossy(&chunk)),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if let Some(v) = stderr_event_value(&stderr, "listener_bound") {
            bound = v.get("addr").and_then(|a| a.as_str()).map(str::to_string);
            break;
        }
        if stderr_event_count(&stderr, "health.startup.refused") > 0 {
            break;
        }
    }
    drop(rx);
    // Do NOT join the reader here. The child is intentionally left running
    // so the caller can query its bound port, so the child's stderr pipe
    // stays open and the reader's blocking `read` never returns; joining
    // would deadlock (the defect that hung this suite and let the overnight
    // environment SIGKILL the whole pre-commit hook). Detach the reader
    // instead: it exits on its own when the caller kills the child, which
    // closes the stderr pipe. slice_07's helper avoids this by killing the
    // child before joining; this helper cannot, because it must hand the
    // live child back for the HTTP query.
    drop(reader);
    (bound, stderr, child)
}

/// Minimal blocking HTTP GET over std `TcpStream` (no new dependency).
/// Returns the response body after the header terminator.
///
/// Reads under a per-read idle timeout into a `Vec` rather than via
/// `read_to_string`: the latter blocks until the peer half-closes, which
/// hangs the test if the server keeps the connection lingering. The full
/// HTTP response arrives before the socket idles, so a short idle timeout
/// (`WouldBlock`/`TimedOut`) means "the server sent everything and went
/// quiet" — we stop and return the complete body we already have. `Ok(0)`
/// is a clean EOF (peer closed). This keeps the whole body without ever
/// blocking indefinitely.
fn http_get_body(addr: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).expect("connect read API");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    let req = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).expect("send request");

    let mut raw: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break, // clean EOF: peer half-closed the connection
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                // Idle: the server has sent the full response and gone
                // quiet. Stop and return what we have.
                break;
            }
            Err(e) => panic!("read response: {e}"),
        }
    }
    let raw = String::from_utf8_lossy(&raw).into_owned();
    raw.split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or(raw)
}

// =========================================================================
// AC-1 (verifier D04, walking skeleton): operator restarts the binary and
// the read API serves every durably acked record.
// =========================================================================

/// Scenario: Operator restarts a crashed collector and the read API serves
/// the intact acked prefix.
///
/// Given a crashed pillar_root whose lumen WAL holds 10 acked records for
///   tenant acme-corp followed by one torn final line with no newline
/// When the operator restarts the log-query-api binary against it
/// Then the binary recovers, binds its listener, and a query over the full
///   time range returns all 10 acked records (the torn 11th absent)
#[test]
fn operator_restart_serves_the_intact_acked_prefix_after_a_torn_tail() {
    // @walking_skeleton @real-io @driving_port @US-01 @AC-1
    let root = temp_pillar_root("d04_binary_query");
    let base = seed_acked_prefix(&root, "acme-corp", 10);
    // A kill -9 tore the 11th batch mid-write: partial JSON, no newline.
    append_torn_tail(
        &base,
        "{\"op\":\"ingest\",\"tenant\":\"acme-corp\",\"records\":[{\"body\":\"order 4471 shi",
    );

    let (bound, stderr, mut child) =
        spawn_until_settled(&root, "acme-corp", Duration::from_secs(20));
    let addr = match bound {
        Some(a) => a,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("binary should recover the prefix and bind. stderr:\n{stderr}");
        }
    };

    // The seeded records carry observed times near epoch (100..109 ns), so
    // a window of [0, 86400] seconds covers them all while staying within
    // the read API's MAX_WINDOW_SECONDS = 86400 cap (ADR-0050 read guard);
    // a wider window is refused with "window exceeds 86400 seconds".
    let body = http_get_body(&addr, "/api/v1/logs?start=0&end=86400");
    let _ = child.kill();
    let _ = child.wait();

    // The read API answers with a BARE JSON array of in-window records
    // (ADR-0047 Decision 1).
    let logs: Value = serde_json::from_str(&body).expect("response is a JSON array");
    let arr = logs.as_array().expect("response is a logs array");
    assert_eq!(
        arr.len(),
        10,
        "all 10 acked records served; the torn 11th is absent. stderr:\n{stderr}"
    );
    cleanup(&root);
}

// =========================================================================
// AC-3 (verifier D04 secondary port): the recovery emits exactly one
// structured WARN naming pillar, line, dropped_bytes.
// =========================================================================

/// Scenario: Operator confirms exactly one torn tail was dropped.
///
/// Given a crashed pillar_root whose lumen WAL holds 3 acked records then
///   one torn final line
/// When the operator restarts the binary
/// Then stderr carries exactly one wal.recovery.torn_tail_dropped event
///   naming pillar=lumen, the 1-based line number, and the dropped byte
///   length
#[test]
fn recovery_emits_one_structured_warning_naming_pillar_line_and_dropped_bytes() {
    // @real-io @driving_port @US-01 @AC-3
    let root = temp_pillar_root("d04_warn");
    let base = seed_acked_prefix(&root, "acme-corp", 3);
    let torn = "{\"op\":\"ingest\",\"tenant\":\"acme-corp\",\"records\":[{\"body\":\"tor";
    let dropped = append_torn_tail(&base, torn);

    let (bound, stderr, mut child) =
        spawn_until_settled(&root, "acme-corp", Duration::from_secs(20));
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        bound.is_some(),
        "the binary recovered and bound. stderr:\n{stderr}"
    );

    assert_eq!(
        stderr_event_count(&stderr, "wal.recovery.torn_tail_dropped"),
        1,
        "exactly one torn-tail WARN; stderr:\n{stderr}"
    );
    let warn = stderr_event_value(&stderr, "wal.recovery.torn_tail_dropped")
        .expect("the WARN line is present");
    assert_eq!(
        warn.get("pillar").and_then(Value::as_str),
        Some("lumen"),
        "WARN names the pillar"
    );
    // Line 4 = the 4th WAL line (three acked, then the torn tail).
    assert_eq!(
        warn.get("line").and_then(Value::as_u64),
        Some(4),
        "WARN names the 1-based line number of the dropped tail"
    );
    assert_eq!(
        warn.get("dropped_bytes").and_then(Value::as_u64),
        Some(dropped as u64),
        "WARN names the byte length of the dropped torn line"
    );
    cleanup(&root);
}
