//! `init` — the orchestrator.
//!
//! Per ADR-0011 §"Internal layout": the full init flow lives in this
//! one module — lint pass, AtomicBool CAS, Resource composition,
//! exporter construction, provider construction, global-set, guard
//! return.
//!
//! Per ADR-0015 §1: the Spark-internal `AtomicBool` flag plus
//! delegation to `opentelemetry::global::set_*_provider` Err path
//! are the two-layer single-init detection. Roll-back-on-failure is
//! the transactional property that keeps the test surface sane.
//!
//! ## DISTILL state
//!
//! `init` panics with `unimplemented!()` at the day-one stub. Every
//! integration test under `tests/` calls `spark::init` (directly or
//! transitively via the `common` helpers) and panics here — that is
//! the canonical RED state. DELIVER (Crafty) replaces this panic
//! with the real orchestration as Slice 01 lands.

use crate::config::SparkConfig;
use crate::error::SparkError;
use crate::guard::SparkGuard;

/// The pub(crate) entry the public `spark::init` delegates to.
///
/// At DISTILL: panics with `unimplemented!()`. At DELIVER (Slice 01+):
/// runs the full init flow per ADR-0015 §1.
pub(crate) fn init(_config: SparkConfig) -> Result<SparkGuard, SparkError> {
    unimplemented!(
        "spark::init is the DISTILL-state stub: the lint pass, OTel SDK pipeline \
         construction, global-provider set, and SparkGuard return all land in DELIVER. \
         The integration tests under tests/ are RED until Slice 01 fills this in."
    )
}
