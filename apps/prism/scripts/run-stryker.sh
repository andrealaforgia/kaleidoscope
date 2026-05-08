#!/usr/bin/env bash
# Kaleidoscope Prism — operator-facing observability SPA
# Copyright (C) 2026 The Kaleidoscope authors
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as
# published by the Free Software Foundation, either version 3 of the
# License, or (at your option) any later version.
#
# Gate 10 (Prism mutation testing, StrykerJS) baseline-cascade
# wrapper. Mirrors the Rust-side gate-5-mutants-* cargo-mutants
# --in-diff cascade: origin/main → HEAD~1 → full. Short-circuits to
# exit 0 when no Prism-touching changes exist vs the chosen baseline.
#
# CRITICAL-1 fix from Forge iter-1 review.

set -euo pipefail

BASELINE=""

# Tier 1 — origin/main (cheapest case, most common).
if git rev-parse --verify origin/main >/dev/null 2>&1; then
  if [ "$(git rev-parse origin/main)" = "$(git rev-parse HEAD)" ]; then
    echo "[skip] HEAD is origin/main; no diff to mutate."
    exit 0
  fi
  if git diff --quiet origin/main HEAD -- 'apps/prism/' ; then
    echo "[skip] no apps/prism/ changes vs origin/main; exit 0"
    exit 0
  fi
  BASELINE="origin/main"
  echo "[info] baseline: origin/main"
fi

# Tier 2 — HEAD~1 fallback (stale-fork PRs, recent rebases).
if [ -z "$BASELINE" ] && git rev-parse --verify HEAD~1 >/dev/null 2>&1; then
  if git diff --quiet HEAD~1 HEAD -- 'apps/prism/' ; then
    echo "[skip] no apps/prism/ changes vs HEAD~1; exit 0"
    exit 0
  fi
  BASELINE="HEAD~1"
  echo "[info] baseline: HEAD~1"
fi

# Tier 3 — full-suite run when no baseline is reachable.
if [ -z "$BASELINE" ]; then
  echo "[info] no baseline reachable; running full mutation suite"
  pnpm --filter prism stryker run
else
  pnpm --filter prism stryker run --incremental --since="$BASELINE"
fi
