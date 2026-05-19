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
    CinderMigrate(MigrateError),
    InvalidTier {
        value: String,
    },
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
            Error::CinderMigrate(e) => write!(f, "cinder migrate: {e}"),
            Error::InvalidTier { value } => {
                write!(f, "invalid tier {value:?}: expected one of hot, warm, cold")
            }
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
    range: TimeRange,
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
    let records = lumen.query(tenant, range).map_err(Error::LumenQuery)?;
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
    range: TimeRange,
) -> Result<usize, Error> {
    let pulse: Arc<dyn MetricStore + Send + Sync> =
        Arc::new(InMemoryMetricStore::new(Box::new(PulseRecorder)));
    let recorder: Box<dyn LumenRec + Send + Sync> = Box::new(LumenToPulseRecorder::new(pulse));
    let lumen =
        FileBackedLogStore::open(lumen_base(data_dir), recorder).map_err(Error::LumenOpen)?;
    let records = lumen.query(tenant, range).map_err(Error::LumenQuery)?;
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

/// Manually migrates `(tenant, item_id)` from its currently-placed
/// tier to `to_tier_arg`. Writes exactly one line to `writer` on
/// success:
///
/// ```text
/// migrated tenant=<tenant> item=<item_id> from=<from> to=<to>\n
/// ```
///
/// where `<from>` and `<to>` render via [`tier_lowercase`] as
/// `hot` / `warm` / `cold`.
///
/// Contract (DESIGN DD1, DD2, DD3 — feature
/// `cli-migrate-subcommand-v0`):
///
/// 1. `to_tier_arg` is parsed via the private [`parse_tier`] helper.
///    Only literal lowercase `hot`/`warm`/`cold` are accepted (no
///    trim, no case-fold). Anything else returns
///    [`Error::InvalidTier`] carrying the verbatim invalid input.
///    The parse runs BEFORE the Cinder store is opened — invalid
///    tier values never touch the filesystem.
/// 2. Cinder is opened against `cinder_base(data_dir)` with a
///    [`CinderRecorder`] (quiescent — no OTLP file). The Lumen store
///    is never opened.
/// 3. `get_entry(tenant, item)` is consulted as a pre-flight to
///    discover the from-tier. `None` → returns
///    [`Error::CinderMigrate`] wrapping
///    [`MigrateError::UnknownItem`] WITHOUT issuing a `migrate`
///    call (no silent insert).
/// 4. Otherwise calls `cinder.migrate(tenant, &item, to_tier,
///    SystemTime::now())` and propagates any [`MigrateError`] as
///    [`Error::CinderMigrate`].
/// 5. On success writes the one-line transition report.
pub fn migrate(
    tenant: &TenantId,
    data_dir: &Path,
    item_id: &str,
    to_tier_arg: &str,
    mut writer: impl Write,
    otlp_log_path: Option<&Path>,
) -> Result<(), Error> {
    let to_tier = parse_tier(to_tier_arg).map_err(|_| Error::InvalidTier {
        value: to_tier_arg.to_string(),
    })?;
    let recorder: Box<dyn CinderRec + Send + Sync> = match otlp_log_path {
        Some(path) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            Box::new(CinderToOtlpJsonWriter::new(file))
        }
        None => Box::new(CinderRecorder),
    };
    let cinder =
        FileBackedTieringStore::open(cinder_base(data_dir), recorder).map_err(Error::CinderOpen)?;
    let item = ItemId::new(item_id.to_string());
    let entry = cinder.get_entry(tenant, &item).ok_or_else(|| {
        Error::CinderMigrate(MigrateError::UnknownItem {
            tenant: tenant.clone(),
            item: item.clone(),
        })
    })?;
    let from = entry.tier;
    cinder
        .migrate(tenant, &item, to_tier, SystemTime::now())
        .map_err(Error::CinderMigrate)?;
    writeln!(
        writer,
        "migrated tenant={} item={} from={} to={}",
        tenant.0,
        item_id,
        tier_lowercase(from),
        tier_lowercase(to_tier)
    )?;
    Ok(())
}

/// Lists every `ItemId` currently placed under `tenant` in `tier`,
/// one per line on `writer`, in lexicographically sorted order.
///
/// Per DESIGN DD2 the list comes from
/// [`cinder::TieringStore::list_by_tier`] and is sorted with
/// `Vec::sort_unstable` at the CLI boundary so the operator's stdout
/// is deterministic across runs (Cinder's underlying `HashMap`
/// iteration is not stable). On empty tier the function writes
/// nothing and returns `Ok(())`. Per DD5 invalid `tier_arg` produces
/// [`Error::InvalidTier`] via the shared [`parse_tier`] helper,
/// byte-identical to `migrate`'s OK3 line.
pub fn list_items(
    tenant: &TenantId,
    data_dir: &Path,
    tier_arg: &str,
    mut writer: impl Write,
) -> Result<(), Error> {
    let tier = parse_tier(tier_arg).map_err(|_| Error::InvalidTier {
        value: tier_arg.to_string(),
    })?;
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .map_err(Error::CinderOpen)?;
    let mut items = cinder.list_by_tier(tenant, tier);
    items.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    for item in &items {
        writeln!(writer, "{}", item.0)?;
    }
    Ok(())
}

/// Parses a literal `hot`/`warm`/`cold` lowercase ASCII tier string.
///
/// Returns `Err(())` for any other input (including upper-case and
/// whitespace-padded variants — no case-fold, no trim per DESIGN
/// DD3). Callers wrap the unit-`Err` into [`Error::InvalidTier`]
/// preserving the verbatim invalid value.
fn parse_tier(s: &str) -> Result<Tier, ()> {
    match s {
        "hot" => Ok(Tier::Hot),
        "warm" => Ok(Tier::Warm),
        "cold" => Ok(Tier::Cold),
        _ => Err(()),
    }
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

/// Howard Hinnant's `civil_from_days` (public domain — see
/// <https://howardhinnant.github.io/date_algorithms.html#civil_from_days>).
/// Converts a signed count of days since the Unix epoch (1970-01-01
/// = 0) into the proleptic Gregorian `(year, month, day)` triple,
/// with `month ∈ [1, 12]` and `day ∈ [1, 31]`.
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

/// Howard Hinnant's `days_from_civil` (public domain — see
/// <https://howardhinnant.github.io/date_algorithms.html#days_from_civil>).
/// Inverse of [`civil_from_days`]: converts a proleptic Gregorian
/// `(year, month, day)` triple into a signed count of days since the
/// Unix epoch (1970-01-01 = 0).
///
/// Caller is responsible for calendar-range validation of inputs;
/// this routine performs no bounds-check (Hinnant's algorithm is a
/// pure integer arithmetic transform).
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y: i64 = (y as i64) - if m <= 2 { 1 } else { 0 };
    let era: i64 = if y >= 0 { y } else { y - 399 } / 400;
    let yoe: u64 = (y - era * 400) as u64; // [0, 399]
    let m_u: u64 = m as u64;
    let d_u: u64 = d as u64;
    let doy: u64 = (153 * if m_u > 2 { m_u - 3 } else { m_u + 9 } + 2) / 5 + d_u - 1; // [0, 365]
    let doe: u64 = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + (doe as i64) - 719_468
}

/// Typed error from [`parse_iso8601_utc_nanos`]. The CLI binary
/// wraps the `Display` of this error with the offending flag name so
/// stderr always names BOTH the flag (`--since` or `--until`) AND
/// the verbatim bad value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsoParseError {
    /// Length not 20 (no-fractional shape) and not 22..=30
    /// (`.D..D` of 1..=9 digits). The `len` field carries the actual
    /// input length so the formatter can render a precise diagnostic.
    BadLength { len: usize },
    /// One of the fixed punctuation slots (`-`, `T`, `:`, `.`, `Z`)
    /// holds a different byte. The `pos` field is the 0-based index.
    BadPunctuation {
        pos: usize,
        expected: char,
        got: char,
    },
    /// A digit slot held a non-ASCII-digit byte.
    NonDigit { pos: usize },
    /// One of the calendar components is outside its admissible
    /// range (year [1970, 9999], month [1, 12], day per month/leap,
    /// hour [0, 23], minute [0, 59], second [0, 59]).
    OutOfRange { field: &'static str, value: u32 },
}

impl fmt::Display for IsoParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IsoParseError::BadLength { len } => {
                write!(
                    f,
                    "invalid ISO 8601 length {len}: expected 20 (no fraction) or 22..=30 (with 1..=9 fractional digits)"
                )
            }
            IsoParseError::BadPunctuation { pos, expected, got } => {
                write!(
                    f,
                    "invalid ISO 8601 punctuation at position {pos}: expected {expected:?}, got {got:?}"
                )
            }
            IsoParseError::NonDigit { pos } => {
                write!(f, "invalid ISO 8601 non-digit at position {pos}")
            }
            IsoParseError::OutOfRange { field, value } => {
                write!(f, "invalid ISO 8601 {field} out of range: {value}")
            }
        }
    }
}

impl std::error::Error for IsoParseError {}

/// Parses an ISO 8601 UTC instant of the shape
/// `YYYY-MM-DDTHH:MM:SSZ` (length 20) or
/// `YYYY-MM-DDTHH:MM:SS.D..DZ` (1..=9 fractional digits) into a
/// `u64` count of nanoseconds since the Unix epoch.
///
/// Calendar-validated: year ∈ [1970, 9999], month ∈ [1, 12], day
/// per month-length with proleptic-Gregorian leap rule (div-by-4,
/// not div-by-100, except div-by-400), hour ∈ [0, 23],
/// minute ∈ [0, 59], second ∈ [0, 59]. No leap-second support
/// (Unix epoch nanos do not encode them either).
///
/// The lower-case `z` form, the trailing `+00:00` offset form, the
/// `T` -> ` ` separator variant, and any non-`Z` zone designator are
/// rejected by [`IsoParseError::BadPunctuation`]. Hand-rolled per
/// DESIGN DD2 to keep the dependency graph tiny (no `chrono`, no
/// `time`).
pub fn parse_iso8601_utc_nanos(s: &str) -> Result<u64, IsoParseError> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    // Acceptable shapes:
    //   - 20: YYYY-MM-DDTHH:MM:SSZ
    //   - 22..=30: YYYY-MM-DDTHH:MM:SS.D..DZ (1..=9 fractional digits)
    let has_fraction = match len {
        20 => false,
        22..=30 => true,
        _ => return Err(IsoParseError::BadLength { len }),
    };

    fn punct(bytes: &[u8], pos: usize, expected: u8) -> Result<(), IsoParseError> {
        if bytes[pos] != expected {
            return Err(IsoParseError::BadPunctuation {
                pos,
                expected: expected as char,
                got: bytes[pos] as char,
            });
        }
        Ok(())
    }

    fn digits(bytes: &[u8], from: usize, to_exclusive: usize) -> Result<u32, IsoParseError> {
        let mut acc: u32 = 0;
        for (offset, b) in bytes[from..to_exclusive].iter().enumerate() {
            if !b.is_ascii_digit() {
                return Err(IsoParseError::NonDigit { pos: from + offset });
            }
            acc = acc * 10 + (b - b'0') as u32;
        }
        Ok(acc)
    }

    punct(bytes, 4, b'-')?;
    punct(bytes, 7, b'-')?;
    punct(bytes, 10, b'T')?;
    punct(bytes, 13, b':')?;
    punct(bytes, 16, b':')?;
    if has_fraction {
        punct(bytes, 19, b'.')?;
        punct(bytes, len - 1, b'Z')?;
    } else {
        punct(bytes, 19, b'Z')?;
    }

    let year = digits(bytes, 0, 4)?;
    let month = digits(bytes, 5, 7)?;
    let day = digits(bytes, 8, 10)?;
    let hour = digits(bytes, 11, 13)?;
    let minute = digits(bytes, 14, 16)?;
    let second = digits(bytes, 17, 19)?;

    if !(1970..=9999).contains(&year) {
        return Err(IsoParseError::OutOfRange {
            field: "year",
            value: year,
        });
    }
    if !(1..=12).contains(&month) {
        return Err(IsoParseError::OutOfRange {
            field: "month",
            value: month,
        });
    }
    let max_day = days_in_month(year, month);
    if !(1..=max_day).contains(&day) {
        return Err(IsoParseError::OutOfRange {
            field: "day",
            value: day,
        });
    }
    if hour > 23 {
        return Err(IsoParseError::OutOfRange {
            field: "hour",
            value: hour,
        });
    }
    if minute > 59 {
        return Err(IsoParseError::OutOfRange {
            field: "minute",
            value: minute,
        });
    }
    if second > 59 {
        return Err(IsoParseError::OutOfRange {
            field: "second",
            value: second,
        });
    }

    let fractional_nanos: u64 = if has_fraction {
        // The fractional digits sit between index 20 and len-1 (the
        // trailing `Z`). 1..=9 digits per DD3.
        let frac_start = 20;
        let frac_end = len - 1;
        let frac_len = frac_end - frac_start;
        let mut acc: u64 = 0;
        for (offset, b) in bytes[frac_start..frac_end].iter().enumerate() {
            if !b.is_ascii_digit() {
                return Err(IsoParseError::NonDigit {
                    pos: frac_start + offset,
                });
            }
            acc = acc * 10 + (b - b'0') as u64;
        }
        // Scale to nanoseconds: 1 digit = 1e8 ns, 9 digits = 1 ns.
        // Multiply by 10^(9 - frac_len).
        let scale: u64 = 10u64.pow((9 - frac_len) as u32);
        acc * scale
    } else {
        0
    };

    let day_index = days_from_civil(year as i32, month, day);
    let secs_of_day: u64 = (hour as u64) * 3_600 + (minute as u64) * 60 + (second as u64);
    let total_seconds: u64 = (day_index as u64) * 86_400 + secs_of_day;
    let total_nanos: u64 = total_seconds * 1_000_000_000 + fractional_nanos;
    Ok(total_nanos)
}

/// Days in a proleptic-Gregorian month, accounting for leap years.
/// Caller guarantees `month ∈ [1, 12]`.
fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => unreachable!("days_in_month called with month out of [1,12]"),
    }
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
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
    use super::{
        cinder_base, civil_from_days, days_from_civil, format_iso8601_utc_nanos, migrate,
        parse_iso8601_utc_nanos, parse_tier, IsoParseError,
    };
    use aegis::TenantId;
    use cinder::{
        FileBackedTieringStore, ItemId, NoopRecorder as CinderRecorder, Tier, TieringStore,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "kal-cli-lib-{name}-{pid}-{nanos}",
            pid = std::process::id()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    // -------- migrate(): white-box mutation-killing witnesses --------

    #[test]
    fn migrate_updates_migrated_at_to_current_clock_above_pre_call_time() {
        // White-box Forge witness — KILLS the
        // `SystemTime::now() -> UNIX_EPOCH` mutant on the
        // `cinder.migrate(..., SystemTime::now())` call. We seed an
        // item, capture its `migrated_at` BEFORE the call (it was set
        // by `place(...)` at the seed-time clock reading), invoke
        // `migrate()`, then re-read the entry through the public
        // `get_entry()` observation surface and assert the new
        // `migrated_at` is >= the captured pre-time. With the
        // UNIX_EPOCH mutant the post-call `migrated_at` collapses to
        // `SystemTime::UNIX_EPOCH`, which is strictly less than any
        // pre-call wall-clock reading — the assertion flips red.
        let root = tmp_dir("migrate_updates_migrated_at");
        let data = root.join("data");
        fs::create_dir_all(&data).expect("mkdir data");
        let acme = TenantId("acme".to_string());
        let item = ItemId::new("acme/forge-item".to_string());

        // Seed via the real Cinder store so the WAL is on disk and
        // `migrate()` will reopen it.
        {
            let cinder = FileBackedTieringStore::open(cinder_base(&data), Box::new(CinderRecorder))
                .expect("open cinder for seeding");
            cinder.place(&acme, &item, Tier::Hot, SystemTime::now());
        }

        // Capture migrated_at BEFORE migrate() is invoked.
        let pre_time = {
            let cinder = FileBackedTieringStore::open(cinder_base(&data), Box::new(CinderRecorder))
                .expect("reopen cinder for pre-time");
            cinder
                .get_entry(&acme, &item)
                .expect("entry exists pre-call")
                .migrated_at
        };

        // Now call the library function under test.
        let mut buf = Vec::<u8>::new();
        migrate(&acme, &data, "acme/forge-item", "cold", &mut buf, None).expect("migrate ok");

        // Re-read migrated_at AFTER migrate() through the public
        // get_entry() surface.
        let post_time = {
            let cinder = FileBackedTieringStore::open(cinder_base(&data), Box::new(CinderRecorder))
                .expect("reopen cinder for post-time");
            cinder
                .get_entry(&acme, &item)
                .expect("entry exists post-call")
                .migrated_at
        };

        // The post-call migrated_at MUST be >= the pre-call
        // reading. With the `SystemTime::now() -> UNIX_EPOCH` mutant,
        // post_time collapses to UNIX_EPOCH which is strictly less
        // than any realistic pre_time — the assertion flips red.
        assert!(
            post_time >= pre_time,
            "post-call migrated_at must be >= pre-call (kills SystemTime::now() -> UNIX_EPOCH; pre={pre_time:?}, post={post_time:?})"
        );

        // Cross-check: post_time must be strictly after the Unix
        // epoch (a real wall-clock reading, not UNIX_EPOCH itself).
        // This is a redundant pin specifically for the named mutant.
        assert!(
            post_time > SystemTime::UNIX_EPOCH,
            "post-call migrated_at must be strictly after UNIX_EPOCH (got {post_time:?})"
        );

        let _ = fs::remove_dir_all(&root);
    }

    // -------- parse_tier(): pins each accepted spelling and the catch-all --------

    #[test]
    fn parse_tier_accepts_each_canonical_lowercase_spelling() {
        assert_eq!(parse_tier("hot"), Ok(Tier::Hot));
        assert_eq!(parse_tier("warm"), Ok(Tier::Warm));
        assert_eq!(parse_tier("cold"), Ok(Tier::Cold));
    }

    #[test]
    fn parse_tier_rejects_case_variants_and_arbitrary_strings() {
        // Per DESIGN DD3 — no case-fold, no trim.
        assert_eq!(parse_tier("HOT"), Err(()));
        assert_eq!(parse_tier("Hot"), Err(()));
        assert_eq!(parse_tier(" hot"), Err(()));
        assert_eq!(parse_tier("hot "), Err(()));
        assert_eq!(parse_tier("LUKEWARM"), Err(()));
        assert_eq!(parse_tier(""), Err(()));
    }

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

    // -------- days_from_civil: inverse of civil_from_days --------

    #[test]
    fn days_from_civil_at_unix_epoch_is_zero() {
        // Anchor: 1970-01-01 IS day 0 since the Unix epoch.
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn days_from_civil_inverts_civil_from_days_across_anchors() {
        // Round-trip discharge: every published anchor in the
        // formatter test block must invert cleanly through
        // days_from_civil.
        for z in [0i64, 1, 31, 58, 59, 365, 789, 20_454] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z, "inverse at z={z}");
        }
    }

    // -------- parse_iso8601_utc_nanos: shape, calendar, round-trip --------

    #[test]
    fn parse_no_fraction_at_unix_epoch_returns_zero() {
        // Anchor: the Unix epoch in the 0-fractional-digits shape
        // (length 20) parses to 0 ns.
        assert_eq!(parse_iso8601_utc_nanos("1970-01-01T00:00:00Z").unwrap(), 0);
    }

    #[test]
    fn parse_nine_fractional_digits_returns_exact_nanos() {
        // 1 ns past the Unix epoch in the 9-fractional-digits shape
        // (length 30): nanos = 1.
        assert_eq!(
            parse_iso8601_utc_nanos("1970-01-01T00:00:00.000000001Z").unwrap(),
            1
        );
    }

    #[test]
    fn parse_intermediate_fractional_digits_scale_to_nanos() {
        // 1 fractional digit `.5` IS 5 * 1e8 ns = 500_000_000 ns.
        assert_eq!(
            parse_iso8601_utc_nanos("1970-01-01T00:00:00.5Z").unwrap(),
            500_000_000
        );
        // 3 fractional digits `.123` IS 123 * 1e6 ns = 123_000_000 ns.
        assert_eq!(
            parse_iso8601_utc_nanos("1970-01-01T00:00:00.123Z").unwrap(),
            123_000_000
        );
        // 6 fractional digits `.000001` IS 1_000 ns (1 microsecond).
        assert_eq!(
            parse_iso8601_utc_nanos("1970-01-01T00:00:00.000001Z").unwrap(),
            1_000
        );
    }

    #[test]
    fn parse_missing_z_returns_bad_punctuation() {
        // No `Z` suffix → punctuation slot at index 19 (no-frac form)
        // fails the `Z` check.
        let err = parse_iso8601_utc_nanos("1970-01-01T00:00:00X").unwrap_err();
        assert!(matches!(err, IsoParseError::BadPunctuation { pos: 19, .. }));
    }

    #[test]
    fn parse_lowercase_z_is_rejected() {
        // Lowercase `z` is NOT accepted — only the canonical
        // capital-Z UTC designator per DD3.
        let err = parse_iso8601_utc_nanos("1970-01-01T00:00:00z").unwrap_err();
        assert!(matches!(err, IsoParseError::BadPunctuation { pos: 19, .. }));
    }

    #[test]
    fn parse_plus_zero_offset_is_rejected() {
        // `+00:00` offset form is NOT accepted — only `Z` per DD3.
        // The total length 25 falls into the 22..=30 fractional
        // range, so it gets past BadLength; the failure surfaces as
        // a punctuation mismatch (`.` expected at position 19).
        let err = parse_iso8601_utc_nanos("1970-01-01T00:00:00+00:00").unwrap_err();
        assert!(matches!(err, IsoParseError::BadPunctuation { pos: 19, .. }));
    }

    #[test]
    fn parse_year_below_1970_is_out_of_range() {
        let err = parse_iso8601_utc_nanos("1969-12-31T23:59:59Z").unwrap_err();
        assert!(matches!(
            err,
            IsoParseError::OutOfRange {
                field: "year",
                value: 1969
            }
        ));
    }

    #[test]
    fn parse_month_thirteen_is_out_of_range() {
        let err = parse_iso8601_utc_nanos("2026-13-01T00:00:00Z").unwrap_err();
        assert!(matches!(
            err,
            IsoParseError::OutOfRange {
                field: "month",
                value: 13
            }
        ));
    }

    #[test]
    fn parse_day_thirty_two_is_out_of_range() {
        let err = parse_iso8601_utc_nanos("2026-01-32T00:00:00Z").unwrap_err();
        assert!(matches!(
            err,
            IsoParseError::OutOfRange {
                field: "day",
                value: 32
            }
        ));
    }

    #[test]
    fn parse_day_zero_is_out_of_range() {
        // Day 0 is below the lower bound; pins the inclusive-lower
        // calendar range check.
        let err = parse_iso8601_utc_nanos("2026-01-00T00:00:00Z").unwrap_err();
        assert!(matches!(
            err,
            IsoParseError::OutOfRange {
                field: "day",
                value: 0
            }
        ));
    }

    #[test]
    fn parse_february_29_on_non_leap_year_is_out_of_range() {
        // 2026 is NOT a leap year, so February has 28 days; day 29
        // exceeds the per-month bound.
        let err = parse_iso8601_utc_nanos("2026-02-29T00:00:00Z").unwrap_err();
        assert!(matches!(
            err,
            IsoParseError::OutOfRange {
                field: "day",
                value: 29
            }
        ));
    }

    #[test]
    fn parse_february_29_on_leap_year_2024_is_accepted() {
        // 2024 IS a leap year (divisible by 4, not by 100); day 29
        // is the maximum for February and must be accepted.
        assert!(parse_iso8601_utc_nanos("2024-02-29T00:00:00Z").is_ok());
    }

    #[test]
    fn parse_february_29_on_century_year_2100_is_out_of_range() {
        // 2100 is divisible by 100 but NOT by 400, so it is NOT a
        // leap year; February has 28 days. Pins the div-by-100
        // exclusion branch of the leap-year predicate.
        let err = parse_iso8601_utc_nanos("2100-02-29T00:00:00Z").unwrap_err();
        assert!(matches!(
            err,
            IsoParseError::OutOfRange {
                field: "day",
                value: 29
            }
        ));
    }

    #[test]
    fn parse_february_29_on_quadricentennial_year_2000_is_accepted() {
        // 2000 IS divisible by 400 so it IS a leap year (overriding
        // the div-by-100 exclusion). Pins the div-by-400 inclusion
        // branch of the leap-year predicate.
        assert!(parse_iso8601_utc_nanos("2000-02-29T00:00:00Z").is_ok());
    }

    #[test]
    fn parse_hour_twenty_five_is_out_of_range() {
        let err = parse_iso8601_utc_nanos("2026-05-18T25:00:00Z").unwrap_err();
        assert!(matches!(
            err,
            IsoParseError::OutOfRange {
                field: "hour",
                value: 25
            }
        ));
    }

    #[test]
    fn parse_minute_sixty_is_out_of_range() {
        let err = parse_iso8601_utc_nanos("2026-05-18T00:60:00Z").unwrap_err();
        assert!(matches!(
            err,
            IsoParseError::OutOfRange {
                field: "minute",
                value: 60
            }
        ));
    }

    #[test]
    fn parse_second_sixty_is_out_of_range() {
        // No leap-second support per DD3; second 60 is rejected.
        let err = parse_iso8601_utc_nanos("2026-05-18T00:00:60Z").unwrap_err();
        assert!(matches!(
            err,
            IsoParseError::OutOfRange {
                field: "second",
                value: 60
            }
        ));
    }

    #[test]
    fn parse_round_trips_format_for_assorted_nanosecond_values() {
        // DD2 round-trip property: parse(format(ns)) == ns for any
        // valid u64 nanos. The formatter always emits exactly nine
        // fractional digits, so the round-trip is exact regardless
        // of which sub-second value is chosen. Witnesses span the
        // arithmetic regimes (epoch, sub-second, second, minute,
        // hour, day, year, far future) so any swap of `+`/`-` on the
        // accumulation path flips at least one witness red.
        for ns in [
            0u64,
            1,
            999_999_999,
            1_000_000_000,
            60 * 1_000_000_000,
            3_600 * 1_000_000_000,
            86_400 * 1_000_000_000,
            365 * 86_400 * 1_000_000_000,
            // 2026-01-01T00:00:00Z = 20_454 days since 1970-01-01.
            20_454 * 86_400 * 1_000_000_000,
            // 2026-01-01T00:00:00.123456789Z
            20_454 * 86_400 * 1_000_000_000 + 123_456_789,
        ] {
            let formatted = format_iso8601_utc_nanos(ns);
            let reparsed = parse_iso8601_utc_nanos(&formatted)
                .unwrap_or_else(|e| panic!("round-trip parse failed for ns={ns}: {e}"));
            assert_eq!(reparsed, ns, "round-trip ns={ns} via {formatted:?}");
        }
    }

    #[test]
    fn parse_wrong_length_is_bad_length() {
        // Length 19 (no `Z` and otherwise shape-correct) → BadLength.
        let err = parse_iso8601_utc_nanos("1970-01-01T00:00:00").unwrap_err();
        assert!(matches!(err, IsoParseError::BadLength { len: 19 }));
        // Length 21 (a single character past the 20-byte no-frac
        // shape, with no `.` separator) → BadLength because the
        // shape table only accepts 20 or 22..=30.
        let err = parse_iso8601_utc_nanos("1970-01-01T00:00:00ZZ").unwrap_err();
        assert!(matches!(err, IsoParseError::BadLength { len: 21 }));
    }

    #[test]
    fn parse_non_digit_in_year_slot_is_non_digit_error() {
        // The `19X0` year slot has a non-digit at position 2 →
        // surfaces as NonDigit (not BadPunctuation), because all
        // five punctuation slots at indexes 4, 7, 10, 13, 16, 19
        // are still correct.
        let err = parse_iso8601_utc_nanos("19X0-01-01T00:00:00Z").unwrap_err();
        assert!(matches!(err, IsoParseError::NonDigit { pos: 2 }));
    }

    // -------- Boundary witnesses: pins `>` against `>=` mutants --------

    #[test]
    fn parse_hour_twenty_three_is_accepted_pins_strict_upper_bound() {
        // Pins `if hour > 23` against the `>=` mutant: hour 23 IS
        // valid; the `>=` mutant would reject it.
        assert!(parse_iso8601_utc_nanos("2026-05-18T23:00:00Z").is_ok());
    }

    #[test]
    fn parse_minute_fifty_nine_is_accepted_pins_strict_upper_bound() {
        // Pins `if minute > 59` against the `>=` mutant: minute 59
        // IS valid; the `>=` mutant would reject it.
        assert!(parse_iso8601_utc_nanos("2026-05-18T00:59:00Z").is_ok());
    }

    #[test]
    fn parse_second_fifty_nine_is_accepted_pins_strict_upper_bound() {
        // Pins `if second > 59` against the `>=` mutant: second 59
        // IS valid; the `>=` mutant would reject it.
        assert!(parse_iso8601_utc_nanos("2026-05-18T00:00:59Z").is_ok());
    }

    // -------- Pin every 30-day month: kills delete-of-`4 | 6 | 9 | 11` arm --------

    #[test]
    fn parse_thirty_day_months_accept_day_thirty() {
        // The `4 | 6 | 9 | 11 => 30` arm in days_in_month covers
        // April, June, September, November. Pinning each at day 30
        // ensures the arm is exercised (the `_ => unreachable!`
        // catch-all panics if the arm is deleted, since month-day
        // validation reaches days_in_month for every parse).
        for month_iso in [
            "2026-04-30T00:00:00Z",
            "2026-06-30T00:00:00Z",
            "2026-09-30T00:00:00Z",
            "2026-11-30T00:00:00Z",
        ] {
            assert!(
                parse_iso8601_utc_nanos(month_iso).is_ok(),
                "30-day month boundary: {month_iso} must parse"
            );
        }
    }

    #[test]
    fn parse_thirty_day_months_reject_day_thirty_one() {
        // Companion to the previous: day 31 in a 30-day month must
        // be rejected by the per-month bound check, which proves
        // the `4 | 6 | 9 | 11 => 30` arm returned 30 (not the
        // fallback 31).
        for (month_iso, _expected_max) in [
            ("2026-04-31T00:00:00Z", 30u32),
            ("2026-06-31T00:00:00Z", 30u32),
            ("2026-09-31T00:00:00Z", 30u32),
            ("2026-11-31T00:00:00Z", 30u32),
        ] {
            let err = parse_iso8601_utc_nanos(month_iso).unwrap_err();
            assert!(
                matches!(
                    err,
                    IsoParseError::OutOfRange {
                        field: "day",
                        value: 31
                    }
                ),
                "30-day month rejects day 31: {month_iso}"
            );
        }
    }

    // -------- IsoParseError Display: kills the body-replacement mutant --------

    #[test]
    fn iso_parse_error_display_renders_each_variant_with_distinguishing_content() {
        // Pins `<impl fmt::Display for IsoParseError>::fmt ->
        // Ok(Default::default())` mutant. The mutant replaces every
        // branch with a no-op write, producing an empty string;
        // asserting each variant contains a recognisable token
        // (`length`, `punctuation`, `non-digit`, `out of range`)
        // flips the mutant red.
        let bad_len = IsoParseError::BadLength { len: 7 }.to_string();
        assert!(
            bad_len.contains("length") && bad_len.contains('7'),
            "BadLength Display includes its key fields: {bad_len:?}"
        );
        let bad_punct = IsoParseError::BadPunctuation {
            pos: 19,
            expected: 'Z',
            got: 'X',
        }
        .to_string();
        assert!(
            bad_punct.contains("punctuation") && bad_punct.contains("19"),
            "BadPunctuation Display includes its key fields: {bad_punct:?}"
        );
        let non_digit = IsoParseError::NonDigit { pos: 2 }.to_string();
        assert!(
            non_digit.contains("non-digit") && non_digit.contains('2'),
            "NonDigit Display includes its key fields: {non_digit:?}"
        );
        let out_of_range = IsoParseError::OutOfRange {
            field: "month",
            value: 13,
        }
        .to_string();
        assert!(
            out_of_range.contains("out of range")
                && out_of_range.contains("month")
                && out_of_range.contains("13"),
            "OutOfRange Display includes its key fields: {out_of_range:?}"
        );
    }

    // -------- Fractional non-digit position: pins `frac_start + offset` arithmetic --------

    #[test]
    fn parse_non_digit_in_fractional_slot_reports_absolute_position() {
        // The fractional digits start at byte offset 20. A non-digit
        // at the SECOND fractional position (absolute byte offset
        // 21) must surface as `NonDigit { pos: 21 }`. This pins the
        // `frac_start + offset` arithmetic against the `-` mutant
        // (which would yield pos = 20 - 1 = 19, the `.` slot) and
        // against the `*` mutant (which would yield pos = 20 * 1 =
        // 20 — wrong absolute index for the second fractional digit).
        let err = parse_iso8601_utc_nanos("2026-05-18T00:00:00.0X3Z").unwrap_err();
        assert!(
            matches!(err, IsoParseError::NonDigit { pos: 21 }),
            "fractional non-digit reports absolute position 21, got {err:?}"
        );
    }

    // -------- days_from_civil: exercise the negative-year else-branch --------

    #[test]
    fn days_from_civil_at_year_zero_january_first_drives_negative_era_branch() {
        // Pins `y - 399` against `y + 399` and `y / 399` mutants on
        // line 451 of days_from_civil. For (year=0, month=1,
        // day=1), the leading `y - if m <= 2 { 1 } else { 0 }` step
        // produces y = -1 (negative), forcing the else-branch
        // `(y - 399) / 400`. With y = -1: (-1 - 399) / 400 = -1
        // (correct era for year 0). The published anchor for year 0
        // Jan 1 is day -719_528 since the Unix epoch (see the
        // companion civil_from_days witness at
        // `civil_from_days_at_minus_719_528_is_year_zero_january_first`).
        // The `-` -> `+` mutant yields (-1 + 399) / 400 = 0,
        // producing a wildly different day count. The `-` -> `/`
        // mutant on `-` is a static type error (i64 / i64 / 400),
        // but cargo-mutants still tries it; either way, the
        // numerical assertion below distinguishes the correct
        // arithmetic from any survivor.
        assert_eq!(days_from_civil(0, 1, 1), -719_528);
    }

    #[test]
    fn days_from_civil_round_trips_civil_from_days_at_negative_anchors() {
        // Cross-check the negative-era branch round-trip against
        // civil_from_days: every pre-Unix-epoch anchor must invert
        // cleanly. -719_528 (year 0 Jan 1) and -719_469 (year 0
        // Feb 29) jointly exercise the negative-era branch with
        // and without the `m <= 2` year-back-off.
        for z in [-1i64, -719_528, -719_469] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z, "inverse at z={z}");
        }
    }
}
