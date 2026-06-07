#!/usr/bin/env bash
# ci-watch.sh — the courtesy watcher for Kaleidoscope's deep CI gates.
#
# WHAT: a thin `gh` wrapper that reports the latest `main` CI run at a
# glance — its conclusion, URL, short SHA, and workflow name — and, on a
# red, drills in with `gh run view --log-failed` and CLASSIFIES the failed
# jobs, explicitly calling out `gate-1-test` (the deep `cargo test
# --workspace --all-targets --locked` suite) and any `gate-5-mutants*`
# (mutation) reds.
#
# WHY: ADR-0072 slimmed the local pre-commit hook's test step to the fast
# `cargo test --workspace --lib --locked` unit subset; the deep suite now
# gates ONLY in CI. This script is the safety net that keeps eyes on the
# deep coverage that no longer blocks the local commit path. gate-1-test
# and gate-5-mutants* are precisely the two gate families the slim hook no
# longer pre-runs, so they are name-checked on a failure.
#
# EARNED TRUST — never a false green: the script probes its substrate
# FIRST (gh present -> authenticated -> GitHub reachable). If any probe
# fails it prints a clear remediation message and exits NON-ZERO. It never
# reports green on an un-probed or unreachable substrate.
#
# USAGE:
#   scripts/ci-watch.sh        # latest 5 main runs considered
#   scripts/ci-watch.sh 10     # latest 10
#
# EXIT SEMANTICS (poll-loop scriptable):
#   0  latest main run succeeded
#   0  latest main run in progress / pending  (it has not failed)
#   1  latest main run failed                 (after the drill-down)
#   1  substrate/probe failure (gh missing / unauthed / unreachable / unknown)
#
# CADENCE: run after every push to main and periodically while working a
# multi-slice task; on a red, fix-forward. See CLAUDE.md `## CI watch`.
#
# NOT auto-run: invoked by hand or by an agent on the cadence — never wired
# into a git hook (that would re-add the latency the feature just removed).

set -euo pipefail

LIMIT="${1:-5}"

red()    { printf '\033[31m%s\033[0m\n' "$*"; }
green()  { printf '\033[32m%s\033[0m\n' "$*"; }
yellow() { printf '\033[33m%s\033[0m\n' "$*"; }
bold()   { printf '\033[1m%s\033[0m\n' "$*"; }

# --- Substrate probe 1: gh present -------------------------------------
if ! command -v gh >/dev/null 2>&1; then
  red "ci-watch: gh CLI not found — install: brew install gh"
  exit 1
fi

# --- Substrate probe 2: gh authenticated -------------------------------
if ! gh auth status >/dev/null 2>&1; then
  red "ci-watch: gh not authenticated — run: gh auth login"
  exit 1
fi

# --- Substrate probe 3 + fetch: GitHub reachable -----------------------
# A failed `gh run list` here means network/API trouble; status is then
# UNKNOWN and we exit non-zero rather than imply green.
runs_json="$(
  gh run list --branch main --limit "$LIMIT" \
    --json status,conclusion,name,databaseId,url,headSha,workflowName \
    2>/dev/null
)" || {
  red "ci-watch: could not reach GitHub (network/API) — status unknown"
  exit 1
}

if [ -z "$runs_json" ] || [ "$runs_json" = "[]" ]; then
  yellow "ci-watch: no runs found on main (limit $LIMIT) — status unknown"
  exit 1
fi

# --- Substrate probe 4: jq present (we parse the captured JSON) ---------
if ! command -v jq >/dev/null 2>&1; then
  red "ci-watch: jq not found — install: brew install jq"
  exit 1
fi

# --- Summarise the latest run ------------------------------------------
# The first element (index 0) is the most recent run. We parse the JSON
# we already captured with `jq`.
read_field() {
  # $1 = jq path expression for the first run
  printf '%s' "$runs_json" | jq -r ".[0].$1 // \"\""
}

status="$(read_field status)"
conclusion="$(read_field conclusion)"
url="$(read_field url)"
head_sha="$(read_field headSha)"
workflow="$(read_field workflowName)"
run_id="$(read_field databaseId)"
short_sha="${head_sha:0:8}"

bold "[ci-watch] latest main run"
echo "  workflow:   ${workflow:-<unknown>}"
echo "  sha:        ${short_sha:-<unknown>}"
echo "  url:        ${url:-<unknown>}"

# Normalise: an empty conclusion on an in-progress/queued run is pending.
if [ "$status" != "completed" ] && [ -z "$conclusion" ]; then
  yellow "  status:     in progress (${status:-queued}) — not a red"
  yellow "ci-watch: run still in progress; re-check shortly."
  exit 0
fi

case "$conclusion" in
  success)
    green "  conclusion: success"
    green "ci-watch: main is green."
    exit 0
    ;;
  failure | timed_out | cancelled | action_required | startup_failure)
    red "  conclusion: $conclusion"
    echo
    bold "[ci-watch] failed jobs (classified):"

    # Classify failed jobs by NAME, not a hardcoded list (25 gate-5 jobs
    # exist and the set drifts as crates are added). Filter to the jobs
    # that actually failed.
    failed_jobs="$(
      gh run view "$run_id" --json jobs \
        --jq '.jobs[] | select(.conclusion=="failure") | .name' \
        2>/dev/null || true
    )"

    if [ -z "$failed_jobs" ]; then
      yellow "  (could not enumerate failed jobs via gh run view --json jobs)"
    else
      saw_gate1=0
      saw_gate5=0
      while IFS= read -r job; do
        [ -z "$job" ] && continue
        case "$job" in
          gate-1-test)
            red   "  ✗ $job   <-- DEEP TESTS (cargo test --all-targets); local --lib does NOT pre-run this"
            saw_gate1=1
            ;;
          gate-5-mutants*)
            red   "  ✗ $job   <-- MUTATION (Gate 5); local hook does NOT pre-run this"
            saw_gate5=1
            ;;
          *)
            red   "  ✗ $job"
            ;;
        esac
      done <<< "$failed_jobs"

      [ "$saw_gate1" -eq 1 ] && yellow "  note: a gate-1-test red is a deep-suite regression the slim local hook cannot catch — fix-forward."
      [ "$saw_gate5" -eq 1 ] && yellow "  note: a gate-5-mutants* red is a mutation-survivor the local hook never runs — fix-forward."
    fi

    echo
    bold "[ci-watch] failing log tail (gh run view --log-failed):"
    gh run view "$run_id" --log-failed 2>/dev/null | tail -n 40 || \
      yellow "  (could not fetch --log-failed)"

    red "ci-watch: main is RED ($conclusion) — see $url"
    exit 1
    ;;
  "" )
    yellow "  conclusion: (none) — status=${status:-unknown}; treating as in progress"
    exit 0
    ;;
  *)
    yellow "  conclusion: $conclusion (unrecognised) — treating as unknown"
    red "ci-watch: unrecognised conclusion '$conclusion' — status unknown"
    exit 1
    ;;
esac
