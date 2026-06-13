# Evolution archive — prism-echarts-paint-e2e-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md`,
`store-fsync-durability-v0-evolution.md`,
`tls-config-reject-v0-evolution.md`,
`claims-honesty-pass-v0-evolution.md`,
`beacon-sighup-reload-v0-evolution.md`,
`cli-ingest-atomic-v0-evolution.md`,
`cinder-wal-error-surfacing-v0-evolution.md`,
`aperture-serve-loop-error-surfacing-v0-evolution.md`,
`beacon-slo-operator-path-v0-evolution.md`,
`aegis-ingest-auth-v0-evolution.md`,
`spark-ingest-auth-v0-evolution.md`,
`perf-kpi-ci-non-gating-v0-evolution.md`,
`aperture-presubscriber-probe-stderr-v0-evolution.md`,
`speed-up-local-precommit-v0-evolution.md`,
`claims-honesty-pass-2-v0-evolution.md`,
`aperture-body-size-cap-v0-evolution.md` and
`read-path-query-api-auth-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below. This is a VERIFICATION feature on the prism SPA (a
real-browser paint proof for the headline ECharts chart, a narrowed
swallow, and an honest e2e un-MARK), so the record is proportionate to
that scope and carries the load-bearing honest-limit and
verified-the-outcome-not-the-proxy lessons in full.

## Status

- State: DELIVERED and pushed on `main`. Delivered across one DEVOPS
  commit, one DELIVER commit, and a docs commit; the whole story is below.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0075
  (`docs/product/architecture/adr-0075-prism-echarts-paint-verification.md`),
  which records the paint-signal contract (D1), the non-blank-canvas
  pixel probe (D2), the swallow narrowing to catch-and-surface (D3), the
  CI-browser dependency and the honest limit (D4), the `testMatch`
  un-MARK scope (D5), the empty-vs-paint reconciliation by placement
  (D6), and the five rejected alternatives (A through E). It extends
  ADR-0030 (the `<EChart>` wrapper, the `setOption({notMerge:true})`
  update path, the pure `buildOption`), cites ADR-0026 / ADR-0027 and
  defers the latency blocks to the perf-KPI gating posture of ADR-0058 /
  ADR-0070. Supersedes nothing.
- Closes: the four-quadrants prism Q3 finding 2, prism's HEADLINE feature
  (the ECharts chart that IS the visual query result) verified by no
  automated test at all. It is the read-path-UI sibling of the
  honour-fsync (ADR-0049 / 0060) and body-size-cap (ADR-0073) honesty
  features, and the inverse of the `claims-honesty-pass-2-v0` MARK: it
  turns a deferred, scaffold-only e2e into real coverage for the two
  in-scope slices.
- Provenance note (honest): the DEVOPS artefacts landed on `main` in
  `daa3d16`. The DISCUSS, DESIGN and DISTILL wave-decision artefacts and
  ADR-0075 itself were authored by their wave agents and are the read
  source for this archive, but they are still in the working tree, not
  yet committed to `main` at the time of writing this record. This
  archive commit stages ONLY the evolution file; those upstream artefacts
  are left for a separate docs commit and are not roped in here.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| devops | `daa3d16` | scope the existing `gate-7-prism-playwright` D4 CI-browser job to headless Chromium (`--project=chromium`), `continue-on-error` feedback posture, `--pass-with-no-tests`; reuses the docker Prometheus fixture and the digest SSOT; adds the DEVOPS wave artefacts |
| deliver | `ff03dd3` | wire the `data-prism-chart-painted` signal (ECharts `finished` + non-empty series) + catch-and-surface the `setOption` swallow; un-MARK and green slice-01 (paint) + slice-03 (empty / error) in headless Chromium; 6 out-of-scope blocks `test.fixme`d; digest SSOT preserved |
| docs | `7186665` | narrative + slide for the feature (the headline feature nobody had ever tested) |

## The problem, in Earned-Trust framing

prism v0 ships a single PromQL query panel. The operator (Priya) opens
the SPA at incident time, types `up`, presses Enter, and reads the shape
of the returned series off an Apache ECharts line chart. The chart is the
only thing she acts on and the product's headline feature. It had NO
automated test that ever instantiated it. Three seams, all verified on
the branch, made it unprovable:

- The jsdom lifecycle skip (`EChart.tsx:69-84`). The mount effect probes
  `document.createElement('canvas').getContext('2d')`; under jsdom
  (Vitest) that returns `null`, so `echarts.init` is never called and the
  whole render lifecycle is skipped. No Vitest test ever instantiated the
  chart. This skip is legitimate (jsdom has no working canvas-2D) and had
  to be preserved; the problem is that nothing covered the real-browser
  path it deliberately excludes.
- The swallowed paint error (`EChart.tsx:88-98`). The update effect
  wrapped `instance.setOption(option, {notMerge:true})` in an empty
  `try { } catch { }` with the comment "jsdom: canvas paint unavailable".
  The comment was BACKWARDS about its own reach: the effect early-returns
  at `if (instance === null) return;`, and `instance` is non-`null` only
  in a real browser (jsdom never inits), so the catch could fire ONLY in
  a real browser. It swallowed exactly the genuine paint failures an e2e
  must catch and never anything in jsdom.
- The dead e2e. The signal the specs already waited on,
  `[data-prism-chart-painted="true"]`, did not exist in the component;
  the six Playwright specs were detailed pseudocode whose `test()` bodies
  threw `UNIMPLEMENTED`; and `playwright.config.ts` rigged `testMatch` to
  match no spec at all.

So the chart was verified only by a human eyeballing a hand-built `dist/`
bundle. A feature checked only by eyeballs is not verified. The
load-bearing honesty requirement, the reason this is an Earned-Trust
feature and not a feature-add, is that the paint assertion MUST be the
empirical probe that the chart genuinely drew the returned series, in the
real browser substrate where it runs, and it MUST be able to fail against
today's behaviour.

## The design decision (ADR-0075)

The decision is a genuine, falsifiable paint signal, asserted three ways,
plus a swallow narrowed from catch-and-swallow to catch-and-surface, plus
an honest e2e un-MARK that graduates exactly the two in-scope specs.

### D1, the paint-signal contract

A doc-hidden boolean attribute, `data-prism-chart-painted`, on the
`<EChart>` container, whose lifecycle is the contract: it is rendered as
the literal `"false"` on mount (never absent, never `"true"`); it flips
to `"true"` ONLY when the real ECharts `finished` event fires AND
`instance.getOption().series` carries at least one series with at least
one data point; it resets to `"false"` before each `setOption` (so a
stale `"true"` is never observable across queries); and it STAYS `"false"`
under jsdom (no `finished` subscription is ever made because `instance`
is `null`). The `finished` event was chosen over `rendered` (alternative
D, rejected) for its settle-once semantics: `rendered` fires every
animation frame and would flap the signal mid-animation, while `finished`
fires once the render and any animation has settled, which also makes the
signal robust to the `prefers-reduced-motion` toggle.

### D2, the three-part falsifiable assertion

The slice-01 walking-skeleton test asserts a conjunction, not a hollow
DOM-exists check (alternative A, rejected, because waiting for the
`role="figure"` `<div>` passes today against a blank canvas and against
the jsdom skip, the exact green-by-vacuum trap this feature retires):

1. `data-prism-chart-painted="true"` (the signal: `finished` fired with a
   non-empty rendered series; this is also the rendered-series half of
   the observable, so a separate `getOption()` reach-through is
   unnecessary), AND
2. canvas pixels are non-uniform: a `getImageData` pixel probe on the
   same-origin, non-tainted `<canvas>` samples at a stride and asserts
   more than one distinct sampled colour, defeating the
   blank-that-looks-broken case, AND
3. (corroborating, free) the accessible fallback `<table>` reads a
   deterministic series / points count, confirming the data reached
   React.

Exposing the ECharts instance on `window` and asserting via
`getDataURL()` / `getOption()` (alternative B) was rejected: it needs a
production test seam purely for the test, and the canvas `getImageData`
probe achieves the same non-uniformity proof with no seam.

### D3, the swallow narrowing (surface, do not swallow)

The genuine jsdom guard is ALREADY the `if (instance === null) return;`
early-return; the surrounding `try { } catch { }` was dead for jsdom and
live only in a real browser, where it ate the paint failures. The
remediation is catch-and-surface: on a real-browser `setOption` failure,
leave the paint signal at `"false"` (so the slice-01 `waitForSelector`
times out and reds) AND emit a `console.error` (so the slice-03
zero-uncaught-error invariant trips and reds), while NOT re-throwing.
Bare removal of the `try/catch` (alternative C) was rejected: a throw
inside a React `useEffect` with no error boundary unmounts the subtree
and blanks the page, violating "the page stays interactive". So the shape
is catch-and-surface, not catch-and-rethrow and not catch-and-swallow.

### D5 and D6, the un-MARK scope and the empty-state reconciliation

`testMatch` graduates from the no-match sentinel to exactly
`slice-01-walking-skeleton.spec.ts` and
`slice-03-error-and-empty-states.spec.ts`. The `PROMETHEUS_IMAGE_DIGEST`
SSOT shared with CI `gate-11` is preserved byte-for-byte; the per-slice
re-add roadmap comment is corrected truthfully, not deleted. Within the
two graduated files, only the in-scope blocks are implemented and the
rest are `test.fixme`d with a disclosed reason in the title (the perf /
p95 wall-clock blocks per MEMORY `p95_wallclock_flakes_overnight`, the
stop-the-shared-container backend-unreachable block, the config-404 and
malformed-URL blocks). D6 falls out for free: an empty result never
mounts `<EChart>` (it renders the visible "No data" message instead), so
the paint attribute is absent from the DOM in the empty state and can
never be confused with a stuck-`"false"` failed paint; no second marker
is introduced.

This was a reuse, not a build-new: the 6 specs already existed as
pseudocode and the CI job already existed; the work was to un-MARK and
harden 2 specs and wire the signal, with 0 create-new production
artefacts beyond the one extracted predicate module.

## The DEVOPS adaptation (daa3d16)

prism already had CI presence: gates 6 through 11, including
`gate-7-prism-playwright` (the Playwright e2e job, from prism-v0 DEVOPS)
and `gate-10-mutants-prism` (StrykerJS). DEVOPS ADAPTED `gate-7` in place
rather than adding a duplicate (existing-infrastructure-first):
chromium-only install and `--project=chromium` (ADR-0075 C7, with the
firefox / webkit projects still defined but not run by this feature);
`--pass-with-no-tests` so the job stays green at the DEVOPS-wave close
(when `testMatch` still matched no spec) and runs the real paint specs
once DELIVER un-MARKs them; and `continue-on-error: true`. The
continue-on-error posture is feedback, not a gate, consistent with the
project's pure trunk-based, no-required-checks stance (MEMORY
`project_kaleidoscope_pure_trunk_based`); the tighten-to-gating path is
documented: remove `continue-on-error` only once the job is observed
green and stable with the un-MARKed specs. The docker Prometheus fixture
(`e2e/global-setup.ts`) and the digest SSOT are reused unchanged; the
HTML report and traces artefact upload is preserved.

## The as-built shape (deliver ff03dd3)

- `apps/prism/src/lib/echarts/EChart.tsx`: the `data-prism-chart-painted`
  attribute wired through the JSX (initial `"false"`), the `finished`
  subscription on the real-browser mount path (with `instance.off` in
  cleanup before dispose), the reset-before-`setOption` in the update
  effect, and the catch-and-surface (`console.error`, no rethrow, signal
  stays `"false"`).
- `apps/prism/src/lib/echarts/paintSignal.ts` (new): the extracted PURE
  `seriesHasInk` predicate (does any series carry at least one point),
  testable under jsdom without a canvas.
- `apps/prism/tests/paint-signal.test.tsx` (new): 17 unit tests over the
  pure predicate, taking the Vitest count from ~114 to 131 passed.
- `apps/prism/e2e/slice-01-walking-skeleton.spec.ts` and
  `slice-03-error-and-empty-states.spec.ts`: the in-scope blocks
  implemented to the D1 paint signal, D2 pixel probe, and the
  parse-empty-success reset sequence; the 6 out-of-scope blocks
  `test.fixme`d (NOT deleted), preserving their pseudocode and disclosed
  reasons.
- `apps/prism/playwright.config.ts`: `testMatch` un-MARKed ATOMICALLY to
  the two slices; the `PROMETHEUS_IMAGE_DIGEST` SSOT byte-for-byte
  preserved; the roadmap comment corrected, not deleted.
- Local gates green: `tsc` and `eslint` clean; Vitest 131 passed (the +17
  paint-signal tests); Playwright 8 in-scope blocks green in headless
  Chromium against the digest-pinned docker fixture.
- Mutation: prism's tool is StrykerJS (`gate-10`), not cargo-mutants
  (cargo-mutants is N/A for a TypeScript app, so the usual DEVOPS
  Decision 9 mutation-strategy question is N/A and CLAUDE.md's
  `## Mutation Testing Strategy` is unchanged). The local AUTOMATED
  StrykerJS run was blocked by a Node 25 (local) vs Node 22 (CI)
  incompatibility, so manual mutation evidence was produced pre-commit:
  7 of 7 killed on the ADR-0075 C10 surface (the paint-signal branch and
  the narrowed-swallow branch), with the browser-only branches
  Stryker-disabled and justified by the e2e coverage that actually
  exercises them.

## The honest limit (load-bearing, recorded not silently omitted)

The paint coverage is proven LOCALLY under headless Chromium (with docker
for the fixture container, a stated precondition). The CI browser job
(`gate-7`) has NOT been observed green: at the DEVOPS-wave close it was a
trivial 0-spec `--pass-with-no-tests` pass, which is not a paint proof,
and the un-MARKed specs have not yet been seen running green in CI.
Therefore, per ADR-0075 C6, no wave, README or narrative may claim the
chart is "CI-verified". The honest interim claim is "verified locally
under headless Chromium; CI verification pending gate-7". This is the
deferred half of the `claims-honesty-pass-2-v0` MARK now turned into real
local coverage, and it is the exact discipline that feature exists to
enforce: do not re-create an advertised-but-vacuous gate.

## The proof and its boundary

- Falsifiability: against HEAD (no `data-prism-chart-painted`, swallowed
  errors, `testMatch` matching none) the slice-01 test cannot pass; the
  attribute never reaches `"true"`. After the wiring it passes only on a
  non-blank, non-empty paint, asserted as the D1 signal AND the D2 pixel
  non-uniformity AND the fallback series / points count.
- The swallow narrowing reds a genuine real-browser paint fault two
  independent ways: the signal never flips (the wait times out) and the
  `console.error` trips the zero-error invariant.
- The Vitest suite stays green by construction (jsdom never reaches
  `setOption` or the `finished` subscription); the new pure-predicate
  tests carry the jsdom-observable half of the logic.
- Mutation: 7 / 7 killed on the C10 surface by the manual run, the
  automated StrykerJS run deferred to CI's Node 22.
- The boundary: local headless Chromium only; CI gate-7 not yet green
  (see the honest limit above).

## Note for the operator

This feature is operator-invisible at runtime: a doc-hidden attribute,
the `finished` subscription, and a narrowed catch. The visible chart, the
fidelity flags (`buildOption.ts`), the palette and the existing banners
are unchanged. What changed for the engineer is that prism's headline
chart now has a real-browser test that fails if the chart does not paint,
and a `setOption` failure in a real browser now surfaces (a
`console.error`, a red test) instead of being silently eaten. Until
gate-7 is observed green in CI the coverage is local-only; treat the
chart as "verified locally under headless Chromium, CI pending" in any
status claim.

## The lesson

A feature verified only by human eyeballs is not verified. prism's
headline chart, the one thing the operator acts on at incident time, had
no automated test that ever instantiated it; it was checked by a human
squinting at a hand-built bundle. The test that matters asserts the
user-visible OUTCOME, ink on the canvas (the `finished` event plus a
non-blank `getImageData` probe plus a non-empty series), not a PROXY (a
DOM node exists), because the proxy passes on a blank, broken chart and
proves nothing. And a catch that swallows the very failure your test
needs to see is worse than no catch: the comment here claimed the catch
guarded the harmless jsdom case, but the early-return already handled
jsdom, so the catch was live only in the real browser, hiding exactly the
harmful failures. The comment that named the harmless reach was concealing
the harmful one. Surface the failure, do not swallow it, and word the
claim to the substrate actually exercised: "verified locally under
headless Chromium" is the honest claim until gate-7 is green, and the
trade is only honest because that limit is stated, not omitted.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted. The four-quadrants
prism Q3 finding 2 (the untested headline chart) is now CLOSED for the
two in-scope slices, locally; its CI graduation is follow-up 1 below.

1. observe gate-7 green in CI. The un-MARKed slice-01 + slice-03 specs
   have not yet been seen running green in CI. Once they are, the claim
   graduates from "verified locally under headless Chromium" to
   "CI-verified", and gate-7 can tighten from `continue-on-error` to
   gating. Open, and the load-bearing one for this feature.

2. automated StrykerJS gate-10 on CI's Node 22. The local automated
   StrykerJS run was blocked by a Node 25 vs Node 22 incompatibility, so
   the mutation evidence for this feature was the 7 / 7 manual run.
   Running gate-10 automatically on the changed component logic needs
   CI's Node 22. Open.

3. slice-01 cold-first-load flake. The slice-01 walking-skeleton headline
   is cold-first-load flaky locally and relies on the CI `retries: 1`.
   Stabilising the cold first load (or confirming the retry is the right
   long-term posture) is open.

4. the remaining prism e2e scope. The other 4 specs (slices 02 / 04 / 05 /
   06, largely non-paint URL-codec / picker / auto-refresh / a11y
   behaviours), the perf / p95 wall-clock blocks (MEMORY
   `p95_wallclock_flakes_overnight`), and the firefox / webkit browser
   matrix all remain scaffolded future work with their marks intact. Open
   only if wanted.

5. prism auto-run-on-mount for a URL-carried query. A literal "paste the
   link, open it, see the chart" story would need auto-run-on-mount when
   the URL carries a non-empty `q`; prism today pre-fills the input but
   does not auto-execute (DISTILL upstream issue 2). A product change
   beyond ADR-0075 scope. Open only if wanted.

6. faster-test-fsync-backend-v0. The fsync-bound durability bins remain
   I/O-bound in CI, paying the honest per-record `sync_all` of
   ADR-0049 / 0060. Open.

7. read role-gating. v0 read auth is authentication and tenant-scoping
   only; any valid catalogued token (viewer or operator) may read. A
   future role gate is one `if ctx.role != … { reject }` with no
   re-plumbing. Open.

8. ingest role-gating. ingest auth is authentication-only; rejecting a
   valid `viewer` on the write path is the deferred authorization
   decision, one gate with no re-plumbing. Open.

9. aegis "JWKS"-vs-HS256 doc-fix. `aegis/src/lib.rs` overstates "JWKS";
   the validator is HS256 pre-shared-key only. A `docs:` fix-forward or a
   trivial micro-wave. Open.

10. sluice nack-past-cap; sluice wiring; sluice torn-tail migration.
    sluice's behaviour past its cap needs its own slice; sluice remains
    UNWIRED (no `src` path drives `FileBackedQueue`); and its inline
    parse-or-die recovery loop still awaits migration to the shared
    `replay_wal_tolerating_torn_tail` routine (ADR-0059 §5). Open.

11. ingest-dedup-v0 and ingest-bounded-memory. A re-run of a successful,
    fully-valid ingest still doubles the store (lumen has no idempotency
    key, ADR-0064 DD-3), and the buffer-all-then-flush design holds the
    whole input in RAM before commit (ADR-0064). Each earns its own
    slice. Open.

12. ADR-0059 Decision 8 layer b, the AST structural check, remains
    UNWIRED. It is feedback, not a gate, consistent with the pure
    trunk-based, no-required-checks posture; when wired it belongs in the
    local pre-commit stage. Open.

13. OTLP partial_success never populated. The OTLP `partial_success`
    response field is never populated, so partial-accept signalling is
    not surfaced to clients. Open.

14. pulse columnar adapter. The Arrow / Parquet / DataFusion / TSDB
    columnar story was reframed FUTURE-tense rather than built. Open only
    if wanted.

15. body-size-cap rejection counter and genuine per-arm body cap
    (ADR-0073 D4 / D1). Each is a future slice if operators report the
    need. Open only if wanted.

16. beacon non-30d error budget periods. v0 supports ONLY a 30d error
    budget period; other windows would each earn their own slice. Open
    only if wanted.
</content>
