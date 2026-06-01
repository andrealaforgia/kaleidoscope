// Kaleidoscope Aegis — slice 01 JWT validate acceptance test
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

//! Slice 01 — `aegis::Validator::validate` walking skeleton
//!
//! Maps to `docs/feature/aegis-v0/slices/slice-01-validate.md`.
//! Companion story: US-AE-01.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aegis::{
    Role, TenantCatalogue, TenantContext, TenantId, TenantRecord, ValidationError, Validator,
    ValidatorConfig,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;

const ISSUER: &str = "https://idp.acme.internal/";
const AUDIENCE: &str = "kaleidoscope-cluster-prod";
const SECRET: &[u8] = b"slice-01-test-secret-do-not-use-in-production";

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
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn make_jwt(claims: &Claims<'_>) -> String {
    let header = Header::new(jsonwebtoken::Algorithm::HS256);
    let key = EncodingKey::from_secret(SECRET);
    encode(&header, claims, &key).expect("encode")
}

fn make_jwt_with_secret(claims: &Claims<'_>, secret: &[u8]) -> String {
    let header = Header::new(jsonwebtoken::Algorithm::HS256);
    let key = EncodingKey::from_secret(secret);
    encode(&header, claims, &key).expect("encode")
}

fn catalogue_with(tenants: &[&str]) -> TenantCatalogue {
    let records: Vec<TenantRecord> = tenants
        .iter()
        .map(|id| TenantRecord {
            id: TenantId((*id).to_string()),
            display_name: None,
            notes: None,
        })
        .collect();
    TenantCatalogue::from_records(records).expect("catalogue")
}

fn make_validator(tenants: &[&str]) -> Validator {
    Validator::new(ValidatorConfig {
        issuer: ISSUER.to_string(),
        audience: AUDIENCE.to_string(),
        hs256_key: SECRET.to_vec(),
        catalogue: catalogue_with(tenants),
    })
}

// --------------------------------------------------------------------
// AC-1.1 — happy path returns TenantContext
// --------------------------------------------------------------------

#[test]
fn signed_current_known_tenant_known_role_returns_tenant_context() {
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp: now_secs() + 3600,
        tenant_id: "acme-prod",
        kaleidoscope_role: "operator",
    });
    let ctx = validator.validate(&token, SystemTime::now()).expect("ok");
    assert_eq!(
        ctx,
        TenantContext {
            tenant_id: TenantId("acme-prod".to_string()),
            role: Role::Operator,
        }
    );
}

#[test]
fn viewer_role_is_accepted() {
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp: now_secs() + 3600,
        tenant_id: "acme-prod",
        kaleidoscope_role: "viewer",
    });
    let ctx = validator.validate(&token, SystemTime::now()).expect("ok");
    assert_eq!(ctx.role, Role::Viewer);
}

// --------------------------------------------------------------------
// AC-1.2 — invalid signature
// --------------------------------------------------------------------

#[test]
fn invalid_signature_returns_invalid_signature_error() {
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt_with_secret(
        &Claims {
            iss: ISSUER,
            aud: AUDIENCE,
            exp: now_secs() + 3600,
            tenant_id: "acme-prod",
            kaleidoscope_role: "operator",
        },
        b"a different secret",
    );
    let err = validator.validate(&token, SystemTime::now()).unwrap_err();
    assert_eq!(err, ValidationError::InvalidSignature);
}

// --------------------------------------------------------------------
// AC-1.3 — expired
// --------------------------------------------------------------------

#[test]
fn expired_token_returns_expired_error() {
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp: now_secs() - 60,
        tenant_id: "acme-prod",
        kaleidoscope_role: "operator",
    });
    let err = validator.validate(&token, SystemTime::now()).unwrap_err();
    assert_eq!(err, ValidationError::Expired);
}

#[test]
fn token_at_exact_exp_returns_expired_error() {
    // exp is the moment the token becomes invalid (>=, not >).
    let validator = make_validator(&["acme-prod"]);
    let exp = now_secs() + 10;
    let token = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp,
        tenant_id: "acme-prod",
        kaleidoscope_role: "operator",
    });
    let now = UNIX_EPOCH + Duration::from_secs(exp as u64);
    let err = validator.validate(&token, now).unwrap_err();
    assert_eq!(err, ValidationError::Expired);
}

// --------------------------------------------------------------------
// AC-1.4 — wrong issuer
// --------------------------------------------------------------------

#[test]
fn wrong_issuer_returns_wrong_issuer_error() {
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt(&Claims {
        iss: "https://other-idp.example.com/",
        aud: AUDIENCE,
        exp: now_secs() + 3600,
        tenant_id: "acme-prod",
        kaleidoscope_role: "operator",
    });
    let err = validator.validate(&token, SystemTime::now()).unwrap_err();
    assert_eq!(err, ValidationError::WrongIssuer);
}

// --------------------------------------------------------------------
// AC-1.5 — wrong audience
// --------------------------------------------------------------------

#[test]
fn wrong_audience_returns_wrong_audience_error() {
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt(&Claims {
        iss: ISSUER,
        aud: "different-cluster",
        exp: now_secs() + 3600,
        tenant_id: "acme-prod",
        kaleidoscope_role: "operator",
    });
    let err = validator.validate(&token, SystemTime::now()).unwrap_err();
    assert_eq!(err, ValidationError::WrongAudience);
}

// --------------------------------------------------------------------
// AC-1.6 — unknown tenant
// --------------------------------------------------------------------

#[test]
fn unknown_tenant_returns_unknown_tenant_error() {
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp: now_secs() + 3600,
        tenant_id: "shut-off-customer",
        kaleidoscope_role: "operator",
    });
    let err = validator.validate(&token, SystemTime::now()).unwrap_err();
    assert_eq!(err, ValidationError::UnknownTenant);
}

// --------------------------------------------------------------------
// AC-1.7 — unknown role
// --------------------------------------------------------------------

#[test]
fn unknown_role_returns_unknown_role_error() {
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp: now_secs() + 3600,
        tenant_id: "acme-prod",
        kaleidoscope_role: "admin", // not in {viewer, operator}
    });
    let err = validator.validate(&token, SystemTime::now()).unwrap_err();
    assert_eq!(err, ValidationError::UnknownRole);
}

// --------------------------------------------------------------------
// Missing-claim diagnostics.
// --------------------------------------------------------------------

#[test]
fn missing_tenant_id_returns_missing_claim_tenant_id() {
    let validator = make_validator(&["acme-prod"]);
    // Build a JWT without tenant_id by using a different claim shape.
    #[derive(Debug, Serialize)]
    struct NoTenant<'a> {
        iss: &'a str,
        aud: &'a str,
        exp: i64,
        kaleidoscope_role: &'a str,
    }
    let header = Header::new(jsonwebtoken::Algorithm::HS256);
    let key = EncodingKey::from_secret(SECRET);
    let token = encode(
        &header,
        &NoTenant {
            iss: ISSUER,
            aud: AUDIENCE,
            exp: now_secs() + 3600,
            kaleidoscope_role: "operator",
        },
        &key,
    )
    .expect("encode");
    let err = validator.validate(&token, SystemTime::now()).unwrap_err();
    assert_eq!(err, ValidationError::MissingClaim("tenant_id"));
}

#[test]
fn missing_exp_returns_missing_claim_exp() {
    let validator = make_validator(&["acme-prod"]);
    #[derive(Debug, Serialize)]
    struct NoExp<'a> {
        iss: &'a str,
        aud: &'a str,
        tenant_id: &'a str,
        kaleidoscope_role: &'a str,
    }
    let header = Header::new(jsonwebtoken::Algorithm::HS256);
    let key = EncodingKey::from_secret(SECRET);
    let token = encode(
        &header,
        &NoExp {
            iss: ISSUER,
            aud: AUDIENCE,
            tenant_id: "acme-prod",
            kaleidoscope_role: "operator",
        },
        &key,
    )
    .expect("encode");
    let err = validator.validate(&token, SystemTime::now()).unwrap_err();
    assert_eq!(err, ValidationError::MissingClaim("exp"));
}

// --------------------------------------------------------------------
// Malformed input.
// --------------------------------------------------------------------

#[test]
fn malformed_token_returns_malformed_error() {
    let validator = make_validator(&["acme-prod"]);
    let err = validator
        .validate("not.a.jwt", SystemTime::now())
        .unwrap_err();
    assert_eq!(err, ValidationError::Malformed);
}

// --------------------------------------------------------------------
// KPI 1 — validation latency p95 ≤ 2 ms
//
// 2 ms not 1 ms: local-workstation JWT validation is ~50-150 µs;
// GitHub Actions ubuntu-latest under contention runs in the
// 800-1300 µs range. Same CI-realism bump batch as Lumen v0,
// Pulse v0, and Cinder v1 (2026-05-19).
// --------------------------------------------------------------------

#[test]
fn validate_p95_latency_under_two_milliseconds() {
    if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
        eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
        return;
    }
    let validator = make_validator(&["acme-prod"]);
    let token = make_jwt(&Claims {
        iss: ISSUER,
        aud: AUDIENCE,
        exp: now_secs() + 3600,
        tenant_id: "acme-prod",
        kaleidoscope_role: "operator",
    });
    let now = SystemTime::now();
    // Warm up.
    for _ in 0..50 {
        let _ = validator.validate(&token, now);
    }
    // Measure 1000 invocations.
    let mut samples: Vec<u128> = Vec::with_capacity(1000);
    for _ in 0..1000 {
        let t0 = std::time::Instant::now();
        let _ = validator.validate(&token, now);
        samples.push(t0.elapsed().as_micros());
    }
    samples.sort_unstable();
    let p95 = samples[950];
    assert!(
        p95 <= 2_000,
        "KPI 1: p95 must be ≤ 2ms (2000us); got {p95}us"
    );
}
