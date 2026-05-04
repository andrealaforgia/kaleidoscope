# Local quality gates

These hooks run as much of the ADR-0005 CI contract as fits in a fast
local feedback loop, so "main is socially always green" stays cheap to
maintain under pure trunk-based development.

## Install once per clone

```sh
bash scripts/hooks/install.sh
```

Sets `core.hooksPath = scripts/hooks` for this repo only; personal
hooks under `.git/hooks` are untouched.

## What runs when

| Hook       | Gate | Command                             | Hot-cache time |
|------------|------|-------------------------------------|----------------|
| pre-commit | -    | `cargo fmt --check`                 | < 1 s          |
| pre-commit | -    | `cargo clippy --all-targets`        | 5–15 s         |
| pre-commit | 4    | `cargo deny check`                  | 1 s            |
| pre-commit | 1    | `cargo test --all-targets --locked` | 5–30 s         |
| pre-push   | 2    | `cargo public-api`                  | 5–15 s         |
| pre-push   | 3    | `cargo semver-checks`               | 5–15 s         |

`cargo mutants` (Gate 5) is CI-only. It can take minutes to hours and
would defeat the point of a local hook.

## Tooling

The hooks gracefully skip a gate (with a clear warning) when the
underlying tool or toolchain is missing locally. To install
everything once:

```sh
cargo install --locked cargo-deny
cargo install --locked cargo-public-api
cargo install --locked cargo-semver-checks
rustup toolchain install \
  "$(grep -E '^[[:space:]]*NIGHTLY_PIN:' .github/workflows/ci.yml | awk '{print $2}')"
```

## Skipping a hook

When you genuinely need to land a commit or push without running the
hooks (e.g. a docs-only fix on a temporarily red main):

```sh
git commit --no-verify
git push --no-verify
```

The escape hatch, not the default.
