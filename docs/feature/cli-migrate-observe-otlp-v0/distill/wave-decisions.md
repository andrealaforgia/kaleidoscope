# Wave Decisions — `cli-migrate-observe-otlp-v0` / DISTILL

- **Wave**: DISTILL
- **Author**: Quinn (`@nw-acceptance-designer`)
- **Date**: 2026-05-19
- **Mode**: Translate DISCUSS / DESIGN / DEVOPS contracts into one new
  Rust integration test file. Idiom is Rust `#[test]` with
  `// Given / // When / // Then` comments (DISCUSS sys-constraint §3);
  no Gherkin. Locked sibling for shape:
  `crates/kaleidoscope-cli/tests/migrate_subcommand.rs`.

## Inputs

`discuss/{user-stories,outcome-kpis,wave-decisions}.md`;
`design/wave-decisions.md` (DD1 sixth-arg `Option<&Path>`; DD2 internal
`match` AFTER `parse_tier?`; DD3 `main.rs` reuses `parse_observe_otlp`;
DD5 six mechanical `, None` call-site updates); `devops/wave-decisions.md`
(A1+A2 zero-workflow-edit, A3 zero new deps); the two locked sibling
tests; `crates/self-observe/src/cinder_otlp_json.rs` (wire shape: scope
`kaleidoscope.cinder`, metric `cinder.migrate.count`, point attrs
`[tenant_id, from, to]`, `asInt = "1"`).

## DWD-01: Four `#[test]` functions, one new file

New file: `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs`.
Four tests, one per KPI:

| # | Test name | KPI | Boundary |
|---|-----------|-----|----------|
| 1 | `migrate_with_observe_otlp_emits_one_cinder_migrate_count_line` | OK1 (principal) | library |
| 2 | `migrate_without_observe_otlp_creates_no_file_at_candidate_path` | OK2 (file-absence half) | library |
| 3 | `migrate_subcommand_unknown_item_with_observe_otlp_leaves_no_emission` | OK3 | subprocess |
| 4 | `migrate_subcommand_invalid_tier_with_observe_otlp_creates_no_sink_file` | OK4 | subprocess |

OK2's stdout byte-equivalence half is owned by the locked
`migrate_subcommand.rs` (NOT edited beyond DD5's four `, None`
suffixes). Test #2 covers the file-absence half the locked file cannot
test (it pre-dates the flag).

## DWD-02: Harness duplication, not extraction

Inline-duplicate `tenant`, `temp_root`, `cleanup`, `cinder_base`,
`place_item`, `bin` per DISCUSS D5 + DEVOPS forward-compat note. NINTH
inline duplication in the cluster. Rule of three octuply discharged;
extraction to `tests/common/mod.rs` is a deliberate cross-file refactor
out of scope here. `temp_root` prefix is `kal-cli-migrate-otlp-` (distinct
from `migrate_subcommand.rs`'s `kal-cli-migrate-` so concurrent runs do
not collide).

## DWD-03: OK1 wire-shape assertions (principal)

Test #1 filters sink lines on `metric.name == "cinder.migrate.count"`
then asserts:

- exactly ONE non-empty filtered line (per-migrate cardinality);
- `scopeMetrics[0].scope.name == "kaleidoscope.cinder"`;
- `resource.attributes[0].value.stringValue == "acme"`;
- `sum.dataPoints[0].asInt == "1"`;
- point attrs CONTAIN `{from: "hot"}` and `{to: "cold"}` — order NOT
  pinned at the CLI level (library pins it; CLI consumes unchanged
  per DESIGN handoff §2);
- file ends with `\n`.

Stdout invariant on the same call: byte-equivalent to the no-flag
report (`migrated tenant=acme item=acme/batch-00042 from=hot to=cold\n`).
Flag adds the sink line; does NOT alter stdout.

## DWD-04: Library-direct vs subprocess split

- **Library-direct (#1, #2)**: pin the LIBRARY contract — wire shape on
  success; file-absence on the `None` arm. Uses a `Vec<u8>` stdout
  buffer + on-disk sink path; captures the only side-effecting change
  the feature adds without subprocess fork cost.
- **Subprocess (#3, #4)**: pin the BINARY contract — `--observe-otlp`
  argv parsing, dispatcher arm, exit-code propagation, sink-path
  threading. Error-path load-bearing assertions are exit code + sink-
  file state; both require the actual binary. Locked
  `migrate_subcommand.rs` established this same split for OK2/OK3 of
  the migrate feature; this file inherits it for OK3/OK4 here.

OK4's file-absence assertion is the load-bearing probe for the parse-
before-open contract (DD2). OK3 permits the file to exist empty (the
`Some(path)` arm runs `OpenOptions::create(true)` before `get_entry`
short-circuits) but pins absence of the `cinder.migrate.count` line.

## DWD-05: RED state and DELIVER signal

The test file calls `migrate(...)` with SIX arguments; the shipped
`migrate(...)` takes five. The file does NOT compile against the
current crate — that compile failure IS the RED gate. Crafty's GREEN
landing executes DEVOPS A2's unified atomic change: add the
`otlp_log_path: Option<&Path>` parameter (DD1), insert the `match`
arm (DD2), thread `parse_observe_otlp(args)?.as_deref()` in `main.rs`
plus the usage-text update (DD3), add the `[[test]]` block, and append
`, None` to the six DD5 call sites (one in `main.rs`, one inline
white-box in `lib.rs`, four in the locked `migrate_subcommand.rs`).
One commit per ADR-0005.

## Out of scope at DISTILL

- No edit to `lib.rs`, `main.rs`, `Cargo.toml`, `ci.yml` (hard task-
  brief constraints; reinforced by DEVOPS A1+A2+A3).
- No edit to `migrate_subcommand.rs` (locked; OK2 stdout oracle).
- No `cargo` invocation; DISTILL deliverable, not a DELIVER gate.
- No new ADR; ADR-0039 §1, §2, §8 are reused unchanged.

## Hand-off

Next: `nw-software-crafter` (DELIVER). Inputs: this file + new test
file + DESIGN DD5 call-site list + DEVOPS A1-A4 constraints.
