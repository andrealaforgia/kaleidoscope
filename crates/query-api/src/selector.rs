// Kaleidoscope query-api — minimal PromQL selector parser
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

//! The v0 PromQL boundary (DD3 / ADR-0042 Decision 3).
//!
//! Slice 01 accepts ONLY a bare metric-name selector. After trimming
//! surrounding ASCII whitespace, the entire query MUST match the
//! Prometheus metric-name production `[a-zA-Z_:][a-zA-Z0-9_:]*` and
//! nothing else. Anything else (empty, `{`-matcher, `[`-range-vector,
//! `(`-function/aggregation, any operator character, whitespace-
//! separated tokens) is honestly rejected as unsupported rather than
//! silently mis-answered.

use pulse::MetricName;

/// Parse the raw query string into a bare metric-name selector.
///
/// On success the trimmed metric name is returned. On rejection a
/// human-readable reason is returned for the `status:error` body. The
/// reason NEVER echoes the raw query (DD6 redaction symmetry): a
/// forwarded secret pasted into the query must not be reflected back.
pub fn parse(raw: &str) -> Result<MetricName, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("unsupported query: expected a bare metric name".to_string());
    }
    if is_bare_metric_name(trimmed) {
        return Ok(MetricName::new(trimmed));
    }
    Err(
        "unsupported query: only a bare metric name is supported at v0 \
         (label matchers, range vectors, functions, aggregations, and \
         operators are not yet supported)"
            .to_string(),
    )
}

/// True when `name` matches the whole Prometheus metric-name production
/// `[a-zA-Z_:][a-zA-Z0-9_:]*`. The caller has already trimmed.
fn is_bare_metric_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if is_name_start(first) => {}
        _ => return false,
    }
    chars.all(is_name_continue)
}

/// First-character class: `[a-zA-Z_:]`.
fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == ':'
}

/// Continuation-character class: `[a-zA-Z0-9_:]`.
fn is_name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == ':'
}

#[cfg(test)]
mod tests {
    use super::*;

    // The acceptance suite reaches the accept path (walking skeleton)
    // and the reject paths (function, operator, empty, range vector,
    // aggregation, matcher, whitespace-trim). These inline tests pin
    // the boundary characters the acceptance suite does not exercise
    // one-by-one, so a mutation that widens or narrows the character
    // classes is caught against a single assertion.

    #[test]
    fn accepts_a_bare_name_with_colon_and_underscore_and_digits() {
        // `:` and `_` are valid name characters; digits are valid in
        // the continuation but not the start. A leading digit must be
        // rejected (kills a mutant that uses is_name_continue for the
        // first char).
        assert!(parse("node:cpu_seconds_total9").is_ok());
        assert!(parse("_internal").is_ok());
        assert!(parse(":colon_start").is_ok());
    }

    #[test]
    fn rejects_a_leading_digit() {
        assert!(parse("9metric").is_err());
    }

    #[test]
    fn rejects_embedded_whitespace_and_punctuation() {
        // A space inside the trimmed name, and any non-name byte, must
        // reject. These cover the multi-token and operator forms at the
        // character level.
        assert!(parse("two tokens").is_err());
        assert!(parse("has-dash").is_err());
        assert!(parse("has.dot").is_err());
    }

    #[test]
    fn an_empty_or_whitespace_only_query_is_rejected() {
        assert!(parse("").is_err());
        assert!(parse("   ").is_err());
    }

    #[test]
    fn the_reason_never_echoes_the_raw_query() {
        // DD6: a forwarded secret pasted into the query must not be
        // reflected into the error text.
        let reason = parse("rate(secret_token_xyz[5m])").expect_err("rejected");
        assert!(!reason.contains("secret_token_xyz"));
    }
}
