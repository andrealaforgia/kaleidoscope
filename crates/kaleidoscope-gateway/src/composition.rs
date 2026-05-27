// Kaleidoscope gateway — Earned-Trust composition seam
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

//! Gateway composition-root logic, lifted out of `main.rs` so the
//! Earned-Trust refuse pattern is unit-testable. Mirrors the read APIs'
//! `composition::probe` shape (`crates/log-query-api/src/composition.rs`,
//! `crates/trace-query-api/src/composition.rs`); the gateway's twist is
//! that it composes TWO independent probes at the same site: (a) the
//! storage-sink probe (DD5 / ADR-0041, "wire-then-probe-then-use") and
//! (b) the fsync-honesty probe (ADR-0049, "Earned-Trust honours fsync").
//!
//! Both probes must pass before the listener binds. On any failure the
//! composition root emits `event=health.startup.refused` and exits
//! non-zero; the listener never binds. The substrate descriptor for an
//! fsync failure rides on the existing event as
//! `substrate=<descriptor>` (ADR-0049 §7).

use std::fmt;
use std::path::Path;

use aperture::ports::Probe;
use pulse::{fsync_probe, FsyncBackend, FsyncProbeError};

/// Typed refusal classes from the gateway's composition seam. Each
/// variant carries the information the binary emits as the payload of
/// `event=health.startup.refused`.
#[derive(Debug)]
pub enum CompositionError {
    /// The storage-sink probe (DD5 / ADR-0041) refused: the sink opened
    /// but the active-write check failed. The string is the underlying
    /// reason produced by `OtlpSink::probe`.
    SinkProbe(String),
    /// The fsync-honesty probe (ADR-0049) refused: the substrate
    /// admitted a class of fsync lie. The probe error names the lie
    /// class via [`FsyncProbeError::substrate_descriptor`].
    FsyncProbe(FsyncProbeError),
}

impl CompositionError {
    /// Map the refusal class to the `substrate=<descriptor>` payload
    /// the composition root attaches to `event=health.startup.refused`.
    /// A sink probe failure descriptor is `sink`; an fsync probe
    /// failure delegates to the typed `FsyncProbeError` descriptor
    /// (`fsync-noop`, `fsync-truncating`, `fsync-corrupting`,
    /// `fsync-io`).
    pub fn substrate_descriptor(&self) -> &'static str {
        match self {
            CompositionError::SinkProbe(_) => "sink",
            CompositionError::FsyncProbe(e) => e.substrate_descriptor(),
        }
    }
}

impl fmt::Display for CompositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompositionError::SinkProbe(reason) => {
                write!(f, "storage sink probe failed: {reason}")
            }
            CompositionError::FsyncProbe(e) => {
                write!(f, "{e}")
            }
        }
    }
}

impl std::error::Error for CompositionError {}

/// Probe-or-refuse: run BOTH Earned-Trust probes (storage sink first,
/// fsync honesty second) before the listener binds. On any failure
/// returns the typed [`CompositionError`] the binary maps to
/// `event=health.startup.refused`; on success returns `Ok(())` and the
/// caller proceeds to bind.
///
/// The fsync probe runs against `fsync_root` (the pulse pillar root,
/// per ADR-0049 §8) using `fsync_backend` (the real backend in
/// production, a lying double in tests).
pub async fn probe_or_refuse<S>(
    sink: &S,
    fsync_root: &Path,
    fsync_backend: &dyn FsyncBackend,
) -> Result<(), CompositionError>
where
    S: Probe + ?Sized,
{
    sink.probe()
        .await
        .map_err(|e| CompositionError::SinkProbe(e.to_string()))?;
    fsync_probe(fsync_root, fsync_backend).map_err(CompositionError::FsyncProbe)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aperture::ports::ProbeError;
    use pulse::{LyingFsyncBackend, RealFsyncBackend};
    use std::future::Future;
    use std::path::PathBuf;
    use std::pin::Pin;

    fn temp_root(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "kgw-composition-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&p).expect("mkdir");
        p
    }

    fn cleanup(root: &Path) {
        let _ = std::fs::remove_dir_all(root);
    }

    /// An honest sink stub: probe always returns Ok.
    struct OkSink;

    impl Probe for OkSink {
        fn probe<'a>(
            &'a self,
        ) -> Pin<Box<dyn Future<Output = std::result::Result<(), ProbeError>> + Send + 'a>>
        {
            Box::pin(async { Ok(()) })
        }
    }

    /// A sink stub that refuses with a custom reason.
    struct RefusingSink {
        reason: String,
    }

    impl Probe for RefusingSink {
        fn probe<'a>(
            &'a self,
        ) -> Pin<Box<dyn Future<Output = std::result::Result<(), ProbeError>> + Send + 'a>>
        {
            let reason = self.reason.clone();
            Box::pin(async move {
                Err(ProbeError::Unreachable {
                    endpoint: "test://refusing".to_string(),
                    reason,
                })
            })
        }
    }

    // Kills the "skip the sink probe" mutant: a failing sink refuses
    // before the fsync probe runs, and the error names the sink reason.
    #[tokio::test]
    async fn refusing_sink_short_circuits_before_fsync_probe() {
        let root = temp_root("sink_refuses_first");
        let sink = RefusingSink {
            reason: "no write quorum".to_string(),
        };
        let backend = RealFsyncBackend;

        let outcome = probe_or_refuse(&sink, &root, &backend)
            .await
            .expect_err("sink refusal propagates");

        match &outcome {
            CompositionError::SinkProbe(reason) => {
                assert!(reason.contains("no write quorum"), "got: {reason}");
            }
            other => panic!("expected SinkProbe; got {other:?}"),
        }
        assert_eq!(outcome.substrate_descriptor(), "sink");
        cleanup(&root);
    }

    // Kills the "skip the fsync probe" mutant: a passing sink leaves
    // the lying fsync substrate to refuse with FsyncIgnored.
    #[tokio::test]
    async fn lying_fsync_substrate_refuses_after_sink_passes() {
        let root = temp_root("fsync_refuses_second");
        let sink = OkSink;
        let backend = LyingFsyncBackend::no_op();

        let outcome = probe_or_refuse(&sink, &root, &backend)
            .await
            .expect_err("lying substrate refuses");

        match &outcome {
            CompositionError::FsyncProbe(FsyncProbeError::FsyncIgnored) => {}
            other => panic!("expected FsyncProbe(FsyncIgnored); got {other:?}"),
        }
        assert_eq!(outcome.substrate_descriptor(), "fsync-noop");
        cleanup(&root);
    }

    // Kills the "swap the probe order" mutant: a refusing sink AND a
    // lying fsync substrate must produce the sink refusal (the first
    // probe), proving the sink probe runs first.
    #[tokio::test]
    async fn sink_probe_runs_before_fsync_probe() {
        let root = temp_root("sink_first");
        let sink = RefusingSink {
            reason: "sink down".to_string(),
        };
        let backend = LyingFsyncBackend::no_op();

        let outcome = probe_or_refuse(&sink, &root, &backend)
            .await
            .expect_err("first refusal wins");

        assert!(
            matches!(outcome, CompositionError::SinkProbe(_)),
            "the sink probe runs first; got {outcome:?}",
        );
        cleanup(&root);
    }

    // Kills the happy-path mutant: both honest probes proceed.
    #[tokio::test]
    async fn both_honest_probes_proceed() {
        let root = temp_root("happy");
        let sink = OkSink;
        let backend = RealFsyncBackend;

        probe_or_refuse(&sink, &root, &backend)
            .await
            .expect("both probes pass; listener may bind");
        cleanup(&root);
    }

    // Kills the substrate_descriptor mutants on CompositionError.
    #[test]
    fn substrate_descriptor_is_distinct_per_refusal_class() {
        assert_eq!(
            CompositionError::SinkProbe("any".to_string()).substrate_descriptor(),
            "sink",
        );
        assert_eq!(
            CompositionError::FsyncProbe(FsyncProbeError::FsyncIgnored).substrate_descriptor(),
            "fsync-noop",
        );
        assert_eq!(
            CompositionError::FsyncProbe(FsyncProbeError::BytesLost).substrate_descriptor(),
            "fsync-truncating",
        );
        assert_eq!(
            CompositionError::FsyncProbe(FsyncProbeError::BytesMismatch).substrate_descriptor(),
            "fsync-corrupting",
        );
    }

    // Kills the Display mutants on CompositionError.
    #[test]
    fn display_renders_each_refusal_class() {
        let sink = format!(
            "{}",
            CompositionError::SinkProbe("write quorum".to_string())
        );
        assert!(sink.contains("write quorum"), "got: {sink}");
        let fsync = format!(
            "{}",
            CompositionError::FsyncProbe(FsyncProbeError::FsyncIgnored)
        );
        assert!(fsync.contains("ignored"), "got: {fsync}");
    }
}
