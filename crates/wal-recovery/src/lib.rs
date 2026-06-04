//! Shared torn-tail-tolerant WAL replay for the Kaleidoscope file-backed
//! storage pillars (ADR-0059).
//!
//! Six file-backed storage pillars recover their state on `open` by
//! replaying an append-only NDJSON write-ahead log: one
//! `serde_json`-serialised record per line, newline-terminated. ADR-0049
//! made the write side crash-honest (per-record `sync_all`), so the
//! residue a crash leaves is a **torn final line**: a partial record with
//! no trailing newline. This crate makes the read-back recover the intact
//! acked prefix past that single torn tail instead of refusing the whole
//! `open`.
//!
//! The tolerance is intentionally **narrow** (ADR-0059 Decision 1): a
//! parse failure is dropped ONLY when it is the final line of the WAL AND
//! the byte stream does not end in `\n`. Every other parse failure (a
//! mid-file failure, or a newline-terminated complete-but-malformed final
//! line) stays fail-closed and is surfaced through `on_parse_error`. A
//! tolerance that swallowed mid-file corruption would be strictly worse
//! than fail-closed; the narrowness is the entire point.

use serde::de::DeserializeOwned;

/// Replays the newline-separated NDJSON records in `wal_bytes`, applying
/// each parsed record through `apply`, and tolerating ONLY a single torn
/// final line.
///
/// Semantics (ADR-0059 Decision 1 and 2):
///
/// * Each non-empty line is parsed with `serde_json::from_str::<R>`. On
///   success the record is handed to `apply`; if `apply` returns `Err`,
///   that error is propagated immediately (the caller's own failure, not
///   a parse failure).
/// * A parse failure is **tolerated** — the torn line is dropped, no
///   `apply` is called for it, and recovery finishes `Ok` with the prefix
///   already accumulated — only when both: the failing line is the LAST
///   line of `wal_bytes`, and `wal_bytes` does NOT end in `\n`. In that
///   single case exactly one structured WARN event
///   (`event="wal.recovery.torn_tail_dropped"`) is emitted naming the
///   `pillar`, the 1-based `line`, and the `dropped_bytes` byte length of
///   the torn line (excluding the absent newline).
/// * Any other parse failure — a failing line that is not the last, or a
///   failing last line when the stream DOES end in `\n` — is returned via
///   `on_parse_error(line, err)` immediately (fail-closed).
/// * An empty WAL applies nothing, emits no warning, and returns `Ok`.
///
/// Generic over the per-pillar record type `R` and the caller's error
/// type `E`; monomorphised per pillar, no `dyn`. The two closures are the
/// seam that absorbs the per-pillar differences (distinct `WalRecord`
/// enums, distinct error types, per-pillar side tables maintained inside
/// `apply`).
pub fn replay_wal_tolerating_torn_tail<R, E>(
    wal_bytes: &[u8],
    pillar: &'static str,
    mut apply: impl FnMut(R) -> Result<(), E>,
    on_parse_error: impl Fn(usize, serde_json::Error) -> E,
) -> Result<(), E>
where
    R: DeserializeOwned,
{
    if wal_bytes.is_empty() {
        return Ok(());
    }

    let ends_with_newline = wal_bytes.last() == Some(&b'\n');
    let text = String::from_utf8_lossy(wal_bytes);

    let parts: Vec<&str> = text.split('\n').collect();
    let last_index = parts.len() - 1;

    for (index, line) in parts.iter().enumerate() {
        if line.is_empty() {
            continue;
        }

        let line_number = index + 1;

        match serde_json::from_str::<R>(line) {
            Ok(record) => apply(record)?,
            Err(error) => {
                let is_last_line = index == last_index;
                if is_last_line && !ends_with_newline {
                    tracing::warn!(
                        event = "wal.recovery.torn_tail_dropped",
                        pillar,
                        line = line_number as u64,
                        dropped_bytes = line.len() as u64,
                    );
                    return Ok(());
                }
                return Err(on_parse_error(line_number, error));
            }
        }
    }

    Ok(())
}

// ====================================================================
// SCAFFOLD: true — store-fsync-durability-v0 DISTILL (Mandate 7, RED-ready).
//
// The durability seam ADR-0060 §4 EXTRACTS into this leaf crate: the
// `FsyncBackend` family (MOVED here from `crates/pulse` in DELIVER) plus
// `atomic_write_snapshot`. These are panicking RED scaffolds so the
// store acceptance suites under `store-fsync-durability-v0` COMPILE and
// are RED (not BROKEN). DELIVER replaces every `__SCAFFOLD__` body with
// the real implementation (moving the pulse `FsyncBackend`/
// `RealFsyncBackend`/`LyingFsyncBackend`/`FsyncProbeError`/`fsync_probe`
// surface here verbatim, and authoring `atomic_write_snapshot` per
// ADR-0060 §2). ZERO `// SCAFFOLD: true` markers must remain after
// DELIVER.
// ====================================================================

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// Durability port: the two fsync primitives a crash-honest store needs.
/// `fsync_file` puts a file's bytes on stable storage (`sync_all` on
/// POSIX); `fsync_dir` makes a directory entry durable (rename
/// durability). Moved here from `crates/pulse` (ADR-0049) in DELIVER.
pub trait FsyncBackend {
    /// Flush the file's bytes to stable storage (POSIX `sync_all`).
    fn fsync_file(&self, file: &File) -> io::Result<()>;
    /// Flush the directory entry to stable storage (parent-dir fsync).
    fn fsync_dir(&self, dir: &Path) -> io::Result<()>;
}

/// Honest backend: delegates to `File::sync_all` and a real dir fsync.
pub struct RealFsyncBackend;

impl FsyncBackend for RealFsyncBackend {
    fn fsync_file(&self, _file: &File) -> io::Result<()> {
        panic!("__SCAFFOLD__ wal_recovery::RealFsyncBackend::fsync_file RED scaffold (store-fsync-durability-v0)")
    }
    fn fsync_dir(&self, _dir: &Path) -> io::Result<()> {
        panic!("__SCAFFOLD__ wal_recovery::RealFsyncBackend::fsync_dir RED scaffold (store-fsync-durability-v0)")
    }
}

/// Lying backend: discards exactly the unsynced bytes a power cut would.
/// `no_op` ignores fsync; `truncating` drops the unsynced tail. The
/// mechanism (b) test double — the ONLY thing that distinguishes
/// `flush` from `sync_all` in-suite.
pub struct LyingFsyncBackend {
    _mode: LyingMode,
}

enum LyingMode {
    NoOp,
    Truncating,
}

impl LyingFsyncBackend {
    /// A substrate that silently ignores every fsync.
    pub fn no_op() -> Self {
        Self {
            _mode: LyingMode::NoOp,
        }
    }
    /// A substrate that drops the unsynced tail on fsync.
    pub fn truncating() -> Self {
        Self {
            _mode: LyingMode::Truncating,
        }
    }
}

impl FsyncBackend for LyingFsyncBackend {
    fn fsync_file(&self, _file: &File) -> io::Result<()> {
        panic!("__SCAFFOLD__ wal_recovery::LyingFsyncBackend::fsync_file RED scaffold (store-fsync-durability-v0)")
    }
    fn fsync_dir(&self, _dir: &Path) -> io::Result<()> {
        panic!("__SCAFFOLD__ wal_recovery::LyingFsyncBackend::fsync_dir RED scaffold (store-fsync-durability-v0)")
    }
}

/// The class of fsync-honesty lie a startup probe detects.
#[derive(Debug)]
pub enum FsyncProbeError {
    /// The substrate ignored the fsync entirely.
    FsyncIgnored,
    /// The substrate dropped bytes the probe had synced.
    BytesLost,
    /// The substrate returned different bytes than were synced.
    BytesMismatch,
    /// An underlying I/O error.
    Io(io::Error),
}

impl FsyncProbeError {
    /// A stable descriptor naming the lie class, for the
    /// `substrate=<descriptor>` refusal field (ADR-0049 vocabulary).
    pub fn substrate_descriptor(&self) -> &'static str {
        panic!("__SCAFFOLD__ wal_recovery::FsyncProbeError::substrate_descriptor RED scaffold (store-fsync-durability-v0)")
    }
}

/// Startup fsync-honesty probe: writes a sentinel under `root`, fsyncs it
/// through `backend`, and verifies the bytes are actually on stable
/// storage. Returns the matching `FsyncProbeError` against a lying
/// substrate. Moved here from `crates/pulse` (ADR-0049) in DELIVER.
pub fn fsync_probe(_root: &Path, _backend: &dyn FsyncBackend) -> Result<(), FsyncProbeError> {
    panic!("__SCAFFOLD__ wal_recovery::fsync_probe RED scaffold (store-fsync-durability-v0)")
}

/// Atomic snapshot (ADR-0060 §2): serialise through `write` to
/// `{canonical}.tmp` in the SAME directory, `fsync_file` the tmp,
/// `rename(tmp, canonical)` (atomic on POSIX), then `fsync_dir` the
/// parent. Whole-or-absent at `canonical` across a crash at ANY point.
pub fn atomic_write_snapshot(
    _canonical: &Path,
    _backend: &dyn FsyncBackend,
    _write: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> io::Result<()> {
    panic!(
        "__SCAFFOLD__ wal_recovery::atomic_write_snapshot RED scaffold (store-fsync-durability-v0)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing::subscriber::DefaultGuard;
    use tracing::{Event, Subscriber};
    use tracing_subscriber::layer::{Context, Layer};
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::registry::LookupSpan;

    #[derive(Debug, Deserialize, PartialEq)]
    struct WalRecord {
        tenant: String,
        value: u64,
    }

    /// Application error the pillars would map to their own
    /// `PersistenceFailed` variant. Carries the 1-based line for the
    /// parse-failure cases and a marker for the apply-failure case.
    #[derive(Debug, PartialEq)]
    enum ReplayError {
        ParseFailed { line: usize, message: String },
        ApplyFailed { value: u64 },
    }

    fn on_parse_error(line: usize, error: serde_json::Error) -> ReplayError {
        ReplayError::ParseFailed {
            line,
            message: error.to_string(),
        }
    }

    // --- WARN capture --------------------------------------------------

    #[derive(Debug, Default, Clone)]
    struct WarnRecord {
        event: Option<String>,
        pillar: Option<String>,
        line: Option<u64>,
        dropped_bytes: Option<u64>,
    }

    #[derive(Default)]
    struct CapturedWarnings(Arc<Mutex<Vec<WarnRecord>>>);

    struct CaptureLayer {
        sink: Arc<Mutex<Vec<WarnRecord>>>,
    }

    impl Visit for WarnRecord {
        fn record_u64(&mut self, field: &Field, value: u64) {
            match field.name() {
                "line" => self.line = Some(value),
                "dropped_bytes" => self.dropped_bytes = Some(value),
                _ => {}
            }
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            match field.name() {
                "event" => self.event = Some(value.to_string()),
                "pillar" => self.pillar = Some(value.to_string()),
                _ => {}
            }
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            // `event = "..."` and `pillar` arrive as &str via record_str;
            // this catch-all keeps the visitor total without affecting the
            // fields under assertion.
            let _ = (field, value);
        }
    }

    impl<S> Layer<S> for CaptureLayer
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
            let mut record = WarnRecord::default();
            event.record(&mut record);
            self.sink.lock().unwrap().push(record);
        }
    }

    /// Installs the capture layer for the duration of the returned guard
    /// and yields the shared sink the test asserts against.
    fn capture_warnings() -> (CapturedWarnings, DefaultGuard) {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let layer = CaptureLayer {
            sink: Arc::clone(&sink),
        };
        let guard = tracing_subscriber::registry().with(layer).set_default();
        (CapturedWarnings(sink), guard)
    }

    impl CapturedWarnings {
        fn torn_tail_events(&self) -> Vec<WarnRecord> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .filter(|record| record.event.as_deref() == Some("wal.recovery.torn_tail_dropped"))
                .cloned()
                .collect()
        }
    }

    // --- Behaviour 1: all-valid WAL with trailing newline --------------

    #[test]
    fn applies_every_record_and_emits_no_warning_for_a_clean_newline_terminated_wal() {
        let (warnings, _guard) = capture_warnings();
        let wal = b"{\"tenant\":\"a\",\"value\":1}\n{\"tenant\":\"b\",\"value\":2}\n";
        let mut applied = Vec::new();

        let result = replay_wal_tolerating_torn_tail::<WalRecord, ReplayError>(
            wal,
            "lumen",
            |record| {
                applied.push(record);
                Ok(())
            },
            on_parse_error,
        );

        assert_eq!(result, Ok(()));
        assert_eq!(
            applied,
            vec![
                WalRecord {
                    tenant: "a".to_string(),
                    value: 1
                },
                WalRecord {
                    tenant: "b".to_string(),
                    value: 2
                },
            ]
        );
        assert!(warnings.torn_tail_events().is_empty());
    }

    // --- Behaviour 2: torn tail tolerated, prefix applied, warned ------

    #[test]
    fn drops_torn_final_line_recovers_prefix_and_emits_one_warning() {
        let (warnings, _guard) = capture_warnings();
        // Valid prefix, then a torn final line with NO trailing newline.
        let wal = b"{\"tenant\":\"a\",\"value\":1}\n{\"tenant\":\"b\",\"val";
        let mut applied = Vec::new();

        let result = replay_wal_tolerating_torn_tail::<WalRecord, ReplayError>(
            wal,
            "lumen",
            |record| {
                applied.push(record);
                Ok(())
            },
            on_parse_error,
        );

        assert_eq!(result, Ok(()));
        // Prefix applied; the torn record is dropped, NOT applied.
        assert_eq!(
            applied,
            vec![WalRecord {
                tenant: "a".to_string(),
                value: 1
            }]
        );

        let events = warnings.torn_tail_events();
        assert_eq!(events.len(), 1, "exactly one torn-tail WARN expected");
        let event = &events[0];
        assert_eq!(
            event.event.as_deref(),
            Some("wal.recovery.torn_tail_dropped")
        );
        assert_eq!(event.pillar.as_deref(), Some("lumen"));
        // 1-based line number of the dropped tail (second line).
        assert_eq!(event.line, Some(2));
        // Byte length of the torn line, excluding the absent newline.
        assert_eq!(
            event.dropped_bytes,
            Some(b"{\"tenant\":\"b\",\"val".len() as u64)
        );
    }

    #[test]
    fn reports_the_emitting_pillar_verbatim_in_the_warning() {
        let (warnings, _guard) = capture_warnings();
        let wal = b"{\"tenant\":\"a\",\"value\":1}\ntorn";

        let _ = replay_wal_tolerating_torn_tail::<WalRecord, ReplayError>(
            wal,
            "pulse",
            |_record| Ok(()),
            on_parse_error,
        );

        let events = warnings.torn_tail_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].pillar.as_deref(), Some("pulse"));
        assert_eq!(events[0].line, Some(2));
        assert_eq!(events[0].dropped_bytes, Some(b"torn".len() as u64));
    }

    // --- Behaviour 3: mid-file parse failure is fail-closed ------------

    #[test]
    fn refuses_a_mid_file_parse_failure_and_emits_no_warning() {
        let (warnings, _guard) = capture_warnings();
        // Unparseable SECOND line followed by a valid third line: the
        // failure is NOT the last line, so it must refuse, not recover.
        let wal = b"{\"tenant\":\"a\",\"value\":1}\nGARBAGE\n{\"tenant\":\"c\",\"value\":3}\n";
        let mut applied = Vec::new();

        let result = replay_wal_tolerating_torn_tail::<WalRecord, ReplayError>(
            wal,
            "lumen",
            |record| {
                applied.push(record);
                Ok(())
            },
            on_parse_error,
        );

        match result {
            Err(ReplayError::ParseFailed { line, .. }) => assert_eq!(line, 2),
            other => panic!("expected fail-closed ParseFailed at line 2, got {other:?}"),
        }
        // The valid first line was applied before the failure; the prefix
        // is NOT silently recovered past the mid-file corruption.
        assert_eq!(
            applied,
            vec![WalRecord {
                tenant: "a".to_string(),
                value: 1
            }]
        );
        assert!(warnings.torn_tail_events().is_empty());
    }

    #[test]
    fn refuses_a_mid_file_parse_failure_even_when_the_wal_lacks_a_trailing_newline() {
        // Pins the conjunction of the two guard conditions: the WAL does
        // NOT end in a newline (so the no-trailing-newline condition is
        // true) but the failing line is NOT the last line. Tolerance must
        // fire ONLY when BOTH hold, so this case must still refuse. If the
        // guard were `is_last_line || !ends_with_newline`, the mid-file
        // GARBAGE would be wrongly dropped and recovery would return Ok.
        let (warnings, _guard) = capture_warnings();
        let wal = b"{\"tenant\":\"a\",\"value\":1}\nGARBAGE\n{\"tenant\":\"c\",\"value\":3}";
        let mut applied = Vec::new();

        let result = replay_wal_tolerating_torn_tail::<WalRecord, ReplayError>(
            wal,
            "lumen",
            |record| {
                applied.push(record);
                Ok(())
            },
            on_parse_error,
        );

        match result {
            Err(ReplayError::ParseFailed { line, .. }) => assert_eq!(line, 2),
            other => panic!("expected fail-closed ParseFailed at line 2, got {other:?}"),
        }
        assert_eq!(
            applied,
            vec![WalRecord {
                tenant: "a".to_string(),
                value: 1
            }]
        );
        assert!(warnings.torn_tail_events().is_empty());
    }

    // --- Behaviour 4: newline-terminated malformed final line refuses --

    #[test]
    fn refuses_a_newline_terminated_malformed_final_line_and_emits_no_warning() {
        let (warnings, _guard) = capture_warnings();
        // Malformed final line that DOES end in a newline: a complete
        // record that happens to be malformed, NOT a tear. Must refuse.
        let wal = b"{\"tenant\":\"a\",\"value\":1}\nGARBAGE\n";

        let result = replay_wal_tolerating_torn_tail::<WalRecord, ReplayError>(
            wal,
            "lumen",
            |_record| Ok(()),
            on_parse_error,
        );

        match result {
            Err(ReplayError::ParseFailed { line, .. }) => assert_eq!(line, 2),
            other => panic!("expected fail-closed ParseFailed at line 2, got {other:?}"),
        }
        assert!(warnings.torn_tail_events().is_empty());
    }

    // --- Behaviour 5: empty input -------------------------------------

    #[test]
    fn applies_nothing_and_emits_no_warning_for_an_empty_wal() {
        let (warnings, _guard) = capture_warnings();
        let mut apply_calls = 0;

        let result = replay_wal_tolerating_torn_tail::<WalRecord, ReplayError>(
            b"",
            "lumen",
            |_record| {
                apply_calls += 1;
                Ok(())
            },
            on_parse_error,
        );

        assert_eq!(result, Ok(()));
        assert_eq!(apply_calls, 0);
        assert!(warnings.torn_tail_events().is_empty());
    }

    // --- Behaviour 6: apply's own error is propagated ------------------

    #[test]
    fn propagates_apply_error_without_treating_it_as_a_parse_failure() {
        let (warnings, _guard) = capture_warnings();
        let wal = b"{\"tenant\":\"a\",\"value\":1}\n{\"tenant\":\"b\",\"value\":2}\n";

        let result = replay_wal_tolerating_torn_tail::<WalRecord, ReplayError>(
            wal,
            "lumen",
            |record| {
                if record.value == 2 {
                    return Err(ReplayError::ApplyFailed { value: 2 });
                }
                Ok(())
            },
            on_parse_error,
        );

        assert_eq!(result, Err(ReplayError::ApplyFailed { value: 2 }));
        assert!(warnings.torn_tail_events().is_empty());
    }
}
