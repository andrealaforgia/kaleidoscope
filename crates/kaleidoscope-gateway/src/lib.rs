// Kaleidoscope gateway — composition-root logic (library surface)
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

//! `kaleidoscope-gateway` — composition-root logic exposed as a library
//! seam so the Earned-Trust probes (storage sink + fsync honesty,
//! ADR-0049) are unit-testable in isolation rather than buried in the
//! binary's `main.rs`. The thin `src/main.rs` only reads the environment
//! and calls these.

#![forbid(unsafe_code)]

pub mod composition;
