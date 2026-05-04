//! Shutdown orchestrator — SIGTERM/SIGINT handler, `/readyz` flip to
//! `Draining`, listener close, drain-deadline-bounded wait on
//! per-transport semaphores, exit-code mapping.
//!
//! See `docs/feature/aperture/design/component-design.md > Module
//! structure :: shutdown/mod.rs` and `docs/feature/aperture/slices/
//! slice-08-graceful-shutdown.md` for the design contract. At DISTILL
//! this module is empty; `aperture::Handle::shutdown` (declared in
//! `lib.rs`) is the integration-test seam, panicking with
//! `unimplemented!()` until DELIVER lands the orchestrator.

// SCAFFOLD: true
// Status: DISTILL placeholder. DELIVER lands `orchestrate_shutdown`
// here per the design contract.
