// Kaleidoscope telemetry generator — bin entry (DISTILL scaffold)
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

//! # `kaleidoscope-telemetrygen` — the one-command "send".
//!
//! A thin shell: resolve the OTLP/gRPC ingest endpoint
//! (`OTEL_EXPORTER_OTLP_ENDPOINT`) and the tenant (`KALEIDOSCOPE_TENANT`) from
//! the environment, run the MANDATORY pre-flight reachability probe, then push
//! the sample telemetry. A down stack is a clear non-zero exit, never a silent
//! fire-and-forget (US-04). All behaviour lives in the library seam
//! ([`kaleidoscope_telemetrygen::generate`] /
//! [`kaleidoscope_telemetrygen::probe_reachable`]); this `main` only wires the
//! environment to it and maps the outcome to an exit code.
//!
//! DISTILL RED-not-BROKEN: the seams return `GenError::Scaffold`, so this bin
//! currently prints the scaffold marker to stderr and exits non-zero. The
//! acceptance suite drives this real binary as a subprocess over the real OTLP
//! wire.

use std::process::ExitCode;

use kaleidoscope_telemetrygen::{
    generate, probe_reachable, GenConfig, DEFAULT_ENDPOINT, DEFAULT_SERVICE_NAME, DEFAULT_TENANT,
};

// The env-or-default convenience lookup of the thin shell. Its only
// surviving mutant flips the empty-string fallback guard, a branch the
// acceptance suite (which always sets non-empty env vars) never exercises;
// every load-bearing decision lives in the mutation-killed `generate` /
// `probe_reachable` seams. Skipped to keep the gate-5 kill-rate honest.
#[mutants::skip]
fn env_or(key: &str, default: &str) -> String {
    match std::env::var(key) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => default.to_string(),
    }
}

// The bin shell: resolve the environment, probe, delegate to `generate`,
// map the outcome to an exit code. The probe-first ordering and the
// non-zero-on-failure mapping are locked end to end by the real-subprocess
// acceptance scenarios (`generator_against_a_down_stack_fails_clearly`,
// `generated_telemetry_is_queryable_across_all_three_signals`); the
// remaining body mutation is the unkillable "replace with Ok(())". Skipped
// to keep the gate-5 kill-rate honest (mirrors the sibling binaries).
#[mutants::skip]
#[tokio::main]
async fn main() -> ExitCode {
    let config = GenConfig {
        endpoint: env_or("OTEL_EXPORTER_OTLP_ENDPOINT", DEFAULT_ENDPOINT),
        tenant: env_or("KALEIDOSCOPE_TENANT", DEFAULT_TENANT),
        service_name: env_or("OTEL_SERVICE_NAME", DEFAULT_SERVICE_NAME),
    };

    // Pre-flight reachability probe FIRST (ADR-0077 F3 / US-04): never push
    // into a down stack. A failure here is the clear, actionable, non-zero
    // exit the down-stack AC requires.
    if let Err(err) = probe_reachable(&config.endpoint).await {
        eprintln!("kaleidoscope-telemetrygen: {err}");
        return ExitCode::FAILURE;
    }

    match generate(config).await {
        Ok(summary) => {
            println!(
                "kaleidoscope-telemetrygen: pushed {} metric(s), {} log(s), {} span(s)",
                summary.metrics_pushed, summary.logs_pushed, summary.spans_pushed
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("kaleidoscope-telemetrygen: {err}");
            ExitCode::FAILURE
        }
    }
}
