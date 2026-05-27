// Kaleidoscope Pulse — Earned-Trust fsync-honesty probe
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

//! Earned-Trust fsync-honesty probe (ADR-0049).
//!
//! The probe writes a sentinel under a root directory, asks the
//! [`FsyncBackend`] to durably persist it, drops the handle, reopens
//! a fresh handle, reads the bytes back, and compares. On any
//! discrepancy the probe refuses to start with a typed
//! [`FsyncProbeError`] that the composition root maps to
//! `event=health.startup.refused` with a `substrate=<descriptor>`
//! payload field.
//!
//! Public surface (frozen by DESIGN ADR-0049 §6):
//!
//! - [`FsyncBackend`] — the seam the probe and the WAL append path
//!   share so a lying substrate can be injected in tests without
//!   mounting a hostile filesystem.
//! - [`RealFsyncBackend`] — production implementation delegating to
//!   `std::fs::File::sync_all` (file handle) and the platform's
//!   parent-directory fsync (POSIX rename durability).
//! - [`LyingFsyncBackend`] — test double with three lie modes
//!   (`no_op`, `truncating`, `byte_flipping`) mirroring
//!   `LyingLogStore` / `LyingTraceStore` in the read APIs.
//! - [`FsyncProbeError`] — typed refusal classes (`FsyncIgnored`,
//!   `BytesLost`, `BytesMismatch`, `Io`) the composition root maps
//!   to the `substrate=<descriptor>` payload on
//!   `event=health.startup.refused`.
//! - [`fsync_probe`] — the free function the gateway's composition
//!   root calls before binding a listener.
//!
//! ## Mapping of lie modes to refusal classes
//!
//! The probe reports BEHAVIOUR, not claims. A backend that returns
//! `Ok(())` from `fsync_file` while leaving the substrate in a bad
//! state is observed by the round-trip read:
//!
//! - [`LyingFsyncBackend::no_op`] — `fsync_file` truncates the file
//!   to zero bytes before returning `Ok(())`. The reopened handle
//!   finds the file empty; the probe reports
//!   [`FsyncProbeError::FsyncIgnored`] (`substrate=fsync-noop`).
//! - [`LyingFsyncBackend::truncating`] — `fsync_file` returns
//!   `Ok(())` then shortens the file by one byte. The reopened
//!   handle finds the file present but shorter than the sentinel;
//!   the probe reports [`FsyncProbeError::BytesLost`]
//!   (`substrate=fsync-truncating`).
//! - [`LyingFsyncBackend::byte_flipping`] — `fsync_file` returns
//!   `Ok(())` then flips the first byte of the file. The reopened
//!   handle finds the file at the expected length but with
//!   different bytes; the probe reports
//!   [`FsyncProbeError::BytesMismatch`]
//!   (`substrate=fsync-corrupting`).

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Fixed sentinel filename written under the probe root. The path is
/// FIXED (not randomised): the probe overwrites this file on every
/// run so no state accumulates across restarts. Documented as a
/// known artefact of the probe in ADR-0049 §Consequences.
const SENTINEL_FILENAME: &str = ".fsync-probe";

/// Sentinel payload. 64 bytes, ASCII, fixed content so the
/// round-trip comparison is deterministic.
const SENTINEL_BYTES: &[u8; 64] =
    b"kaleidoscope-fsync-probe-sentinel-v0-do-not-edit-or-remove-XXXX\n";

/// Seam over `fsync` so the probe and the WAL append path can be
/// exercised against a lying substrate in unit tests without mounting
/// a hostile filesystem (ADR-0049 §6).
pub trait FsyncBackend {
    /// Sync the file's data and metadata to stable storage. Maps to
    /// `File::sync_all` on POSIX (`fsync(2)`) and `FlushFileBuffers`
    /// on Windows. The honest implementation MUST persist; the lying
    /// implementations in tests return `Ok(())` while leaving the
    /// substrate in a bad state.
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
    fn fsync_file(&self, file: &File) -> io::Result<()> {
        file.sync_all()
    }

    fn fsync_dir(&self, dir: &Path) -> io::Result<()> {
        // On Unix the parent directory must be opened read-only and
        // sync_all'd to make rename durability work. On Windows
        // directory fsync is not meaningful (FlushFileBuffers on a
        // directory handle is documented not to flush the directory
        // entry); we treat it as a no-op there for v0. The semantic
        // gap is documented in ADR-0049 §Consequences (Windows is a
        // coarse-grained platform for the substrate descriptor
        // classes).
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

/// Test double of [`FsyncBackend`] simulating three classes of
/// substrate lie (ADR-0049 §6). Returns `Ok(())` from `fsync_file`
/// in every mode but leaves the substrate in a bad state matching the
/// lie class:
///
/// - [`LyingFsyncBackend::no_op`] — does not persist (a fresh handle
///   reads back an empty file);
/// - [`LyingFsyncBackend::truncating`] — silently shortens the file
///   by one byte;
/// - [`LyingFsyncBackend::byte_flipping`] — keeps the length but
///   flips the first byte so the bytes read back differ from the
///   bytes written.
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

    /// Lie mode: fsync returns `Ok(())` but the substrate flips a
    /// byte; a fresh handle finds the file at the expected length but
    /// with different bytes.
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
                // file: the probe observes "the bytes never persisted"
                // via the round-trip read.
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
                    // Flip the first byte. We can clone the handle
                    // (which shares the underlying file) so we can
                    // read+write without taking `&mut File`.
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
        // The directory-fsync surface is exercised by the
        // CountingFsyncBackend in the slice 03 acceptance tests
        // (write-path durability); the LyingFsyncBackend's job is
        // file-level lies, not directory-level. Returning Ok keeps
        // the probe's no_op variant focused on the bytes-vanished
        // class.
        Ok(())
    }
}

/// Typed refusal classes from the fsync-honesty probe. The
/// composition root in `kaleidoscope-gateway/src/main.rs` maps each
/// variant to a `substrate=<descriptor>` payload field on the
/// existing `event=health.startup.refused` event (ADR-0049 §7).
#[derive(Debug)]
pub enum FsyncProbeError {
    /// The sentinel was written and fsynced but the fresh handle
    /// read back nothing: the substrate's fsync is a no-op
    /// (`substrate=fsync-noop`).
    FsyncIgnored,
    /// The sentinel was written and fsynced but the fresh handle
    /// read back a shorter file: the substrate silently truncated
    /// (`substrate=fsync-truncating`).
    BytesLost,
    /// The sentinel was written and fsynced; the fresh handle read
    /// back the expected length but different bytes
    /// (`substrate=fsync-corrupting`).
    BytesMismatch,
    /// The probe could not run because an IO step (open, write,
    /// sync, reopen, read) errored
    /// (`substrate=fsync-io`).
    Io(io::Error),
}

impl FsyncProbeError {
    /// Map the refusal class to the `substrate=<descriptor>` payload
    /// the composition root attaches to `event=health.startup.refused`
    /// (ADR-0049 §7). Kept as a method so the mapping is a single
    /// source of truth callers cannot drift from.
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
/// reads back, and compares. On any mismatch returns the typed
/// refusal class. On success removes its sentinel and returns
/// `Ok(())`.
///
/// Called from the gateway's composition root BEFORE the listener
/// binds (wire-then-probe-then-use, ADR-0042 Decision 8 preserved).
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

    // Phase 3: classify and (on failure) clean up the sentinel before
    // returning; on success also clean up. The sentinel is FIXED and
    // overwritten on every probe run, but a successful run cleans it
    // up so `pillar_root` does not accumulate artefacts (ADR-0049
    // §Consequences).
    let outcome = classify_roundtrip(&roundtrip, SENTINEL_BYTES);

    // Cleanup attempt: best-effort. If removal fails (e.g. on a
    // read-only filesystem) we still surface the probe outcome
    // rather than masking it with the cleanup error.
    let _ = std::fs::remove_file(&sentinel);

    outcome
}

/// Compare the round-tripped bytes against the sentinel and classify
/// the outcome. Extracted as a free function so the three lie classes
/// can be exercised via unit tests without touching the filesystem.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "pulse-fsync-probe-unit-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&p).expect("mkdir");
        p
    }

    fn cleanup(root: &Path) {
        let _ = std::fs::remove_dir_all(root);
    }

    // Kills the classify_roundtrip mutants: each branch maps to a
    // distinct refusal class, and the equal-length-and-bytes branch
    // is Ok. The classifier is the single source of truth the probe
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

    // Kills the substrate_descriptor mutants: each error variant has
    // a distinct descriptor that the composition root surfaces on
    // event=health.startup.refused. If two variants drifted to the
    // same descriptor, operators could not distinguish the lie
    // classes.
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

    // Kills the Display impl mutants: each variant has a distinct
    // human-readable rendering, and the IO variant carries the
    // underlying error message.
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

    // Kills the cleanup-on-success mutant in `fsync_probe`: the
    // honest path removes the sentinel.
    #[test]
    fn honest_probe_removes_the_sentinel_on_success() {
        let root = temp_root("honest_cleanup");
        let backend = RealFsyncBackend;
        fsync_probe(&root, &backend).expect("honest substrate passes");
        assert!(
            !root.join(SENTINEL_FILENAME).exists(),
            "successful probe must clean up its sentinel",
        );
        cleanup(&root);
    }

    // Kills the cleanup-on-failure mutant in `fsync_probe`: the
    // failure path also removes the sentinel, so a lying substrate
    // does not leave stale bytes that could confuse subsequent
    // probes.
    #[test]
    fn lying_probe_removes_the_sentinel_on_failure() {
        let root = temp_root("lying_cleanup");
        let backend = LyingFsyncBackend::no_op();
        let outcome = fsync_probe(&root, &backend);
        assert!(outcome.is_err(), "no-op substrate must refuse");
        assert!(
            !root.join(SENTINEL_FILENAME).exists(),
            "failed probe must still clean up its sentinel",
        );
        cleanup(&root);
    }

    // Kills the "create_dir_all is removable" mutant in
    // `fsync_probe`: when the caller passes a non-existent root the
    // probe creates it rather than failing.
    #[test]
    fn probe_creates_a_missing_root_directory() {
        let mut root = temp_root("missing_parent");
        root.push("nested");
        root.push("deeper");
        assert!(!root.exists());
        let backend = RealFsyncBackend;
        fsync_probe(&root, &backend).expect("probe creates missing root");
        assert!(root.exists());
        let mut top = root.clone();
        top.pop();
        top.pop();
        cleanup(&top);
    }

    // Kills the RealFsyncBackend::fsync_file mutant: the honest
    // implementation actually persists. We cannot directly observe
    // fsync(2) but we can confirm the call returns Ok against a real
    // tempdir, which is the same contract the probe relies on.
    #[test]
    fn real_backend_fsync_file_returns_ok_on_a_real_file() {
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
        let backend = RealFsyncBackend;
        backend.fsync_file(&file).expect("real fsync_file ok");
        backend.fsync_dir(&root).expect("real fsync_dir ok");
        cleanup(&root);
    }

    // Kills the LyingFsyncBackend::no_op mutant: no_op truncates so
    // the reopened file is empty.
    #[test]
    fn lying_no_op_leaves_an_empty_file() {
        let root = temp_root("lying_no_op_observable");
        let path = root.join("sentinel");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&path)
            .expect("open");
        file.write_all(b"abcdef").expect("write");
        LyingFsyncBackend::no_op()
            .fsync_file(&file)
            .expect("no_op returns ok");
        drop(file);
        let bytes = std::fs::read(&path).expect("reread");
        assert!(bytes.is_empty(), "no_op zeroes the file; got {bytes:?}");
        cleanup(&root);
    }

    // Kills the LyingFsyncBackend::truncating mutant: truncating
    // shortens the file by one byte.
    #[test]
    fn lying_truncating_drops_one_byte() {
        let root = temp_root("lying_truncating_observable");
        let path = root.join("sentinel");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&path)
            .expect("open");
        file.write_all(b"abcdef").expect("write");
        LyingFsyncBackend::truncating()
            .fsync_file(&file)
            .expect("truncating returns ok");
        drop(file);
        let bytes = std::fs::read(&path).expect("reread");
        assert_eq!(bytes.len(), 5, "truncating drops one byte; got {bytes:?}");
        cleanup(&root);
    }

    // Kills the LyingFsyncBackend::byte_flipping mutant: same length
    // but the first byte differs.
    #[test]
    fn lying_byte_flipping_changes_a_byte() {
        let root = temp_root("lying_flip_observable");
        let path = root.join("sentinel");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&path)
            .expect("open");
        file.write_all(b"abcdef").expect("write");
        LyingFsyncBackend::byte_flipping()
            .fsync_file(&file)
            .expect("byte_flipping returns ok");
        drop(file);
        let bytes = std::fs::read(&path).expect("reread");
        assert_eq!(bytes.len(), 6, "length is unchanged");
        assert_ne!(bytes[0], b'a', "first byte was flipped");
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
}
