# Kaleidoscope — agent guidance

This file is the bootstrap pointer for AI agents working in this
repository. It is intentionally short. Per-feature waves keep their
deeper context under `docs/feature/<feature-id>/`.

## Development Paradigm

Rust idiomatic: data + free functions + traits where polymorphism is
genuinely needed. No class-style inheritance hierarchies (Rust has none),
no `dyn Trait` indirection where direct generic monomorphisation
suffices, composition over inheritance throughout. This shape matches
Morgan's DESIGN brief for `otlp-conformance-harness-v0` and is the
natural shape for validation-and-decode libraries in the wider Rust
ecosystem (`serde_json`, `prost`, `regex` all expose this shape).

For implementation work, use `@nw-software-crafter`. The crafter agent
is the only agent that writes production source under
`crates/<name>/src/`. All other agents write specifications, ADRs, peer
reviews, or workflow YAML.

## Mutation Testing Strategy

This project uses per-feature mutation testing. Runs after refactoring
during each delivery, scoped to modified files. Kill rate gate: 100%
(per ADR-0005 Gate 5).
