# syntax=docker/dockerfile:1.7
#
# Kaleidoscope CLI runtime image.
#
# Multi-stage build:
#
#   1. `builder`: official rust:1.88-slim-bookworm. Compiles
#      `kaleidoscope-cli` in release mode against the workspace
#      MSRV (pinned by `rust-toolchain.toml`).
#   2. `runtime`: debian:bookworm-slim. Carries only the
#      compiled binary. No build toolchain, no cargo cache, no
#      git, no source. The image is small enough to pull into
#      ephemeral environments quickly.
#
# Usage:
#
#   docker build -t kaleidoscope-cli .
#   echo '{"observed_time_unix_nano":100,"severity_number":9,...}' \
#     | docker run --rm -i -v "$(pwd)/data:/data" kaleidoscope-cli \
#         ingest acme /data
#   docker run --rm -v "$(pwd)/data:/data" kaleidoscope-cli read acme /data
#
# v1 of the image is intentionally simple. Cargo-chef-style
# dependency caching, fsync-strict storage modes, and an OTLP
# exporter all land at v2.

# -----------------------------------------------------------------
# Stage 1 — Build
# -----------------------------------------------------------------
FROM rust:1.88-slim-bookworm AS builder

WORKDIR /workspace

# Copy only the files the kaleidoscope-cli build needs. The
# .dockerignore keeps target/, node_modules/, .git/, etc. out
# of the context.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY xtask ./xtask

# Build release. `--locked` enforces the Cargo.lock the
# workspace shipped with, matching what the local + CI
# toolchains see.
RUN cargo build --release -p kaleidoscope-cli --locked

# -----------------------------------------------------------------
# Stage 2 — Runtime
# -----------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# kaleidoscope-cli has no runtime deps beyond libc and the
# Debian base. v1 keeps it that way (no openssl, no
# ca-certificates) because the CLI doesn't dial out.

COPY --from=builder /workspace/target/release/kaleidoscope-cli \
    /usr/local/bin/kaleidoscope-cli

# Mount this as a volume for data persistence across runs.
WORKDIR /data
VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/kaleidoscope-cli"]
CMD ["--help"]
