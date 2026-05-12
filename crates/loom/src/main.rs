// Kaleidoscope Loom — change-control CLI
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

//! Loom CLI entry point.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use loom::{apply, plan, validate};

#[derive(Debug, Parser)]
#[command(
    name = "loom",
    about = "Kaleidoscope Loom — Git-backed change-control surface for operator catalogues",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate every `.toml` rule file in a directory against
    /// Beacon's loader. Exits 0 on success, 1 if any file fails,
    /// 2 if the directory is unreadable.
    Validate {
        /// Directory of `.toml` rule files. Walked recursively.
        #[arg(long, value_name = "DIR")]
        rules: PathBuf,
    },
    /// Compute the per-rule diff between a source directory (Git
    /// working tree) and a destination directory (deployed
    /// catalogue). Exits 0 on success, 1 if either side has loader
    /// diagnostics, 2 if either directory is unreadable.
    Plan {
        /// Source directory (Git working tree).
        #[arg(long, value_name = "DIR")]
        from: PathBuf,
        /// Destination directory (deployed catalogue).
        #[arg(long, value_name = "DIR")]
        to: PathBuf,
        /// Emit per-field deltas under each `~ changed` line.
        #[arg(long)]
        diff: bool,
    },
    /// Make the destination directory match the source using atomic
    /// file operations. Source must validate cleanly; otherwise no
    /// writes happen. Idempotent: a second run is a no-op. Exits 0
    /// on success, 1 if source validation failed, 2 on filesystem
    /// error.
    Apply {
        /// Source directory (Git working tree).
        #[arg(long, value_name = "DIR")]
        from: PathBuf,
        /// Destination directory (deployed catalogue).
        #[arg(long, value_name = "DIR")]
        to: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { rules } => run_validate(&rules),
        Command::Plan { from, to, diff } => run_plan(&from, &to, diff),
        Command::Apply { from, to } => run_apply(&from, &to),
    }
}

fn run_validate(dir: &std::path::Path) -> ExitCode {
    let outcome = validate(dir);
    if let Some(err) = &outcome.fatal {
        eprintln!("{}: {err}", dir.display());
        return ExitCode::from(outcome.exit_code());
    }
    for diag in &outcome.diagnostics {
        eprintln!("{}", diag.display());
    }
    println!(
        "validated {} rules, rejected {}",
        outcome.rules_loaded,
        outcome.diagnostics.len()
    );
    ExitCode::from(outcome.exit_code())
}

fn run_plan(from: &std::path::Path, to: &std::path::Path, include_diff: bool) -> ExitCode {
    let outcome = plan(from, to);
    if let Some(err) = &outcome.fatal {
        eprintln!("{err}");
        return ExitCode::from(outcome.exit_code());
    }
    for diag in &outcome.diagnostics_from {
        eprintln!("from: {}", diag.display());
    }
    for diag in &outcome.diagnostics_to {
        eprintln!("to: {}", diag.display());
    }
    print!("{}", outcome.render(include_diff));
    ExitCode::from(outcome.exit_code())
}

fn run_apply(from: &std::path::Path, to: &std::path::Path) -> ExitCode {
    let outcome = apply(from, to);
    if let Some(err) = &outcome.fatal {
        eprintln!("{err}");
        return ExitCode::from(outcome.exit_code());
    }
    for diag in &outcome.diagnostics {
        eprintln!("{}", diag.display());
    }
    print!("{}", outcome.render());
    ExitCode::from(outcome.exit_code())
}
