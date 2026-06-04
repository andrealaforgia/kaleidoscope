# Slice 01 — README codename honesty

- **Story**: US-01
- **Priority**: P1 (cheapest, sharpest)
- **Type**: Pure prose (README only)
- **Independently shippable**: yes
- **DESIGN weight**: light (prose alignment, no decision)

## Value

The first table an evaluator reads stops overstating four crates' capabilities.

## Exact loci (verified)

| File:line | False (present-tense) | Corrected (truth + future tense) | Canonical truth source |
|-----------|-----------------------|----------------------------------|------------------------|
| `README.md:171` | Spark "Auto-instrumentation SDKs" | "Manual-init OTel SDK wrapper" (auto-instrumentation: v0.2/v1) | `spark/src/lib.rs:1-17`; `docs/feature/spark/wave-decisions.md:177,191` |
| `README.md:179` | Strata "Continuous profiling" | "Profile storage (passive sink)" (continuous scraping: roadmap) | `strata/src/lib.rs:17-46` |
| `README.md:213` | "Continuous profiling as a top-tier add-on … Strata is included." | profiling included as first-party storage; continuous scraping is roadmap | same |
| `README.md:180` | Cinder "cold-tier coordinator" | "Local tier-metadata coordinator" (object-storage S3/OpenDAL/Iceberg cold tier: v2) | `cinder/src/lib.rs:17-48`; `cinder/Cargo.toml:7` |
| `README.md:185` | Loom "Dashboards-as-code, alert-rules-as-code" | "Rule-catalogue change control (TOML)" (dashboards-as-code: v1+) | `loom/src/lib.rs:17-38`; `loom/Cargo.toml:11` |

## Acceptance shape (for DISTILL)

Guard: each of the four false phrases ABSENT as a present-tense claim; each
corrected phrase + its future-tense qualifier PRESENT; each present-tense claim
consistent with the crate `lib.rs`.

## Guardrails

- README only. Do NOT touch the roadmap (C.6 already future-tense) or the
  already-true durability `Status` block (lines 89-95).
