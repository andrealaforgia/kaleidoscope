// Kaleidoscope integration suite — cross-crate composition test
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

//! Cross-crate composition test for the v1 adapters.
//!
//! Proves that Cinder v1, Sluice v1, and Lumen v1 compose under
//! a shared `aegis::TenantId` and that all three survive a
//! restart together with consistent state. This is the first
//! integration evidence that the platform is one thing, not 18
//! disconnected libraries.
//!
//! ## Scenario
//!
//! A platform engineer ingests a batch of log records for tenant
//! `acme` into Lumen v1, queues a "batch processed" notification
//! in Sluice v1 (so a downstream consumer can react), and places
//! a tier metadata entry for the batch in Cinder v1 Hot tier.
//! The process drops. On restart the test asserts:
//!
//! 1. The Lumen records are still queryable for tenant `acme`.
//! 2. The Sluice queue still has the pending notification for
//!    tenant `acme`.
//! 3. The Cinder tier metadata for the batch still says Hot.
//! 4. None of tenant `globex`'s parallel state has leaked into
//!    tenant `acme`'s.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, NoopRecorder as CinderRecorder, Tier, TieringStore};
use lumen::{
    FileBackedLogStore, LogBatch, LogRecord, LogStore, NoopRecorder as LumenRecorder,
    SeverityNumber, TimeRange,
};
use sluice::{FileBackedQueue, NoopRecorder as SluiceRecorder, Queue};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn temp_root(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("kal-integ-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir root");
    path
}

fn cleanup(root: &std::path::Path) {
    let _ = fs::remove_dir_all(root);
}

fn log_record(observed: u64, service: &str, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    LogRecord {
        observed_time_unix_nano: observed,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

#[test]
fn cinder_sluice_lumen_compose_under_shared_tenant_id_and_survive_restart() {
    let root = temp_root("compose_restart");
    let lumen_base = root.join("lumen-store");
    let sluice_base = root.join("sluice-queue");
    let cinder_base = root.join("cinder-tiers");

    let acme = tenant("acme");
    let globex = tenant("globex");
    let batch_item = ItemId::new("acme/2026-05-15/batch-001");

    // --- Phase 1: ingest, queue, place; then drop everything. ---
    {
        let lumen =
            FileBackedLogStore::open(&lumen_base, Box::new(LumenRecorder)).expect("open lumen");
        let sluice = FileBackedQueue::open(&sluice_base, 100, Box::new(SluiceRecorder))
            .expect("open sluice");
        let cinder = FileBackedTieringStore::open(&cinder_base, Box::new(CinderRecorder))
            .expect("open cinder");

        // Ingest a batch of three log records for tenant acme.
        lumen
            .ingest(
                &acme,
                LogBatch::with_records(vec![
                    log_record(100, "checkout", "first"),
                    log_record(200, "checkout", "second"),
                    log_record(300, "checkout", "third"),
                ]),
            )
            .expect("lumen ingest acme");

        // Parallel ingest for tenant globex — must NOT bleed into
        // acme's state across any of the three adapters.
        lumen
            .ingest(
                &globex,
                LogBatch::with_records(vec![log_record(150, "billing", "globex-only")]),
            )
            .expect("lumen ingest globex");

        // Queue a "batch processed" notification for acme,
        // payload is the item id so the consumer can correlate.
        let _msg_id = sluice
            .enqueue(&acme, batch_item.as_str().as_bytes().to_vec())
            .expect("sluice enqueue acme");
        sluice
            .enqueue(&globex, b"globex-only".to_vec())
            .expect("sluice enqueue globex");

        // Place tier metadata for the batch in Cinder Hot tier.
        cinder.place(&acme, &batch_item, Tier::Hot, SystemTime::now());
        cinder.place(
            &globex,
            &ItemId::new("globex/2026-05-15/batch-001"),
            Tier::Cold,
            SystemTime::now(),
        );

        // Stores drop at end of scope; BufWriter flushes.
    }

    // --- Phase 2: reopen, verify consistent state across all three. ---
    let lumen2 =
        FileBackedLogStore::open(&lumen_base, Box::new(LumenRecorder)).expect("reopen lumen");
    let sluice2 =
        FileBackedQueue::open(&sluice_base, 100, Box::new(SluiceRecorder)).expect("reopen sluice");
    let cinder2 = FileBackedTieringStore::open(&cinder_base, Box::new(CinderRecorder))
        .expect("reopen cinder");

    // Lumen: acme has three records in observed-time order.
    let recs = lumen2.query(&acme, TimeRange::all()).expect("lumen query");
    assert_eq!(recs.len(), 3, "acme lumen records recovered");
    assert_eq!(recs[0].body, "first");
    assert_eq!(recs[1].body, "second");
    assert_eq!(recs[2].body, "third");

    // Lumen: globex's record is isolated — not visible under acme.
    assert!(recs.iter().all(|r| r.body != "globex-only"));
    let globex_recs = lumen2.query(&globex, TimeRange::all()).expect("globex");
    assert_eq!(globex_recs.len(), 1);
    assert_eq!(globex_recs[0].body, "globex-only");

    // Sluice: acme's notification is still pending.
    assert_eq!(sluice2.depth(&acme), 1);
    let msg = sluice2.dequeue(&acme).expect("dequeue acme");
    assert_eq!(msg.payload, batch_item.as_str().as_bytes().to_vec());
    assert_eq!(msg.tenant, acme);

    // Sluice: globex's notification is isolated.
    assert_eq!(sluice2.depth(&globex), 1);
    let globex_msg = sluice2.dequeue(&globex).expect("dequeue globex");
    assert_eq!(globex_msg.payload, b"globex-only".to_vec());

    // Cinder: acme's batch is still Hot.
    assert_eq!(
        cinder2.get_tier(&acme, &batch_item),
        Some(Tier::Hot),
        "acme tier metadata recovered"
    );
    // Cinder: globex's tier metadata is isolated AND different.
    assert_eq!(
        cinder2.get_tier(&globex, &ItemId::new("globex/2026-05-15/batch-001")),
        Some(Tier::Cold)
    );
    // Cinder: acme has no entry under globex's namespace.
    assert_eq!(
        cinder2.get_tier(&acme, &ItemId::new("globex/2026-05-15/batch-001")),
        None
    );

    cleanup(&root);
}

#[test]
fn tenant_id_is_the_cross_crate_identity_contract() {
    // This test exists purely to document — by example, in
    // compiled and exercised code — that the same TenantId
    // crosses Lumen, Sluice, and Cinder with no conversion.
    // If aegis ever changes TenantId's shape, this test breaks
    // at compile time, alerting the maintainer that the
    // cross-crate contract has shifted.
    let root = temp_root("identity_contract");

    let one_tenant = tenant("shared");

    let lumen =
        FileBackedLogStore::open(root.join("l"), Box::new(LumenRecorder)).expect("open lumen");
    let sluice =
        FileBackedQueue::open(root.join("s"), 10, Box::new(SluiceRecorder)).expect("open sluice");
    let cinder = FileBackedTieringStore::open(root.join("c"), Box::new(CinderRecorder))
        .expect("open cinder");

    // The same `&TenantId` reference passes to all three
    // adapters. No conversion, no clone-per-call, no
    // adapter-specific tenant types.
    lumen
        .ingest(
            &one_tenant,
            LogBatch::with_records(vec![log_record(100, "svc", "body")]),
        )
        .expect("lumen ingest");
    sluice
        .enqueue(&one_tenant, b"payload".to_vec())
        .expect("sluice enqueue");
    cinder.place(
        &one_tenant,
        &ItemId::new("entry"),
        Tier::Hot,
        SystemTime::now(),
    );

    // All three are observable under the same tenant.
    assert_eq!(lumen.query(&one_tenant, TimeRange::all()).unwrap().len(), 1);
    assert_eq!(sluice.depth(&one_tenant), 1);
    assert_eq!(
        cinder.get_tier(&one_tenant, &ItemId::new("entry")),
        Some(Tier::Hot)
    );

    cleanup(&root);
}
