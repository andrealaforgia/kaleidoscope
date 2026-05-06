//! Aperture binary — `aperture --config /path/to/aperture.toml`.
//!
//! See `docs/feature/aperture/design/component-design.md > What the
//! binary actually does at startup (sequenced)` for the full contract
//! `main()` honours. The binary parses argv for an optional
//! `--config <path>` flag and, when present, delegates to
//! `Config::from_toml_path` (the figment-driven loader landed at
//! ADR-0008 / Slice 07's schema work). When `--config` is absent, the
//! binary falls back to `Config::builder().build()` so an operator
//! can run `cargo run -p aperture` to exercise the end-to-end shape
//! without writing a TOML file first.
//!
//! Exit codes:
//! - `0` — clean drain (every in-flight request completed within the
//!   configured deadline).
//! - `1` — drain deadline exceeded (in-flight requests were
//!   abandoned; `event=drain_deadline_exceeded` warn line on stderr
//!   names the dropped count).
//! - `2` — config error (pre-init; stderr direct print). Covers both
//!   argv parse errors (e.g. `--config` with no path) and TOML
//!   loader errors (file missing, malformed, unknown fields).

use aperture::config::Config;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let argv: Vec<String> = std::env::args().collect();

    let config = match parse_argv(&argv) {
        Ok(Some(path)) => match Config::from_toml_path(&path) {
            Ok(c) => c,
            Err(e) => {
                // Pre-init failure path: tracing subscriber not yet
                // installed (config feeds into compose, which inits the
                // logger). Use stderr directly for this narrow window;
                // the design contract's "tracing is the only stderr-
                // writing path" rule applies post-init only.
                eprintln!("aperture: config error: {e}");
                return std::process::ExitCode::from(2);
            }
        },
        Ok(None) => match Config::builder().build() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("aperture: config error: {e}");
                return std::process::ExitCode::from(2);
            }
        },
        Err(e) => {
            eprintln!("aperture: argv error: {e}");
            return std::process::ExitCode::from(2);
        }
    };

    match aperture::run(config).await {
        Ok(exit_code) => std::process::ExitCode::from(exit_code),
        Err(e) => {
            tracing::error!(error = %e, "aperture exited with error");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Parse argv for `--config <path>`. Returns `Ok(Some(path))` when the
/// flag is present with a value, `Ok(None)` when it is absent, and
/// `Err(...)` when the flag appears without a following value or
/// duplicated. `argv[0]` is the program name and is ignored.
///
/// The parser is deliberately tiny: one supported flag, one
/// position. Aperture v0 has no other CLI surface; future flags grow
/// into a structured parser when the surface widens.
fn parse_argv(argv: &[String]) -> Result<Option<String>, String> {
    let mut iter = argv.iter().skip(1);
    let mut config_path: Option<String> = None;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" => match iter.next() {
                Some(path) => {
                    if config_path.is_some() {
                        return Err("--config given more than once".to_string());
                    }
                    config_path = Some(path.clone());
                }
                None => return Err("--config requires a path argument".to_string()),
            },
            "--help" | "-h" => {
                eprintln!(
                    "aperture: usage: aperture [--config <path>]\n  \
                     --config <path>  load configuration from the given TOML file\n  \
                     (no flag)        run with built-in defaults"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unrecognised argument: {other}")),
        }
    }
    Ok(config_path)
}

#[cfg(test)]
mod tests {
    use super::parse_argv;

    fn argv(args: &[&str]) -> Vec<String> {
        std::iter::once("aperture")
            .chain(args.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn no_args_returns_no_config_path() {
        assert_eq!(parse_argv(&argv(&[])), Ok(None));
    }

    #[test]
    fn config_with_path_returns_path() {
        assert_eq!(
            parse_argv(&argv(&["--config", "/etc/aperture/aperture.toml"])),
            Ok(Some("/etc/aperture/aperture.toml".to_string()))
        );
    }

    #[test]
    fn config_without_path_is_an_error() {
        assert!(parse_argv(&argv(&["--config"])).is_err());
    }

    #[test]
    fn unrecognised_flag_is_an_error() {
        assert!(parse_argv(&argv(&["--bogus"])).is_err());
    }

    #[test]
    fn duplicate_config_flag_is_an_error() {
        assert!(parse_argv(&argv(&["--config", "/a", "--config", "/b"])).is_err());
    }
}
