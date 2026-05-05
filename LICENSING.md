# Licensing

Kaleidoscope is split into two licence classes by component role.

## Platform components — AGPL-3.0-or-later

The platform components run server-side. Anyone is free to use, modify, and redistribute them; anyone offering them as a network service to others must publish their modifications under the same licence. The licence text is in [`LICENSE-AGPL-3.0`](LICENSE-AGPL-3.0).

This class includes (current and planned):

- `aperture` — OTLP gateway
- `sieve` — routing, deduplication, normalisation
- `sluice` — durable buffer
- `pulse`, `lumen`, `ray`, `strata`, `cinder` — storage engines
- `prism` — query layer
- `beacon` — alerting engine
- `augur` — anomaly detector
- `aegis` — identity and tenancy
- `loom` — dashboards-as-code
- `codex` — schema registry (server-side parts)

## SDKs and protocol libraries — Apache-2.0

Code intended to be embedded in third-party applications is licensed under Apache-2.0 so it can be linked into proprietary code without copyleft contamination. Apache-2.0 also gives an explicit patent grant. The licence text is in [`LICENSE-APACHE-2.0`](LICENSE-APACHE-2.0).

This class includes (current and planned):

- `otlp-conformance-harness` — OTLP wire-format validator
- `spark` — SDK and OTel-compatible client library
- Protocol message crates and generated code packages
- Schema registry client packages
- The on-disk format specification crates

## Why this split

The platform components are the value Kaleidoscope offers as infrastructure. AGPL-3.0 closes the SaaS loophole that drove Elastic, MongoDB, Redis, and HashiCorp to abandon open source. Anyone is free to run Kaleidoscope; anyone hosting it as a service to others must publish the modifications under the same licence. That is the strongest "always free and open source" guarantee available within the OSI-approved perimeter.

The SDK class needs Apache-2.0 because it must be embeddable in third-party application code without compromising that code's licence. AGPL on an SDK kills adoption and defeats the point.

This split is the same arrangement Grafana Labs used before they moved to AGPL across the board, and that MongoDB used before they moved to SSPL. It is the most battle-tested arrangement for keeping infrastructure software free against vendor pressure.

## Trademark

The name **Kaleidoscope** and the logo are reserved trademarks of the project. The code is free; the name and the logo are not. This prevents bad-faith forks claiming to be the original.

## Contributions

Contributions are accepted under the Developer Certificate of Origin. There is no Contributor Licence Agreement and there will be no Contributor Licence Agreement. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for details.

The no-CLA policy is structural protection: with many contributors and no concentrated copyright assignment, no single maintainer or future entity can unilaterally re-license, because nobody will own enough of the copyright to legally do it.

## Per-crate licence

The `license` field of each crate's `Cargo.toml` declares the SPDX identifier for that crate. Tooling (cargo, crates.io, dependency analysers) reads it from there.

| Crate                       | Licence              |
|-----------------------------|----------------------|
| `aperture`                  | AGPL-3.0-or-later    |
| `otlp-conformance-harness`  | Apache-2.0           |

Future crates added to the workspace must declare an explicit `license` matching their role per the classes above.

## History

Earlier in the project's life Kaleidoscope was dedicated to the public domain under CC0-1.0. That dedication is preserved in the git history: any commit on or before tag `aperture/v0.1.0` is CC0-1.0. The migration to AGPL-3.0-or-later + Apache-2.0 took place on 2026-05-05 and applies from the migration commit forward. Existing tags are not retroactively re-licensed.
