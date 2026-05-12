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
use loom::validate;

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
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { rules } => run_validate(&rules),
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
