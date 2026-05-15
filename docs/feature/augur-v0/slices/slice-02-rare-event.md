# Slice 02 — `AnomalyObserver<String>` + RareEventObserver (US-AU-02)

## Goal

Extend the same generic trait to categorical streams with
a frequency-baseline detector.

## IN scope

- `RareEventObserver` implementing
  `AnomalyObserver<String>`
- `HashMap<String, u64>` frequency baseline
- First-crossing emission semantics
- KPI 2

## OUT scope

- Embedding-based clustering (v1, sentence-transformers)
- LLM summarisation of clusters (v1)
- Rolling-window evaluation (v1)
- Cross-tenant baseline sharing

## Learning hypothesis

Disproves "a simple frequency table is good enough at v0
to catch unseen log bodies without false-positive spam".
If KPI 2 fails on 1k vocabulary size, the v1 substrate
work needs to land sooner.

## Effort

≤1 day.
