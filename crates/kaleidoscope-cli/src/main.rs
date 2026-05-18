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
//! kaleidoscope-cli read <tenant_id> <data_dir>
//! ```
//!
//! With `--observe-otlp` set, the ingest subcommand also appends
//! NDJSON OTLP-JSON metric lines to the given path. `tail -f` it
//! to watch the stream.

#![forbid(unsafe_code)]

use std::io::{self, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use aegis::TenantId;
use kaleidoscope_cli::{ingest, read, DEFAULT_BATCH_SIZE};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("ingest") => run_ingest(&args),
        Some("read") => run_read(&args),
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

  kaleidoscope-cli read <tenant_id> <data_dir>
      Query every record for <tenant_id> and write NDJSON to stdout.

Stats are emitted to stderr after `ingest` completes."
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
    let stdout = io::stdout();
    let writer = stdout.lock();
    let count = read(&tenant, &data_dir, writer)?;
    eprintln!("read ok: records={count}");
    Ok(())
}

fn parse_positional(args: &[String]) -> Result<(TenantId, PathBuf), Box<dyn std::error::Error>> {
    // args[0] = bin, args[1] = subcommand, args[2] = tenant,
    // args[3] = data_dir.
    let tenant = args.get(2).ok_or("missing <tenant_id>")?.clone();
    let data_dir = args.get(3).ok_or("missing <data_dir>")?.clone();
    Ok((TenantId(tenant), PathBuf::from(data_dir)))
}
