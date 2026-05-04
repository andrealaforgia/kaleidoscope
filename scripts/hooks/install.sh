#!/usr/bin/env bash
# One-time per-clone hook installation for Kaleidoscope.
#
# Sets git's per-repo `core.hooksPath` to point at the version-
# controlled hooks under scripts/hooks/. The hooks then ride with
# every clone and never collide with personal hooks under
# .git/hooks/.

set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

git config core.hooksPath scripts/hooks
chmod +x scripts/hooks/pre-commit scripts/hooks/pre-push

echo "[ok] core.hooksPath set to scripts/hooks"
echo
echo "  pre-commit:  cargo fmt + clippy + deny + test"
echo "  pre-push:    cargo public-api + semver-checks (nightly-bound)"
echo "  Gate 5 (cargo mutants) remains CI-only."
echo
echo "Skip a hook for a single op with:"
echo "  git commit --no-verify"
echo "  git push --no-verify"
