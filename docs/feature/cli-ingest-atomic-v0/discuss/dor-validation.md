# Definition of Ready Validation — `cli-ingest-atomic-v0`

## Story: US-01 — Operator ingests a file with a malformed line and the command commits nothing, names the bad line, and survives a re-run without double-counting

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1. Problem statement clear, domain language | PASS | "When I ingest a file and a line partway through is malformed, the command commits NOTHING and tells me which line broke" — the all-or-nothing ingest job in operator domain language. Pinned to the verifier's K13 reproduction (run 1 commits 100, re-run commits 200). No technical solution in the problem statement. |
| 2. User/persona with specific characteristics | PASS | Priya the platform operator: runs a multi-tenant Kaleidoscope deployment for a fintech; ingests operator-provided NDJSON files; reflexively re-runs failed batch jobs; reads count back via `stats`/`read`; knows (in v0) that re-ingesting a SUCCESSFUL valid file still doubles (out of scope). Inherited from the `kaleidoscope-cli` cluster. |
| 3. 3+ domain examples with real data | PASS | Five concrete examples with real data: (1) 100 valid + malformed line 101 for `acme` under `/tmp/data`, run-rerun-fix-ingest; (2) 3 valid + malformed line 4 at `batch_size=3`; (3) re-run of the still-malformed `batch_size=3` input, count stays 0; (4) fully-valid 250-record file (3 batches: 100+100+50); (5) malformed FIRST line names line 1. Real tenant (`acme`), real batch sizes, real counts. |
| 4. UAT in Given/When/Then (3-7 scenarios) | PASS | Five UAT scenarios: parse-error-commits-nothing, re-run-no-double, corrected-file-ingests-once, valid-file-negative-control, malformed-first-line boundary. Each Given/When/Then uses concrete data and observable outcomes (the `Err(ParseRecord{line})` shape, the `Ok(IngestStats{..})` shape, the post-call `read`/`stats` count). Within the 3-7 band. |
| 5. AC derived from UAT | PASS | Eight checkbox AC, the first five mirroring the five UAT scenarios one-to-one (the four verifier-pinned ones plus the first-line boundary), plus three guard AC (locked tests stay green; new test file added; no new dependency / no new `Error` variant). Each AC is observable and automatable. |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | 1 story, 5 UAT scenarios, 1 bounded context (`kaleidoscope-cli`), 1 modified `src/` file, 1 new test file, 1 manifest line. Estimated well under 1 day. Scope assessment in `story-map.md` and `wave-decisions.md` confirms 0 oversized signals. |
| 7. Technical notes: constraints/dependencies | PASS | Technical Notes section names: the DESIGN-locked mechanism choice (D-BufferVsStream: buffer-all vs two-pass); the modified file (`ingest`'s commit discipline at `lib.rs:157-246`); the preserved `Error::ParseRecord`/`IngestStats` shapes; the new test file; the Gate-5 mutation requirement; the explicit out-of-scope dedup deferral. Dependencies section enumerates the existing Lumen/Cinder/`Error`/`IngestStats`/`aegis` surfaces reused unchanged. |
| 8. Dependencies resolved or tracked | PASS | All dependencies already exist and are in-tree: `lumen::FileBackedLogStore`, `cinder::FileBackedTieringStore`, `Error::ParseRecord`, `IngestStats`, `aegis::TenantId`, `serde_json`. No new external or internal crate dependency. The one DESIGN-deferred decision (D-BufferVsStream mechanism) is explicitly tracked and does not block readiness — the behaviour contract (the four AC) is fixed regardless of mechanism. The deferred dedup concern is tracked as a future feature (D-DedupFuture), not a blocking dependency. |
| 9. Outcome KPIs defined with measurable targets | PASS | Four KPIs in `outcome-kpis.md`: OK1 parse-error-commits-nothing (100%, baseline 0% — verifier reproduced partial commit), OK2 re-run-no-double (100%, baseline 0% — verifier reproduced double-count), OK3 corrected-file-ingests-once (100%), OK4 valid-file-no-regression (100% byte-equivalence guardrail). Each has Who/Does-what/By-how-much/Baseline/Measured-by. North star = OK1. |

## DoR Status: PASSED

All 9 items pass with evidence. No remediation required.

## Notes

- **DIVERGE artefacts absent**: no `docs/feature/cli-ingest-atomic-v0/diverge/recommendation.md`
  or `job-analysis.md`. Recorded as a LOW risk in `wave-decisions.md`
  (the job is singular and pinned by the verifier's K13 reproduction;
  there is exactly one reasonable behaviour — commit zero on any parse
  failure). Story traceability is to the inline JTBD job statement in
  `wave-decisions.md`, not to a DIVERGE `job-analysis.md`.
- **Mechanism deliberately left to DESIGN**: the buffer-vs-stream
  trade-off (D-BufferVsStream) is flagged for DESIGN, not decided
  here. This is correct per the solution-neutral requirements posture:
  DISCUSS locks the all-or-nothing BEHAVIOUR (the four AC); DESIGN
  owns the MECHANISM. Readiness is not blocked by the open mechanism
  choice because every candidate mechanism satisfies the same fixed
  behaviour contract.
- **Out-of-scope dedup is explicit**: success-case re-run dedup is
  deferred (D-DedupFuture) and called out in the System Constraints,
  the Out-of-Scope section of US-01, and the KPI doc. This prevents
  scope creep into the `lumen` bounded context and keeps the slice
  right-sized.
