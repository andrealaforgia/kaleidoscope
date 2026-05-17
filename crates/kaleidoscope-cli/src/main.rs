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
//! kaleidoscope-cli read   <tenant_id> <data_dir> [--service <name>] [--min-severity <level>]
//! kaleidoscope-cli compact <data_dir>
//! ```
//!
//! With `--observe-otlp` set, the ingest subcommand also appends
//! NDJSON OTLP-JSON metric lines to the given path. `tail -f` it
//! to watch the stream.
//!
//! With `--service` and/or `--min-severity` set, `read` filters
//! records server-side via Lumen's `query_with(predicate)`. The
//! severity name is one of `TRACE|DEBUG|INFO|WARN|ERROR|FATAL`.
//!
//! `compact` triggers `snapshot()` on the file-backed Lumen and
//! Cinder stores, bounding the next `open()`'s replay time. It
//! is a whole-store operation, not per-tenant.

#![forbid(unsafe_code)]

use std::io::{self, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use aegis::TenantId;
use kaleidoscope_cli::{compact, ingest, parse_severity, read_filtered, DEFAULT_BATCH_SIZE};
use lumen::Predicate;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("ingest") => run_ingest(&args),
        Some("read") => run_read(&args),
        Some("compact") => run_compact(&args),
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
    eprintln!(
        "kaleidoscope-cli — operator CLI for Lumen v1 + Cinder v1

Usage:
  kaleidoscope-cli ingest <tenant_id> <data_dir> [--observe-otlp <path>]
      Read NDJSON lumen::LogRecord from stdin and persist into <data_dir>.
      Each batch lands in Lumen and a single Cinder Hot tier entry is placed.
      --observe-otlp appends NDJSON OTLP-JSON metric lines to <path>; a
      sidecar can `tail -f` it and forward to a real OTLP/HTTP collector.

  kaleidoscope-cli read <tenant_id> <data_dir> [--service <name>] [--min-severity <level>]
      Query records for <tenant_id> and write NDJSON to stdout. Optional
      --service filters by resource attribute service.name. Optional
      --min-severity is one of TRACE|DEBUG|INFO|WARN|ERROR|FATAL and
      keeps only records whose severity_number >= level.

  kaleidoscope-cli compact <data_dir>
      Trigger snapshot() on Lumen v1 and Cinder v1 stores. Bounds the
      next open() replay time. Whole-store operation, not per-tenant.

Stats are emitted to stderr after `ingest` and `compact` complete."
    );
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
    let (tenant, data_dir) = parse_positional(args)?;
    let predicate = parse_read_predicate(args)?;
    let stdout = io::stdout();
    let writer = stdout.lock();
    let count = read_filtered(&tenant, &data_dir, &predicate, writer)?;
    eprintln!("read ok: records={count}");
    Ok(())
}

fn parse_read_predicate(args: &[String]) -> Result<Predicate, Box<dyn std::error::Error>> {
    // Hand-rolled flag scan, matching parse_observe_otlp's
    // shape: walk args after the subcommand, recognise the two
    // optional `--key value` pairs, error on unknown flags so the
    // operator notices typos instead of getting silent full-table
    // scans.
    let mut predicate = Predicate::new();
    let mut iter = args.iter().skip(2);
    let mut positional_seen = 0usize;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--service" => {
                let name = iter.next().ok_or("--service requires a value")?;
                predicate = predicate.service(name.clone());
            }
            "--min-severity" => {
                let level = iter.next().ok_or("--min-severity requires a value")?;
                let sev = parse_severity(level).ok_or_else(|| {
                    format!(
                        "--min-severity: unknown level {level:?} \
                         (expected TRACE|DEBUG|INFO|WARN|ERROR|FATAL)"
                    )
                })?;
                predicate = predicate.min_severity(sev);
            }
            s if s.starts_with("--") => {
                return Err(format!("read: unknown flag {s:?}").into());
            }
            _ => {
                // tenant_id and data_dir are the two positional args.
                positional_seen += 1;
                if positional_seen > 2 {
                    return Err(format!("read: unexpected extra argument {arg:?}").into());
                }
            }
        }
    }
    Ok(predicate)
}

fn run_compact(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // compact takes no tenant — it's a whole-store operation.
    // args[0] = bin, args[1] = "compact", args[2] = data_dir.
    let data_dir = args.get(2).ok_or("missing <data_dir>")?.clone();
    let stats = compact(&PathBuf::from(data_dir))?;
    eprintln!(
        "compact ok: lumen_snapshotted={} cinder_snapshotted={}",
        stats.lumen_snapshotted, stats.cinder_snapshotted
    );
    Ok(())
}

fn parse_positional(args: &[String]) -> Result<(TenantId, PathBuf), Box<dyn std::error::Error>> {
    // args[0] = bin, args[1] = subcommand, args[2] = tenant,
    // args[3] = data_dir.
    let tenant = args.get(2).ok_or("missing <tenant_id>")?.clone();
    let data_dir = args.get(3).ok_or("missing <data_dir>")?.clone();
    Ok((TenantId(tenant), PathBuf::from(data_dir)))
}
