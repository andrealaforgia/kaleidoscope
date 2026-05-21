# Journey: closing the read loop (query-range-api-v0)

Backend feature, lightweight UX. The "user" is twofold: Prism's HTTP query client
(the immediate machine consumer with a pinned contract) and the operator behind the
browser who finally sees a metric plotted. The journey is the READ half of the
already-shipped write loop.

## End-to-end flow

```
WRITE LOOP (already shipped)
  OTLP in --> aperture gateway --> StorageSink --> pulse FileBackedMetricStore (durable)

READ LOOP (this feature)
  [Operator opens Prism]
        |  config.json gives backend.url; App mounts QueryPanel
        v
  [Operator types a metric name, clicks Run]
        |  QueryPanel: queryRange({ q, range }, { backend.url, headers })
        v
  GET {backend}/query_range?query=<name>&start=<sec>&end=<sec>&step=15s
        |
        v
  [QUERY SERVICE  <-- this feature]
        |  1. resolve tenant (RED CARD 1; slice-01 = configured, fail-closed)
        |  2. parse minimal selector: bare metric name (+ optional {label="value"})
        |  3. pulse.query(&tenant, &MetricName, TimeRange[start..end in nanos])
        |  4. group (Metric, MetricPoint) rows into matrix series by label set
        |  5. render Prometheus matrix JSON
        v
  { status:"success", data:{ resultType:"matrix",
      result:[ { metric:{...labels}, values:[[ts_sec,"value"], ...] } ] } }
        |
        v
  [Prism client validates with isPromSuccess --> kind:'success']
        |  parseSeries maps values to [ts_ms, number] points
        v
  [ECharts renders the series; footer shows "N series, M points, K ms"]
        |
        v
  GOAL: operator sees a real metric, served from durable Pulse. Read loop CLOSED.
```

## Contract touchpoints (the pinned shape, quoted)

Request the client builds (`queryRange.ts` `buildUrl`):

```
GET {backend}/query_range?query={q}&start={epoch_seconds}&end={epoch_seconds}&step=15s
```

- `query`  : a RAW PromQL string (the operator's free text). Slice 01 accepts only
             a bare metric name; slice 02 adds a single `{label="value"}` matcher.
- `start`  : float epoch SECONDS (e.g. `1716200000`).
- `end`    : float epoch SECONDS.
- `step`   : literal `15s` (STEP_SECONDS = 15). Slice 01 does NOT resample to step.

Response the client's validator accepts (success path, `isPromSuccess`):

```json
{ "status": "success",
  "data": { "resultType": "matrix",
            "result": [ { "metric": { "__name__": "...", "service.name": "..." },
                          "values": [ [1716200000, "0.42"], [1716200015, "0.55"] ] } ] } }
```

- `status` MUST equal `"success"`.
- `data.result` MUST be an array (validator checks `Array.isArray(data.result)`).
- each `values` pair is `[number_seconds, string_value]`. The client multiplies the
  timestamp by 1000 (to ms) and `parseFloat`s the value string; `"NaN"` is honoured.
- empty `result: []` is a first-class CALM state ("No data for {range}"), NOT an error.

Error response the client's validator accepts (`isPromError`):

```json
{ "status": "error", "error": "<operator-readable reason>" }
```

- emitted with HTTP 400 for a query the service cannot parse (bad selector). The
  client renders the verbatim `error` text in a warning banner. The service MUST NOT
  leak header/secret values into this field (mirrors ADR-0027 §6 redaction posture).

## Emotional arc (Problem Relief)

| Phase | Who | Feeling | Design lever |
|-------|-----|---------|--------------|
| Before | Operator | Frustrated: data goes in, nothing comes out. Prism shows a disabled, unmounted panel | The whole feature exists to remove this |
| First query | Operator | Hopeful but wary: "will it actually return my data?" | A real series renders within the latency budget; footer confirms "N series, M points, K ms" |
| Empty result | Operator | Mildly anxious | Calm "No data for {range}. Check the metric name or widen the range." NOT an error banner |
| Bad selector | Operator | Briefly blocked | HTTP 400 + verbatim, jargon-free `error` text rendered above the chart; query input keeps focus |
| Success | Operator | Relieved, trusting | The read loop is closed; the platform is now honest end to end |

No jarring transitions: empty and error are distinct, calm, recoverable states the
Prism client already renders distinctly. The service's job is to feed those arms
truthfully.

## Failure modes (feed DISTILL error-scenario generation)

- Unknown metric name -> Pulse returns empty -> `{status:success, data:{result:[]}}` (empty arm, not error).
- Unparseable selector (operator, function, aggregation) -> HTTP 400 + `{status:error, error:"..."}`.
- Missing/invalid tenant resolution -> fail-closed; service refuses to serve (RED CARD 1).
- start > end, or non-numeric start/end -> HTTP 400 + `{status:error}` with a clear reason.
- Pulse persistence error (`MetricStoreError::PersistenceFailed`) -> HTTP 500; the
  Prism client maps any 5xx to `transport-error: http-status` with a calm banner.
