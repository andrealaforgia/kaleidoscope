//! Driving adapters — gRPC server (`tonic`) and HTTP server (`axum`).
//!
//! See `docs/feature/aperture/design/component-design.md > Module
//! structure :: transport/grpc.rs and transport/http.rs` for the
//! design contract; ADR-0006 for the library choices. At DISTILL this
//! module is empty; the integration tests enter Aperture through real
//! tonic/axum/reqwest clients against ephemeral loopback ports — the
//! servers themselves are DELIVER-owned.

// SCAFFOLD: true
// Status: DISTILL placeholder. DELIVER lands two driving adapters:
//   - transport::grpc::spawn — tonic Server with LogsService /
//     TracesService / MetricsService impls
//   - transport::http::spawn — axum Router routing /v1/{logs,traces,metrics}
//     plus /healthz + /readyz on the same listener
// Both adapters call into `app::ingest_*` for body validation and sink
// hand-off; both hold per-transport semaphores per ADR-0010.
