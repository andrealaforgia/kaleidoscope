# Slice 03 — House attribute completeness

## Outcome added

The three Kaleidoscope-house attributes are first-class members of the
catalogue alongside the upstream OTel semconv set:

- `tenant.id` — exact name match.
- `experiment.id` — exact name match.
- `feature_flag.{key}` — prefix-and-arbitrary-suffix; any attribute
  whose name starts with `feature_flag.` and continues with a non-empty
  suffix is blessed. Matches the convention Spark already uses when it
  composes feature-flag attributes on a Resource.

A Resource carrying e.g. `feature_flag.checkout_v2 = "treatment"` and
`tenant.id = "acme-prod"` and `experiment.id = "exp-2026-q2-pricing"`
validates clean.

## What it lights up

- The pattern-matching shape inside `BlessedAttribute`. `tenant.id` and
  `experiment.id` are exact matches; `feature_flag.{key}` is a prefix
  rule. The internal `BlessedAttribute` enum (or struct with a `kind`
  field) needs to carry both shapes — a discriminator `Exact(&str)`
  vs `Prefix(&str)` is the recommended encoding.
- The lookup path now branches: an exact-match miss falls through to a
  prefix-match scan against the prefix-blessed entries.

## Demo command

```sh
cargo test -p codex --test slice_03_house_attribute_completeness
```

The test validates a Resource carrying all three house attributes
(with realistic-looking values), plus a representative spread of OTel
semconv attributes from Slice 02.

## Acceptance summary

- `tenant.id` and `experiment.id` validate clean.
- `feature_flag.checkout_v2`, `feature_flag.dark_mode`,
  `feature_flag.tiered_pricing.v3` all validate clean — the suffix can
  itself contain dots and underscores.
- A bare `feature_flag.` (empty suffix) does NOT validate clean — it
  is treated as Unknown; documented in the slice test.
- The prefix rule is documented in rustdoc on `BlessedAttribute`.
- 100% mutation kill rate on the modified files.

## Complexity drivers

- The prefix-rule shape is the first place v0 admits more than one
  kind of blessed entry. Worth getting the encoding right now —
  `enum BlessedAttribute { Exact(&'static str), Prefix(&'static str) }`
  keeps the lookup code a single match.
- House attributes deserve a comment block in the source naming each
  one and citing the Kaleidoscope convention that introduced it
  (Spark's existing `feature_flag.*` usage). This is documentation
  the future Aegis integration will read.

## Out of scope

- Per-tenant overlays — house attributes are global, not tenant-scoped
  at v0.
- Validation of feature-flag *values* (`treatment` / `control` /
  variant names) — v0 validates names only.
- Lint diagnostics for unknown attributes (Slice 04).
- Fuzzy match suggestions for typos like `feature_flg.checkout_v2`
  (Slice 05).
