// Kaleidoscope CLI — library
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

//! # Kaleidoscope CLI library
//!
//! The CLI is split into a thin binary (`src/main.rs`) and this
//! library (`src/lib.rs`). The binary parses arguments and
//! dispatches to two operations; the library owns the actual
//! work and is exercised by tests that pipe controlled
//! readers/writers through it.
//!
//! ## Operations
//!
//! - [`ingest`] reads NDJSON `lumen::LogRecord` from a reader,
//!   batches them, ingests into a `FileBackedLogStore`, and
//!   places one `Cinder` tier entry per batch under the Hot
//!   tier. The Lumen `MetricsRecorder` is the
//!   `self_observe::LumenToPulseRecorder`, so the platform
//!   observes its own ingest activity via Pulse.
//! - [`read`] queries every record for the tenant from the
//!   Lumen store and writes them back as NDJSON to a writer.
//!
//! ## Storage layout
//!
//! Given `--data-dir <dir>`:
//!
//! - `<dir>/lumen.*` — Lumen v1 WAL + snapshot
//! - `<dir>/cinder.*` — Cinder v1 WAL + snapshot
//!
//! Both adapters survive process restarts. A second invocation
//! of `read` reads back data written by an earlier invocation
//! of `ingest`.

#![forbid(unsafe_code)]

use std::fmt;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use aegis::TenantId;
use cinder::{
    FileBackedTieringStore, ItemId, MetricsRecorder as CinderRec, MigrateError,
    NoopRecorder as CinderRecorder, Tier, TieringStore,
};
use lumen::{
    FileBackedLogStore, LogBatch, LogRecord, LogStore, LogStoreError, MetricsRecorder as LumenRec,
    TimeRange,
};
use pulse::{InMemoryMetricStore, MetricStore, NoopRecorder as PulseRecorder};
use self_observe::{CinderToOtlpJsonWriter, LumenToOtlpJsonWriter, LumenToPulseRecorder};

/// Configurable batch flush size. Smaller for tests; larger for
/// production. The default chosen here matches the KPI batch
/// shape used in Lumen v1's acceptance suite.
pub const DEFAULT_BATCH_SIZE: usize = 100;

#[derive(Debug)]
pub enum Error {
    LumenOpen(LogStoreError),
    LumenIngest(LogStoreError),
    LumenQuery(LogStoreError),
    CinderOpen(MigrateError),
    Io(std::io::Error),
    ParseRecord {
        line: usize,
        source: serde_json::Error,
    },
    SerialiseRecord(serde_json::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::LumenOpen(e) => write!(f, "lumen open: {e}"),
            Error::LumenIngest(e) => write!(f, "lumen ingest: {e}"),
            Error::LumenQuery(e) => write!(f, "lumen query: {e}"),
            Error::CinderOpen(e) => write!(f, "cinder open: {e}"),
            Error::Io(e) => write!(f, "io: {e}"),
            Error::ParseRecord { line, source } => {
                write!(f, "parse record at line {line}: {source}")
            }
            Error::SerialiseRecord(e) => write!(f, "serialise record: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Statistics emitted after a successful `ingest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestStats {
    pub records_ingested: usize,
    pub batches_flushed: usize,
    pub tier_items_placed: usize,
}

fn lumen_base(data_dir: &Path) -> PathBuf {
    data_dir.join("lumen")
}

fn cinder_base(data_dir: &Path) -> PathBuf {
    data_dir.join("cinder")
}

/// Reads NDJSON `LogRecord` from `reader`, batches them in
/// groups of `batch_size`, ingests into Lumen, and places one
/// Cinder Hot-tier entry per batch.
///
/// The Lumen `MetricsRecorder` is wired to a fresh in-process
/// Pulse store via `LumenToPulseRecorder` so the binary's own
/// observability is available for inspection (currently
/// dropped at end of call). If `otlp_log_path` is `Some`, the
/// recorder is replaced by `LumenToOtlpJsonWriter` which
/// appends NDJSON OTLP-JSON metrics lines to that file. An
/// operator can then `tail -f <path>` to watch the metric
/// stream, or a sidecar process can read the file and forward
/// to a real OTLP/HTTP collector.
pub fn ingest(
    tenant: &TenantId,
    data_dir: &Path,
    batch_size: usize,
    reader: impl BufRead,
    otlp_log_path: Option<&Path>,
) -> Result<IngestStats, Error> {
    std::fs::create_dir_all(data_dir)?;
    let (lumen_recorder, cinder_recorder): (
        Box<dyn LumenRec + Send + Sync>,
        Box<dyn CinderRec + Send + Sync>,
    ) = match otlp_log_path {
        Some(path) => {
            // ADR-0039 §8: open the path ONCE with create+append, then
            // try_clone() for the second File handle. POSIX O_APPEND
            // (PIPE_BUF = 4096, well above the ~540-byte worst-case
            // line) gives cross-writer atomicity on the shared file
            // description. The original goes to Lumen; the clone to
            // Cinder.
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            let file_clone = file.try_clone()?;
            (
                Box::new(LumenToOtlpJsonWriter::new(file)),
                Box::new(CinderToOtlpJsonWriter::new(file_clone)),
            )
        }
        None => {
            let pulse: Arc<dyn MetricStore + Send + Sync> =
                Arc::new(InMemoryMetricStore::new(Box::new(PulseRecorder)));
            (
                Box::new(LumenToPulseRecorder::new(pulse)),
                Box::new(CinderRecorder),
            )
        }
    };
    let lumen =
        FileBackedLogStore::open(lumen_base(data_dir), lumen_recorder).map_err(Error::LumenOpen)?;
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), cinder_recorder)
        .map_err(Error::CinderOpen)?;

    let mut buffer: Vec<LogRecord> = Vec::with_capacity(batch_size);
    let mut records_ingested = 0usize;
    let mut batches_flushed = 0usize;
    let mut tier_items_placed = 0usize;

    for (idx, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let record: LogRecord = serde_json::from_str(&line).map_err(|e| Error::ParseRecord {
            line: idx + 1,
            source: e,
        })?;
        buffer.push(record);
        if buffer.len() >= batch_size {
            flush(
                tenant,
                &lumen,
                &cinder,
                &mut buffer,
                batches_flushed,
                &mut tier_items_placed,
                &mut records_ingested,
            )?;
            batches_flushed += 1;
        }
    }
    if !buffer.is_empty() {
        flush(
            tenant,
            &lumen,
            &cinder,
            &mut buffer,
            batches_flushed,
            &mut tier_items_placed,
            &mut records_ingested,
        )?;
        batches_flushed += 1;
    }

    Ok(IngestStats {
        records_ingested,
        batches_flushed,
        tier_items_placed,
    })
}

fn flush(
    tenant: &TenantId,
    lumen: &FileBackedLogStore,
    cinder: &FileBackedTieringStore,
    buffer: &mut Vec<LogRecord>,
    batch_seq: usize,
    tier_items_placed: &mut usize,
    records_ingested: &mut usize,
) -> Result<(), Error> {
    let count = buffer.len();
    let batch = LogBatch::with_records(std::mem::take(buffer));
    let receipt = lumen.ingest(tenant, batch).map_err(Error::LumenIngest)?;
    *records_ingested += receipt.count;
    let item = ItemId::new(format!("{}/batch-{:05}", tenant.0, batch_seq));
    cinder.place(tenant, &item, Tier::Hot, SystemTime::now());
    *tier_items_placed += 1;
    debug_assert_eq!(receipt.count, count);
    Ok(())
}

/// Queries every record for the tenant from Lumen and writes
/// them as NDJSON to `writer`, one record per line.
///
/// If `otlp_log_path` is `Some`, the Lumen `MetricsRecorder` is
/// wired to `LumenToOtlpJsonWriter` which appends one
/// `lumen.query.count` OTLP-JSON line per `read()` invocation to
/// that file (single `OpenOptions::create(true).append(true)`
/// open per ADR-0039 §8). If `None`, the recorder is
/// `LumenToPulseRecorder` and the on-disk OTLP file is not
/// created — byte-equivalent to pre-feature behaviour (OK2
/// guardrail).
pub fn read(
    tenant: &TenantId,
    data_dir: &Path,
    mut writer: impl Write,
    otlp_log_path: Option<&Path>,
) -> Result<usize, Error> {
    let recorder: Box<dyn LumenRec + Send + Sync> = match otlp_log_path {
        Some(path) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            Box::new(LumenToOtlpJsonWriter::new(file))
        }
        None => {
            let pulse: Arc<dyn MetricStore + Send + Sync> =
                Arc::new(InMemoryMetricStore::new(Box::new(PulseRecorder)));
            Box::new(LumenToPulseRecorder::new(pulse))
        }
    };
    let lumen =
        FileBackedLogStore::open(lumen_base(data_dir), recorder).map_err(Error::LumenOpen)?;
    let records = lumen
        .query(tenant, TimeRange::all())
        .map_err(Error::LumenQuery)?;
    let count = records.len();
    for record in records {
        let line = serde_json::to_string(&record).map_err(Error::SerialiseRecord)?;
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(count)
}

/// Queries every record for the tenant from Lumen and writes a
/// summary as plain-text `key=value\n` lines to `writer`. Returns
/// the matched record count (mirrors [`read`]'s return shape).
///
/// Output contract (DESIGN DD5 / discuss D5):
///
/// - Populated tenant (`N > 0`): three lines, in order —
///   `records=N\n`, `earliest=<ISO 8601 UTC>\n`,
///   `latest=<ISO 8601 UTC>\n`.
/// - Empty tenant (`N == 0`): exactly one line — `records=0\n`. No
///   `earliest=`, no `latest=`.
///
/// Timestamps are rendered with [`format_iso8601_utc_nanos`] from
/// the seeded `observed_time_unix_nano` of the first and last
/// returned records. `LogStore::query` returns records sorted in
/// ascending `observed_time_unix_nano` order, so `records.first()`
/// is the earliest and `records.last()` is the latest (DD3).
pub fn stats(tenant: &TenantId, data_dir: &Path, mut writer: impl Write) -> Result<usize, Error> {
    let pulse: Arc<dyn MetricStore + Send + Sync> =
        Arc::new(InMemoryMetricStore::new(Box::new(PulseRecorder)));
    let recorder: Box<dyn LumenRec + Send + Sync> = Box::new(LumenToPulseRecorder::new(pulse));
    let lumen =
        FileBackedLogStore::open(lumen_base(data_dir), recorder).map_err(Error::LumenOpen)?;
    let records = lumen
        .query(tenant, TimeRange::all())
        .map_err(Error::LumenQuery)?;
    let count = records.len();
    writeln!(writer, "records={count}")?;
    if let (Some(first), Some(last)) = (records.first(), records.last()) {
        let earliest = format_iso8601_utc_nanos(first.observed_time_unix_nano);
        let latest = format_iso8601_utc_nanos(last.observed_time_unix_nano);
        writeln!(writer, "earliest={earliest}")?;
        writeln!(writer, "latest={latest}")?;
    }
    writer.flush()?;
    Ok(count)
}

/// Sibling of [`stats`] that additionally surfaces the per-tenant
/// Cinder tier-placement counts after the Lumen summary lines.
///
/// Output contract (DESIGN DD1 / DD4):
///
/// - Lumen lines exactly as [`stats`] emits — `records=N\n`, then
///   `earliest=<ISO>\n` and `latest=<ISO>\n` only when `N > 0`.
/// - Then, in fixed order `Hot`, `Warm`, `Cold` (DD4), one
///   `<tier>=<count>\n` line per tier whose
///   `list_by_tier(tenant, tier).len() > 0`. Tiers with a zero
///   count emit NO line (Option B per DD4) — so the OK4 backwards-
///   compatibility invariant holds for tenants whose Cinder side
///   is empty.
///
/// Returns the matched Lumen record count (mirrors [`stats`]'s
/// return shape).
pub fn stats_with_tiers(
    tenant: &TenantId,
    data_dir: &Path,
    mut writer: impl Write,
) -> Result<usize, Error> {
    let pulse: Arc<dyn MetricStore + Send + Sync> =
        Arc::new(InMemoryMetricStore::new(Box::new(PulseRecorder)));
    let recorder: Box<dyn LumenRec + Send + Sync> = Box::new(LumenToPulseRecorder::new(pulse));
    let lumen =
        FileBackedLogStore::open(lumen_base(data_dir), recorder).map_err(Error::LumenOpen)?;
    let records = lumen
        .query(tenant, TimeRange::all())
        .map_err(Error::LumenQuery)?;
    let count = records.len();
    writeln!(writer, "records={count}")?;
    if let (Some(first), Some(last)) = (records.first(), records.last()) {
        let earliest = format_iso8601_utc_nanos(first.observed_time_unix_nano);
        let latest = format_iso8601_utc_nanos(last.observed_time_unix_nano);
        writeln!(writer, "earliest={earliest}")?;
        writeln!(writer, "latest={latest}")?;
    }
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .map_err(Error::CinderOpen)?;
    // DD2: hardcoded fixed-order tier array (Tier::all() does not
    // exist on the cinder crate). DD4: Option B — emit no line for
    // tiers whose count is zero.
    for tier in [Tier::Hot, Tier::Warm, Tier::Cold] {
        let placements = cinder.list_by_tier(tenant, tier).len();
        if placements > 0 {
            writeln!(writer, "{}={}", tier_lowercase(tier), placements)?;
        }
    }
    writer.flush()?;
    Ok(count)
}

/// Renders a [`Tier`] as the exact lowercase ASCII byte sequence
/// expected on stdout (`hot` / `warm` / `cold`). Local to this
/// crate so the output-shape contract for the stats subcommand is
/// pinned at one site; the equivalent helper inside
/// `self_observe::cinder_bridge` is private to that crate.
fn tier_lowercase(tier: Tier) -> &'static str {
    match tier {
        Tier::Hot => "hot",
        Tier::Warm => "warm",
        Tier::Cold => "cold",
    }
}

/// Renders a `u64` count of nanoseconds since the Unix epoch as an
/// ISO 8601 UTC string of the exact shape
/// `YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ` — always nine nanosecond digits,
/// always `Z` suffix.
///
/// Hand-rolled per DESIGN DD1 to keep the dependency graph tiny
/// (no `chrono`, no `time`). The civil-from-days conversion uses
/// Howard Hinnant's algorithm
/// (<https://howardhinnant.github.io/date_algorithms.html#civil_from_days>),
/// which is public domain. All arithmetic is integer; no leap-second
/// support (Unix epoch nanos do not encode them either).
fn format_iso8601_utc_nanos(ns: u64) -> String {
    let total_seconds: u64 = ns / 1_000_000_000;
    let nanos_of_second: u64 = ns % 1_000_000_000;
    let total_days: i64 = (total_seconds / 86_400) as i64;
    let secs_of_day: u64 = total_seconds % 86_400;
    let hour: u64 = secs_of_day / 3_600;
    let minute: u64 = (secs_of_day % 3_600) / 60;
    let second: u64 = secs_of_day % 60;
    let (year, month, day) = civil_from_days(total_days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{nanos_of_second:09}Z")
}

/// Howard Hinnant's `civil_from_days` (public domain). Converts a
/// signed count of days since the Unix epoch (1970-01-01 = 0) into
/// the proleptic Gregorian `(year, month, day)` triple, with
/// `month ∈ [1, 12]` and `day ∈ [1, 31]`.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era: i64 = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe: u64 = (z - era * 146_097) as u64; // [0, 146096]
    let yoe: u64 = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y: i64 = yoe as i64 + era * 400;
    let doy: u64 = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp: u64 = (5 * doy + 2) / 153; // [0, 11]
    let d: u32 = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m: u32 = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    let year: i32 = (y + if m <= 2 { 1 } else { 0 }) as i32;
    (year, m, d)
}

// --------------------------------------------------------------------
// Inline mutation-killing unit tests for the hand-rolled ISO 8601
// formatter and `civil_from_days`. The acceptance suite in
// `tests/stats_subcommand.rs` is locked (Scholar-authored,
// Eclipse-APPROVED); these in-process micro-tests cover the
// arithmetic seams that the acceptance suite cannot reach
// (boundary years, leap-year logic, hour/minute/second splits) and
// exist to discharge `cargo mutants` against the hand-rolled
// formatter, which is mutation-rich per DESIGN DD1.
//
// Every assertion below traces to either:
//   - a known anchor point (Unix epoch == 1970-01-01T00:00:00Z, etc.),
//   - a published ISO 8601 calendar conversion, or
//   - cross-checks against successive seconds/days that pin the
//     `/`, `%`, `+`, `-`, `<` mutants on each arithmetic line.
// --------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::{civil_from_days, format_iso8601_utc_nanos};

    // -------- format_iso8601_utc_nanos: time-of-day arithmetic --------

    #[test]
    fn format_unix_epoch_is_1970_01_01_t00_00_00_zero_nanos() {
        // Anchor: the documented Unix epoch in ISO 8601 UTC.
        assert_eq!(
            format_iso8601_utc_nanos(0),
            "1970-01-01T00:00:00.000000000Z"
        );
    }

    #[test]
    fn format_one_nanosecond_after_epoch_advances_only_the_nanos_field() {
        // Kills `replace % with /` on line 347 (nanos_of_second):
        // with `/`, this returns 0 ns and matches the epoch string.
        assert_eq!(
            format_iso8601_utc_nanos(1),
            "1970-01-01T00:00:00.000000001Z"
        );
    }

    #[test]
    fn format_one_second_after_epoch_advances_only_the_seconds_field() {
        // Kills `replace / with %` on line 346 (total_seconds): with
        // `%`, total_seconds = 1_000_000_000 % 1_000_000_000 = 0.
        assert_eq!(
            format_iso8601_utc_nanos(1_000_000_000),
            "1970-01-01T00:00:01.000000000Z"
        );
    }

    #[test]
    fn format_one_minute_after_epoch_rolls_seconds_to_minutes() {
        // Kills `replace % with /` on line 352 (second = secs_of_day
        // % 60): at 60 s, that would yield second=1 instead of 0.
        assert_eq!(
            format_iso8601_utc_nanos(60 * 1_000_000_000),
            "1970-01-01T00:01:00.000000000Z"
        );
    }

    #[test]
    fn format_one_hour_after_epoch_rolls_minutes_to_hours() {
        // Kills `replace / with %` AND `replace / with *` on line 350
        // (hour = secs_of_day / 3_600): at 3600 s, `% 3600` = 0 and
        // `* 3600` overflows away from 1.
        // Also kills `replace / with %` AND `replace / with *` on
        // line 351 (minute = (secs_of_day % 3_600) / 60).
        assert_eq!(
            format_iso8601_utc_nanos(3_600 * 1_000_000_000),
            "1970-01-01T01:00:00.000000000Z"
        );
    }

    #[test]
    fn format_one_hour_and_one_minute_after_epoch_separates_hour_and_minute_fields() {
        // Additional witness for the minute split on line 351: with
        // `replace / with %`, minute = (3660 % 3600) % 60 = 60 (no!
        // 60 doesn't fit `{:02}`-wise here; matters is the byte
        // difference). With `replace % with /`, minute =
        // (3660 / 3600) / 60 = 0, which is also wrong.
        assert_eq!(
            format_iso8601_utc_nanos(3_660 * 1_000_000_000),
            "1970-01-01T01:01:00.000000000Z"
        );
    }

    #[test]
    fn format_one_day_after_epoch_advances_to_1970_01_02() {
        // Pins the day-boundary path: total_days = 1 yields
        // 1970-01-02. Failing this kills any mutation that breaks
        // the seconds-to-days division or the civil_from_days entry.
        assert_eq!(
            format_iso8601_utc_nanos(86_400 * 1_000_000_000),
            "1970-01-02T00:00:00.000000000Z"
        );
    }

    #[test]
    fn format_one_second_before_one_day_after_epoch_is_23_59_59_on_day_zero() {
        // Anchor for the boundary between secs_of_day = 86_399 and
        // total_days = 0 vs total_days = 1.
        assert_eq!(
            format_iso8601_utc_nanos(86_399 * 1_000_000_000),
            "1970-01-01T23:59:59.000000000Z"
        );
    }

    // -------- civil_from_days: the calendar arithmetic --------

    #[test]
    fn civil_from_days_at_zero_is_unix_epoch() {
        // Anchor: day 0 since 1970-01-01 IS 1970-01-01.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn civil_from_days_at_one_is_jan_second_1970() {
        // Smallest positive witness: kills several "+ -> -" mutants
        // on the `d` and `m` computations (lines 371-373) that
        // would otherwise return January 1 again.
        assert_eq!(civil_from_days(1), (1970, 1, 2));
    }

    #[test]
    fn civil_from_days_at_31_is_february_first_1970() {
        // Month rollover anchor: day 31 since 1970-01-01 IS
        // 1970-02-01. Kills "<" -> "<=" on line 372 and the "+ -> -"
        // on the year-correction line 373 (m <= 2).
        assert_eq!(civil_from_days(31), (1970, 2, 1));
    }

    #[test]
    fn civil_from_days_at_58_is_february_28_1970() {
        // Non-leap February witness: 1970 is not a leap year, so
        // February has 28 days. Day 58 since 1970-01-01 IS
        // 1970-02-28 (31 Jan days + 27 Feb days elapsed; day 31 is
        // Feb 1, so day 31+27 = 58 is Feb 28).
        assert_eq!(civil_from_days(58), (1970, 2, 28));
    }

    #[test]
    fn civil_from_days_at_59_is_march_first_1970() {
        // Cross-check on the non-leap rollover: day 59 IS
        // 1970-03-01 (not 1970-02-29). Pins the boundary day-count
        // arithmetic and the m <= 2 year-correction branch.
        assert_eq!(civil_from_days(59), (1970, 3, 1));
    }

    #[test]
    fn civil_from_days_at_365_is_january_first_1971() {
        // Year rollover: 1970 has 365 days, so day 365 IS
        // 1971-01-01. Kills mutants that swap `+` for `-` on the
        // `y = yoe + era*400` line (line 368) and the era offset
        // (line 367).
        assert_eq!(civil_from_days(365), (1971, 1, 1));
    }

    #[test]
    fn civil_from_days_at_789_is_feb_29_1972_a_leap_day() {
        // 1972 is a leap year (divisible by 4, not by 100). Day 789
        // since 1970-01-01: 365 (1970) + 365 (1971) = 730 places us
        // at 1972-01-01, then + 31 (Jan) → 1972-02-01 at day 761,
        // then + 28 → day 789 = 1972-02-29. Pins the leap-year
        // branch of doe / 1460 and doe / 36_524.
        assert_eq!(civil_from_days(789), (1972, 2, 29));
    }

    #[test]
    fn civil_from_days_at_20_454_is_january_first_2026() {
        // Acceptance-test seed alignment: 56 years (14 leap days)
        // from 1970-01-01 to 2026-01-01 = 56 * 365 + 14 = 20_454.
        // Pins the 400-year era cycle (era = 0, yoe = 56).
        assert_eq!(civil_from_days(20_454), (2026, 1, 1));
    }

    #[test]
    fn civil_from_days_at_negative_one_is_dec_31_1969() {
        // Pre-epoch anchor: day -1 IS 1969-12-31. Kills `replace -
        // with +` on line 365 (`z - 146_096`) and on line 367
        // (`doe - doe/1460 + ...`).
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn civil_from_days_at_minus_719_528_is_year_zero_january_first() {
        // Deep-negative witness: this drives `z + 719_468` strictly
        // negative inside civil_from_days, which forces the
        // `if z >= 0 { z } else { z - 146_096 }` branch on the
        // `else` side. Without this, the `z - 146_096` and the
        // subsequent `(z - era * 146_097)` lines (363:45, 365:53)
        // are dead code and their `-` mutants survive.
        //
        // Day -719_528 since 1970-01-01 corresponds to 0000-01-01
        // (proleptic Gregorian). 1970-01-01 is 719_528 days after
        // year 0 Jan 1 (computed: 1970 * 365 + leap_days_through_1969
        // accounting for year-0-is-leap by the divisible-by-400
        // rule). Hinnant's algorithm exposes year 0 as integer 0.
        assert_eq!(civil_from_days(-719_528), (0, 1, 1));
    }

    #[test]
    fn civil_from_days_at_minus_719_469_is_feb_29_year_zero_leap() {
        // Companion pre-epoch witness, 59 days after year-0 Jan 1.
        // Year 0 is a leap year in proleptic Gregorian (divisible
        // by 400), so day 59 of year 0 IS February 29 — pins the
        // leap-year arithmetic on the negative side of the `era`
        // branch as well as the day 31 (Jan) + 28 (Feb 1..28) =
        // day 59 = Feb 29 calendar split.
        assert_eq!(civil_from_days(-719_469), (0, 2, 29));
    }
}
