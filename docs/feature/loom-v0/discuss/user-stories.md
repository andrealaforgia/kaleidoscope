# Loom v0 — user stories

Four LeanUX user stories with mandatory Elevator Pitches per the
nWave DISCUSS template. Personas drawn from `acme-observability`,
the same fictional team Beacon, Sieve, Codex, Spark, and Prism have
been built for.

The principal user is **Sasha, a platform engineer** who maintains
the team's rule + dashboard + sampling catalogues in a Git
repository. Sasha wants the operator-side configuration of every
Kaleidoscope component to be reviewable, auditable, and reproducible
— the same shape as the application code the team already ships.

The secondary user is **Riley, an SRE** on the receiving end of
Sasha's deployments. Riley needs the deployed rules to match what
was reviewed in the pull request, and needs to know — from `git
log` alone — when a particular rule was activated.

System constraints (apply to every story):

1. CLI + small server. Loom v0 ships a Rust crate (`loom`) exposing
   the planner / applier as a library, plus a `loom` binary that
   wraps the library with a CLI. The "server" component (Git-backed
   state daemon) arrives at v1 — at v0 the binary runs in CI on
   pull-request and on `main` push, no long-lived process.
2. AGPL-3.0-or-later. Same licensing posture as Aperture, Sieve,
   Codex, Prism, Beacon per `LICENSING.md`.
3. The scope at v0 is **Beacon rule catalogues only**. Sieve
   sampling rules, Prism dashboards, and Aegis policies arrive at
   v1 / v2 once each consumer's contract is settled. The Loom
   pattern transfers verbatim: validate + plan + apply.
4. The schema language is **TOML at v0**, mirroring Beacon's
   ADR-0034 SPIKE outcome. The roadmap names CUE as the long-term
   authority; the migration is a parser swap when the Rust CUE
   ecosystem matures.
5. Three commands at v0: `loom validate`, `loom plan`,
   `loom apply`. Each is operator-readable and operates on a Git
   working tree as input plus a target deployment directory as
   output. The "deployment directory" is whatever path Beacon's
   `--rules <dir>` points at.
6. State storage. v0's "state" is the Git history of the rule files
   themselves; Loom does not maintain a separate state file at v0.
   Drift between the deployed rules and the latest Git HEAD is
   detected by `loom plan` showing a diff. Per-rule deployment
   history lives in `git log`.
7. No remote API at v0. Loom reads + writes the local filesystem
   only. Operators use SSH / volume-mounted directories to apply
   Loom's output to remote Beacon deployments. A future v1
   introduces a small Loom gRPC server for multi-target apply.
8. Pre-commit hook integration. `loom validate` runs in CI on every
   PR; failure blocks merge per the team's policy. Pre-commit hook
   for local validation is recommended but not enforced by Loom.
9. No telemetry. Loom is a CLI tool; it does not phone home or emit
   OTLP telemetry of its own. Operator's own logs (stdout +
   stderr) are the audit trail.
10. The applier is **idempotent**. Running `loom apply` twice on
    the same Git HEAD against the same deployment directory
    produces the same byte-equal result. This is the load-bearing
    contract that makes Loom safe in CI.

---

## US-LO-01 — Walking skeleton: validate one Beacon rule file

### Elevator Pitch

- **Before**: Sasha edits Beacon rule TOML files in a Git
  repository. The repo has 35 rule files. A typo in any file is
  caught only when she rsyncs the directory to the Beacon
  deployment and the daemon reload diagnoses it. The feedback loop
  is ten minutes — long enough for the typo to merge.
- **After**: run `loom validate --rules ./rules/` → sees Loom
  parse every `.toml` file using Beacon's loader, return exit code
  0 if all parse cleanly, non-zero with operator-readable
  `file:line: <message>` if any rejected. The feedback loop is one
  second; the typo is caught in pre-commit.
- **Decision enabled**: Sasha wires `loom validate` into the team's
  pre-commit hook and CI workflow; broken rules can no longer reach
  `main`.

### Acceptance criteria

- AC-1.1 — `loom validate --rules <dir>` walks the directory tree
  and calls `beacon::load_rules(dir)`.
- AC-1.2 — Exit code 0 if every `.toml` file parses cleanly.
- AC-1.3 — Exit code 1 if any file fails; stderr lists each
  diagnostic with `file:line: <message>\n    did you mean "<x>"?`
  (when applicable), one per line.
- AC-1.4 — Exit code 2 if the directory is unreadable.
- AC-1.5 — Stdout reports `validated N rules, rejected M` on
  success and failure.

### KPI anchor

- KPI 1 (Feedback latency): `loom validate` returns in ≤ 100 ms on
  a 50-rule corpus.

---

## US-LO-02 — Plan: diff between Git HEAD and the deployed catalogue

### Elevator Pitch

- **Before**: when Sasha rsyncs the Git directory to the Beacon
  deployment, she has no visibility into what's changing. Are
  three rules being added? Is one being removed? Is the deployment
  about to lose a rule the team didn't intend to remove? The
  reality lands during incident response.
- **After**: run `loom plan --from ./rules/ --to /var/beacon/rules/`
  → sees Loom compute the per-rule diff: which rules are new,
  which are removed, which have changed fields. The output is
  pull-request-shaped (added / removed / changed counts, with
  file paths). A `--diff` flag reveals the per-field changes.
- **Decision enabled**: Sasha includes `loom plan` output in every
  PR description; the team reviews not just the source files but
  the operational delta against the running Beacon.

### Acceptance criteria

- AC-2.1 — `loom plan --from <src-dir> --to <dst-dir>` loads both
  directories via `beacon::load_rules` and computes a diff keyed
  by rule.name.
- AC-2.2 — Output format: `+ added: <name>` / `- removed: <name>`
  / `~ changed: <name>` lines, one per rule, plus a footer
  `summary: A added, R removed, C changed`.
- AC-2.3 — Exit code 0 if both directories validate; 1 if either
  has loader diagnostics.
- AC-2.4 — `--diff` flag adds per-field deltas under each changed
  rule: `~ changed: foo\n    severity: warning → critical\n
  query: up == 0 → up{job="x"} == 0`.
- AC-2.5 — Output is deterministic — same inputs produce
  byte-equal stdout. This is the pinned property that makes Loom
  safe in CI's PR-diff workflow.

### KPI anchor

- KPI 2 (Plan determinism): on a 50-rule corpus with 5 randomised
  changes, `loom plan` produces byte-equal output across 100
  successive runs.

---

## US-LO-03 — Apply: write the new catalogue with idempotency

### Elevator Pitch

- **Before**: rsync is the only tool the team has for "make the
  deployment match Git". It works but has no validation, no diff
  preview, no transaction semantics. A failed rsync leaves the
  deployment in a half-state.
- **After**: run `loom apply --from ./rules/ --to /var/beacon/rules/`
  → sees Loom validate first, then write each `.toml` atomically
  (write to `.tmp` + rename), then remove deleted files. The
  applier is idempotent: running it twice on the same input
  produces a no-op on the second run and exits 0. A SIGHUP signal
  to Beacon picks up the change.
- **Decision enabled**: the team migrates entirely off rsync;
  every Beacon deployment is reproducible from `git log`.

### Acceptance criteria

- AC-3.1 — `loom apply --from <src-dir> --to <dst-dir>` validates
  the source, then atomically writes each rule file (write `.tmp`
  + rename, per POSIX atomicity).
- AC-3.2 — Files present in `<dst-dir>` but not in `<src-dir>`
  are removed.
- AC-3.3 — Files unchanged between `<src-dir>` and `<dst-dir>` are
  not touched (byte-equal content + matching mtime preserved).
- AC-3.4 — Exit code 0 on success; 1 on validation failure (no
  writes); 2 on filesystem error (partial writes possible but the
  atomic-rename pattern ensures no half-written file).
- AC-3.5 — Idempotency: running `loom apply` twice on the same
  inputs produces byte-identical outputs both times and the
  second run reports `summary: 0 added, 0 removed, 0 changed`.

### KPI anchor

- KPI 3 (Apply idempotency): on a 50-rule corpus, the second
  `loom apply` against the same inputs writes zero files.

---

## US-LO-04 — Pre-commit + CI integration

### Elevator Pitch

- **Before**: the team's existing pre-commit hook checks Rust
  formatting and clippy, but does not validate the Beacon rule
  catalogue. CI lacks any rule-catalogue validation. A broken rule
  reaches main, gets noticed when an operator deploys, and a
  fix-forward commit is required.
- **After**: pre-commit hook calls `loom validate` on staged rule
  files; a broken rule blocks the commit locally. CI's pull-request
  workflow runs `loom validate` over the full catalogue and `loom
  plan` against a fixture deployment, posting the plan as a PR
  comment so reviewers see the operational delta. The team's
  policy: PRs cannot merge until the plan is reviewed.
- **Decision enabled**: rule changes follow the same review
  discipline as application code; the change-control surface is
  PR review on a CUE/TOML repository.

### Acceptance criteria

- AC-4.1 — `loom validate` is callable from a pre-commit hook
  without surprising the operator (clean exit codes, terse output
  on success, structured diagnostics on failure).
- AC-4.2 — `loom plan` output is machine-readable (the per-rule
  +/-/~ lines plus the summary footer) so a CI step can post it
  as a PR comment without re-parsing.
- AC-4.3 — `loom plan --json` produces a structured representation
  for tooling: `{added: [...], removed: [...], changed: [...]}`
  with stable field ordering.
- AC-4.4 — The binary's `--help` text names every command, every
  flag, and every exit code so a CI engineer can wire Loom into
  the workflow without reading the source.

### KPI anchor

- KPI 4 (Operator-readable diagnostics): on a 50-rule corpus
  with 5 broken rules, every diagnostic line is parseable by
  `grep -E '^.+:[0-9]+: '` — file + line + message, no escaping
  surprises.
