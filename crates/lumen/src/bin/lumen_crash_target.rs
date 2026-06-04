// Kaleidoscope Lumen — out-of-process crash target (kill-target helper)
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

//! SCAFFOLD: true — store-fsync-durability-v0 DISTILL (Mandate 7, RED-ready).
//!
//! Kill-target helper binary for mechanism (a) — snapshot-atomicity proving
//! (ADR-0060 §1, C5). The lumen snapshot-atomicity acceptance suite
//! (`tests/v1_slice_04_crash_durability.rs`) spawns THIS binary as a real
//! child PROCESS (`std::process::Command`), lets it ack a write, then
//! `SIGKILL`s it (via `Child::kill`) WHILE it is writing a snapshot — the
//! out-of-process true crash ADR-0049 §3/alt-A RESERVED and this feature
//! uses. The parent then reopens the store and asserts the crash-at-ANY-point
//! invariant (canonical path holds the OLD or NEW whole snapshot, never a
//! torn one) and that `open()` succeeds.
//!
//! Contract (the parent test drives these argv/env; DELIVER implements):
//!   - reads pillar root from `$KALEIDOSCOPE_CRASH_PILLAR_ROOT`.
//!   - mode `--seed-then-loop-snapshot`: open the store, ingest the acked
//!     records named on argv, print a readiness sentinel line to stdout
//!     (`CRASH_TARGET_READY`) so the parent kills at a controlled moment,
//!     then loop calling `snapshot()` forever so a kill lands mid-snapshot.
//!   - mode `--probe-lying`: drive the composition root with a
//!     `LyingFsyncBackend`; emit `event=health.startup.refused
//!     substrate=<descriptor>` to stderr and exit non-zero WITHOUT opening
//!     the store for writes (AC-substrate-refusal, mechanism (b) variant).
//!
//! The binary writes ONLY under the tmp pillar root the parent hands it
//! (`tempfile`/`TempDir`), never a fixed path, so concurrent runs and the
//! clean+ci environments do not collide (DEVOPS environments.yaml).
//!
//! DELIVER replaces this `panic!` body with the real seed/loop/probe logic.
//! ZERO `// SCAFFOLD: true` markers remain after DELIVER.

fn main() {
    panic!("__SCAFFOLD__ lumen_crash_target RED scaffold (store-fsync-durability-v0 slice 01)");
}
