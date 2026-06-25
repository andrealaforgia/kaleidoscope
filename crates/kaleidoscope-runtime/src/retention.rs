// Kaleidoscope consolidated runtime — age-based retention (B1 / slice S1)
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

//! Age-based retention for the metrics read path (slice S1).
//!
//! A single global window — a maximum age — is read from configuration
//! (`KALEIDOSCOPE_RETENTION_MAX_AGE`, e.g. `60s` / `7d`). When set, the metrics
//! query path is wrapped with [`RetentionMetricStore`]: before every read it
//! deletes from the underlying [`FileBackedMetricStore`] every point older than
//! the window, so an expired sample is genuinely GONE — absent even from a raw,
//! unfiltered read of the store, not merely hidden behind a query-time filter
//! (S1.E2/S1.E6). The deletion reuses the store's own
//! [`FileBackedMetricStore::purge_older_than`], which compacts the survivors to
//! disk, so the purge is durable and a later WAL replay cannot resurrect it.
//!
//! S1 is metrics-only and enforces on the read. With NO window configured the
//! decorator is never installed and nothing is ever purged — the safe
//! keep-forever default (S1.E4).

use std::sync::Arc;
use std::time::Duration;

use aegis::TenantId;
use kaleidoscope_demo_overlay::{Clock, SystemClock};
use pulse::{
    FileBackedMetricStore, IngestReceipt, Metric, MetricBatch, MetricName, MetricPoint,
    MetricStore, MetricStoreError, Predicate, TimeRange,
};

/// Parse a retention window expressed as `<positive integer><unit>` where unit
/// is one of `s` (seconds), `m` (minutes), `h` (hours), `d` (days). Examples:
/// `60s`, `15m`, `1h`, `7d`. Whitespace is trimmed. Returns the window as a
/// [`Duration`], or a human-readable error the binary surfaces on a misconfig.
pub fn parse_max_age(input: &str) -> Result<Duration, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("retention window is empty".to_string());
    }
    if !s.is_ascii() {
        return Err(format!("retention window '{s}' is not ascii"));
    }
    let (value_part, unit) = s.split_at(s.len() - 1);
    let unit_secs: u64 = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3_600,
        "d" => 86_400,
        other => {
            return Err(format!(
                "unknown retention unit '{other}' in '{s}' (use s, m, h or d)"
            ))
        }
    };
    let value: u64 = value_part
        .parse()
        .map_err(|_| format!("invalid retention value '{value_part}' in '{s}'"))?;
    if value == 0 {
        return Err(format!("retention window '{s}' must be greater than zero"));
    }
    let secs = value
        .checked_mul(unit_secs)
        .ok_or_else(|| format!("retention window '{s}' overflows"))?;
    Ok(Duration::from_secs(secs))
}

/// A read-side decorator over the durable [`FileBackedMetricStore`] that
/// enforces an age-based retention window by really DELETING expired points
/// before each read (S1). Generic over the [`Clock`] seam so the window is
/// deterministic under test; production threads [`SystemClock`].
///
/// The write path is never wrapped — ingest delegates straight through — so
/// retention only ever removes, never blocks, a write.
pub struct RetentionMetricStore<C = SystemClock> {
    inner: Arc<FileBackedMetricStore>,
    max_age: Duration,
    clock: C,
}

impl<C> RetentionMetricStore<C> {
    /// Wrap `inner`, enforcing `max_age` against `clock`.
    pub fn new(inner: Arc<FileBackedMetricStore>, max_age: Duration, clock: C) -> Self {
        Self {
            inner,
            max_age,
            clock,
        }
    }
}

impl<C: Clock> RetentionMetricStore<C> {
    /// Delete every point older than the window from the underlying store.
    /// `cutoff = now - max_age`; `purge_older_than` removes points strictly
    /// older than the cutoff (a sample exactly at the window edge is kept).
    /// A purge that removes nothing touches neither the index nor the disk, so
    /// this stays cheap on the common read where nothing has aged out.
    fn enforce(&self) -> Result<(), MetricStoreError> {
        let now = self.clock.now_unix_nano();
        let max_age_nanos = u64::try_from(self.max_age.as_nanos()).unwrap_or(u64::MAX);
        let cutoff = now.saturating_sub(max_age_nanos);
        self.inner.purge_older_than(cutoff)?;
        Ok(())
    }
}

impl<C: Clock> MetricStore for RetentionMetricStore<C> {
    /// Writes are never retained-on-write; delegate straight through.
    fn ingest(
        &self,
        tenant: &TenantId,
        batch: MetricBatch,
    ) -> Result<IngestReceipt, MetricStoreError> {
        self.inner.ingest(tenant, batch)
    }

    /// Enforce the window (delete expired points) THEN read. Because the
    /// expired points are gone from the store, the result excludes them as a
    /// consequence of a real delete, not a read-time mask.
    fn query(
        &self,
        tenant: &TenantId,
        metric_name: &MetricName,
        range: TimeRange,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        self.enforce()?;
        self.inner.query(tenant, metric_name, range)
    }

    fn query_with(
        &self,
        tenant: &TenantId,
        metric_name: &MetricName,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        self.enforce()?;
        self.inner.query_with(tenant, metric_name, range, predicate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use pulse::{MetricKind, NoopRecorder};

    struct FixedClock(u64);
    impl Clock for FixedClock {
        fn now_unix_nano(&self) -> u64 {
            self.0
        }
    }

    fn temp_base(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "retention-dec-{name}-{}-{nanos}",
            std::process::id()
        ));
        p
    }

    fn store(base: &std::path::Path) -> Arc<FileBackedMetricStore> {
        Arc::new(FileBackedMetricStore::open(base, Box::new(NoopRecorder)).expect("open"))
    }

    fn point(time: u64, value: f64) -> MetricPoint {
        MetricPoint {
            time_unix_nano: time,
            start_time_unix_nano: 0,
            attributes: BTreeMap::new(),
            value,
        }
    }

    fn gauge(points: Vec<MetricPoint>) -> Metric {
        let mut resource = BTreeMap::new();
        resource.insert("service.name".to_string(), "svc".to_string());
        Metric {
            name: MetricName::new("rps"),
            description: String::new(),
            unit: "1".to_string(),
            kind: MetricKind::Gauge,
            points,
            resource_attributes: resource,
        }
    }

    fn acme() -> TenantId {
        TenantId("acme".to_string())
    }

    // S1.E2/S1.E6: a query through the decorator excludes the expired point AND
    // really deletes it — a subsequent RAW read of the inner store (no
    // retention) also misses it, proving delete-not-mask. Boundary: now=1000,
    // max_age=500ns => cutoff=500; the t=100 point expires, t=500 (the edge)
    // and t=800 survive.
    #[test]
    fn query_deletes_expired_points_not_merely_masks_them() {
        let base = temp_base("delete-not-mask");
        let inner = store(&base);
        inner
            .ingest(
                &acme(),
                MetricBatch::with_metrics(vec![gauge(vec![
                    point(100, 1.0),
                    point(500, 5.0),
                    point(800, 8.0),
                ])]),
            )
            .expect("ingest");

        let decorated = RetentionMetricStore::new(
            Arc::clone(&inner),
            Duration::from_nanos(500),
            FixedClock(1000),
        );

        let rows = decorated
            .query(&acme(), &MetricName::new("rps"), TimeRange::all())
            .expect("query");
        let times: Vec<u64> = rows.iter().map(|(_, p)| p.time_unix_nano).collect();
        assert_eq!(times, vec![500, 800], "the expired t=100 point is absent");

        // Raw read of the inner store — NO retention decorator, no age filter.
        let raw = inner
            .query(&acme(), &MetricName::new("rps"), TimeRange::all())
            .expect("raw query");
        let raw_times: Vec<u64> = raw.iter().map(|(_, p)| p.time_unix_nano).collect();
        assert_eq!(
            raw_times,
            vec![500, 800],
            "the expired point is gone from the store itself — a real delete, not a mask"
        );

        let _ = std::fs::remove_file({
            let mut p = base.as_os_str().to_owned();
            p.push(".wal");
            PathBuf::from(p)
        });
        let _ = std::fs::remove_file({
            let mut p = base.as_os_str().to_owned();
            p.push(".snapshot");
            PathBuf::from(p)
        });
    }

    // S1.E3: the in-window point is returned unchanged after enforcement.
    #[test]
    fn in_window_point_is_returned_unchanged() {
        let base = temp_base("unchanged");
        let inner = store(&base);
        let fresh = point(800, 42.5);
        inner
            .ingest(
                &acme(),
                MetricBatch::with_metrics(vec![gauge(vec![point(100, 1.0), fresh.clone()])]),
            )
            .expect("ingest");
        let decorated = RetentionMetricStore::new(
            Arc::clone(&inner),
            Duration::from_nanos(500),
            FixedClock(1000),
        );
        let rows = decorated
            .query(&acme(), &MetricName::new("rps"), TimeRange::all())
            .expect("query");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, fresh, "the in-window point is unchanged");
    }

    #[test]
    fn parse_max_age_accepts_units() {
        assert_eq!(parse_max_age("60s").unwrap(), Duration::from_secs(60));
        assert_eq!(parse_max_age("15m").unwrap(), Duration::from_secs(900));
        assert_eq!(parse_max_age("1h").unwrap(), Duration::from_secs(3_600));
        assert_eq!(parse_max_age("7d").unwrap(), Duration::from_secs(604_800));
        assert_eq!(parse_max_age("  30s ").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn parse_max_age_rejects_garbage() {
        assert!(parse_max_age("").is_err());
        assert!(parse_max_age("60").is_err(), "missing unit");
        assert!(parse_max_age("xs").is_err(), "non-numeric value");
        assert!(parse_max_age("10y").is_err(), "unknown unit");
        assert!(parse_max_age("0s").is_err(), "zero window");
    }
}
