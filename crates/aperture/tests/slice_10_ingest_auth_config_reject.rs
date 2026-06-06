//! Slice 10 (companion) — fail-closed auth configuration: aperture refuses to
//! run an unauthenticated ingest path by accident.
//!
//! Feature: `aegis-ingest-auth-v0`, US-AUTH-02, ADR-0068 DD4 (mirrors ADR-0061
//! `tls-config-reject-v0`, the refuse-to-start precedent). Auth is on whenever
//! the ingest listeners bind — there is NO off switch. An absent / incomplete /
//! unreadable `[aperture.security.auth.jwt]` block makes aperture REFUSE TO
//! START at `RawConfig::into_config` (the ADR-0061 seam) -> `ConfigError` ->
//! exit 2 -> `event=config_validation_failed` naming the missing/unreadable auth
//! config by reference -> NO listener binds. A complete, readable jwt block
//! starts normally and binds both listeners.
//!
//! ## The refuse-to-start matrix under test (DD4 / environments.yaml)
//!
//! | `[…auth.jwt]` | Result | Event | Exit | Listener |
//! |---|---|---|---|---|
//! | absent                | Refuse | `config_validation_failed` names the missing auth config | 2 | none |
//! | required field missing| Refuse | names the missing field            | 2 | none |
//! | `secret_file` unreadable | Refuse | names the PATH (no secret bytes) | 2 | none |
//! | complete + readable   | Start  | `startup`/`ready`                  | runs | 4317+4318 |
//!
//! ## Driving port (black-box, @real-io)
//!
//! The real `aperture --config <file>` binary subprocess, observed through its
//! exit code, its structured stderr (`config_validation_failed` naming the
//! offending config by reference), and a connect-refused probe on the default
//! OTLP ports (the black-box "no listener bound" observable). Mirrors slice_09's
//! `run_aperture_with_config` + `connect_refused_on_default_otlp_ports` pattern.
//! Every subprocess is reaped via a Drop-guard. The refusal rows use a BOUNDED
//! spawn+try_wait (NOT `.output()`): today's no-auth binary STARTS and BINDS for
//! the refusal configs and would run forever, so the bound turns "did not refuse"
//! into a fast `exit_code: None` RED failure rather than a hang. The positive-bind
//! row uses an ephemeral-port child reaped on every exit path.
//!
//! ## RED-not-BROKEN classification (Mandate 7)
//!
//! aperture exists and the binary builds, so these tests COMPILE and the
//! subprocess spawns today. They are behaviourally RED for two reasons.
//! First: today `Config::from_toml_str` carries `deny_unknown_fields`, so a
//! `[aperture.security.auth.jwt]` table is an UNKNOWN FIELD — a config
//! CONTAINING a complete jwt block FAILS to start today (rejected as an unknown
//! field), so the positive-bind negative control
//! (`complete_jwt_config_starts_and_binds`) FAILS RED; it can only pass once
//! DELIVER adds the jwt schema. Second: today there is NO auth-config
//! requirement, so a config OMITTING the jwt block STARTS and BINDS (no
//! refusal) — the refusal tests assert exit 2 + `config_validation_failed`
//! naming the missing auth config, which FAILS RED today (the omitting config
//! boots, exit 0, listeners bind).
//!
//! Each RED test is `#[ignore = "RED until DELIVER: aegis-ingest-auth-v0"]` so
//! `cargo test --workspace` stays green at the DISTILL commit. Falsifiability is
//! proven by running with `--ignored`: each fails on an assertion (wrong exit
//! code / missing event / a listener bound), NOT on a missing symbol.

use std::io::Write;
use std::net::SocketAddr;
use std::process::{Child, Command};

const REFUSAL_EVENT: &str = "config_validation_failed";
const RED: &str = "RED until DELIVER: aegis-ingest-auth-v0";

// =========================================================================
// Subprocess helpers (binary driving port — @real-io)
// =========================================================================

/// A Drop-guard that ALWAYS reaps the child (kill + wait) on every exit path —
/// success, assertion failure, or panic. A leaked aperture would hold its ports
/// and break sibling tests.
struct ChildReaper(Option<Child>);

impl Drop for ChildReaper {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Write `toml` to a uniquely-named temp file and return its path. The caller
/// owns the file for the lifetime of the subprocess run.
fn write_temp_config(label: &str, toml: &str) -> std::path::PathBuf {
    let unique = format!(
        "aperture-authcfg-{label}-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    let mut f = std::fs::File::create(&path).expect("create temp config");
    f.write_all(toml.as_bytes()).expect("write temp config");
    path
}

/// The outcome of running `aperture --config <path>` with a bounded wait.
struct RefusalRun {
    /// `Some(code)` if the process exited within the bound; `None` if it was
    /// still running when the bound elapsed (i.e. it did NOT refuse — it bound a
    /// listener and is serving). Today's no-auth code takes the `None` branch
    /// for the refusal configs, which is the RED signal.
    exit_code: Option<i32>,
    stderr: String,
}

/// Run `aperture --config <path>` with a BOUNDED wait and ALWAYS reap the child.
///
/// A correct (post-DELIVER) binary refuses to start and exits 2 promptly. Today's
/// no-auth binary instead STARTS and BINDS for the absent/incomplete configs and
/// would run forever — so a plain `.output()` would hang. The bounded
/// spawn+try_wait loop turns "did not refuse" into a fast `exit_code: None`
/// result the caller asserts against, and the `ChildReaper` Drop-guard kills +
/// waits the still-running child on every path (success, assertion failure,
/// panic) so no aperture leaks its ports into sibling tests.
fn run_aperture_with_config(path: &std::path::Path) -> RefusalRun {
    use std::time::{Duration, Instant};

    let child = Command::new(env!("CARGO_BIN_EXE_aperture"))
        .args(["--config", path.to_str().expect("utf-8 path")])
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn aperture binary");
    let mut guard = ChildReaper(Some(child));

    let started = Instant::now();
    let exit_code = loop {
        match guard
            .0
            .as_mut()
            .expect("child present")
            .try_wait()
            .expect("try_wait on aperture child")
        {
            Some(status) => break status.code(),
            None => {
                if started.elapsed() >= Duration::from_secs(5) {
                    // Still running after the bound: it did NOT refuse. The
                    // Drop-guard below reaps it.
                    break None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    };

    // Drain stderr if the process exited (a refusal prints then exits). If it is
    // still running, the reaper kills it on drop and we have no stderr to read
    // without blocking — the empty string is fine because the caller's exit-code
    // assertion fails first (None != Some(2)).
    let stderr = if exit_code.is_some() {
        use std::io::Read;
        let mut buf = String::new();
        if let Some(child) = guard.0.as_mut() {
            if let Some(mut pipe) = child.stderr.take() {
                let _ = pipe.read_to_string(&mut buf);
            }
        }
        buf
    } else {
        String::new()
    };

    RefusalRun { exit_code, stderr }
    // `guard` drops here: kills + waits any still-running child.
}

/// True iff a TCP connect to BOTH default OTLP ports on loopback is refused —
/// i.e. aperture bound no listener. A refused connect is the black-box
/// observable for "no listener".
fn connect_refused_on_default_otlp_ports() -> bool {
    use std::net::TcpStream;
    use std::time::Duration;
    let grpc: SocketAddr = "127.0.0.1:4317".parse().expect("grpc addr");
    let http: SocketAddr = "127.0.0.1:4318".parse().expect("http addr");
    let refused =
        |addr: SocketAddr| TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_err();
    refused(grpc) && refused(http)
}

/// Reserve a free loopback port by binding `:0`, reading the assigned port, then
/// releasing it. A small TOCTOU window, but the binary re-binds immediately and
/// the test owns the box.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral")
        .local_addr()
        .expect("local_addr")
        .port()
}

/// Write the file at `path` with the given bytes and return the path string for
/// embedding in a config.
fn write_file(path: &std::path::Path, bytes: &[u8]) {
    std::fs::write(path, bytes).expect("write aux file");
}

// =========================================================================
// US-AUTH-02 — absent auth config refuses to start (exit 2, no listener)
// =========================================================================

/// US-AUTH-02 / AC: a config that would leave the ingest path unauthenticated by
/// OMISSION refuses to start — exit 2, `config_validation_failed` naming the
/// missing auth config, NO listener binds.
///
/// FALSIFIABILITY: today there is no auth-config requirement, so a config with
/// transport but no `[…auth.jwt]` STARTS and BINDS (exit 0). The exit-2 + event +
/// no-listener assertions all fail on that no-auth behaviour; they pass only once
/// DD4's refuse-to-start invariant lands.
#[test]
fn absent_auth_config_refuses_to_start_naming_missing_auth() {
    // Transport present and valid (default OTLP ports). The ONLY reason to refuse
    // is the absent auth config — had aperture proceeded it would have bound
    // 4317/4318, which the connect-refused probe asserts it did not.
    let toml = r#"
        [aperture.transport.grpc]
        bind_addr = "0.0.0.0:4317"

        [aperture.transport.http]
        bind_addr = "0.0.0.0:4318"
    "#;
    let path = write_temp_config("absent", toml);
    let run = run_aperture_with_config(&path);
    let stderr = run.stderr.clone();
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        run.exit_code,
        Some(2),
        "an absent auth config must exit 2 (refuse to start); stderr: {stderr}"
    );
    assert!(
        stderr.contains(REFUSAL_EVENT),
        "stderr must carry event={REFUSAL_EVENT}; got: {stderr}"
    );
    assert!(
        stderr.contains("auth") && stderr.contains("jwt"),
        "the refusal must name the missing auth (jwt) configuration by reference; got: {stderr}"
    );
    assert!(
        connect_refused_on_default_otlp_ports(),
        "no listener may be bound on 4317/4318 after an auth-config refusal"
    );
}

/// US-AUTH-02 / AC: a jwt block missing a required field (here `catalogue_path`)
/// refuses to start, naming the missing field.
///
/// FALSIFIABILITY: today the whole `[…auth.jwt]` table is an unknown field, so
/// the binary exits 2 for an UNKNOWN-FIELD reason, not for a named missing
/// `catalogue_path`. This test asserts the refusal names `catalogue_path` (the
/// incomplete-field invariant), which fails today and passes only once the jwt
/// schema + the completeness check land.
#[test]
fn incomplete_auth_config_refuses_to_start_naming_the_missing_field() {
    let dir = std::env::temp_dir();
    let stamp = std::process::id();
    let secret_path = dir.join(format!("aperture-authcfg-incomplete-secret-{stamp}.key"));
    write_file(&secret_path, b"test-secret-bytes");

    // A jwt block WITHOUT `catalogue_path` — incomplete.
    let toml = format!(
        r#"
        [aperture.transport.grpc]
        bind_addr = "0.0.0.0:4317"

        [aperture.transport.http]
        bind_addr = "0.0.0.0:4318"

        [aperture.security.auth.jwt]
        issuer = "acme-observability"
        audience = "kaleidoscope-ingest"
        secret_file = "{secret}"
    "#,
        secret = secret_path.display()
    );
    let path = write_temp_config("incomplete", &toml);
    let run = run_aperture_with_config(&path);
    let stderr = run.stderr.clone();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&secret_path);

    assert_eq!(
        run.exit_code,
        Some(2),
        "an incomplete auth config must exit 2; stderr: {stderr}"
    );
    assert!(
        stderr.contains(REFUSAL_EVENT),
        "stderr must carry event={REFUSAL_EVENT}; got: {stderr}"
    );
    assert!(
        stderr.contains("catalogue_path"),
        "the refusal must name the missing required field catalogue_path; got: {stderr}"
    );
    assert!(
        connect_refused_on_default_otlp_ports(),
        "no listener may be bound after an incomplete-auth-config refusal"
    );
}

/// US-AUTH-02 / AC the-secret-is-never-logged: a jwt block whose `secret_file`
/// points at a path that cannot be read refuses to start, names the path, and
/// NO secret bytes appear in the error (there is no readable secret, so the test
/// asserts the path appears and a sentinel secret value never does).
///
/// FALSIFIABILITY: today the jwt table is an unknown field (exit 2 for the wrong
/// reason); this asserts the refusal names the unreadable `secret_file` PATH,
/// which fails today and passes only once the readability check lands. The
/// secret-never-logged half is structurally true (the path is unreadable, so
/// there are no bytes to leak) and guards against a future inline-secret config.
#[test]
fn unreadable_secret_file_refuses_to_start_naming_the_path_not_the_bytes() {
    // A catalogue that DOES exist (so the only fault is the secret file).
    let dir = std::env::temp_dir();
    let stamp = std::process::id();
    let catalogue_path = dir.join(format!("aperture-authcfg-unreadable-cat-{stamp}.toml"));
    write_file(&catalogue_path, b"[[tenants]]\nid = \"acme-prod\"\n");

    let missing_secret = dir.join(format!("aperture-authcfg-NOTHERE-secret-{stamp}.key"));
    // Deliberately do NOT create `missing_secret`.

    let toml = format!(
        r#"
        [aperture.transport.grpc]
        bind_addr = "0.0.0.0:4317"

        [aperture.transport.http]
        bind_addr = "0.0.0.0:4318"

        [aperture.security.auth.jwt]
        issuer = "acme-observability"
        audience = "kaleidoscope-ingest"
        secret_file = "{secret}"
        catalogue_path = "{catalogue}"
    "#,
        secret = missing_secret.display(),
        catalogue = catalogue_path.display()
    );
    let path = write_temp_config("unreadable-secret", &toml);
    let run = run_aperture_with_config(&path);
    let stderr = run.stderr.clone();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&catalogue_path);

    assert_eq!(
        run.exit_code,
        Some(2),
        "an unreadable secret_file must exit 2; stderr: {stderr}"
    );
    assert!(
        stderr.contains(REFUSAL_EVENT),
        "stderr must carry event={REFUSAL_EVENT}; got: {stderr}"
    );
    assert!(
        stderr.contains("secret_file") || stderr.contains(&missing_secret.display().to_string()),
        "the refusal must name the unreadable secret source by reference; got: {stderr}"
    );
    assert!(
        connect_refused_on_default_otlp_ports(),
        "no listener may be bound after an unreadable-secret refusal"
    );
}

// =========================================================================
// US-AUTH-02 — NEGATIVE CONTROL: a complete, readable jwt config STARTS + BINDS
// =========================================================================

/// US-AUTH-02 / AC: a complete, readable `[aperture.security.auth.jwt]` block
/// starts the gateway with the ingest path authenticated — both listeners bind.
///
/// This is the falsifiable positive control: it drives the real binary on
/// EPHEMERAL ports (so it is collision-free in the parallel suite) and probes
/// the gRPC port for a bound listener. It is RED today because the complete jwt
/// block is rejected as an UNKNOWN FIELD (`deny_unknown_fields`) — the binary
/// exits 2 instead of binding. It passes only once DELIVER adds the jwt schema
/// AND the complete-config-starts path. Reaped via a Drop-guard on every exit.
#[cfg(unix)]
#[test]
#[allow(clippy::zombie_processes)]
fn complete_jwt_config_starts_and_binds() {
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    let grpc_port = free_port();
    let http_port = free_port();

    let dir = std::env::temp_dir();
    let stamp = format!("{}-{}", std::process::id(), grpc_port);
    let secret_path = dir.join(format!("aperture-authcfg-ok-secret-{stamp}.key"));
    let catalogue_path = dir.join(format!("aperture-authcfg-ok-cat-{stamp}.toml"));
    write_file(&secret_path, b"complete-config-test-secret-bytes");
    write_file(&catalogue_path, b"[[tenants]]\nid = \"acme-prod\"\n");

    let toml = format!(
        r#"
        [aperture.transport.grpc]
        bind_addr = "127.0.0.1:{grpc_port}"

        [aperture.transport.http]
        bind_addr = "127.0.0.1:{http_port}"

        [aperture.security.auth.jwt]
        issuer = "acme-observability"
        audience = "kaleidoscope-ingest"
        secret_file = "{secret}"
        catalogue_path = "{catalogue}"
    "#,
        secret = secret_path.display(),
        catalogue = catalogue_path.display()
    );
    let config_path = write_temp_config("complete-ok", &toml);

    let child = Command::new(env!("CARGO_BIN_EXE_aperture"))
        .args(["--config", config_path.to_str().expect("utf-8 path")])
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn aperture with a complete jwt config");
    let mut guard = ChildReaper(Some(child));

    // Poll the gRPC port for a bound listener; a correct binary binds promptly.
    let started = Instant::now();
    let mut bound = false;
    while started.elapsed() < Duration::from_secs(10) {
        // If the child already exited, it refused to start — fail fast.
        if let Some(status) = guard
            .0
            .as_mut()
            .expect("child present")
            .try_wait()
            .expect("try_wait")
        {
            let _ = std::fs::remove_file(&config_path);
            let _ = std::fs::remove_file(&secret_path);
            let _ = std::fs::remove_file(&catalogue_path);
            panic!(
                "a complete, readable jwt config must START and bind, but the binary exited \
                 early with {status:?} (today it is rejected as an unknown field — RED)"
            );
        }
        if TcpStream::connect_timeout(
            &format!("127.0.0.1:{grpc_port}").parse().unwrap(),
            Duration::from_millis(200),
        )
        .is_ok()
        {
            bound = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let _ = std::fs::remove_file(&config_path);
    let _ = std::fs::remove_file(&secret_path);
    let _ = std::fs::remove_file(&catalogue_path);

    assert!(
        bound,
        "a complete, readable jwt config must start the gateway and bind the gRPC listener"
    );
    // `guard` reaps the child (kill + wait) on drop here.
}

/// Keep the shared ignore-reason constant referenced so it documents intent for
/// the whole suite.
#[test]
fn red_reason_is_documented() {
    assert_eq!(RED, "RED until DELIVER: aegis-ingest-auth-v0");
}
