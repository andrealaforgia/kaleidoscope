# Slice 03 — `loom apply` (US-LO-03)

## Goal

Make the destination directory match the source directory using
atomic file operations and idempotent semantics.

## IN scope

- `apply --from <src> --to <dst>` subcommand
- Validate first (refuse to apply on validation failure)
- Atomic writes: write each `.toml` to `.tmp`, fsync, rename per POSIX
- Remove `.toml` files present in `<dst>` but not in `<src>`
- Touch nothing else (binaries, scripts, hidden files preserved)
- Idempotency: second run on the same input writes zero files
- Acceptance test against fixture dirs + the KPI 3 property test

## OUT scope

- JSON output (slice 04)
- Remote API (v1)

## Learning hypothesis

Disproves "atomic write + idempotency hold under realistic
catalogue churn". Risk: the rename-after-fsync sequence may
surprise on some filesystems (NTFS, FAT). Mitigation: rely on
`std::fs::rename` cross-platform behaviour; document POSIX
filesystems as the supported target.
