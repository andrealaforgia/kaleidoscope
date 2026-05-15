# Augur v0 — user stories

Two LeanUX user stories with mandatory Elevator Pitches per
the nWave DISCUSS template. Personas drawn from
`acme-observability`.

The principal user is **Sasha, a platform engineer** who has
just finished the storage plane v0. Beacon's static
thresholds catch known failure modes, but the unknown
unknowns slip past. Augur v0 is the first cross-pillar
feature in the roadmap that is NOT another storage engine
clone: it is the anomaly-detection layer that watches
numeric streams (Pulse data) and categorical streams (Lumen
log bodies, Ray span names) and emits anomaly events. The
Phase 9 substrate (Bayesian online change-point detection,
sentence-transformer embeddings, vLLM-served Qwen/Mistral
summarisation) is research-grade and high-risk; v0 is the
port-first cut, with two genuinely useful detectors that
exercise the trait shape.

The secondary user is **Riley, an SRE** investigating an
incident she did not page for. Riley wants Augur to surface
"this metric value is unusual" and "this log body is rare"
without writing a Beacon rule first. v0 detects step
changes on numeric streams and rare events on categorical
streams; v1 lifts both to proper statistical models and
ties results to Beacon's incident channel.

System constraints (apply to every story):

1. Library at v0. Augur ships as a Rust crate (`augur`)
   exposing the `AnomalyObserver<T>` trait + two concrete
   observers. The v1 statistical models (BOCPD, vector
   similarity) ship behind the same trait.
2. AGPL-3.0-or-later.
3. **Generic over the observed signal type**. `T = f64` for
   numeric streams (Pulse-style); `T = String` for
   categorical streams (Lumen log bodies, Ray span names).
   The trait stays small and generic.
4. **Per-tenant isolation**. Every observation carries an
   `aegis::TenantId`; baselines are per-tenant.
5. **Streaming online algorithm**. Observers maintain a
   running state and emit anomalies as observations arrive.
   No batch reanalysis at v0.
6. **No LLM summarisation at v0**. The roadmap explicitly
   defers Qwen/Mistral summarisation to v1 with strict
   guardrails. v0 emits structured anomaly events.
7. **No Beacon integration at v0**. Augur produces events;
   the operator binary at v1 wires them into Beacon's
   incident channel.
8. **No ML libraries at v0**. No `numpy`, no
   `scikit-learn`, no `sentence-transformers`. The Phase 9
   roadmap accepts those at v1; v0 ships hand-rolled
   numerical methods (Welford's algorithm for online
   mean/variance, simple frequency baseline for
   categorical rarity) so the v0 dependency graph stays
   tiny.
9. **`MetricsRecorder` seam carries forward**.

---

## US-AU-01 — Numeric anomaly detection via z-score

### Elevator Pitch

- **Before**: Sasha has Beacon for known thresholds and
  the four storage engines for raw data. The unknown
  unknowns (a metric that drifts into a regime no rule was
  written for) slip past.
- **After**: run `cargo test -p augur --test slice_01_zscore`
  → sees `test result: ok. N passed; 0 failed`. The
  acceptance test feeds a stable baseline of values into a
  `ZScoreObserver`, injects a step change three standard
  deviations above the running mean, and asserts the
  observer emits exactly one `Anomaly` event with the
  z-score recorded.
- **Decision enabled**: Sasha installs a `ZScoreObserver`
  per `(tenant, metric_name)` and surfaces every flagged
  anomaly to Riley. v1 replaces this with BOCPD.

### Acceptance criteria

- AC-1.1 — `AnomalyObserver<f64>` is a trait with
  `observe(tenant, value, observed_at) ->
  Option<Anomaly<f64>>`.
- AC-1.2 — `ZScoreObserver::new(threshold: f64,
  min_samples: usize)` builds an observer with a configurable
  z-score threshold and a minimum number of warm-up
  samples before any anomaly can be emitted.
- AC-1.3 — During warm-up (`samples_seen < min_samples`)
  the observer returns `None` regardless of value.
- AC-1.4 — After warm-up, an observation whose `|z| >=
  threshold` returns `Some(Anomaly { tenant, value,
  z_score, observed_at })`.
- AC-1.5 — Anomalies update the baseline (Welford's
  algorithm) — repeated anomalies still emit but they also
  pull the baseline towards the new regime, so the
  detector eventually adapts. (v1 will treat sustained
  anomalies as a change point; v0 is intentionally
  simpler.)
- AC-1.6 — Two tenants observed by the same observer would
  conflate baselines — at v0 the operator creates one
  observer per `(tenant, signal)`; AC-1.6 asserts that two
  *separate* `ZScoreObserver` instances maintain isolated
  baselines.
- AC-1.7 — Reset clears the baseline:
  `ZScoreObserver::reset()` returns to the pre-warm-up
  state.

### KPI anchor

- KPI 1 (Observation latency): p95 ≤ 10 µs per `observe`
  call. Augur sits on the read path of every Pulse
  point; observation must be cheap.

---

## US-AU-02 — Rare-event detection via frequency baseline

### Elevator Pitch

- **Before**: Lumen and Ray can scan-and-filter, but
  cannot say "this log body has not been seen in the last
  hour". The "what's unusual?" question is left to the
  human.
- **After**: run `cargo test -p augur --test slice_02_rare_event`
  → sees `test result: ok. N passed; 0 failed`. The
  acceptance test feeds a stable mix of frequent events
  into a `RareEventObserver`, then injects an event that
  never appeared, and asserts the observer emits one
  `Anomaly<String>` flagging the rare event.
- **Decision enabled**: Sasha installs a
  `RareEventObserver` per `(tenant, signal)` on Lumen log
  bodies and Ray span names. v1 lifts this to
  embedding-based clustering.

### Acceptance criteria

- AC-2.1 — `RareEventObserver::new(rarity_threshold: f64,
  min_samples: usize)` builds an observer where an event
  with observed frequency below `rarity_threshold`
  (fraction of total observations) is emitted as an
  anomaly.
- AC-2.2 — During warm-up the observer returns `None`.
- AC-2.3 — After warm-up, a *new* event (frequency =
  1 / (n+1)) emits an `Anomaly` if it is below
  `rarity_threshold`.
- AC-2.4 — A *previously seen* event whose ongoing
  frequency dips below `rarity_threshold` is NOT
  re-emitted on every observation — only its first crossing
  fires. (v0 simplification: an event becomes "known" the
  moment its count exceeds the rarity floor once; v1 will
  re-evaluate over rolling windows.)
- AC-2.5 — Reset clears the frequency table.

### KPI anchor

- KPI 2 (Observation latency on categorical streams):
  p95 ≤ 20 µs per `observe` call on a vocabulary of up to
  1 000 distinct events. Augur runs inline with Lumen log
  ingest; the per-event cost must be tiny.
