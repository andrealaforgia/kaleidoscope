// Kaleidoscope Loom — Git-backed change-control surface
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

//! `loom apply` — make the destination directory match the source
//! using atomic file operations and idempotent semantics.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use beacon::{load_rules, LoaderDiagnostic};

/// Result of one `loom apply` invocation.
#[derive(Debug)]
pub struct ApplyOutcome {
    /// `.toml` files written or replaced.
    pub written: Vec<PathBuf>,
    /// `.toml` files removed from the destination.
    pub removed: Vec<PathBuf>,
    /// `.toml` files unchanged (byte-equal source + destination).
    pub unchanged: Vec<PathBuf>,
    /// Diagnostics from the source validate step. Apply refuses to
    /// write if this is non-empty.
    pub diagnostics: Vec<LoaderDiagnostic>,
    /// Hard error from the directory walk or a write failure.
    pub fatal: Option<String>,
}

impl ApplyOutcome {
    /// Exit code per slice 03 AC-3.4.
    ///
    /// - `0` — applied (or no-op on idempotent re-run)
    /// - `1` — source validation failed; no writes happened
    /// - `2` — filesystem error during apply
    pub fn exit_code(&self) -> u8 {
        if self.fatal.is_some() {
            return 2;
        }
        if !self.diagnostics.is_empty() {
            return 1;
        }
        0
    }

    /// Operator-readable summary in the same shape as
    /// [`crate::PlanOutcome::render`].
    pub fn render(&self) -> String {
        format!(
            "summary: {} written, {} removed, {} unchanged\n",
            self.written.len(),
            self.removed.len(),
            self.unchanged.len(),
        )
    }
}

/// Make `to` match `from`. Source must validate cleanly; otherwise
/// no writes happen and the diagnostics propagate.
///
/// Atomicity: each `.toml` file is written to a sibling `.tmp` path,
/// fsynced, and renamed onto the final path. The rename is atomic
/// on POSIX. Non-`.toml` files in `to` are preserved untouched.
pub fn apply(from: &Path, to: &Path) -> ApplyOutcome {
    // Validate source first.
    let source = match load_rules(from) {
        Ok(o) => o,
        Err(err) => {
            return ApplyOutcome {
                written: Vec::new(),
                removed: Vec::new(),
                unchanged: Vec::new(),
                diagnostics: Vec::new(),
                fatal: Some(format!("from: {err}")),
            };
        }
    };
    if !source.diagnostics.is_empty() {
        return ApplyOutcome {
            written: Vec::new(),
            removed: Vec::new(),
            unchanged: Vec::new(),
            diagnostics: source.diagnostics,
            fatal: None,
        };
    }

    // Walk the source and the destination, collecting `.toml`
    // paths relative to their roots so we can match them up.
    let source_files = match collect_relative_toml(from) {
        Ok(files) => files,
        Err(err) => {
            return ApplyOutcome {
                written: Vec::new(),
                removed: Vec::new(),
                unchanged: Vec::new(),
                diagnostics: Vec::new(),
                fatal: Some(format!("from walk: {err}")),
            };
        }
    };
    // Ensure destination exists before walking it.
    if let Err(err) = fs::create_dir_all(to) {
        return ApplyOutcome {
            written: Vec::new(),
            removed: Vec::new(),
            unchanged: Vec::new(),
            diagnostics: Vec::new(),
            fatal: Some(format!("to mkdir: {err}")),
        };
    }
    let dest_files = match collect_relative_toml(to) {
        Ok(files) => files,
        Err(err) => {
            return ApplyOutcome {
                written: Vec::new(),
                removed: Vec::new(),
                unchanged: Vec::new(),
                diagnostics: Vec::new(),
                fatal: Some(format!("to walk: {err}")),
            };
        }
    };

    let mut written = Vec::new();
    let mut removed = Vec::new();
    let mut unchanged = Vec::new();
    let mut fatal: Option<String> = None;

    // Write or skip every source file based on byte-equality.
    for (rel, src_path) in &source_files {
        let dst_path = to.join(rel);
        let new_bytes = match fs::read(src_path) {
            Ok(b) => b,
            Err(err) => {
                fatal = Some(format!("read {}: {err}", src_path.display()));
                break;
            }
        };
        let same = match fs::read(&dst_path) {
            Ok(existing) => existing == new_bytes,
            Err(_) => false,
        };
        if same {
            unchanged.push(dst_path);
        } else {
            match atomic_write(&dst_path, &new_bytes) {
                Ok(()) => written.push(dst_path),
                Err(err) => {
                    fatal = Some(format!("write {}: {err}", dst_path.display()));
                    break;
                }
            }
        }
    }

    if fatal.is_none() {
        // Remove any `.toml` in destination that has no counterpart
        // in source.
        for (rel, dst_path) in &dest_files {
            if !source_files.contains_key(rel) {
                if let Err(err) = fs::remove_file(dst_path) {
                    fatal = Some(format!("remove {}: {err}", dst_path.display()));
                    break;
                }
                removed.push(dst_path.clone());
            }
        }
    }

    ApplyOutcome {
        written,
        removed,
        unchanged,
        diagnostics: Vec::new(),
        fatal,
    }
}

/// Walk `root` recursively, collecting every `.toml` path keyed by
/// its path relative to `root`. The relative path is the join key
/// between source and destination.
fn collect_relative_toml(root: &Path) -> std::io::Result<BTreeMap<PathBuf, PathBuf>> {
    let mut out = BTreeMap::new();
    walk(root, root, &mut out)?;
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, PathBuf>) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            if let Ok(rel) = path.strip_prefix(root) {
                out.insert(rel.to_path_buf(), path);
            }
        }
    }
    Ok(())
}

/// Write `bytes` to `dest` atomically. Writes to `dest.tmp`, fsyncs,
/// renames onto `dest`. POSIX guarantees atomic rename within the
/// same filesystem.
fn atomic_write(dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = with_extension(dest, "tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, dest)?;
    Ok(())
}

fn with_extension(path: &Path, ext: &str) -> PathBuf {
    let mut out = path.as_os_str().to_owned();
    out.push(".");
    out.push(ext);
    PathBuf::from(out)
}
