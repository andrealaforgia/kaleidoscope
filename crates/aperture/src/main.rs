//! Aperture binary — `aperture --config /path/to/aperture.toml`.
//!
//! See `docs/feature/aperture/design/component-design.md > What the
//! binary actually does at startup (sequenced)` for the contract this
//! `main()` will honour: parse args, load config, init logging,
//! warn-on-v0-security-knobs, wire sink, probe, bind listeners, flip
//! `/readyz` to `ready`, await SIGTERM/SIGINT, drain, exit.
//!
//! At DISTILL the binary panics with `unimplemented!()` immediately;
//! that panic IS the canonical RED state. DELIVER lands the
//! sequenced-startup logic per the design contract and the binary
//! goes GREEN slice by slice as `aperture::compose::run` is filled in.

// SCAFFOLD: true
// Status: DISTILL RED scaffold. The binary compiles and links against
// the stub library but panics at runtime on the first call.

fn main() {
    unimplemented!(
        "aperture binary — RED scaffold; DELIVER lands the entry per design/component-design.md"
    )
}
