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

//! The v0 PromQL boundary (DD3 / ADR-0042 Decision 3, refined by
//! ADR-0044).
//!
//! After trimming surrounding ASCII whitespace, the selector is
//! `metric_name [ "{" matcher_list "}" ]`:
//!
//! - `metric_name` is the unchanged Prometheus metric-name production
//!   `[a-zA-Z_:][a-zA-Z0-9_:]*` and still selects the metric via Pulse.
//! - `matcher_list` is zero or more comma-separated matchers (a single
//!   trailing comma tolerated), each `label_name op string_literal`.
//! - `op` is `=` or `!=` ONLY; regex `=~`/`!~` is an honest 400.
//! - `string_literal` is double-quoted with the minimal escapes `\"`,
//!   `\\`, `\n`, `\t`; an empty value `""` is valid and load-bearing.
//! - `label_name` is `[a-zA-Z_:][a-zA-Z0-9_:.]*` (dots ALLOWED, an
//!   OTel-shaped divergence so `service.name` is nameable; ADR-0044
//!   Decision 2).
//!
//! Anything else (functions, range vectors, aggregations, operators
//! outside braces, an unterminated brace, an unquoted value, an empty
//! label name, a bad escape, trailing junk) is honestly rejected as a
//! 400 rather than silently mis-answered. A malformed brace section is
//! NEVER degraded to a bare-name query or a partial filter.

use pulse::MetricName;

/// A parsed selector: the metric name that drives the store query plus
/// the label matchers that filter the result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub name: MetricName,
    pub matchers: Vec<LabelMatcher>,
}

/// One label matcher: a derived-label-set key, an operator, and the
/// unquoted, unescaped literal value (which may be empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelMatcher {
    pub name: String,
    pub op: MatchOp,
    pub value: String,
}

/// The label-matcher operators. `Equal`/`NotEqual` are the exact-string
/// matchers; `Matches`/`NotMatches` are the regex matchers (`=~`/`!~`),
/// whose raw pattern is carried in [`LabelMatcher::value`] and compiled
/// filter-side (ADR-0046 Decision 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchOp {
    Equal,
    NotEqual,
    Matches,
    NotMatches,
}

/// Parse the raw query string into a [`Selector`].
///
/// On success the trimmed metric name and the (possibly empty) matcher
/// list are returned. On rejection a human-readable reason is returned
/// for the `status:error` body. The reason NEVER echoes the raw query
/// (DD6 redaction symmetry): a forwarded secret pasted into the query
/// must not be reflected back.
pub fn parse(raw: &str) -> Result<Selector, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("unsupported query: expected a bare metric name".to_string());
    }

    let (name_part, brace_part) = split_at_brace(trimmed)?;

    if !is_bare_metric_name(name_part) {
        return Err(unsupported_reason());
    }
    let name = MetricName::new(name_part);

    let matchers = match brace_part {
        Some(inner) => parse_matcher_list(inner)?,
        None => Vec::new(),
    };

    Ok(Selector { name, matchers })
}

/// The honest-reject reason for any non-matcher unsupported form (a
/// function, an aggregation, a range vector, an operator outside braces,
/// or a malformed metric name). Mirrors slice 01.
fn unsupported_reason() -> String {
    "unsupported query: only a bare metric name with optional = / != label \
     matchers is supported at v0 (range vectors, functions, aggregations, \
     and operators are not yet supported)"
        .to_string()
}

/// Split a trimmed selector into the metric-name part and the optional
/// brace contents (the text between `{` and the final `}`). A `{` with
/// no closing `}`, or trailing junk after `}`, is a malformed 400.
fn split_at_brace(trimmed: &str) -> Result<(&str, Option<&str>), String> {
    let Some(open) = trimmed.find('{') else {
        return Ok((trimmed, None));
    };
    if !trimmed.ends_with('}') {
        return Err("malformed query: the label matcher section is not closed".to_string());
    }
    let name_part = &trimmed[..open];
    // The closing brace is the final byte; the inner text is what lies
    // between them. An extra `}` inside surfaces as a matcher-syntax 400.
    let inner = &trimmed[open + 1..trimmed.len() - 1];
    Ok((name_part, Some(inner)))
}

/// Parse the comma-separated matcher list between the braces. An empty
/// (or whitespace-only) list is valid and yields no matchers. A single
/// trailing comma is tolerated.
fn parse_matcher_list(inner: &str) -> Result<Vec<LabelMatcher>, String> {
    let mut matchers = Vec::new();
    let mut chars = inner.chars().peekable();
    loop {
        skip_whitespace(&mut chars);
        if chars.peek().is_none() {
            return Ok(matchers);
        }
        matchers.push(parse_matcher(&mut chars)?);
        skip_whitespace(&mut chars);
        match chars.peek() {
            None => return Ok(matchers),
            Some(',') => {
                chars.next();
            }
            Some(_) => {
                return Err("malformed query: unrecognised label matcher syntax".to_string());
            }
        }
    }
}

/// Parse one `label_name op string_literal` matcher, consuming exactly
/// its characters (the caller handles the surrounding commas).
fn parse_matcher(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Result<LabelMatcher, String> {
    let name = read_label_name(chars)?;
    skip_whitespace(chars);
    let op = read_operator(chars)?;
    skip_whitespace(chars);
    let value = read_string_literal(chars)?;
    Ok(LabelMatcher { name, op, value })
}

/// Read a label name `[a-zA-Z_:][a-zA-Z0-9_:.]*`. An empty name (the
/// next character is not a name-start) is a malformed 400.
fn read_label_name(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, String> {
    let mut name = String::new();
    match chars.peek() {
        Some(&c) if is_name_start(c) => {
            name.push(c);
            chars.next();
        }
        _ => return Err("malformed query: a matcher is missing its label name".to_string()),
    }
    while let Some(&c) = chars.peek() {
        if is_label_name_continue(c) {
            name.push(c);
            chars.next();
        } else {
            break;
        }
    }
    Ok(name)
}

/// Read the operator `=`, `!=`, `=~`, or `!~`. The regex operators carry
/// their raw pattern in the matcher value and are compiled filter-side;
/// `=` followed by anything but `~` is `Equal`, `!=` followed by anything
/// but `~` is `NotEqual`. Anything else is a malformed 400.
fn read_operator(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<MatchOp, String> {
    match chars.next() {
        Some('=') => match chars.peek() {
            Some('~') => {
                chars.next();
                Ok(MatchOp::Matches)
            }
            _ => Ok(MatchOp::Equal),
        },
        Some('!') => match chars.next() {
            Some('~') => Ok(MatchOp::NotMatches),
            Some('=') => match chars.peek() {
                Some('~') => {
                    chars.next();
                    Err("malformed query: unrecognised label matcher syntax".to_string())
                }
                _ => Ok(MatchOp::NotEqual),
            },
            _ => Err("malformed query: unrecognised label matcher syntax".to_string()),
        },
        _ => Err("malformed query: unrecognised label matcher syntax".to_string()),
    }
}

/// Read a double-quoted string literal with the minimal escape set
/// `\"`, `\\`, `\n`, `\t`. An unquoted value, an unterminated string, or
/// an unknown escape is a malformed 400.
fn read_string_literal(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Result<String, String> {
    match chars.next() {
        Some('"') => {}
        _ => return Err("malformed query: matcher values must be double-quoted".to_string()),
    }
    let mut value = String::new();
    loop {
        match chars.next() {
            None => {
                return Err("malformed query: a matcher string literal is not closed".to_string());
            }
            Some('"') => return Ok(value),
            Some('\\') => match chars.next() {
                Some('"') => value.push('"'),
                Some('\\') => value.push('\\'),
                Some('n') => value.push('\n'),
                Some('t') => value.push('\t'),
                _ => {
                    return Err(
                        "malformed query: unrecognised string escape in a matcher value"
                            .to_string(),
                    );
                }
            },
            Some(c) => value.push(c),
        }
    }
}

/// Advance past ASCII whitespace.
fn skip_whitespace(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while let Some(&c) = chars.peek() {
        if c.is_ascii_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
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

/// First-character class: `[a-zA-Z_:]`. Shared by metric names and label
/// names; a leading dot is rejected (the start class excludes `.`).
fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == ':'
}

/// Metric-name continuation class: `[a-zA-Z0-9_:]` (no dot).
fn is_name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == ':'
}

/// Label-name continuation class: `[a-zA-Z0-9_:.]` (the metric-name
/// continuation class plus `.`, the OTel-shaped divergence; ADR-0044
/// Decision 2).
fn is_label_name_continue(c: char) -> bool {
    is_name_continue(c) || c == '.'
}

#[cfg(test)]
mod tests {
    use super::*;

    // The acceptance suite reaches the accept path (walking skeleton)
    // and the reject paths (function, operator, empty, range vector,
    // aggregation, whitespace-trim, and the new matcher reject arms).
    // These inline tests pin the boundary characters and the matcher
    // grammar the acceptance suite does not exercise one-by-one, so a
    // mutation that widens or narrows a class or flips a reject arm is
    // caught against a single assertion.

    #[test]
    fn accepts_a_bare_name_with_colon_and_underscore_and_digits() {
        // `:` and `_` are valid name characters; digits are valid in
        // the continuation but not the start. A leading digit must be
        // rejected (kills a mutant that uses is_name_continue for the
        // first char).
        assert_eq!(
            parse("node:cpu_seconds_total9")
                .expect("bare name")
                .matchers,
            Vec::new()
        );
        assert!(parse("_internal").is_ok());
        assert!(parse(":colon_start").is_ok());
    }

    #[test]
    fn rejects_a_leading_digit() {
        assert!(parse("9metric").is_err());
    }

    #[test]
    fn rejects_embedded_whitespace_and_punctuation_in_a_bare_name() {
        // A space inside the trimmed name, and any non-name byte, must
        // reject. A dotted BARE name (no braces) is still rejected: dots
        // are allowed only in label names, not the metric name.
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

    #[test]
    fn an_empty_brace_section_is_a_bare_name_equivalent() {
        // `name{}` parses with no matchers, equivalent to the bare name.
        let selector = parse("http_requests_total{}").expect("empty braces");
        assert_eq!(selector.name, MetricName::new("http_requests_total"));
        assert!(selector.matchers.is_empty());
    }

    #[test]
    fn parses_equality_and_inequality_with_a_dotted_label_name() {
        // The headline filter: a dotted OTel label name, both operators,
        // multiple matchers ANDed, whitespace and a trailing comma
        // tolerated.
        let selector =
            parse("http_requests_total{ service.name = \"checkout\" , code != \"500\" , }")
                .expect("dotted matchers parse");
        assert_eq!(selector.name, MetricName::new("http_requests_total"));
        assert_eq!(
            selector.matchers,
            vec![
                LabelMatcher {
                    name: "service.name".to_string(),
                    op: MatchOp::Equal,
                    value: "checkout".to_string(),
                },
                LabelMatcher {
                    name: "code".to_string(),
                    op: MatchOp::NotEqual,
                    value: "500".to_string(),
                },
            ]
        );
    }

    #[test]
    fn an_empty_value_is_valid_and_load_bearing() {
        let selector = parse("m{code=\"\"}").expect("empty value parses");
        assert_eq!(selector.matchers[0].value, "");
        assert_eq!(selector.matchers[0].op, MatchOp::Equal);
    }

    #[test]
    fn supported_escapes_decode_inside_a_value() {
        let selector = parse("m{path=\"a\\tb\\nc\\\"d\\\\e\"}").expect("escapes decode");
        assert_eq!(selector.matchers[0].value, "a\tb\nc\"d\\e");
    }

    #[test]
    fn a_regex_operator_now_parses_carrying_its_raw_pattern() {
        // ADR-0046: `=~`/`!~` parse to the regex operators and carry the
        // raw pattern in the matcher value; compilation is filter-side.
        let eq = parse("m{l=~\"x.*\"}").expect("=~ parses");
        assert_eq!(eq.matchers[0].op, MatchOp::Matches);
        assert_eq!(eq.matchers[0].value, "x.*");
        let neq = parse("m{l!~\"x.*\"}").expect("!~ parses");
        assert_eq!(neq.matchers[0].op, MatchOp::NotMatches);
        assert_eq!(neq.matchers[0].value, "x.*");
    }

    #[test]
    fn an_unterminated_brace_is_rejected_not_treated_as_a_bare_name() {
        let reason = parse("m{l=\"x\"").expect_err("unterminated rejected");
        assert!(reason.contains("not closed"));
    }

    #[test]
    fn an_unquoted_value_is_rejected() {
        assert!(parse("m{l=x}").is_err());
    }

    #[test]
    fn an_empty_label_name_is_rejected() {
        assert!(parse("m{=\"x\"}").is_err());
    }

    #[test]
    fn an_unknown_escape_is_rejected() {
        assert!(parse("m{l=\"a\\xb\"}").is_err());
    }

    #[test]
    fn an_unterminated_string_literal_is_rejected() {
        assert!(parse("m{l=\"x}").is_err());
    }

    #[test]
    fn junk_between_matchers_is_rejected() {
        assert!(parse("m{l=\"x\" k=\"y\"}").is_err());
    }
}
