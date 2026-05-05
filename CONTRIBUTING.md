# Contributing to Kaleidoscope

Kaleidoscope is currently a single-author project. External contributions, including pull requests, are not yet accepted.

The repository is public so the design can be observed and read. Star or watch the repository to be notified when contribution opens.

## When contribution opens

The model will be simple, structural, and built to make re-licensing impossible.

### Developer Certificate of Origin (DCO), no Contributor Licence Agreement (CLA)

Every commit is signed off:

```
Signed-off-by: Name <email>
```

This asserts that the contributor has the right to submit the work under the project's licence. The full DCO text is at <https://developercertificate.org/>.

There is no Contributor Licence Agreement. There will not be one. The DCO sign-off is sufficient.

### Why no CLA

A CLA assigns or grants re-licensing rights to a single corporate entity. With a CLA, a future maintainer can unilaterally re-license the project; that is the mechanism Elastic, MongoDB, Redis, and HashiCorp used to abandon open source.

With DCO and no CLA, no single entity owns enough of the copyright to relicense. The structural protection is in the contribution model itself, not in the licence text.

### Per-component licensing

Contributions inherit the licence of the file they touch.

| Component class | Licence |
|---|---|
| Platform components (aperture, future sieve / sluice / engines / query / alerting / etc.) | [AGPL-3.0-or-later](LICENSE-AGPL-3.0) |
| SDKs and protocol libraries (otlp-conformance-harness, future spark, protocol crates, on-disk spec) | [Apache-2.0](LICENSE-APACHE-2.0) |

The full rationale and per-crate table are in [`LICENSING.md`](LICENSING.md).

### Sign-off mechanics

Use `git commit --signoff` (or the shorter `-s`) to add the trailer automatically:

```
git commit -s -m "your commit message"
```

Configure git to do it by default:

```
git config --global format.signoff true
```

The pre-push hook will eventually verify the trailer is present once contributions open. Until then, the convention is recorded so that the first external contributor's first commit signs off correctly.

### Trademark

The name **Kaleidoscope** and the logo are reserved trademarks. The code is free; the name and logo are not. Forks may continue under any compatible licence but may not call themselves Kaleidoscope.

— Andrea Laforgia, sole maintainer
