// Kaleidoscope Aegis — slice 03 audit-log acceptance test
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

//! Slice 03 — Aegis audit log via tracing
//!
//! Maps to `docs/feature/aegis-v0/slices/slice-03-audit.md`.
//! Companion story: US-AE-03. KPI 3: 100% of validations produce
//! exactly one structured audit event with stable field names.

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use aegis::{TenantCatalogue, TenantId, TenantRecord, Validator, ValidatorConfig};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use tracing::field::{Field, Visit};
use tracing::subscriber::with_default;
use tracing::{Event, Level, Subscriber};

const ISSUER: &str = "https://idp.acme.internal/";
const AUDIENCE: &str = "kaleidoscope-cluster-prod";
const SECRET: &[u8] = b"slice-03-audit-test-secret";

#[derive(Debug, Serialize)]
struct Claims<'a> {
    iss: &'a str,
    aud: &'a str,
    exp: i64,
    tenant_id: &'a str,
    kaleidoscope_role: &'a str,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn make_jwt(claims: &Claims<'_>) -> String {
    encode(
        &Header::new(jsonwebtoken::Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(SECRET),
    )
    .expect("encode")
}

fn make_validator(tenants: &[&str]) -> Validator {
    let records: Vec<TenantRecord> = tenants
        .iter()
        .map(|id| TenantRecord {
            id: TenantId((*id).to_string()),
            display_name: None,
            notes: None,
        })
        .collect();
    Validator::new(ValidatorConfig {
        issuer: ISSUER.to_string(),
        audience: AUDIENCE.to_string(),
        hs256_key: SECRET.to_vec(),
        catalogue: TenantCatalogue::from_records(records).expect("catalogue"),
    })
}

// --------------------------------------------------------------------
// Minimal in-process tracing subscriber that captures events for
// inspection. Avoids pulling `tracing-test` as a dep.
// --------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
struct AuditEvent {
    pub level: String,
    pub fields: std::collections::BTreeMap<String, String>,
}

#[derive(Default)]
struct AuditSubscriber {
    events: Arc<Mutex<Vec<AuditEvent>>>,
}

impl AuditSubscriber {
    fn new() -> (Self, Arc<Mutex<Vec<AuditEvent>>>) {
        let events: Arc<Mutex<Vec<AuditEvent>>> = Arc::default();
        (
            Self {
                events: Arc::clone(&events),
            },
            events,
        )
    }
}

struct FieldVisitor<'a> {
    fields: &'a mut std::collections::BTreeMap<String, String>,
}

impl<'a> Visit for FieldVisitor<'a> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_string(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}

impl Subscriber for AuditSubscriber {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn event(&self, event: &Event<'_>) {
        let level = match *event.metadata().level() {
            Level::ERROR => "error",
            Level::WARN => "warn",
            Level::INFO => "info",
            Level::DEBUG => "debug",
            Level::TRACE => "trace",
        };
        let mut fields = std::collections::BTreeMap::new();
        event.record(&mut FieldVisitor {
            fields: &mut fields,
        });
        self.events.lock().unwrap().push(AuditEvent {
            level: level.to_string(),
            fields,
        });
    }
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}
}

// --------------------------------------------------------------------
// AC-3.1 — allow path emits info event
// --------------------------------------------------------------------

#[test]
fn allow_path_emits_info_event_with_stable_fields() {
    let (subscriber, events) = AuditSubscriber::new();
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp: now_secs() + 3600,
        tenant_id: "acme-prod",
        kaleidoscope_role: "operator",
    });

    with_default(subscriber, || {
        let _ = validator.validate(&token, SystemTime::now());
    });

    let captured = events.lock().unwrap();
    assert_eq!(captured.len(), 1);
    let ev = &captured[0];
    assert_eq!(ev.level, "info");
    assert_eq!(ev.fields.get("decision").map(String::as_str), Some("allow"));
    assert_eq!(ev.fields.get("role").map(String::as_str), Some("operator"));
    assert!(ev.fields.contains_key("tenant_id"));
    assert!(ev.fields.contains_key("subject"));
    assert!(ev.fields.contains_key("reason"));
}

// --------------------------------------------------------------------
// AC-3.2 — deny paths emit warn events with the correct reason
// --------------------------------------------------------------------

#[test]
fn deny_path_emits_warn_event_with_reason() {
    let (subscriber, events) = AuditSubscriber::new();
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp: now_secs() - 60,
        tenant_id: "acme-prod",
        kaleidoscope_role: "operator",
    });

    with_default(subscriber, || {
        let _ = validator.validate(&token, SystemTime::now());
    });

    let captured = events.lock().unwrap();
    assert_eq!(captured.len(), 1);
    let ev = &captured[0];
    assert_eq!(ev.level, "warn");
    assert_eq!(ev.fields.get("decision").map(String::as_str), Some("deny"));
    assert_eq!(ev.fields.get("reason").map(String::as_str), Some("expired"));
}

#[test]
fn unknown_tenant_deny_carries_unknown_tenant_reason() {
    let (subscriber, events) = AuditSubscriber::new();
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp: now_secs() + 3600,
        tenant_id: "ghost",
        kaleidoscope_role: "operator",
    });

    with_default(subscriber, || {
        let _ = validator.validate(&token, SystemTime::now());
    });

    let captured = events.lock().unwrap();
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0].fields.get("reason").map(String::as_str),
        Some("unknown_tenant")
    );
}

// --------------------------------------------------------------------
// AC-3.4 — subject parameter is honoured
// --------------------------------------------------------------------

#[test]
fn validate_with_subject_records_subject_in_audit_event() {
    let (subscriber, events) = AuditSubscriber::new();
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp: now_secs() + 3600,
        tenant_id: "acme-prod",
        kaleidoscope_role: "operator",
    });

    with_default(subscriber, || {
        let _ = validator.validate_with_subject(&token, SystemTime::now(), "query_range");
    });

    let captured = events.lock().unwrap();
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0].fields.get("subject").map(String::as_str),
        Some("query_range")
    );
}

// --------------------------------------------------------------------
// KPI 3 — every validation produces exactly one event
// --------------------------------------------------------------------

#[test]
fn one_hundred_validations_mix_produce_one_hundred_audit_events() {
    let (subscriber, events) = AuditSubscriber::new();
    let validator = make_validator(&["acme-prod"]);
    let ok = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp: now_secs() + 3600,
        tenant_id: "acme-prod",
        kaleidoscope_role: "operator",
    });
    let expired = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp: now_secs() - 1,
        tenant_id: "acme-prod",
        kaleidoscope_role: "operator",
    });

    with_default(subscriber, || {
        for i in 0..100 {
            let token = if i % 2 == 0 { &ok } else { &expired };
            let _ = validator.validate(token, SystemTime::now());
        }
    });

    let captured = events.lock().unwrap();
    assert_eq!(
        captured.len(),
        100,
        "KPI 3: every validation must emit exactly one audit event"
    );
    let allows = captured
        .iter()
        .filter(|e| e.fields.get("decision").map(String::as_str) == Some("allow"))
        .count();
    let denies = captured
        .iter()
        .filter(|e| e.fields.get("decision").map(String::as_str) == Some("deny"))
        .count();
    assert_eq!(allows, 50);
    assert_eq!(denies, 50);
}
