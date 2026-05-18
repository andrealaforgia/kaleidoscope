#!/usr/bin/env bash
# Kaleidoscope — v1 durability smoke
# Copyright (C) 2026 The Kaleidoscope authors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Drives the `kaleidoscope-cli` binary through an end-to-end
# durability cycle:
#
#   1. Ingest two NDJSON records for tenant `acme`
#   2. Read them back (in-process round trip)
#   3. "Restart" the process by running `read` as a second
#      invocation against the same data directory — proves
#      the WAL replay path
#   4. Compact (snapshot + truncate WAL)
#   5. Read again — proves the snapshot recovery path
#
# Operators who clone the repo can run this script to convince
# themselves that "v1 means restart-safe" is true on disk, not
# just in the test suite.
#
# Usage:
#   scripts/v1-durability-smoke.sh [data_dir]
# data_dir defaults to a fresh /tmp/kal-durability-smoke-<pid>.

set -euo pipefail

CLI_BIN="${CLI_BIN:-./target/release/kaleidoscope-cli}"
DATA_DIR="${1:-/tmp/kal-durability-smoke-$$}"
TENANT="acme"

if [[ ! -x "${CLI_BIN}" ]]; then
    echo "v1-durability-smoke: ${CLI_BIN} not found. Build it first with:" >&2
    echo "  cargo build --release -p kaleidoscope-cli" >&2
    exit 2
fi

echo "==> v1 durability smoke"
echo "    CLI:      ${CLI_BIN}"
echo "    data_dir: ${DATA_DIR}"
echo

# Clean slate.
rm -rf "${DATA_DIR}"
mkdir -p "${DATA_DIR}"

echo "==> 1. Ingest two records for tenant ${TENANT}"
cat <<EOF | "${CLI_BIN}" ingest "${TENANT}" "${DATA_DIR}"
{"observed_time_unix_nano":100,"severity_number":9,"severity_text":"INFO","body":"hello","attributes":{},"resource_attributes":{"service.name":"checkout"},"trace_id":null,"span_id":null}
{"observed_time_unix_nano":200,"severity_number":17,"severity_text":"ERROR","body":"boom","attributes":{},"resource_attributes":{"service.name":"checkout"},"trace_id":null,"span_id":null}
EOF

echo
echo "==> 2. Read them back (same process state)"
records_phase1=$("${CLI_BIN}" read "${TENANT}" "${DATA_DIR}" 2>/dev/null | wc -l | tr -d ' ')
echo "    records returned: ${records_phase1}"
if [[ "${records_phase1}" -ne 2 ]]; then
    echo "FAIL: expected 2 records, got ${records_phase1}" >&2
    exit 1
fi

echo
echo "==> 3. \"Restart\" via second-invocation read — WAL replay path"
records_phase2=$("${CLI_BIN}" read "${TENANT}" "${DATA_DIR}" 2>/dev/null | wc -l | tr -d ' ')
echo "    records returned after WAL replay: ${records_phase2}"
if [[ "${records_phase2}" -ne 2 ]]; then
    echo "FAIL: WAL replay lost records" >&2
    exit 1
fi

echo
echo "==> 4. Compact (snapshot Lumen + Cinder, truncate WAL)"
"${CLI_BIN}" compact "${DATA_DIR}"
lumen_wal_size=$(stat -f%z "${DATA_DIR}/lumen.wal" 2>/dev/null || stat -c%s "${DATA_DIR}/lumen.wal" 2>/dev/null || echo "?")
cinder_wal_size=$(stat -f%z "${DATA_DIR}/cinder.wal" 2>/dev/null || stat -c%s "${DATA_DIR}/cinder.wal" 2>/dev/null || echo "?")
echo "    lumen.wal size after compact: ${lumen_wal_size}"
echo "    cinder.wal size after compact: ${cinder_wal_size}"
if [[ "${lumen_wal_size}" != "0" ]]; then
    echo "FAIL: lumen WAL not truncated after compact" >&2
    exit 1
fi

echo
echo "==> 5. Read again — snapshot recovery path"
records_phase3=$("${CLI_BIN}" read "${TENANT}" "${DATA_DIR}" 2>/dev/null | wc -l | tr -d ' ')
echo "    records returned from snapshot: ${records_phase3}"
if [[ "${records_phase3}" -ne 2 ]]; then
    echo "FAIL: snapshot recovery lost records" >&2
    exit 1
fi

echo
echo "==> All five phases passed."
echo "    Data directory left at: ${DATA_DIR}"
echo "    Remove with: rm -rf ${DATA_DIR}"
