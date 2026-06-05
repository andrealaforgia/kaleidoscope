# Story Map — `cli-ingest-atomic-v0`

## User: Priya the platform operator

## Goal: ingest an operator-provided NDJSON file all-or-nothing, so a malformed line commits nothing, names the bad line, and re-running is safe

## Backbone

The operator's end-to-end ingest-and-recover journey, left to right:

| Prepare ingest | Run ingest | Read the outcome | Recover from a bad line |
|----------------|------------|------------------|--------------------------|
| Pipe NDJSON file on stdin to `kaleidoscope-cli ingest <tenant> <data_dir>` | Command validates every line, then commits all-or-nothing | Read exit code + stderr; read store count via `stats`/`read` | On a named bad line: re-run safely (no double), fix the line, re-ingest once |

## Walking Skeleton

Not required (brownfield, per `wave-decisions.md` D2). The CLI, the
`ingest` function, and the `read`/`stats` read-back surfaces all
already exist and connect end-to-end. This feature changes the
COMMIT DISCIPLINE of one existing backbone step ("Run ingest"); it
does not stand up a new end-to-end flow.

## Single Slice — Slice 01: all-or-nothing ingest on parse error

This feature is a SINGLE slice with a SINGLE story (US-01). The
behaviour change is atomic and indivisible: validate-all-before-commit
either holds end-to-end or it does not. There is no thinner coherent
slice — you cannot ship "commits nothing on error" without also
shipping "re-run is a no-op" and "corrected file ingests once",
because all three are consequences of the same single discipline
change, and the negative control (valid file unchanged) is the
no-regression guard that must ride with it.

The four pinned acceptance criteria mirror the verifier's K13
reproduction and all sit in US-01:

| AC | What it verifies | Backbone step covered |
|----|------------------|-----------------------|
| parse-error-commits-nothing | Run 1 on `valid + malformed line N` commits 0, names line N | Run ingest → Read the outcome |
| re-run-no-double | Re-run of still-malformed input keeps count at 0 (no double) | Recover from a bad line |
| corrected-file-ingests-once | After fixing the named line, the corrected file commits every record exactly once, exit 0 | Recover from a bad line |
| valid-file-negative-control | A fully-valid file commits every record exactly once, exit 0, no `IngestStats`/stderr regression | Run ingest → Read the outcome |

## Priority Rationale

Single slice, so priority ordering within the feature is trivial —
US-01 is the only story and is P1 (Must Have). The justification for
why this feature is worth doing at all, and why it is scoped exactly
this thin:

- **Outcome impact (Value = 5)**: it closes a RED-ish footgun (K13 /
  issue 009) that the black-box verifier reproduced exactly and the
  four-quadrants assessment named (kaleidoscope-cli Q2-MEDIUM). The
  defect violates the project's durability/honesty posture directly:
  a command that exits non-zero ("I failed") has committed a partial
  prefix, and the obvious recovery (re-run) silently doubles it.
  Trust in the ingest count after a failure is currently zero; this
  restores it to 100%.
- **Urgency (Urgency = 4)**: the footgun is live on `main` (HEAD
  2e2ed58) and is triggered by the most ordinary operator event (a
  malformed line in a piped file) followed by the most ordinary
  operator reflex (re-run a failed job). It is time-sensitive because
  every operator ingest of an imperfect file is exposed.
- **Effort (Effort = 2)**: 1 bounded context (`kaleidoscope-cli`), 1
  modified `src/` file (`ingest`'s commit discipline), 1 new test
  file, 1 manifest line. The recommended mechanism (buffer-all-then-
  flush) widens an existing per-batch buffer to whole-input; the code
  already buffers. No new crate, no new trait, no new `Error`
  variant.
- **Priority score** = Value(5) x Urgency(4) / Effort(2) = 10 — the
  highest band. Tie-breaking is moot (single story).

Why NOT bundle success-case dedup (the deferred concern): it would
flip Effort from 2 to a high band (it touches the `lumen` bounded
context — a second context — and introduces a new persistent
idempotency concept), pulling the slice past right-sized. It is a
SEPARATE outcome (re-running a SUCCESSFUL ingest of a valid file
should not double) with a different blast radius, and the verifier's
K13 does not require it (the all-or-nothing fix already removes the
parse-error-path double-count). Deferring it keeps this slice thin
and independently shippable. See `wave-decisions.md` D-DedupFuture.

## Scope Assessment: PASS — 1 story, 1 bounded context, estimated < 1 day

Oversized-signal check (need 2+ to be oversized; this feature has 0):

- Stories: 1 (US-01). Not >10.
- Bounded contexts / modules: 1 (`kaleidoscope-cli`). Not >3. The
  dedup concern that would pull in the `lumen` context is explicitly
  deferred (D-DedupFuture).
- Walking-skeleton integration points: not applicable (brownfield;
  the flow already exists). Not >5.
- Estimated effort: well under 1 day (one commit-discipline change in
  one function + one new test file). Not >2 weeks.
- Independent shippable outcomes: 1 (all-or-nothing on parse error).
  The deferred dedup is the only other candidate outcome and is
  explicitly out of scope.

Right-sized. No split required.
