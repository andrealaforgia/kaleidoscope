# Slice 02 — Codex stub-declaration honesty

- **Story**: US-02
- **Priority**: P2
- **Type**: Pure prose (Cargo.toml comment + test headers)
- **Independently shippable**: yes
- **DESIGN weight**: light

## Value

A fully-delivered, green crate stops declaring itself an unbuilt DISTILL stub.

## Exact loci (verified)

| File:line | False claim | Truth source |
|-----------|-------------|--------------|
| `codex/Cargo.toml:17-24` | "DISTILL-state stub … every acceptance test under `tests/` panics with `unimplemented!()`" | `codex/src/lib.rs:43-48` "Fully implemented and green" |
| `codex/tests/slice_01_walking_skeleton.rs:11` | "Tests panic on `unimplemented!()` until DELIVER lands" | live `SchemaCatalogue::validate` |
| `codex/tests/slice_02_otel_semconv_corpus.rs:12` | same | live |
| `codex/tests/slice_03_house_attributes.rs:12` | same | live |
| `codex/tests/slice_04_unknown_attribute_lint.rs:11` | "Tests panic … until DELIVER lands the Err path + Display" | test asserts `result.is_err()` against live Err path |
| `codex/tests/slice_05_fuzzy_suggestions.rs:10` | same | live |
| `codex/tests/common/mod.rs:14-16` | "the validation method itself panics with `unimplemented!()` until DELIVER" | live |

## Acceptance shape (for DISTILL)

Guard: the stub phrases ABSENT in all 7 loci; corrected status PRESENT and
matching the green bodies; `cargo test -p codex` green; no codex test had its
RED/GREEN meaning changed (all were already green).

## Guardrails

- Confirm each test is green before editing its header. No codex test is genuinely
  RED — verified by grep (no active `unimplemented!`/`#[ignore]` in codex tests).
