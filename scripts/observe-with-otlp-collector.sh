#!/usr/bin/env bash
# Kaleidoscope sidecar: forward LumenToOtlpJsonWriter output to a
# real OTLP/HTTP collector.
#
# Usage:
#   ./scripts/observe-with-otlp-collector.sh <otlp-file> <collector-url>
#
# Example end-to-end (assuming a collector listening on
# http://localhost:14318/v1/metrics):
#
#   ./scripts/observe-with-otlp-collector.sh /tmp/otlp.log \
#     http://localhost:14318/v1/metrics &
#
#   echo '{"observed_time_unix_nano":100,...}' \
#     | ./target/release/kaleidoscope-cli ingest acme ./data \
#         --observe-otlp /tmp/otlp.log
#
# How it works:
#   LumenToOtlpJsonWriter emits one OTLP-JSON ResourceMetrics object
#   per line. The collector's /v1/metrics endpoint expects a
#   MetricsData envelope shaped as {"resourceMetrics": [...]}. This
#   script wraps each line in the envelope and POSTs it.
#
# Three dependencies: bash, tail, curl. No Rust toolchain, no
# Python, no OTLP SDK.
#
# v1 sidecar — synchronous, one POST per line, no batching, no
# retry. Loses an event on collector outage. v2 would add retry
# with backoff and optional local queueing. Keep this tiny on
# purpose: the operator's real sidecar will look like this plus
# whatever durability story their environment needs.

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <otlp-file> <collector-url>" >&2
    exit 2
fi

OTLP_FILE="$1"
COLLECTOR_URL="$2"

# Create the file if missing so `tail -F` does not race with the
# first writer.
touch "${OTLP_FILE}"

tail -F "${OTLP_FILE}" 2>/dev/null | while IFS= read -r line; do
    # Skip blank lines (defensive — the writer never produces
    # them today, but a future writer might).
    [[ -z "${line}" ]] && continue
    curl --silent --show-error --max-time 5 \
        -X POST \
        -H "Content-Type: application/json" \
        --data "{\"resourceMetrics\":[${line}]}" \
        "${COLLECTOR_URL}" > /dev/null
done
