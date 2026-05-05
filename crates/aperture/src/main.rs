//! Aperture binary — `aperture --config /path/to/aperture.toml`.
//!
//! See `docs/feature/aperture/design/component-design.md > What the
//! binary actually does at startup (sequenced)` for the full contract
//! `main()` will honour. Slice 01 lit up the smallest viable shape:
//! parse `--config`, build a default `Config`, wire the sink, run the
//! listeners, await SIGTERM/SIGINT, drain. Slice 08 makes the drain
//! deterministic and observable through the orchestrator declared in
//! `aperture::shutdown`.
//!
//! Exit codes:
//! - `0` — clean drain (every in-flight request completed within the
//!   configured deadline).
//! - `1` — drain deadline exceeded (in-flight requests were
//!   abandoned; `event=drain_deadline_exceeded` warn line on stderr
//!   names the dropped count).
//! - `2` — config error (pre-init; stderr direct print).

use aperture::config::Config;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Slice 07 lands the `--config <path>` figment-driven loader; the
    // walking-skeleton binary uses defaults so an operator can run
    // `cargo run -p aperture` to exercise the end-to-end shape.
    let config = match Config::builder().build() {
        Ok(c) => c,
        Err(e) => {
            // Pre-init failure path: the tracing subscriber is not yet
            // installed (config feeds into compose, which inits the
            // logger). Use stderr directly for this narrow window;
            // the design contract's "tracing is the only stderr-writing
            // path" rule applies post-init only. Slice 07 lands the
            // figment-driven path that emits
            // `event=config_validation_failed` after `init_logging`.
            eprintln!("aperture: config error: {e}");
            return std::process::ExitCode::from(2);
        }
    };

    match aperture::run(config).await {
        Ok(exit_code) => std::process::ExitCode::from(exit_code),
        Err(e) => {
            // Post-init failure: tracing is initialised by the time
            // `run` returns; route the message through it so operators
            // see one structured stream.
            tracing::error!(error = %e, "aperture exited with error");
            std::process::ExitCode::FAILURE
        }
    }
}
