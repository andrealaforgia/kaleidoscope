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

//! JSON rendering for `--json` flag on `loom validate` and `loom
//! plan`. Per US-LO-04 AC-4.3: stable schema field at the top of
//! every payload so consumers can version-gate their parsing.

use serde::Serialize;

use crate::plan::PlanOutcome;
use crate::ValidateOutcome;

/// Schema version baked into every JSON payload. Bumped when the
/// structure changes incompatibly. Slice 04 ships `loom.v0`; a
/// hypothetical v1 with a new field would bump to `loom.v1`.
pub const SCHEMA: &str = "loom.v0";

#[derive(Debug, Serialize)]
struct ValidateJson<'a> {
    schema: &'a str,
    rules_loaded: usize,
    diagnostics: Vec<DiagnosticJson<'a>>,
    fatal: Option<&'a str>,
    exit_code: u8,
}

#[derive(Debug, Serialize)]
struct DiagnosticJson<'a> {
    file: String,
    message: &'a str,
    suggestion: Option<&'a str>,
}

/// Serialise a [`ValidateOutcome`] as JSON for CI tooling.
pub fn render_validate_json(outcome: &ValidateOutcome) -> String {
    let payload = ValidateJson {
        schema: SCHEMA,
        rules_loaded: outcome.rules_loaded,
        diagnostics: outcome
            .diagnostics
            .iter()
            .map(|d| DiagnosticJson {
                file: d.file.display().to_string(),
                message: &d.message,
                suggestion: d.suggestion.as_deref(),
            })
            .collect(),
        fatal: outcome.fatal.as_deref(),
        exit_code: outcome.exit_code(),
    };
    serde_json::to_string_pretty(&payload).expect("validate JSON serialise")
}

#[derive(Debug, Serialize)]
struct PlanJson<'a> {
    schema: &'a str,
    added: &'a [String],
    removed: &'a [String],
    changed: Vec<ChangeJson<'a>>,
    diagnostics_from: Vec<DiagnosticJson<'a>>,
    diagnostics_to: Vec<DiagnosticJson<'a>>,
    fatal: Option<&'a str>,
    exit_code: u8,
}

#[derive(Debug, Serialize)]
struct ChangeJson<'a> {
    name: &'a str,
    fields: Vec<FieldJson<'a>>,
}

#[derive(Debug, Serialize)]
struct FieldJson<'a> {
    field: &'a str,
    before: &'a str,
    after: &'a str,
}

/// Serialise a [`PlanOutcome`] as JSON for CI tooling.
pub fn render_plan_json(outcome: &PlanOutcome) -> String {
    let payload = PlanJson {
        schema: SCHEMA,
        added: &outcome.added,
        removed: &outcome.removed,
        changed: outcome
            .changed
            .iter()
            .map(|c| ChangeJson {
                name: &c.name,
                fields: c
                    .fields
                    .iter()
                    .map(|f| FieldJson {
                        field: f.field,
                        before: &f.before,
                        after: &f.after,
                    })
                    .collect(),
            })
            .collect(),
        diagnostics_from: outcome
            .diagnostics_from
            .iter()
            .map(|d| DiagnosticJson {
                file: d.file.display().to_string(),
                message: &d.message,
                suggestion: d.suggestion.as_deref(),
            })
            .collect(),
        diagnostics_to: outcome
            .diagnostics_to
            .iter()
            .map(|d| DiagnosticJson {
                file: d.file.display().to_string(),
                message: &d.message,
                suggestion: d.suggestion.as_deref(),
            })
            .collect(),
        fatal: outcome.fatal.as_deref(),
        exit_code: outcome.exit_code(),
    };
    serde_json::to_string_pretty(&payload).expect("plan JSON serialise")
}
