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

//! # Loom — change-control surface for Beacon rule catalogues
//!
//! Slice 01 ships `loom validate`: wraps Beacon's `load_rules`
//! function and maps its outcome to operator-readable diagnostics
//! plus stable exit codes. Slice 02 adds `loom plan` (per-rule
//! diff); slice 03 adds `loom apply` (atomic file operations);
//! slice 04 adds JSON output for CI integration.
//!
//! ## Public surface
//!
//! - [`validate`] — the slice 01 entry point. Takes a directory
//!   path, returns a [`ValidateOutcome`] carrying loaded-rule
//!   count, per-file diagnostics, and the canonical exit code.
//!
//! ## Architectural posture
//!
//! - **Library plus CLI binary.** The library is testable in
//!   isolation; the binary is a thin shell around it.
//! - **No I/O beyond filesystem reads.** Loom v0 reads `.toml`
//!   files, that is all.
//! - **AGPL-3.0-or-later.** Symmetric with the rest of the
//!   platform.

#![forbid(unsafe_code)]

use std::path::Path;

use beacon::{load_rules, LoaderDiagnostic};

/// Result of one `loom validate` invocation against a directory.
#[derive(Debug)]
pub struct ValidateOutcome {
    /// Number of rules that loaded successfully across every file.
    pub rules_loaded: usize,
    /// Diagnostics — one per file that failed to parse.
    pub diagnostics: Vec<LoaderDiagnostic>,
    /// Hard error from the directory walk (unreadable directory).
    /// `Some` means the directory could not be read at all;
    /// `None` means the walk succeeded (per-file diagnostics may
    /// still be present in `diagnostics`).
    pub fatal: Option<String>,
}

impl ValidateOutcome {
    /// Canonical exit code per slice 01 AC-1.2 .. AC-1.4.
    ///
    /// - `0` — every rule loaded, no diagnostics
    /// - `1` — at least one diagnostic (some rules rejected)
    /// - `2` — directory unreadable
    pub fn exit_code(&self) -> u8 {
        if self.fatal.is_some() {
            return 2;
        }
        if !self.diagnostics.is_empty() {
            return 1;
        }
        0
    }
}

/// Validate every `.toml` file in `dir` against Beacon's loader.
///
/// Returns a [`ValidateOutcome`] regardless of success or failure.
/// The caller (CLI shell) is expected to map the outcome to stderr
/// lines + exit code via [`ValidateOutcome::exit_code`].
pub fn validate(dir: &Path) -> ValidateOutcome {
    match load_rules(dir) {
        Ok(load_outcome) => ValidateOutcome {
            rules_loaded: load_outcome.rules.len(),
            diagnostics: load_outcome.diagnostics,
            fatal: None,
        },
        Err(err) => ValidateOutcome {
            rules_loaded: 0,
            diagnostics: Vec::new(),
            fatal: Some(err.to_string()),
        },
    }
}
