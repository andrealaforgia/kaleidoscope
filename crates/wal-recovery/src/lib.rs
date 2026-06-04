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
// Durability seam (ADR-0060 §4) — the `FsyncBackend` family MOVED here
// from `crates/pulse` (ADR-0049) plus `atomic_write_snapshot` (ADR-0060
// §2). All seven file-backed pillars depend on this leaf crate INWARD
// and reuse this one seam, so the fsync calls + the
// tmp+fsync+rename+fsync-dir snapshot ordering live once and cannot
// drift. pulse re-exports these names so its public surface stays
// byte-identical (Gate 2).
// ====================================================================

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Fixed sentinel filename written under the probe root. The path is
/// FIXED (not randomised): the probe overwrites this file on every run
/// so no state accumulates across restarts (ADR-0049 §Consequences).
const SENTINEL_FILENAME: &str = ".fsync-probe";

/// Sentinel payload. 64 bytes, ASCII, fixed content so the round-trip
/// comparison is deterministic.
const SENTINEL_BYTES: &[u8; 64] =
    b"kaleidoscope-fsync-probe-sentinel-v0-do-not-edit-or-remove-XXXX\n";

/// Durability port: the two fsync primitives a crash-honest store needs
/// (ADR-0049 §6). `fsync_file` puts a file's bytes on stable storage
/// (`sync_all` on POSIX, `FlushFileBuffers` on Windows); `fsync_dir`
/// makes a directory entry durable (POSIX rename durability). The seam
/// lets the probe and the WAL append path be exercised against a lying
/// substrate in tests without mounting a hostile filesystem.
pub trait FsyncBackend {
    /// Sync the file's data and metadata to stable storage. The honest
    /// implementation MUST persist; the lying implementations in tests
    /// return `Ok(())` while leaving the substrate in a bad state.
    fn fsync_file(&self, file: &File) -> io::Result<()>;

    /// Sync the parent directory so the file's directory entry is
    /// durable (POSIX rename durability). Required after creating or
    /// removing a file the recovery path depends on.
    fn fsync_dir(&self, dir: &Path) -> io::Result<()>;
}

/// Production implementation of [`FsyncBackend`]. Delegates to
/// `File::sync_all` and the platform's parent-directory fsync.
pub struct RealFsyncBackend;

impl FsyncBackend for RealFsyncBackend {
    // `sync_all`'s durability effect is UNOBSERVABLE in-process: a same-host
    // test cannot distinguish `file.sync_all()` from `Ok(())` because the
    // bytes are already in the kernel page cache (which survives until a
    // real power cut) — this is the central thesis of ADR-0060 §1. The
    // honest behaviour is proven OUT-OF-PROCESS by the consumer pillars'
    // crash-durability acceptance suites (e.g. lumen's
    // v1_slice_04_crash_durability), not here. Skipped to avoid a
    // false-negative equivalent mutant; the call is covered behaviourally
    // by those suites.
    #[cfg_attr(test, mutants::skip)]
    fn fsync_file(&self, file: &File) -> io::Result<()> {
        file.sync_all()
    }

    fn fsync_dir(&self, dir: &Path) -> io::Result<()> {
        // On Unix the parent directory must be opened read-only and
        // sync_all'd to make rename durability work. On Windows
        // directory fsync is not meaningful (FlushFileBuffers on a
        // directory handle is documented not to flush the directory
        // entry); we treat it as a no-op there for v0 (ADR-0049
        // §Consequences).
        #[cfg(unix)]
        {
            let dir_handle = File::open(dir)?;
            dir_handle.sync_all()
        }
        #[cfg(not(unix))]
        {
            let _ = dir;
            Ok(())
        }
    }
}

/// Test double of [`FsyncBackend`] simulating three classes of substrate
/// lie (ADR-0049 §6). Returns `Ok(())` from `fsync_file` in every mode
/// but leaves the substrate in a bad state matching the lie class — it
/// discards exactly the unsynced bytes a power cut would. This is the
/// mechanism (b) test double: the ONLY thing that distinguishes `flush`
/// from `sync_all` in-suite, because a same-host SIGKILL leaves the
/// unsynced bytes in the page cache.
///
/// - [`LyingFsyncBackend::no_op`] — does not persist (a fresh handle
///   reads back an empty file);
/// - [`LyingFsyncBackend::truncating`] — silently shortens the file by
///   one byte;
/// - [`LyingFsyncBackend::byte_flipping`] — keeps the length but flips
///   the first byte so the bytes read back differ from those written.
pub struct LyingFsyncBackend {
    mode: LyingMode,
}

#[derive(Debug, Clone, Copy)]
enum LyingMode {
    NoOp,
    Truncating,
    ByteFlipping,
}

impl LyingFsyncBackend {
    /// Lie mode: fsync returns `Ok(())` but the bytes never reach the
    /// substrate; a fresh handle finds the file empty.
    pub fn no_op() -> Self {
        Self {
            mode: LyingMode::NoOp,
        }
    }

    /// Lie mode: fsync returns `Ok(())` but the substrate silently
    /// shortens the file by one byte; a fresh handle finds the file
    /// present but shorter than the sentinel.
    pub fn truncating() -> Self {
        Self {
            mode: LyingMode::Truncating,
        }
    }

    /// Lie mode: fsync returns `Ok(())` but the substrate flips a byte;
    /// a fresh handle finds the file at the expected length but with
    /// different bytes.
    pub fn byte_flipping() -> Self {
        Self {
            mode: LyingMode::ByteFlipping,
        }
    }
}

impl FsyncBackend for LyingFsyncBackend {
    fn fsync_file(&self, file: &File) -> io::Result<()> {
        match self.mode {
            LyingMode::NoOp => {
                // Truncate to zero bytes so the reopen sees an empty
                // file: the round-trip read observes "bytes never
                // persisted".
                file.set_len(0)?;
                Ok(())
            }
            LyingMode::Truncating => {
                let len = file.metadata()?.len();
                if len > 0 {
                    file.set_len(len - 1)?;
                }
                Ok(())
            }
            LyingMode::ByteFlipping => {
                let len = file.metadata()?.len();
                if len > 0 {
                    // Clone the handle (shares the underlying file) so
                    // we can read+write without taking `&mut File`.
                    let mut handle = file.try_clone()?;
                    handle.seek(SeekFrom::Start(0))?;
                    let mut byte = [0u8; 1];
                    handle.read_exact(&mut byte)?;
                    byte[0] ^= 0xff;
                    handle.seek(SeekFrom::Start(0))?;
                    handle.write_all(&byte)?;
                }
                Ok(())
            }
        }
    }

    fn fsync_dir(&self, _dir: &Path) -> io::Result<()> {
        // The LyingFsyncBackend's job is file-level lies, not
        // directory-level; the directory-fsync surface is exercised by
        // the honest backend in the write-path durability tests.
        Ok(())
    }
}

/// Typed refusal classes from the fsync-honesty probe (ADR-0049 §7).
/// The gateway composition root maps each variant to a
/// `substrate=<descriptor>` payload field on the existing
/// `event=health.startup.refused` event.
#[derive(Debug)]
pub enum FsyncProbeError {
    /// The sentinel was written and fsynced but the fresh handle read
    /// back nothing: the substrate's fsync is a no-op
    /// (`substrate=fsync-noop`).
    FsyncIgnored,
    /// The sentinel was written and fsynced but the fresh handle read
    /// back a shorter file: the substrate silently truncated
    /// (`substrate=fsync-truncating`).
    BytesLost,
    /// The sentinel was written and fsynced; the fresh handle read back
    /// the expected length but different bytes
    /// (`substrate=fsync-corrupting`).
    BytesMismatch,
    /// The probe could not run because an IO step (open, write, sync,
    /// reopen, read) errored (`substrate=fsync-io`).
    Io(io::Error),
}

impl FsyncProbeError {
    /// Map the refusal class to the `substrate=<descriptor>` payload the
    /// composition root attaches to `event=health.startup.refused`
    /// (ADR-0049 §7). A single source of truth callers cannot drift
    /// from.
    pub fn substrate_descriptor(&self) -> &'static str {
        match self {
            FsyncProbeError::FsyncIgnored => "fsync-noop",
            FsyncProbeError::BytesLost => "fsync-truncating",
            FsyncProbeError::BytesMismatch => "fsync-corrupting",
            FsyncProbeError::Io(_) => "fsync-io",
        }
    }
}

impl fmt::Display for FsyncProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsyncProbeError::FsyncIgnored => write!(f, "fsync ignored: sentinel did not persist"),
            FsyncProbeError::BytesLost => write!(f, "bytes lost: sentinel shorter on reopen"),
            FsyncProbeError::BytesMismatch => {
                write!(f, "bytes mismatch: sentinel differs on reopen")
            }
            FsyncProbeError::Io(e) => write!(f, "fsync probe IO error: {e}"),
        }
    }
}

impl std::error::Error for FsyncProbeError {}

impl From<io::Error> for FsyncProbeError {
    fn from(e: io::Error) -> Self {
        FsyncProbeError::Io(e)
    }
}

/// Earned-Trust fsync-honesty probe (ADR-0049 §1). Writes a sentinel
/// under `root`, syncs through `backend`, drops the handle, reopens,
/// reads back, and compares. On any mismatch returns the typed refusal
/// class. On success removes its sentinel and returns `Ok(())`.
///
/// Called from a store's composition root BEFORE the listener binds
/// (wire-then-probe-then-use).
pub fn fsync_probe(root: &Path, backend: &dyn FsyncBackend) -> Result<(), FsyncProbeError> {
    // Make sure the probe root exists; if the caller passed a
    // non-existent path we cannot tell honest from lying.
    if !root.exists() {
        std::fs::create_dir_all(root)?;
    }

    let sentinel = root.join(SENTINEL_FILENAME);

    // Phase 1: write the sentinel through the backend.
    {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&sentinel)?;
        file.write_all(SENTINEL_BYTES)?;
        backend.fsync_file(&file)?;
        // Drop the handle by scope exit so the reopen below sees a
        // fresh kernel-cache state.
    }

    // Phase 2: reopen with a fresh handle and read back.
    let mut roundtrip = Vec::new();
    {
        let mut file = File::open(&sentinel)?;
        file.read_to_end(&mut roundtrip)?;
    }

    // Phase 3: classify and (always) clean up the sentinel.
    let outcome = classify_roundtrip(&roundtrip, SENTINEL_BYTES);

    // Best-effort cleanup: surface the probe outcome rather than mask it
    // with any removal error.
    let _ = std::fs::remove_file(&sentinel);

    outcome
}

/// Compare the round-tripped bytes against the sentinel and classify the
/// outcome. Extracted so the three lie classes can be exercised via unit
/// tests without touching the filesystem.
fn classify_roundtrip(read_back: &[u8], expected: &[u8]) -> Result<(), FsyncProbeError> {
    if read_back.is_empty() {
        return Err(FsyncProbeError::FsyncIgnored);
    }
    if read_back.len() < expected.len() {
        return Err(FsyncProbeError::BytesLost);
    }
    if read_back != expected {
        return Err(FsyncProbeError::BytesMismatch);
    }
    Ok(())
}

/// Atomic snapshot (ADR-0060 §2): serialise through `write` to
/// `{canonical}.tmp` in the SAME directory, flush + `fsync_file` the
/// tmp, `rename(tmp, canonical)` (atomic on POSIX), then `fsync_dir` the
/// parent. Whole-or-absent at `canonical` across a crash at ANY point —
/// before the rename the canonical path still holds the previous whole
/// snapshot (a stray `.tmp` is never read on reopen); the rename is
/// atomic; after the parent fsync the new snapshot is durable. Same-dir
/// temp so `rename(2)` stays within the filesystem and does not degrade
/// to copy+unlink.
pub fn atomic_write_snapshot(
    canonical: &Path,
    backend: &dyn FsyncBackend,
    write: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> io::Result<()> {
    let parent = canonical
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let mut tmp_name = canonical.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp_path = std::path::PathBuf::from(tmp_name);

    // Serialise to the same-directory temp, flush the user-space buffer,
    // then fsync the temp's bytes onto stable storage BEFORE the rename.
    {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)?;
        let mut writer = io::BufWriter::new(file);
        write(&mut writer)?;
        writer.flush()?;
        let file = writer
            .into_inner()
            .map_err(|e| io::Error::other(e.to_string()))?;
        backend.fsync_file(&file)?;
    }

    // Atomic on POSIX: the canonical path points at the old OR the new
    // whole file at every instant, never a torn one.
    std::fs::rename(&tmp_path, canonical)?;

    // The rename is not crash-safe until the parent directory entry is
    // durable.
    backend.fsync_dir(&parent)?;

    Ok(())
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

    // --- Durability seam: fsync probe + atomic snapshot ----------------

    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "wal-recovery-durability-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&p).expect("mkdir");
        p
    }

    fn cleanup_dir(root: &Path) {
        let _ = std::fs::remove_dir_all(root);
    }

    // Kills the classify_roundtrip mutants: each branch maps to a
    // distinct refusal class, and the equal-length-and-bytes branch is
    // Ok. The classifier is the single source of truth the probe
    // delegates to.
    #[test]
    fn classify_distinguishes_each_lie_class() {
        assert!(matches!(
            classify_roundtrip(&[], SENTINEL_BYTES),
            Err(FsyncProbeError::FsyncIgnored),
        ));
        assert!(matches!(
            classify_roundtrip(&SENTINEL_BYTES[..SENTINEL_BYTES.len() - 1], SENTINEL_BYTES),
            Err(FsyncProbeError::BytesLost),
        ));
        let mut flipped = SENTINEL_BYTES.to_vec();
        flipped[0] ^= 0xff;
        assert!(matches!(
            classify_roundtrip(&flipped, SENTINEL_BYTES),
            Err(FsyncProbeError::BytesMismatch),
        ));
        assert!(classify_roundtrip(SENTINEL_BYTES, SENTINEL_BYTES).is_ok());
    }

    // Kills the substrate_descriptor mutants: each error variant has a
    // distinct descriptor the composition root surfaces on
    // event=health.startup.refused.
    #[test]
    fn substrate_descriptor_is_distinct_per_variant() {
        assert_eq!(
            FsyncProbeError::FsyncIgnored.substrate_descriptor(),
            "fsync-noop",
        );
        assert_eq!(
            FsyncProbeError::BytesLost.substrate_descriptor(),
            "fsync-truncating",
        );
        assert_eq!(
            FsyncProbeError::BytesMismatch.substrate_descriptor(),
            "fsync-corrupting",
        );
        assert_eq!(
            FsyncProbeError::Io(io::Error::other("x")).substrate_descriptor(),
            "fsync-io",
        );
    }

    // Kills the Display impl mutants: each variant renders distinctly and
    // the IO variant carries the underlying message.
    #[test]
    fn display_renders_each_variant() {
        let ignored = format!("{}", FsyncProbeError::FsyncIgnored);
        let lost = format!("{}", FsyncProbeError::BytesLost);
        let mismatch = format!("{}", FsyncProbeError::BytesMismatch);
        let ioerr = format!("{}", FsyncProbeError::Io(io::Error::other("boom")));
        assert!(ignored.contains("ignored"), "got: {ignored}");
        assert!(lost.contains("lost"), "got: {lost}");
        assert!(mismatch.contains("mismatch"), "got: {mismatch}");
        assert!(ioerr.contains("boom"), "got: {ioerr}");
    }

    // Kills the From<io::Error> mutant: the conversion wraps the
    // underlying error rather than discarding it.
    #[test]
    fn io_error_round_trips_through_from_impl() {
        let e: FsyncProbeError = io::Error::new(io::ErrorKind::PermissionDenied, "nope").into();
        match e {
            FsyncProbeError::Io(inner) => {
                assert_eq!(inner.kind(), io::ErrorKind::PermissionDenied);
            }
            other => panic!("expected Io variant; got {other:?}"),
        }
    }

    // Kills the honest-probe and cleanup-on-success mutants: an honest
    // backend passes and the sentinel is removed afterwards.
    #[test]
    fn honest_probe_passes_and_removes_the_sentinel() {
        let root = temp_root("honest");
        fsync_probe(&root, &RealFsyncBackend).expect("honest substrate passes");
        assert!(
            !root.join(SENTINEL_FILENAME).exists(),
            "successful probe must clean up its sentinel",
        );
        cleanup_dir(&root);
    }

    // Kills the cleanup-on-failure mutant and the lying-no_op mutant: a
    // no-op substrate refuses with FsyncIgnored and still cleans up.
    #[test]
    fn lying_no_op_substrate_refuses_with_fsync_ignored_and_cleans_up() {
        let root = temp_root("lying_noop");
        let outcome = fsync_probe(&root, &LyingFsyncBackend::no_op());
        assert!(
            matches!(outcome, Err(FsyncProbeError::FsyncIgnored)),
            "no-op substrate must refuse with FsyncIgnored; got {outcome:?}",
        );
        assert!(
            !root.join(SENTINEL_FILENAME).exists(),
            "failed probe must still clean up its sentinel",
        );
        cleanup_dir(&root);
    }

    // Kills the truncating-substrate mutant: a one-byte truncation is
    // observed as BytesLost.
    #[test]
    fn lying_truncating_substrate_refuses_with_bytes_lost() {
        let root = temp_root("lying_trunc");
        let outcome = fsync_probe(&root, &LyingFsyncBackend::truncating());
        assert!(
            matches!(outcome, Err(FsyncProbeError::BytesLost)),
            "truncating substrate must refuse with BytesLost; got {outcome:?}",
        );
        cleanup_dir(&root);
    }

    // Kills the byte-flipping-substrate mutant: a flipped byte at the
    // same length is observed as BytesMismatch.
    #[test]
    fn lying_byte_flipping_substrate_refuses_with_bytes_mismatch() {
        let root = temp_root("lying_flip");
        let outcome = fsync_probe(&root, &LyingFsyncBackend::byte_flipping());
        assert!(
            matches!(outcome, Err(FsyncProbeError::BytesMismatch)),
            "byte-flipping substrate must refuse with BytesMismatch; got {outcome:?}",
        );
        cleanup_dir(&root);
    }

    // Kills the create-missing-root mutant: when the caller passes a
    // non-existent root the probe creates it rather than failing.
    #[test]
    fn probe_creates_a_missing_root_directory() {
        let mut root = temp_root("missing_parent");
        root.push("nested");
        root.push("deeper");
        assert!(!root.exists());
        fsync_probe(&root, &RealFsyncBackend).expect("probe creates missing root");
        assert!(root.exists());
        let mut top = root.clone();
        top.pop();
        top.pop();
        cleanup_dir(&top);
    }

    // Kills the RealFsyncBackend::fsync_file / fsync_dir mutants: the
    // honest implementation actually returns Ok against a real tempdir
    // (the same contract the probe and snapshot rely on).
    #[test]
    fn real_backend_fsyncs_a_real_file_and_dir() {
        let root = temp_root("real_backend");
        let path = root.join("touch");
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&path)
            .expect("open");
        let mut writer = file.try_clone().expect("clone");
        writer.write_all(b"hello").expect("write");
        RealFsyncBackend.fsync_file(&file).expect("fsync_file ok");
        RealFsyncBackend.fsync_dir(&root).expect("fsync_dir ok");
        cleanup_dir(&root);
    }

    // Kills the `fsync_dir -> Ok(())` mutant: the honest dir fsync opens
    // the directory and sync_all's it, so a NON-EXISTENT directory makes
    // the real impl return Err (File::open fails) while the Ok(()) mutant
    // would wrongly succeed.
    #[cfg(unix)]
    #[test]
    fn real_backend_fsync_dir_errors_on_a_missing_directory() {
        let root = temp_root("real_fsync_dir_missing");
        let missing = root.join("does-not-exist");
        let outcome = RealFsyncBackend.fsync_dir(&missing);
        assert!(
            outcome.is_err(),
            "an honest dir fsync must fail on a missing directory, not silently succeed",
        );
        cleanup_dir(&root);
    }

    // Kills the `> with >=` mutants in truncating/no_op: fsyncing an EMPTY
    // file must NOT underflow `len - 1` (the `len > 0` guard). The `>=`
    // mutant would take the branch at len==0 and compute `0 - 1`,
    // panicking on debug overflow.
    #[test]
    fn lying_truncating_on_an_empty_file_does_not_underflow() {
        let root = temp_root("lying_trunc_empty");
        let path = root.join("empty");
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&path)
            .expect("open empty file");
        // No write: file length is 0.
        LyingFsyncBackend::truncating()
            .fsync_file(&file)
            .expect("truncating on an empty file is a no-op, not an underflow");
        drop(file);
        let bytes = std::fs::read(&path).expect("reread");
        assert!(bytes.is_empty(), "an empty file stays empty; got {bytes:?}");
        cleanup_dir(&root);
    }

    // Kills the `> with >=` mutant in byte_flipping: fsyncing an EMPTY
    // file must take the no-op branch (`len > 0` is false). The `>=`
    // mutant would enter the branch at len==0 and `read_exact` one byte
    // from an empty file, returning Err instead of the honest Ok.
    #[test]
    fn lying_byte_flipping_on_an_empty_file_is_a_no_op() {
        let root = temp_root("lying_flip_empty");
        let path = root.join("empty");
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&path)
            .expect("open empty file");
        LyingFsyncBackend::byte_flipping()
            .fsync_file(&file)
            .expect("byte_flipping on an empty file returns Ok, not an EOF error");
        drop(file);
        let bytes = std::fs::read(&path).expect("reread");
        assert!(bytes.is_empty(), "an empty file stays empty; got {bytes:?}");
        cleanup_dir(&root);
    }

    // Kills the `^= with |=` mutant in byte_flipping: flipping a byte that
    // is ALREADY 0xff distinguishes XOR (0xff ^ 0xff = 0x00, changes) from
    // OR (0xff | 0xff = 0xff, no change). The first sentinel byte is 0xff.
    #[test]
    fn lying_byte_flipping_xors_rather_than_ors() {
        let root = temp_root("lying_flip_xor");
        let path = root.join("ff");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&path)
            .expect("open");
        file.write_all(&[0xff, 0x00, 0x00]).expect("write");
        LyingFsyncBackend::byte_flipping()
            .fsync_file(&file)
            .expect("byte_flipping returns ok");
        drop(file);
        let bytes = std::fs::read(&path).expect("reread");
        assert_eq!(
            bytes[0], 0x00,
            "XOR of 0xff with 0xff is 0x00; an OR would leave it 0xff",
        );
        cleanup_dir(&root);
    }

    // Kills the atomic_write_snapshot happy-path mutants: the bytes the
    // `write` closure produces land at the canonical path after a
    // tmp+fsync+rename+fsync-dir round trip, and the temp file does not
    // remain (the rename moved it).
    #[test]
    fn atomic_write_snapshot_lands_bytes_at_the_canonical_path() {
        let root = temp_root("atomic_happy");
        let canonical = root.join("store.snapshot");
        atomic_write_snapshot(&canonical, &RealFsyncBackend, |w| {
            w.write_all(b"snapshot-payload")
        })
        .expect("atomic snapshot succeeds");

        let mut got = Vec::new();
        File::open(&canonical)
            .expect("canonical exists")
            .read_to_end(&mut got)
            .expect("read");
        assert_eq!(got, b"snapshot-payload");

        let mut tmp = canonical.as_os_str().to_owned();
        tmp.push(".tmp");
        assert!(
            !PathBuf::from(tmp).exists(),
            "the temp file is renamed away, not left behind",
        );
        cleanup_dir(&root);
    }

    // Kills the rename mutant in atomic_write_snapshot: a SECOND snapshot
    // REPLACES the first whole-or-absent. If the rename were dropped the
    // canonical path would still hold the first payload.
    #[test]
    fn atomic_write_snapshot_replaces_a_prior_snapshot_wholesale() {
        let root = temp_root("atomic_replace");
        let canonical = root.join("store.snapshot");
        atomic_write_snapshot(&canonical, &RealFsyncBackend, |w| w.write_all(b"first"))
            .expect("first snapshot");
        atomic_write_snapshot(&canonical, &RealFsyncBackend, |w| {
            w.write_all(b"second-longer")
        })
        .expect("second snapshot");

        let mut got = Vec::new();
        File::open(&canonical)
            .expect("canonical exists")
            .read_to_end(&mut got)
            .expect("read");
        assert_eq!(
            got, b"second-longer",
            "the second whole snapshot replaces the first at the canonical path",
        );
        cleanup_dir(&root);
    }

    // Kills the write-error-propagation mutant: an error from the `write`
    // closure aborts the snapshot and leaves NO canonical file (the
    // rename never runs), so a crash mid-serialise cannot tear the
    // canonical path.
    #[test]
    fn atomic_write_snapshot_propagates_write_errors_without_touching_canonical() {
        let root = temp_root("atomic_write_err");
        let canonical = root.join("store.snapshot");
        let outcome = atomic_write_snapshot(&canonical, &RealFsyncBackend, |_w| {
            Err(io::Error::other("serialise blew up"))
        });
        assert!(outcome.is_err(), "the write error is propagated");
        assert!(
            !canonical.exists(),
            "a failed serialise never renames onto the canonical path",
        );
        cleanup_dir(&root);
    }
}
