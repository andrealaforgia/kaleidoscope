# DISTILL upstream-issues — aperture-storage-sink-v0

## US-03 AC reconciliation: unsupported metric types are SKIPPED, not refused

**Flagged by**: DESIGN (back-propagation), recorded in ADR-0041 Decision 3 and
`design/wave-decisions.md` DD8.

**Before**: US-03's DISCUSS Domain Example 3, its UAT scenario "An unsupported
metric point type is refused", and its acceptance criterion all said a histogram
(unsupported) metric is **refused** with a `SinkError` naming the unsupported
type, and nothing is written to pulse.

**Conflict**: ADR-0041 Decision 3 (and DD8) decided the opposite, and explicitly
records that it *supersedes* the DISCUSS AC. pulse v0 persists only gauge + sum
number data points. Histogram / ExponentialHistogram / Summary are **skipped**
with one observable `event=metric_point_type_skipped` (warn) per skipped metric
naming the type and metric name. The record is NOT refused and NOT fatal: `accept`
returns `Ok`, supported gauge/sum points in the same request still persist, and a
request carrying only unsupported types translates to an empty `MetricBatch`
(accepted, nothing persisted, skip events emitted). This is the collector-faithful
behaviour: real exporters mix gauges/sums with histograms, and refusing the whole
batch would reject the supported points too.

**Reconciliation** (this DISTILL): updated US-03 in `discuss/user-stories.md` —
Domain Example 3, the UAT scenario (now "skipped, not refused"), and the relevant
acceptance criterion — to assert skip-with-observable-event, accept returns Ok,
supported points persist, and only-unsupported requests persist nothing. Citation:
**ADR-0041 Decision 3** / **DD8**.

**Value-less supported point** (a `NumberDataPoint` whose `value` oneof is unset):
neither the AC nor the arch mapping table (section 6.3) defines a row for it; the
arch only defines `as_double` / `as_int`. Consistent with skip-not-refuse and the
`data: None` "skip (no data)" row, the slice-03 acceptance test asserts the least
surprising contract — the individual value-less point is **skipped** (not refused,
not defaulted to 0), the rest of the request still persists. Flagged to DELIVER as
a contract choice to confirm; if DESIGN later pins a different rule, that test
updates with it.
