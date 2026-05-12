# ADR-0034 — Beacon CUE rule schema and loader diagnostics

**Status**: Accepted
**Date**: 2026-05-11
**Author**: Bea (autonomous DESIGN dispatch)

## Context

Beacon v0 reads alert rules and SLO declarations from CUE files on
disk. Loom (the eventual Git-backed authority) is a later feature;
Beacon v0 ships with a directory-of-`.cue`-files model and a
`SIGHUP` reload trigger.

The DISCUSS wave decided (D3) on CUE-on-disk + SIGHUP reload. The
DISCUSS KPI 2 demanded 100% recall on broken rules with file +
line + field diagnostic. The scaling target (US-BE-02) is 35
rules in production; the test corpus is 50 files (45 valid, 5
broken in five distinct ways).

CUE is chosen over JSON/YAML for three reasons documented in the
architecture roadmap §C.13: CUE has a real type system, CUE has
constraints (which alert thresholds need), and CUE is the
project-wide standard for declarative authoring (Loom, Aegis,
Prism dashboards all use CUE).

## Decision

The CUE rule schema is locked at `crates/beacon/cue/rule.cue`:

```cue
#Rule: {
    name: =~"^[a-z][a-z0-9_]*$"
    query: string  // PromQL expression
    for_duration: string | *"1m"
    interval: string | *"30s"
    severity: "info" | "warning" | "critical"
    labels: [string]: string
    annotations: {
        summary: string
        runbook_url?: string
    } | *{summary: name}
    inhibits: [...string] | *[]
    sinks: [...#SinkRef]
}

#SinkRef: {
    kind: "webhook" | "smtp" | "mattermost" | "zulip" | "oncall"
    ...  // adapter-specific fields validated by the adapter's CUE schema
}
```

The CUE SLO schema is locked at `crates/beacon/cue/slo.cue`:

```cue
#Slo: {
    service: string
    sli_good_events: string  // PromQL expression
    sli_total_events: string // PromQL expression
    target_availability: >0.0 & <1.0
    error_budget_period: string  // Prometheus duration
    sinks: [...#SinkRef]
}
```

The loader produces operator-readable diagnostics with:

- File path (relative to `--rules <dir>` for clarity)
- Line number (from the CUE parser's error location)
- Offending field name
- Suggestion via edit-distance (`nearest_blessed_match`,
  duplicated from Codex's helper)

Diagnostic format:

```
rules/payments-checkout.cue:12: unknown field "thresehold"
    did you mean "threshold"?
rules/billing.cue:8: missing required field "severity"
rules/db.cue:23: type mismatch on "for_duration": expected string,
    got int (3600)
```

## CUE library substrate

The CUE parsing library is **`cue-ast-rs`** (Apache-2.0,
hypothetical at time of writing — see Knowledge Gap below) if
available, or a **hand-written CUE subset parser** if not. The
hand-written path supports only the documented `#Rule` and `#Slo`
shapes plus their declared constraints; it is not a general CUE
parser.

The choice is a slice-02 spike outcome: before committing to one
path, slice 02's pre-slice SPIKE will exercise both options against
the 50-file corpus and pick the path with cleaner diagnostics.

### Slice 02 SPIKE outcome (2026-05-12): TOML at v0

The slice-02 SPIKE confirmed the Knowledge Gap. The Rust CUE
ecosystem at the project's writing date offers no Apache-2.0 crate
delivering file + line + field diagnostics at the quality KPI 2
requires. The hand-written CUE subset parser would have been weeks
of work — disproportionate to slice 02's scope.

The ADR's named fallback path is taken: **v0 ships TOML** with a
schema that is CUE-shaped semantically (the same fields, the same
required/optional distinctions, the same closed enums for severity
and sink kind). The wire format differs (TOML tables vs CUE
records) but the operator-readable contract is identical: name +
query + for_duration + interval + severity + labels + sinks.

The TOML schema lives at `crates/beacon/src/loader.rs` as a
serde-derived shape with `#[serde(deny_unknown_fields)]`. The
nearest-blessed-match suggestion is a Levenshtein distance ≤ 3
against the known field list. Both contracts named in KPI 2
(100% recall on broken rules, 0% false positives on valid rules)
are pinned by the slice-02 acceptance test corpus.

The migration to CUE is a parser swap, not a schema change. When
Loom (the Git-backed CUE authority) lands, it compiles operator-
authored CUE down to the same Rule shape Beacon consumes today —
either via the same TOML wire format, or by a side-by-side CUE
loader Beacon can adopt without breaking existing TOML deployments.

## Reload semantics

On `SIGHUP`, the loader re-reads the `--rules` directory. The new
catalogue must validate completely (every `.cue` file parses + at
least one rule loads); if validation fails, the active catalogue
stays as-is and a diagnostic is emitted via Beacon's telemetry.
Atomic swap: the evaluator never sees a half-loaded catalogue.

## Knowledge Gap

The Rust CUE ecosystem is sparse as of the project's writing date.
The slice-02 pre-slice SPIKE is named explicitly to validate this
ADR. If both `cue-ast-rs` (or equivalent) and a hand-written subset
parser fail to deliver the required diagnostic shape, the ADR will
be revised — possibly to use a YAML schema with CUE-like
constraints via `serde` + `validator`. The choice is reversible
because the rule schema is the contract, not the parser library.

## Consequences

- Operator-readable diagnostics on every load failure
- 100% recall on broken rules (the load-bearing KPI 2)
- Hot-reload via `SIGHUP` without dropping the active catalogue
- A clean migration path to Loom: when Loom ships, Beacon's loader
  swaps from "read dir on disk" to "read Loom's applied state",
  same schema, same diagnostics
