// Kaleidoscope CLI — binary entry point
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

//! Thin binary wrapper. All real work lives in
//! `kaleidoscope_cli::{ingest, read}`. Argument parsing is
//! hand-rolled to keep the dependency graph tiny — `clap` would
//! be the convention but a two-subcommand positional CLI does
//! not earn it.
//!
//! Usage:
//!
//! ```text
//! kaleidoscope-cli ingest <tenant_id> <data_dir> [--observe-otlp <path>]
//! kaleidoscope-cli read   <tenant_id> <data_dir> [--observe-otlp <path>]
//! kaleidoscope-cli migrate <tenant_id> <data_dir> <item_id> <to_tier> [--observe-otlp <path>]
//! kaleidoscope-cli list-items <tenant_id> <data_dir> <tier>
//! ```
//!
//! With `--observe-otlp` set, both subcommands append NDJSON
//! OTLP-JSON metric lines to the given path. `tail -f` it to
//! watch the stream. Pointing both subcommands at the same path
//! in one shell session yields a single file containing the full
//! `lumen.ingest.count` + `cinder.place.count` + `lumen.query.count`
//! lifecycle.

#![forbid(unsafe_code)]

use std::io::{self, BufReader, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use aegis::TenantId;
use kaleidoscope_cli::{
    ingest, list_items, migrate, parse_iso8601_utc_nanos, read, stats_with_tiers,
    DEFAULT_BATCH_SIZE,
};
use lumen::TimeRange;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("ingest") => run_ingest(&args),
        Some("read") => run_read(&args),
        Some("stats") => run_stats(&args),
        Some("migrate") => run_migrate(&args),
        Some("list-items") => run_list_items(&args),
        Some("--help") | Some("-h") | None => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        Some(other) => {
            eprintln!("kaleidoscope-cli: unknown subcommand {other:?}\n");
            print_usage();
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kaleidoscope-cli: {e}");
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    write_usage(&mut io::stderr()).expect("write usage to stderr");
}

/// Writes the usage text to `w`. Extracted so unit tests can
/// observe the exact bytes without process-level stderr capture.
fn write_usage(w: &mut impl Write) -> io::Result<()> {
    writeln!(
        w,
        "kaleidoscope-cli — operator CLI for Lumen v1 + Cinder v1

Usage:
  kaleidoscope-cli ingest <tenant_id> <data_dir> [--observe-otlp <path>]
      Read NDJSON lumen::LogRecord from stdin and persist into <data_dir>.
      Each batch lands in Lumen and a single Cinder Hot tier entry is placed.
      --observe-otlp appends NDJSON OTLP-JSON metric lines to <path>; a
      sidecar can `tail -f` it and forward to a real OTLP/HTTP collector.

  kaleidoscope-cli read <tenant_id> <data_dir> [--observe-otlp <path>] \\
                       [--since <ISO 8601 UTC>] [--until <ISO 8601 UTC>]
      Query records for <tenant_id> and write NDJSON to stdout.
      --since / --until restrict the query to the half-open interval
      [since, until). The accepted timestamp shapes are
      YYYY-MM-DDTHH:MM:SSZ and YYYY-MM-DDTHH:MM:SS.D..DZ (1..=9
      fractional digits); lower-case `z` and `+00:00` offset forms are
      rejected. Missing flags default to 0 (since) / u64::MAX (until)
      — byte-equivalent to a pre-flag query under TimeRange::all().
      --observe-otlp appends one `lumen.query.count` OTLP-JSON line per
      invocation to <path>; pointing it at the same file used by `ingest`
      gives a single sidecar feed for the full ingest+read lifecycle.

  kaleidoscope-cli stats <tenant_id> <data_dir> \\
                       [--since <ISO 8601 UTC>] [--until <ISO 8601 UTC>]
      Print a plain-text key=value summary of the stored records for
      <tenant_id> to stdout. Populated tenants get three Lumen lines:
      records=N, earliest=<ISO 8601 UTC>, latest=<ISO 8601 UTC>.
      Empty tenants (or empty windows) get a single line: records=0.
      Then, for each Cinder tier (hot, warm, cold in that fixed
      order) with a non-zero per-tenant placement count, one extra
      line `hot=H` / `warm=W` / `cold=C`. Tiers with a zero count
      emit no line (the output is byte-equivalent to the predecessor
      for tenants whose Cinder side is empty).
      --since / --until restrict the Lumen lines to the half-open
      interval [since, until). The Cinder lines are state-snapshot
      and IGNORE the window — they always reflect the current
      per-tenant placement counts regardless of --since / --until.
      Accepted timestamp shapes are YYYY-MM-DDTHH:MM:SSZ and
      YYYY-MM-DDTHH:MM:SS.D..DZ (1..=9 fractional digits); lower-case
      `z` and `+00:00` offset forms are rejected. Missing flags
      default to 0 (since) / u64::MAX (until) — byte-equivalent to a
      pre-flag query under TimeRange::all().

  kaleidoscope-cli migrate <tenant_id> <data_dir> <item_id> <to_tier> [--observe-otlp <path>]
      Manually migrate a previously-placed Cinder item to a new tier.
      <to_tier> MUST be the literal lowercase string `hot`, `warm`, or
      `cold`; any other value (including upper-case) is rejected with
      `invalid tier \"<value>\": expected one of hot, warm, cold`.
      Items that were never `place`d are rejected with `cinder migrate:
      cannot migrate unknown item \"<item_id>\" for tenant <tenant>`.
      On success writes exactly one line to stdout:
      `migrated tenant=<tenant> item=<item_id> from=<from> to=<to>`.
      --observe-otlp appends one `cinder.migrate.count` OTLP-JSON line
      per successful migrate to <path>, carrying tenant_id resource
      attribute plus `from` and `to` point attributes — same wire
      shape ingest and read already emit.

  kaleidoscope-cli list-items <tenant_id> <data_dir> <tier>
      Print every ItemId currently placed under <tenant_id> in
      <tier>, one per line on stdout, lex-sorted. <tier> MUST be
      the literal lowercase string `hot`, `warm`, or `cold`.
      Empty tier prints nothing; exit code is 0 either way.
      Suitable for piping to xargs: `... list-items acme /tmp/data
      cold | xargs -I X kaleidoscope-cli migrate acme /tmp/data
      X warm`.

Stats are emitted to stderr after `ingest` completes."
    )
}

fn run_ingest(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (tenant, data_dir) = parse_positional(args)?;
    let otlp_path = parse_observe_otlp(args)?;
    let stdin = io::stdin();
    let reader = BufReader::new(stdin.lock());
    let stats = ingest(
        &tenant,
        &data_dir,
        DEFAULT_BATCH_SIZE,
        reader,
        otlp_path.as_deref(),
    )?;
    eprintln!(
        "ingest ok: records={} batches={} tier_items={}",
        stats.records_ingested, stats.batches_flushed, stats.tier_items_placed
    );
    Ok(())
}

fn parse_observe_otlp(args: &[String]) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    // Look for `--observe-otlp <path>` anywhere after the
    // subcommand. Hand-rolled because there are exactly two
    // optional flags planned for the lifetime of this binary.
    let mut iter = args.iter().skip(2);
    while let Some(arg) = iter.next() {
        if arg == "--observe-otlp" {
            let path = iter
                .next()
                .ok_or("--observe-otlp requires a path argument")?;
            return Ok(Some(PathBuf::from(path)));
        }
    }
    Ok(None)
}

fn run_read(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = io::stdout();
    let stderr = io::stderr();
    run_read_with(args, stdout.lock(), stderr.lock())
}

/// Inner form of `run_read` parameterised on `stdout` and `stderr`
/// sinks. Testable in-process: a unit test below pipes captured
/// `Vec<u8>` buffers in to assert the bytes produced.
fn run_read_with<O: Write, E: Write>(
    args: &[String],
    stdout: O,
    mut stderr: E,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tenant, data_dir) = parse_positional(args)?;
    let otlp_path = parse_observe_otlp(args)?;
    let range = parse_time_range(args)?;
    let count = read(&tenant, &data_dir, stdout, otlp_path.as_deref(), range)?;
    writeln!(stderr, "read ok: records={count}")?;
    Ok(())
}

/// Scans argv for `--since <ISO>` and `--until <ISO>` and returns a
/// [`TimeRange`] suitable for `lumen.query`. Missing flags default
/// to `0` / `u64::MAX` so the no-flag invocation is byte-equivalent
/// to a pre-feature `TimeRange::all()` query (OK2 guardrail).
///
/// On parse failure, returns an `Err` whose `Display` contains both
/// the offending flag name (`--since` / `--until`) AND the verbatim
/// bad value the user supplied, so the operator sees both fragments
/// on stderr without guessing which flag misfired (OK4 fail-fast
/// contract per DESIGN DD2 / DD3).
fn parse_time_range(args: &[String]) -> Result<TimeRange, Box<dyn std::error::Error>> {
    let since = parse_flag_iso(args, "--since")?;
    let until = parse_flag_iso(args, "--until")?;
    Ok(TimeRange::new(
        since.unwrap_or(0),
        until.unwrap_or(u64::MAX),
    ))
}

fn parse_flag_iso(
    args: &[String],
    flag: &'static str,
) -> Result<Option<u64>, Box<dyn std::error::Error>> {
    let mut iter = args.iter().skip(2);
    while let Some(arg) = iter.next() {
        if arg == flag {
            let value = iter
                .next()
                .ok_or_else(|| format!("{flag} requires an ISO 8601 UTC argument"))?;
            return match parse_iso8601_utc_nanos(value) {
                Ok(ns) => Ok(Some(ns)),
                Err(e) => Err(format!("{flag} {value:?}: {e}").into()),
            };
        }
    }
    Ok(None)
}

fn run_stats(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = io::stdout();
    let stderr = io::stderr();
    run_stats_with(args, stdout.lock(), stderr.lock())
}

/// Inner form of `run_stats` parameterised on `stdout` and `stderr`
/// sinks. Mirrors the `run_read_with` shape so the inline unit tests
/// below can pipe captured buffers in to assert observable bytes
/// without spawning a subprocess.
fn run_stats_with<O: Write, E: Write>(
    args: &[String],
    stdout: O,
    mut stderr: E,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tenant, data_dir) = parse_positional(args)?;
    let range = parse_time_range(args)?;
    let count = stats_with_tiers(&tenant, &data_dir, stdout, range)?;
    writeln!(stderr, "stats ok: records={count}")?;
    Ok(())
}

fn run_migrate(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = io::stdout();
    run_migrate_with(args, stdout.lock())
}

/// Inner form of `run_migrate` parameterised on `stdout`. Parses
/// the four positional args (`<tenant> <data_dir> <item_id>
/// <to_tier>`), then delegates to [`kaleidoscope_cli::migrate`].
/// The library function owns the actual work; this wrapper is the
/// argv-to-call adapter. Per DESIGN DD3 the tier parse error is
/// surfaced as `kaleidoscope-cli: invalid tier "<value>": ...` by
/// the top-level `main` Display-prefix; no special handling here
/// because `migrate()` itself returns `Error::InvalidTier`.
fn run_migrate_with<O: Write>(
    args: &[String],
    stdout: O,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tenant, data_dir) = parse_positional(args)?;
    let item_id = args.get(4).ok_or("missing <item_id>")?.clone();
    let to_tier = args.get(5).ok_or("missing <to_tier>")?.clone();
    let otlp_path = parse_observe_otlp(args)?;
    migrate(
        &tenant,
        &data_dir,
        &item_id,
        &to_tier,
        stdout,
        otlp_path.as_deref(),
    )?;
    Ok(())
}

fn run_list_items(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = io::stdout();
    run_list_items_with(args, stdout.lock())
}

/// Inner form of `run_list_items` parameterised on `stdout`. Parses
/// the three positional args (`<tenant> <data_dir> <tier>`), then
/// delegates to [`kaleidoscope_cli::list_items`]. The library
/// function owns the actual work; this wrapper is the argv-to-call
/// adapter.
fn run_list_items_with<O: Write>(
    args: &[String],
    stdout: O,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tenant, data_dir) = parse_positional(args)?;
    let tier = args.get(4).ok_or("missing <tier>")?.clone();
    list_items(&tenant, &data_dir, &tier, stdout)?;
    Ok(())
}

fn parse_positional(args: &[String]) -> Result<(TenantId, PathBuf), Box<dyn std::error::Error>> {
    // args[0] = bin, args[1] = subcommand, args[2] = tenant,
    // args[3] = data_dir.
    let tenant = args.get(2).ok_or("missing <tenant_id>")?.clone();
    let data_dir = args.get(3).ok_or("missing <data_dir>")?.clone();
    Ok((TenantId(tenant), PathBuf::from(data_dir)))
}

// --------------------------------------------------------------------
// Inline mutation-killing unit tests. The acceptance suite in
// `tests/observe_otlp_read_flag.rs` is locked; these in-process
// micro-tests cover the binary-only seams (`write_usage`,
// `run_read_with`) that the locked acceptance tests cannot reach
// without spawning a subprocess. They exist to discharge
// `cargo mutants` on `src/main.rs`.
// --------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    fn tmp(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "kal-cli-main-{name}-{pid}-{nanos}",
            pid = std::process::id()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn write_usage_emits_subcommand_help_with_both_read_and_ingest() {
        // Kills `replace print_usage with ()`. The mutant turns
        // `print_usage` into a no-op; asserting the byte content of
        // `write_usage`'s sink (which `print_usage` delegates to)
        // fails iff the body has been removed.
        let mut buf: Vec<u8> = Vec::new();
        write_usage(&mut buf).expect("write usage");
        let text = String::from_utf8(buf).expect("utf8 usage");
        assert!(text.contains("kaleidoscope-cli ingest"), "ingest help");
        assert!(text.contains("kaleidoscope-cli read"), "read help");
        assert!(text.contains("--observe-otlp"), "flag documented");
    }

    #[test]
    fn run_read_with_writes_records_to_stdout_and_summary_to_stderr() {
        // Kills `replace run_read -> Ok(())`. The mutant skips the
        // entire body: nothing is written to either sink, and no
        // record is read from the data_dir. Asserting on the bytes
        // observed in both sinks fails iff the body is skipped.
        use lumen::{LogRecord, SeverityNumber};
        use std::collections::BTreeMap;
        let root = tmp("run_read_with");
        let data = root.join("data");

        // Seed one record via the library `ingest` so we have
        // something to read back.
        let acme = TenantId("acme".to_string());
        let rec = LogRecord {
            observed_time_unix_nano: 1,
            severity_number: SeverityNumber::INFO,
            severity_text: "INFO".to_string(),
            body: "hi".to_string(),
            attributes: BTreeMap::new(),
            resource_attributes: BTreeMap::new(),
            trace_id: None,
            span_id: None,
        };
        let mut ndjson = serde_json::to_string(&rec).expect("serialise seed");
        ndjson.push('\n');
        ingest(
            &acme,
            &data,
            DEFAULT_BATCH_SIZE,
            Cursor::new(ndjson.into_bytes()),
            None,
        )
        .expect("seed ingest");

        let args = vec![
            "kaleidoscope-cli".to_string(),
            "read".to_string(),
            "acme".to_string(),
            data.to_string_lossy().into_owned(),
        ];
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        run_read_with(&args, &mut stdout, &mut stderr).expect("run_read_with");

        let stdout_text = String::from_utf8(stdout).expect("utf8 stdout");
        assert!(
            stdout_text.contains("\"body\":\"hi\""),
            "stdout must contain the seeded record body"
        );
        let stderr_text = String::from_utf8(stderr).expect("utf8 stderr");
        assert_eq!(
            stderr_text.trim_end(),
            "read ok: records=1",
            "stderr summary line"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn run_stats_with_writes_summary_to_stdout_and_records_line_to_stderr() {
        // Kills both `replace run_stats -> Ok(())` (line 163) and
        // `replace run_stats_with -> Ok(())` (line 177). Both mutants
        // skip the body, so neither sink receives any bytes;
        // asserting on bytes observed in BOTH sinks fails iff the
        // body is skipped.
        use lumen::{LogRecord, SeverityNumber};
        use std::collections::BTreeMap;
        let root = tmp("run_stats_with");
        let data = root.join("data");

        // Seed one record so the populated-tenant branch fires
        // (records.first()/last() are Some, three lines emitted).
        let acme = TenantId("acme".to_string());
        let rec = LogRecord {
            observed_time_unix_nano: 0,
            severity_number: SeverityNumber::INFO,
            severity_text: "INFO".to_string(),
            body: "hello".to_string(),
            attributes: BTreeMap::new(),
            resource_attributes: BTreeMap::new(),
            trace_id: None,
            span_id: None,
        };
        let mut ndjson = serde_json::to_string(&rec).expect("serialise seed");
        ndjson.push('\n');
        ingest(
            &acme,
            &data,
            DEFAULT_BATCH_SIZE,
            Cursor::new(ndjson.into_bytes()),
            None,
        )
        .expect("seed ingest");

        let args = vec![
            "kaleidoscope-cli".to_string(),
            "stats".to_string(),
            "acme".to_string(),
            data.to_string_lossy().into_owned(),
        ];
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        run_stats_with(&args, &mut stdout, &mut stderr).expect("run_stats_with");

        let stdout_text = String::from_utf8(stdout).expect("utf8 stdout");
        assert!(
            stdout_text.starts_with("records=1\n"),
            "stdout begins with the records= line (populated branch)"
        );
        assert!(
            stdout_text.contains("earliest=1970-01-01T00:00:00.000000000Z"),
            "stdout includes the earliest= line rendered by format_iso8601_utc_nanos"
        );
        assert!(
            stdout_text.contains("latest=1970-01-01T00:00:00.000000000Z"),
            "stdout includes the latest= line rendered by format_iso8601_utc_nanos"
        );
        let stderr_text = String::from_utf8(stderr).expect("utf8 stderr");
        assert_eq!(
            stderr_text.trim_end(),
            "stats ok: records=1",
            "stderr summary line"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn run_stats_propagates_missing_argument_error_when_no_tenant_supplied() {
        // Kills `replace run_stats -> Ok(())` (main.rs:163). The
        // outer `run_stats` wrapper only delegates to
        // `run_stats_with`, which makes most in-process tests bounce
        // off the inner wrapper instead. Here we discriminate the
        // mutant by calling `run_stats` with a deliberately short
        // argv (no tenant, no data_dir): the real wrapper propagates
        // the `"missing <tenant_id>"` error from `parse_positional`
        // BEFORE any I/O happens (so we don't pollute the real
        // process's stdout/stderr); the mutant short-circuits to
        // `Ok(())` and the assertion that it is `Err` flips red.
        let args = vec!["kaleidoscope-cli".to_string(), "stats".to_string()];
        let result = run_stats(&args);
        assert!(
            result.is_err(),
            "run_stats must propagate the parse_positional error for missing args"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("missing <tenant_id>"),
            "error message comes from parse_positional, not a stats-level failure: {msg:?}"
        );
    }

    // -------- parse_time_range: --since / --until argv scanning --------

    #[test]
    fn parse_time_range_with_no_flags_defaults_to_time_range_all() {
        // No --since, no --until → TimeRange::new(0, u64::MAX),
        // which IS TimeRange::all() (OK2 byte-equivalence guardrail).
        let args = vec![
            "kaleidoscope-cli".to_string(),
            "read".to_string(),
            "acme".to_string(),
            "/tmp/x".to_string(),
        ];
        let range = parse_time_range(&args).expect("no-flag parse");
        assert_eq!(range, TimeRange::all());
        assert_eq!(range, TimeRange::new(0, u64::MAX));
    }

    #[test]
    fn parse_time_range_with_since_only_uses_u64_max_upper() {
        // --since 1970-01-01T00:00:00.000000200Z → since=200 ns,
        // until=u64::MAX (OK3a half-bounded leading).
        let args = vec![
            "kaleidoscope-cli".to_string(),
            "read".to_string(),
            "acme".to_string(),
            "/tmp/x".to_string(),
            "--since".to_string(),
            "1970-01-01T00:00:00.000000200Z".to_string(),
        ];
        let range = parse_time_range(&args).expect("since-only parse");
        assert_eq!(range, TimeRange::new(200, u64::MAX));
    }

    #[test]
    fn parse_time_range_with_until_only_uses_zero_lower() {
        // --until 1970-01-01T00:00:00.000000200Z → since=0,
        // until=200 ns (OK3b half-bounded trailing).
        let args = vec![
            "kaleidoscope-cli".to_string(),
            "read".to_string(),
            "acme".to_string(),
            "/tmp/x".to_string(),
            "--until".to_string(),
            "1970-01-01T00:00:00.000000200Z".to_string(),
        ];
        let range = parse_time_range(&args).expect("until-only parse");
        assert_eq!(range, TimeRange::new(0, 200));
    }

    #[test]
    fn parse_time_range_with_both_flags_builds_half_open_interval() {
        // --since 200 ns --until 400 ns → TimeRange::new(200, 400)
        // (OK1 bounded-window shape).
        let args = vec![
            "kaleidoscope-cli".to_string(),
            "read".to_string(),
            "acme".to_string(),
            "/tmp/x".to_string(),
            "--since".to_string(),
            "1970-01-01T00:00:00.000000200Z".to_string(),
            "--until".to_string(),
            "1970-01-01T00:00:00.000000400Z".to_string(),
        ];
        let range = parse_time_range(&args).expect("both-flag parse");
        assert_eq!(range, TimeRange::new(200, 400));
    }

    #[test]
    fn parse_time_range_with_bad_since_names_flag_and_value_in_error() {
        // Malformed --since value → Err whose Display contains both
        // `--since` and the verbatim bad value `not-an-iso` (OK4a
        // fail-fast contract).
        let args = vec![
            "kaleidoscope-cli".to_string(),
            "read".to_string(),
            "acme".to_string(),
            "/tmp/x".to_string(),
            "--since".to_string(),
            "not-an-iso".to_string(),
        ];
        let err = parse_time_range(&args).expect_err("bad --since must fail");
        let msg = err.to_string();
        assert!(msg.contains("--since"), "stderr names --since: {msg:?}");
        assert!(
            msg.contains("not-an-iso"),
            "stderr names verbatim bad value: {msg:?}"
        );
    }

    #[test]
    fn parse_time_range_with_bad_until_names_flag_and_value_in_error() {
        // Calendar-out-of-range --until value → Err whose Display
        // contains both `--until` and the verbatim bad value (OK4b
        // fail-fast contract). Uses the exact byte sequence the
        // locked spec subprocess test passes through argv.
        let args = vec![
            "kaleidoscope-cli".to_string(),
            "read".to_string(),
            "acme".to_string(),
            "/tmp/x".to_string(),
            "--until".to_string(),
            "2026-13-32T25:99:99Z".to_string(),
        ];
        let err = parse_time_range(&args).expect_err("bad --until must fail");
        let msg = err.to_string();
        assert!(msg.contains("--until"), "stderr names --until: {msg:?}");
        assert!(
            msg.contains("2026-13-32T25:99:99Z"),
            "stderr names verbatim bad value: {msg:?}"
        );
    }

    #[test]
    fn run_stats_with_on_empty_tenant_writes_only_records_zero_to_stdout() {
        // Reinforces the kill of `replace run_stats_with -> Ok(())`
        // via the empty-tenant branch: stdout must contain
        // `records=0\n` even though no records were ingested for
        // this tenant. With the mutant, stdout is empty and stderr
        // is empty.
        let root = tmp("run_stats_with_empty");
        let data = root.join("data");

        // Seed `acme` so the Lumen store opens cleanly, but query
        // the never-ingested `acmee` tenant.
        use lumen::{LogRecord, SeverityNumber};
        use std::collections::BTreeMap;
        let acme = TenantId("acme".to_string());
        let rec = LogRecord {
            observed_time_unix_nano: 0,
            severity_number: SeverityNumber::INFO,
            severity_text: "INFO".to_string(),
            body: "x".to_string(),
            attributes: BTreeMap::new(),
            resource_attributes: BTreeMap::new(),
            trace_id: None,
            span_id: None,
        };
        let mut ndjson = serde_json::to_string(&rec).expect("serialise seed");
        ndjson.push('\n');
        ingest(
            &acme,
            &data,
            DEFAULT_BATCH_SIZE,
            Cursor::new(ndjson.into_bytes()),
            None,
        )
        .expect("seed ingest");

        let args = vec![
            "kaleidoscope-cli".to_string(),
            "stats".to_string(),
            "acmee".to_string(),
            data.to_string_lossy().into_owned(),
        ];
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        run_stats_with(&args, &mut stdout, &mut stderr).expect("run_stats_with");

        let stdout_text = String::from_utf8(stdout).expect("utf8 stdout");
        assert_eq!(
            stdout_text, "records=0\n",
            "empty-tenant stdout is exactly `records=0\\n`"
        );
        let stderr_text = String::from_utf8(stderr).expect("utf8 stderr");
        assert_eq!(
            stderr_text.trim_end(),
            "stats ok: records=0",
            "stderr summary line for empty tenant"
        );

        let _ = fs::remove_dir_all(&root);
    }
}
