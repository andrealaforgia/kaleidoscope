// Kaleidoscope Beacon — out-of-process crash target (kill-target helper)
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
//! Kill-target helper binary for beacon's rule-state-store
//! snapshot-atomicity proving (mechanism (a), ADR-0060 §1, C5). Spawned as
//! a real child PROCESS by `tests/v1_slice_03_crash_durability.rs`; acks a
//! rule-state transition, then loops writing snapshots so a `SIGKILL` lands
//! mid-snapshot. Reads the pillar root from
//! `$KALEIDOSCOPE_CRASH_PILLAR_ROOT`; writes only under the tmp root the
//! parent hands it. DELIVER replaces this `panic!` body; ZERO
//! `// SCAFFOLD: true` markers remain after DELIVER.

fn main() {
    panic!("__SCAFFOLD__ beacon_crash_target RED scaffold (store-fsync-durability-v0 slice 06)");
}
