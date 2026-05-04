//! Application core — `ingest_logs/traces/metrics`, readiness state,
//! shutdown orchestrator, response mappers.
//!
//! See `docs/feature/aperture/design/component-design.md > app::*` for
//! the full module breakdown DELIVER will land. At DISTILL this module
//! is empty; the integration tests do not import it (they enter
//! through driving ports — gRPC and HTTP listeners — over real
//! loopback TCP).

// SCAFFOLD: true
// Status: DISTILL placeholder. DELIVER lands `ingest_logs`,
// `ingest_traces`, `ingest_metrics`, `framing_for_transport`,
// `ReadinessState`, `summarise_record`, `responses::*`, and the
// shutdown orchestrator inside this module per the design contract.
