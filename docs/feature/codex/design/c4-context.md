# Codex v0 — C4 L1 (System Context)

```mermaid
flowchart LR
    SASHA[Sasha<br/>platform engineer]
    RILEY[Riley<br/>SRE]
    APP[Application code]

    subgraph K[Kaleidoscope]
        SP[Spark SDK]
        CO[Codex<br/>schema authority]
        AP[Aperture<br/>OTLP receiver]
    end

    SASHA -->|wires Spark| APP
    APP --> SP
    SP -->|validate Resource| CO
    CO -->|Ok or LintReport| SP
    SP -->|warn or Err| SASHA
    SP -->|OTLP| AP
    AP -->|recording sink| RILEY
```

Codex sits inside the Kaleidoscope deployment as a library Spark
consumes at init time. The principal user-visible surface is Sasha's
boot-time integration of Spark; Riley benefits indirectly because
typo'd attributes never reach her dashboards.

External integrations: zero. Codex v0 has no network surface and no
external services.
