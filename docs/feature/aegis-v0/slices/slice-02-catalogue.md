# Slice 02 — Tenant catalogue loader (US-AE-02)

## Goal

`aegis::load_catalogue(path) -> Result<TenantCatalogue, ...>` reads
a TOML file declaring active tenants, returns a typed catalogue
with O(1) `contains` lookup.

## IN scope

- TOML schema: `[[tenants]]` with `id`, optional `display_name`,
  optional `notes`
- `deny_unknown_fields` per Beacon's loader pattern
- Duplicate-id rejection at load time
- O(1) lookup via `HashSet<TenantId>`
- 1000-tenant load latency test (KPI 2)

## OUT scope

- FoundationDB adapter (v1)
- Tenant lifecycle (creation / deletion API) — v0 is read-only

## Learning hypothesis

Disproves "TOML loader pattern from Beacon scales to a different
schema (tenants) without library duplication". Low risk; the
pattern is established.
