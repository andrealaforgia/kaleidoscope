# Story Map — cli-unknown-flag-rejection-v0

## Backbone (operator activity sequence)

```text
Operator runs kaleidoscope-cli  ->  Operator mistypes input  ->  CLI rejects loudly  ->  Operator corrects and re-runs
```

The whole feature lives in the third activity: "CLI rejects loudly". The
backbone is a single operator typing a wrong token at the shell and reading
the error before trusting the exit code.

## Activities and stories

| Activity | Story | Kind | Verdict |
|---|---|---|---|
| Reject mistyped top-level flag | US-01 | RE-ANCHOR (test only) | Behaviour correct today |
| Reject unknown subcommand flag | US-02 | CODE GAP (additive fix) | Silent acceptance today |
| Reject mistyped subcommand verb | US-03 | RE-ANCHOR (test only) | Behaviour correct today |
| Keep correct input working | US-04 | RE-ANCHOR (regression) | Must stay byte-equivalent |

## Walking skeleton

Not applicable (Decision 2 = No). This is a brownfield CLI with an existing
hand-rolled parser. There is no end-to-end skeleton to stand up; the
feature is a thin behavioural completion (US-02) wrapped in acceptance
coverage (US-01, US-03, US-04) that gives EDD defect K11 a fresh anchor.

## Priority Rationale

Priority is driven by outcome impact and the code-vs-re-anchor split
established in `wave-decisions.md`:

1. **US-02 first.** It is the only genuine code gap (silent acceptance of
   unknown subcommand-level flags, exit 0). It carries the highest outcome
   risk: an operator currently trusts a wrong result. Doing it first also
   surfaces the exit-code and message-shape decisions (DESIGN flags 2 and
   3) that the re-anchor stories then assert against, so the contract is
   pinned once and reused.
2. **US-04 alongside US-02.** The regression guard must land with the code
   change, not after, because US-02 is the only story that could break
   correct input. Pairing them keeps the fix provably additive.
3. **US-01 and US-03 last.** Pure re-anchor: the behaviour already exists,
   so these are acceptance tests written against the message and exit code
   pinned during US-02. They give K11 its fresh, non-reverted anchor.

Dependency chain: US-02 pins the observable contract (exit code + stderr
substring) -> US-01, US-03, US-04 assert against that pinned contract. No
story depends on the reverted commit e7fbee0; the new tests are the anchor.

## Scope Assessment: PASS — 4 stories, 1 module (kaleidoscope-cli), estimated 1 day

Oversized signals checked (0 of 5 tripped):

- Stories: 4 (limit 10). PASS.
- Bounded contexts / modules: 1 (`kaleidoscope-cli`). PASS.
- Integration points for any slice: 0 (self-contained CLI binary). PASS.
- Estimated effort: about 1 day (one small additive code change plus
  acceptance tests). PASS.
- Independent shippable outcomes: 1 coherent outcome (loud rejection of
  unknown input). PASS.

The feature is right-sized. No split required.
