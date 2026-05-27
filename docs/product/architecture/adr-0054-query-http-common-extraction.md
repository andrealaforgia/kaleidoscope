# ADR-0054: query-http-common extraction

- Status: Accepted
- Date: 2026-05-27
- Supersedes: none
- Superseded by: none
- Related: ADR-0048 Decision 6 (read-API seam noted as deferred; shipped here, unchanged); ADR-0050 (cap origin, unchanged); ADR-0052 (severity filter sibling style, unchanged); ADR-0053 (rule of three pin and pressure annotation, unchanged); docs/feature/query-http-common-v0/

## Context

Three read-side crates duplicate the same scaffolding. The duplication is now a documented pressure: ADR-0052 noted it after the second copy and ADR-0053 named the rule of three when the third arm landed. Four families of code are repeated:

- the cap constants `MAX_WINDOW_SECONDS` and `MAX_RESULT_ROWS`
- the helper `parse_time_range` (in two of three crates)
- the JSON error body shape and the literal reason texts
- the tenant resolution match block (in four handler arms)

Mutation testing has a structural problem with the duplication: a mutant that changes `MAX_RESULT_ROWS` in one crate is killed by one suite, but the same mutation in another crate is killed by a different suite, and the third by a third suite. The signal that the cap is a real constant is split. A change to the reason text needs three edits and three review eyes. A fourth read endpoint would be a fourth copy.

## Decision

Extract a new workspace member crate `query-http-common` and rewire the three consumer crates against it through a Mikado-ordered refactor that keeps `cargo test --workspace` green at every step.

Public API surface (minimum needed; nothing else):

- `pub const MAX_WINDOW_SECONDS: u64 = 86_400`
- `pub const MAX_RESULT_ROWS: usize = 100_000`
- `pub fn parse_time_range(start: Option<&str>, end: Option<&str>) -> Result<TimeRange, &'static str>` (signature copied verbatim from query-api)
- `pub fn resolve_tenant_or_refuse(tenant: &Option<TenantId>, service_label: &'static str) -> Result<&TenantId, Response>`
- `pub fn error_response(status: StatusCode, reason: &'static str) -> Response`
- `pub struct ErrorBody { error: &'static str }` with `#[derive(Serialize)]`
- Pub const reason text literals: `REASON_INVALID_TIME_RANGE`, `REASON_WINDOW_TOO_LARGE`, `REASON_TOO_MANY_ROWS`, `REASON_MISSING_TENANT`, and any others discovered during the grep pass

Rewire order is pinned in `docs/feature/query-http-common-v0/design/mikado-plan.md` (steps A through H). The tag `query-http-common/v0.1.0` lands at DELIVER close, not at DESIGN close.

## Consequences

Positive. There is a single source of truth for the cap constants, the reason texts, the parse helper, and the tenant resolution. Mutation kill rate becomes meaningful: a single mutant on `MAX_RESULT_ROWS` is killed by tests in all four arms through one site. The dependency direction is clean (the common crate depends on `aegis` and `axum`, the read APIs depend on it, none of the read APIs depend on each other). A fourth read endpoint adds one workspace dependency declaration, not ninety lines of copy-paste.

Negative. The workspace has one more crate node, and `cargo build` builds one more compilation unit. The cost is compile-time only; Rust monomorphises the constants to literals at call sites and the helpers are small enough to inline.

## Alternatives considered

A. `macro_rules!` macro that expands the scaffold inside each consumer crate. Rejected: this is duplicated code at the expansion site, which keeps the mutation-testing problem and makes diff review harder.

B. Sub-module inside one of the consumer crates (for example trace-query-api), and the other two read APIs depend on that crate. Rejected: this couples the library to one of its consumers and creates an arbitrary parent.

C. Cargo workspace feature gating. Rejected: over-engineering for a refactor whose purpose is to remove indirection, not add it.

D. Leave the seam deferred indefinitely as ADR-0048 Decision 6 allowed. Rejected: ADR-0053 already documented that the rule of three had arrived. Deferring further would erode the discipline that pinned the rule in the first place.

## References

- ADR-0048 Decision 6 (read-API seam, deferred at the time, shipped by this ADR; ADR-0048 unchanged)
- ADR-0050 (read-side caps, unchanged)
- ADR-0052 (severity filter, unchanged)
- ADR-0053 (trace lookup by id, rule-of-three pin, unchanged)
- `docs/feature/query-http-common-v0/discuss/`
- `docs/feature/query-http-common-v0/design/wave-decisions.md`
- `docs/feature/query-http-common-v0/design/application-architecture.md`
- `docs/feature/query-http-common-v0/design/mikado-plan.md`
