# Journey: Filter a metric to the series that matter (label matchers)

British English. No em dashes. Backend feature; the operator-observable surface is the
`GET /api/v1/query_range` request/response and Prism's rendering of the matrix.

## Persona

Sara Okafor, on-call SRE for the "checkout" service, tenant "acme-prod". Mid-incident:
`http_requests_total` plots a dozen overlapping lines across every service. She knows the
metric; she needs only the "checkout" series. She types a Prometheus label matcher because
that is the muscle memory she brings from Prometheus.

## Emotional arc (Problem Relief)

Frustrated (too many lines, cannot see checkout) -> Hopeful (types the matcher she knows)
-> Relieved (only checkout's series remain, server-side, fast).

## Flow

```
[Frustrated]            [Hopeful]                  [Relieved]
too many series         types matcher she knows    only matching series
       |                       |                          |
       v                       v                          v
+-----------------+   +----------------------+   +--------------------------+
| Step 1          |   | Step 2               |   | Step 3                   |
| Sees noisy plot |-->| Sends labelled query |-->| Sees filtered matrix     |
| (all services)  |   | name{service="..."}  |   | (checkout only)          |
+-----------------+   +----------------------+   +--------------------------+
                              |  (malformed)              ^
                              v                           | (no series match)
                      +----------------------+   +--------------------------+
                      | Step 2e: honest 400  |   | Step 3e: calm empty arm  |
                      | unsupported/malformed|   | result: []               |
                      +----------------------+   +--------------------------+
```

## Step 1: The noisy starting point (context, not new behaviour)

Sara queries the bare metric. Today (slice 01 shipped) this already works and returns
every series under the name.

```
GET /api/v1/query_range?query=http_requests_total&start=...&end=...&step=15s

200 { "status":"success", "data":{ "resultType":"matrix", "result":[
  { "metric":{"__name__":"http_requests_total","service.name":"checkout"}, "values":[...] },
  { "metric":{"__name__":"http_requests_total","service.name":"cart"},     "values":[...] },
  { "metric":{"__name__":"http_requests_total","service.name":"search"},   "values":[...] },
  ... a dozen more ...
] } }
```

Prism plots a dozen overlapping lines. Sara cannot read checkout.

Emotion: frustrated. She wants ONE service.

## Step 2: The labelled query (the new behaviour)

Sara types the matcher she knows from Prometheus. Prism forwards `${query}` verbatim
(confirmed in `queryRange.ts buildUrl`).

```
GET /api/v1/query_range?query=http_requests_total{service.name="checkout"}&start=...&end=...&step=15s
                              \_______________________________________/
                               ${query} forwarded raw by Prism, URL-encoded

backend parse: name = "http_requests_total"
               matchers = [ service.name = "checkout" ]   (= equality)
```

The metric name still selects the metric via `pulse.query(&tenant, &MetricName, range)`.
The matcher filters the derived label set of each returned row.

Emotion: hopeful. The query was accepted; now does it narrow?

### Step 2e: malformed or unsupported matcher (honest 400)

A matcher the slice cannot honour (regex `=~`, unterminated brace, missing quotes,
empty label name) is rejected, never half-answered.

```
GET ...query=http_requests_total{service.name=~"check.*"}...

400 { "status":"error",
      "error":"unsupported query: regex matchers (=~, !~) are not supported at v0; use = or !=" }
```

The reason names what is unsupported and what is accepted. It NEVER echoes the raw query
(DD6 redaction symmetry; pinned by an existing test in `selector.rs`).

## Step 3: The filtered matrix (relief)

```
200 { "status":"success", "data":{ "resultType":"matrix", "result":[
  { "metric":{"__name__":"http_requests_total","service.name":"checkout"}, "values":[...] }
] } }
```

One series. Prism plots a single clean line. Server-side filtering: the dozen other
services never crossed the wire.

Emotion: relieved. She can read checkout's trend and decide.

### Step 3e: no series matches (calm empty arm)

A matcher that matches nothing (typo in the value, or a `!=` that excludes everything)
returns the same calm empty arm the operator already trusts from slice 01.

```
GET ...query=http_requests_total{service.name="chekout"}...   (typo)

200 { "status":"success", "data":{ "resultType":"matrix", "result":[] } }
```

Prism renders "No data for {range}." Sara reads it as "my matcher excluded everything",
not "the backend broke", and corrects the value.

## Matcher semantics, shown by example (the correctness-critical part)

Series under `http_requests_total` for tenant "acme-prod":

| series | labels (derived) |
|--------|------------------|
| A | `{__name__, service.name="checkout", code="200"}` |
| B | `{__name__, service.name="checkout"}` (no `code`) |
| C | `{__name__, service.name="cart", code="500"}` |

| matcher | keeps | why |
|---------|-------|-----|
| `service.name="checkout"` | A, B | present and equal |
| `code="200"` | A | present and equal; B excluded (absent), C excluded (different) |
| `code=""` | B | empty-string value matches the ABSENT label |
| `code!="500"` | A, B | A differs, B absent both match; C present-and-equal is excluded |
| `code!=""` | A, C | present-and-non-empty; B absent is excluded |
| `service.name="checkout", code!="500"` | A, B | ANDed: both matchers must hold |

These rows are the executable heart of the feature.

## CLI/contract UX notes

- This is a backend feature; the request is machine-issued by Prism. The "command" is the
  HTTP query string.
- The response shape is the PINNED Prism contract (`isPromSuccess` / `isPromError`). The
  matcher feature does not change the response envelope; it changes WHICH series the
  success arm contains.
- Error arm answers what/why/what-next ("regex not supported; use = or !=").
- Redaction: no status:error text echoes the raw query or a forwarded header value.
