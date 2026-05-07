# Slice 02 — Full OTel semconv 0.27 corpus

## Outcome added

The `SchemaCatalogue` knows every attribute the upstream
`opentelemetry-semantic-conventions = "=0.27.x"` crate exposes.
Resource attributes drawn from the standard semconv set
(`service.name`, `service.version`, `service.namespace`,
`service.instance.id`, `deployment.environment`, `host.name`,
`host.arch`, `os.type`, `process.runtime.name`, ...) all validate
clean.

## What it lights up

- The corpus-generation step that turns the upstream semconv crate's
  exported constants into Codex's `&'static [BlessedAttribute]` table.
  This can be a `build.rs`-time generation, a checked-in generated
  file under `crates/codex/src/generated/`, or a hand-curated list
  derived from the upstream crate at the pinned version. The slice
  picks one approach and documents the regeneration ritual.
- The lookup performance characteristic: validation of a typical
  Resource (10–20 attributes) against a corpus of low-hundreds of
  blessed names completes in well under a millisecond.

## Demo command

```sh
cargo test -p codex --test slice_02_otel_semconv_corpus
```

The test instantiates the catalogue, builds a Resource carrying a
realistic spread of standard semconv attributes (the set Spark itself
composes today, plus a handful of common extras), and asserts
`validate(...)` returns `Ok(())` for every entry in the spread.

## Acceptance summary

- The catalogue exposes a count accessor (or equivalent) such that the
  slice test can assert at least N blessed attributes are present,
  where N is the cardinality of the upstream 0.27 semconv resource
  attribute set. The exact figure is documented in the slice test.
- A representative real-world Resource attribute set validates clean.
- The regeneration ritual is documented in `crates/codex/README.md`
  (or rustdoc on the catalogue type) so that bumping the upstream pin
  has a single repeatable procedure.
- 100% mutation kill rate on the modified files.

## Complexity drivers

- Choice of generation strategy. Build-script generation keeps the
  source of truth in the upstream crate but adds a `build.rs`. A
  checked-in generated file is auditable in review and gives faster
  builds, at the cost of needing a regeneration step on pin bumps.
  Recommendation: checked-in generated file, regeneration script
  documented; the corpus changes only when the pin moves.
- Scope discipline: 0.27 includes attributes for many signal types
  (trace, metric, log, resource). v0 cares about *resource* attributes
  only. The slice should bless only the resource subset, with a clear
  comment in the generator about why.

## Out of scope

- House attributes beyond `tenant.id` (Slice 03).
- Multi-version semconv catalogues (post-v0).
- Validation of attribute *values* — v0 only validates names.
- Trace / metric / log scoped attributes — only Resource attributes
  are blessed at v0.
- Lint diagnostics on failure paths (Slice 04 onwards).
