#!/usr/bin/env node
// Kaleidoscope Prism — operator-facing observability SPA
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// Gate 8 — Prism bundle size. Asserts the gzipped JS bundle is
// ≤ 300 KB per the DISCUSS cross-KPI guardrail and ADR-0030 §7.
// Emits apps/prism/dist/bundle-size-report.json per the schema in
// docs/feature/prism-v0/devops/ci-cd-pipeline.md §3.3.

import { readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import { execSync } from 'node:child_process';

const root = fileURLToPath(new URL('..', import.meta.url));
const distDir = join(root, 'dist');
const assetsDir = join(distDir, 'assets');
const reportPath = join(distDir, 'bundle-size-report.json');

const LIMIT_BYTES = 300 * 1024; // 307200

function listJsAssets(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const s = statSync(full);
    if (s.isFile() && full.endsWith('.js')) out.push(full);
  }
  return out;
}

function gzipSize(file) {
  const raw = readFileSync(file);
  return gzipSync(raw).byteLength;
}

const chunks = listJsAssets(assetsDir)
  .map((file) => {
    const gz = gzipSize(file);
    return {
      path: relative(distDir, file),
      gzipped_bytes: gz,
      percentage_of_limit: (gz / LIMIT_BYTES) * 100,
    };
  })
  .sort((a, b) => b.gzipped_bytes - a.gzipped_bytes);

const total = chunks.reduce((acc, c) => acc + c.gzipped_bytes, 0);
const passed = total <= LIMIT_BYTES;

const sha = execSync('git rev-parse HEAD', { encoding: 'utf-8' }).trim();
const report = {
  total_gzipped_bytes: total,
  limit_gzipped_bytes: LIMIT_BYTES,
  passed,
  chunks,
  built_at: new Date().toISOString(),
  built_from_sha: sha,
};

writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n');

if (!passed) {
  console.error(
    `[fail] bundle size ${total} bytes exceeds limit ${LIMIT_BYTES} bytes (${((total / LIMIT_BYTES) * 100).toFixed(1)}%)`,
  );
  process.exit(1);
}
console.log(
  `[pass] bundle size ${total} bytes within limit ${LIMIT_BYTES} bytes (${((total / LIMIT_BYTES) * 100).toFixed(1)}%)`,
);
