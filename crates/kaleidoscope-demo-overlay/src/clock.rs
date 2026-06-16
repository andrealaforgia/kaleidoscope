// Kaleidoscope demo overlay — the clock seam
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

//! The clock seam. Synthesis anchors demo timestamps to "now"; "now" enters
//! through this trait so tests are deterministic and never read ambient time.

/// The single source of "now" the demo synthesis anchors against, in
/// nanoseconds since the Unix epoch (matching `Span::start_time_unix_nano`).
/// Production wires [`SystemClock`]; tests inject a fixed clock so the
/// now-relative timestamps are deterministic.
pub trait Clock: Send + Sync {
    /// The current wall-clock time in nanoseconds since the Unix epoch.
    fn now_unix_nano(&self) -> u64;
}

/// The production clock: the host wall clock. Currency depends on a correct,
/// NTP-synchronised host clock (ADR-0079); offset/window-math bugs are caught by
/// the startup currency probe (slice D), host clock skew is mitigated operationally.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_nano(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The production clock reads the real host wall clock — a plausible
    /// current instant, not a stub. Pins that `SystemClock` returns genuine
    /// `now` (well after 2023-11), so synthesis anchored to it is current.
    #[test]
    fn system_clock_reads_a_plausible_current_wall_clock() {
        // 2023-11-14T22:13:20Z in nanos — any real run is comfortably after it.
        const AFTER_2023: u64 = 1_700_000_000_000_000_000;
        assert!(
            SystemClock.now_unix_nano() > AFTER_2023,
            "the system clock must read a real, current wall-clock time"
        );
    }
}
