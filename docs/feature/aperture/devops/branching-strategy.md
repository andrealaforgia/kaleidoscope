# Branching Strategy — `aperture` v0 (DEVOPS)

> **Wave**: DEVOPS (`nw-platform-architect` / Apex).
> **Date**: 2026-05-04.
> **Author**: Apex.
> **Authoritative source**:
> [`docs/feature/otlp-conformance-harness-v0/devops/branching-strategy.md`](../../otlp-conformance-harness-v0/devops/branching-strategy.md).

---

## Strategy

**Aperture inherits the project-wide branching strategy: Trunk-Based
Development.** No per-feature branching strategy is needed.

The harness's DEVOPS wave codified the strategy as project-wide; this
document is a thin pointer that confirms Aperture honours it
unchanged.

---

## What Aperture inherits, verbatim

From the harness's `branching-strategy.md`:

- **Single long-lived branch**: `main`.
- **Short-lived feature branches optional, never required.** Direct
  commits to `main` permitted by humans and AI agents.
- **Conventional Commits** for commit messages. Allowed `<type>`
  values: `feat`, `fix`, `docs`, `chore`, `refactor`, `test`,
  `build`, `ci`, `perf`. Allowed `<scope>` values: unbounded; the
  convention so far is per-crate names (`aperture` for crate-level
  changes, `workspace` for repository-root changes, `otlp-harness`
  for harness-level changes).
- **Branch protection on `main`**: linear history, no force-push, no
  deletions. **No required status checks; admins not enforced.** Per
  the harness DEVOPS post-merge correction "branch protection
  relaxed to pure trunk-based" (2026-05-04, same day as the harness
  DEVOPS wave's close), the project explicitly chose "main is
  socially always green via fast feedback + fix-forward" over "main
  is mathematically always green via blocking gates".
- **No release workflow at v0**. The crate is `publish = false`. Tags
  are reserved for future v0.x or v1 releases when the binary ships
  to a distribution channel (crates.io, GitHub Releases, or a
  bundled Phase-1 distribution). The release workflow YAML will be
  added when the first release is cut.

The harness's document remains the load-bearing source; every change
to the project's branching strategy goes through that document, not
this one. This document does not duplicate its content; it confirms
the inheritance.

---

## Aperture-specific notes

The harness's branching strategy is generic to "any Kaleidoscope
crate". Aperture introduces no new branching concerns. In particular:

| Potential per-feature concern | Resolution for Aperture |
|---|---|
| Long-lived feature branch for the DELIVER cycle's eight slices | **Not used.** Each slice lands as a direct commit (or short-lived working branch merged via fast-forward) to `main`, with a Conventional Commits message naming the slice. The harness's eight DELIVER commits direct-to-`main` are the precedent. |
| Per-slice commit message scope | Use `feat(aperture-slice-NN)` or `feat(aperture)` per the team's preference; the harness convention favours the crate-level scope (`feat(otlp-harness)`) without the slice number. Apply the same here: `feat(aperture)` for slice landings, `fix(aperture)` for fix-forwards, `test(aperture)` for test-only commits, `docs(aperture)` for documentation. |
| Git tag for v0 milestone | None at v0. A future `aperture-v0.1` tag is a release-wave concern, not a DELIVER-wave concern. |
| `main` is red during the Aperture DELIVER cycle | **Disallowed.** Per `wave-decisions.md > A2`, Aperture's tests are RED at DISTILL by `unimplemented!()` panic. CI's Gate 1 stays scoped to `-p otlp-conformance-harness` during the DELIVER cycle so `main`'s CI remains green for the harness's gates. DELIVER's last commit performs the lockstep graduation (Gate 1 → workspace; Gate 5 → multi-pkg; pre-commit hook un-excludes aperture). |
| Hotfix branch policy | None. Trunk-based does not have hotfix branches; a fix is a direct commit to `main`. |
| Two-aperture-developers concurrency concern | n/a. Aperture v0 is built by a single AI agent (`nw-software-crafter`) under Andrea's direction. Conway's Law check at DESIGN passes trivially (architecture-overview.md > Conway's Law check). When Sieve lands and the team boundary becomes "Aperture maintainers + Sieve maintainers", the trait IS the seam (DESIGN ADR-0007). The branching strategy at that point remains trunk-based, with the trait providing the integration boundary. |

---

## Hand-off to the next feature's DEVOPS wave

Same hand-off shape as the harness's wave, with one addition: when
the third Kaleidoscope crate (after the harness and Aperture) reaches
its DEVOPS wave, its platform-architect should:

1. Read the harness's `branching-strategy.md`; do not re-derive the
   strategy.
2. Add the new crate to the workflow's per-`-p` invocations (Gate 1,
   Gate 2, Gate 3, Gate 5) when the crate's tests are green —
   following the same RED-scaffold-then-graduation pattern Aperture
   adopted (per Aperture's `wave-decisions.md > A2`, A4).
3. Confirm the existing branch-protection rules (linear-history-only)
   still apply.
4. Add a thin per-feature `branching-strategy.md` (like this one) to
   confirm the inheritance.

The strategy itself is unlikely to change between Phase-0 and
Phase-1 features. If it ever does (e.g. Sieve's Phase-1 introduction
of multiple maintainer teams demands a different posture), the change
goes through the project-wide DEVOPS-wave discipline, recorded in
the harness's authoritative document, and back-propagated to the
per-feature pointers.

---

## Summary

Aperture v0's branching strategy is **the project's branching
strategy**, unchanged. This document exists to make the inheritance
explicit so future audits can see Aperture honoured the strategy by
intent, not by accident.
</content>
</invoke>
