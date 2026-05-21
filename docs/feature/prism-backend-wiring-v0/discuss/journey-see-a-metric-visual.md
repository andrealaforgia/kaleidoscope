# Journey: See a metric plotted in a browser (visual)

Feature: `prism-backend-wiring-v0`. Persona: Priya, an on-call SRE who has
just been paged. She knows ingest -> store -> query works (the read loop is
green in CI) but she has never actually seen a metric in a browser, because
Prism refuses to mount its QueryPanel: it cannot load a valid `/config.json`,
and even with one a browser-served Prism calling query-api on another origin
would be blocked by the same-origin policy.

## Emotional arc

Problem Relief: Frustrated (the dashboard is dark) -> Hopeful (the panel
mounts) -> Relieved (a real series renders from the durable Pulse store).

## ASCII flow

```
[Trigger]            [Step 1]              [Step 2]               [Goal]
Priya opens   -->    Prism loads      -->  Priya enters     -->   Series plotted
Prism in a           /config.json,         a metric name          from query-api
browser              QueryPanel mounts      and runs the query     over Pulse

Feels: dark,         Feels: hopeful        Feels: focused         Feels: relieved,
the loop is          ("Backend: Pulse"     (input has focus,      trust earned
invisible            not "(unconfigured)") "Run" enabled)         (real shape on screen)

Artifacts:           Artifacts:            Artifacts:             Artifacts:
config.json (none    config.json served    backend.url + /query_  Prometheus matrix
yet) -> panel        same-origin/CORS;     range; cross-origin    JSON -> ECharts
stays dark           backend.url valid     fetch reaches query-api series + table
```

## TUI / browser mockups

### Before this feature (today): the panel refuses to mount

```
+-- Prism -----------------------------------------------------------+
| Backend: (unconfigured)                                            |
|                                                                    |
|  [!] Configuration is missing.                                     |
|      Contact your Prism administrator.                             |
|      fetch-failed: HTTP 404 Not Found                              |
|                                                                    |
+--------------------------------------------------------------------+
```

The operator sees a coherent but dark page. No QueryPanel, no query input,
no chart. This is the wire-then-probe-then-use posture: Prism refuses to
pretend it can talk to a backend it has no honest URL for.

### Step 1: config loads, QueryPanel mounts

```
+-- Prism · Pulse (durable) ----------------------------------------+
| Backend: Pulse (durable)        Prism v0.1.0    Auto-refresh: idle |
|                                                                    |
| PromQL query                                                       |
| [ last 15 minutes v ] [ off v ]  [ up________________ ]  [ Run ]   |
|                                                                    |
|  (empty chart area — awaiting first query)                         |
+--------------------------------------------------------------------+
```

The backend label flips from "(unconfigured)" to the configured label.
The query input takes focus. The operator can type.

### Step 2 -> Goal: a real series renders from Pulse

```
+-- Prism · Pulse (durable) ----------------------------------------+
| Backend: Pulse (durable)        Prism v0.1.0    Auto-refresh: idle |
|                                                                    |
| PromQL query                                                       |
| [ last 15 minutes v ] [ off v ]  [ up________________ ]  [ Run ]   |
|                                                                    |
|   value                                                            |
|   1.0 |  *----*----*----*----*----*----*----*                      |
|       |                                                            |
|   0.0 +--------------------------------------------- time          |
|                                                                    |
|   Series                          Points   Latest value           |
|   __name__="up", job="self"          61        1                   |
|                                                                    |
|   1 series • 61 points • 7 ms                                      |
+--------------------------------------------------------------------+
```

The series comes from query-api reading the durable Pulse store. The footer
confirms the round-trip succeeded (point count + query latency).

### Failure arm: cross-origin reach blocked (the risk this feature removes)

```
+-- Prism · Pulse (durable) ----------------------------------------+
| Backend: Pulse (durable)        Prism v0.1.0    Auto-refresh: idle |
|                                                                    |
|  [!] Cannot reach Pulse (durable).                                 |
|      Transport failure: network                                    |
|                                                                    |
+--------------------------------------------------------------------+
```

If config.json is valid but the browser's cross-origin fetch is refused
(no CORS, different origin) the QueryPanel mounts but every query renders
as a transport-error. The panel stays usable; the operator is told the
backend is unreachable rather than seeing a blank screen. Removing this
arm IS the feature: either CORS is configured, or the two are served
same-origin.

## The central design fork (for DESIGN, NOT decided here)

The requirement is solution-neutral: **a browser-served Prism must reach a
running query-api and render a real series**. Two honest topologies achieve
it; the choice belongs to the DESIGN wave (see `wave-decisions.md`).

```
Option 1: CROSS-ORIGIN + CORS              Option 2: SAME-ORIGIN
+-----------+      +-----------+           +----------------------------+
| Prism     |      | query-api |           | one server                 |
| origin A  |----->| origin B  |           |  /          -> Prism bundle |
| (static)  | CORS | + CORS    |           |  /api/v1/*  -> query routes |
+-----------+      +-----------+           +----------------------------+
 decoupled, needs CORS config               coupled deploy, no CORS
```
