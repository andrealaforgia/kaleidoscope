# Slice 03 — Audit log via `tracing` (US-AE-03)

## Goal

Every `validate` call emits exactly one structured `tracing`
event with stable field names (`tenant_id`, `role`, `decision`,
`subject`, `reason`).

## IN scope

- `tracing::info!` on allow path with `decision = "allow"`
- `tracing::warn!` on deny path with `decision = "deny"`,
  `reason = "<typed_error_kind>"`
- Optional `subject: &str` parameter on `validate` for action
  attribution
- KPI 3 acceptance test: install test subscriber, assert
  100% event coverage on a 100-call mix of allow + deny

## OUT scope

- Audit event sink configuration (operator-owned)
- Lumen integration (Lumen doesn't exist yet)

## Learning hypothesis

Disproves "the `tracing` ecosystem's `tracing-subscriber` test
layer captures structured events reliably enough to underpin a
KPI 3 assertion". The `tracing-test` crate exists; if not, we
write a minimal layer ourselves.
