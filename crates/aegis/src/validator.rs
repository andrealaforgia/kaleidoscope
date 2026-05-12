// Kaleidoscope Aegis — tenancy + auth + audit
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

//! JWT validator: pre-loaded with issuer + audience + signing key
//! and tenant catalogue. The `validate` method is the slice 01
//! entry point.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

use crate::catalogue::TenantCatalogue;

/// Stable identifier for a tenant. Newtype around `String` so the
/// rest of the platform can take `&TenantId` and refuse to confuse
/// tenant ids with role names or user ids.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TenantId(pub String);

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Two-level RBAC at v0. Full OPA matrix arrives at v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Viewer,
    Operator,
}

impl Role {
    /// Render as the canonical JWT claim string.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Operator => "operator",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Typed context returned by [`Validator::validate`] on success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantContext {
    pub tenant_id: TenantId,
    pub role: Role,
}

/// Typed failure modes. Every variant maps to a stable audit
/// `reason` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Token signature did not verify against the configured key.
    InvalidSignature,
    /// Token's `exp` claim is in the past relative to `now`.
    Expired,
    /// Token's `iss` claim does not match the configured issuer.
    WrongIssuer,
    /// Token's `aud` claim does not match the configured audience.
    WrongAudience,
    /// Token is missing one of the required claims
    /// (`tenant_id`, `kaleidoscope_role`).
    MissingClaim(&'static str),
    /// `tenant_id` claim is not in the operator's tenant catalogue.
    UnknownTenant,
    /// `kaleidoscope_role` claim is neither `viewer` nor `operator`.
    UnknownRole,
    /// Token is malformed (not three base64 segments, bad JSON, etc.).
    Malformed,
}

impl ValidationError {
    /// Stable `reason` string for audit logs.
    pub fn reason(&self) -> &'static str {
        match self {
            ValidationError::InvalidSignature => "invalid_signature",
            ValidationError::Expired => "expired",
            ValidationError::WrongIssuer => "wrong_issuer",
            ValidationError::WrongAudience => "wrong_audience",
            ValidationError::MissingClaim(_) => "missing_claim",
            ValidationError::UnknownTenant => "unknown_tenant",
            ValidationError::UnknownRole => "unknown_role",
            ValidationError::Malformed => "malformed",
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::InvalidSignature => f.write_str("invalid signature"),
            ValidationError::Expired => f.write_str("token expired"),
            ValidationError::WrongIssuer => f.write_str("issuer mismatch"),
            ValidationError::WrongAudience => f.write_str("audience mismatch"),
            ValidationError::MissingClaim(name) => write!(f, "missing required claim: {name}"),
            ValidationError::UnknownTenant => f.write_str("tenant not in catalogue"),
            ValidationError::UnknownRole => f.write_str("unknown role"),
            ValidationError::Malformed => f.write_str("malformed JWT"),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Configuration for a [`Validator`]. Constructed once at startup;
/// the validator pre-computes everything it needs so [`Validator::validate`]
/// has no I/O.
#[derive(Debug, Clone)]
pub struct ValidatorConfig {
    pub issuer: String,
    pub audience: String,
    /// Pre-shared signing key bytes. v0 supports HS256 (HMAC-SHA256);
    /// v1 will add RS256 + JWKS rotation.
    pub hs256_key: Vec<u8>,
    pub catalogue: TenantCatalogue,
}

/// Pre-loaded validator. Constructed once; cheap to clone.
#[derive(Clone)]
pub struct Validator {
    issuer: String,
    audience: String,
    key: DecodingKey,
    catalogue: TenantCatalogue,
}

impl fmt::Debug for Validator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Validator")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("key", &"<opaque>")
            .field("catalogue_len", &self.catalogue.len())
            .finish()
    }
}

impl Validator {
    /// Build a validator from configuration. Cheap; no I/O.
    pub fn new(config: ValidatorConfig) -> Self {
        Self {
            issuer: config.issuer,
            audience: config.audience,
            key: DecodingKey::from_secret(&config.hs256_key),
            catalogue: config.catalogue,
        }
    }

    /// Validate one JWT. Returns the typed tenant context on
    /// success, or a typed error naming the failure mode. Emits
    /// exactly one structured `tracing` event per call.
    pub fn validate(&self, token: &str, now: SystemTime) -> Result<TenantContext, ValidationError> {
        self.validate_with_subject(token, now, "validate")
    }

    /// Same as [`Self::validate`] but the audit event carries the
    /// caller-supplied `subject` field (e.g. `"query_range"`).
    pub fn validate_with_subject(
        &self,
        token: &str,
        now: SystemTime,
        subject: &str,
    ) -> Result<TenantContext, ValidationError> {
        let result = self.validate_inner(token, now);
        match &result {
            Ok(ctx) => {
                tracing::info!(
                    tenant_id = %ctx.tenant_id,
                    role = ctx.role.as_str(),
                    decision = "allow",
                    subject = subject,
                    reason = "allow",
                    "aegis authz decision"
                );
            }
            Err(err) => {
                tracing::warn!(
                    tenant_id = "",
                    role = "",
                    decision = "deny",
                    subject = subject,
                    reason = err.reason(),
                    "aegis authz decision"
                );
            }
        }
        result
    }

    fn validate_inner(
        &self,
        token: &str,
        now: SystemTime,
    ) -> Result<TenantContext, ValidationError> {
        // Build the Validation struct. We disable jsonwebtoken's
        // built-in exp check and audience check so we can map each
        // failure to its own typed variant.
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = false;
        validation.validate_aud = false;
        validation.required_spec_claims.clear();

        let token_data = decode::<RawClaims>(token, &self.key, &validation).map_err(map_err)?;
        let claims = token_data.claims;

        if claims.iss.as_deref() != Some(self.issuer.as_str()) {
            return Err(ValidationError::WrongIssuer);
        }
        if claims.aud.as_deref() != Some(self.audience.as_str()) {
            return Err(ValidationError::WrongAudience);
        }
        let exp = claims.exp.ok_or(ValidationError::MissingClaim("exp"))?;
        let now_secs = now
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if now_secs >= exp {
            return Err(ValidationError::Expired);
        }
        let tenant_str = claims
            .tenant_id
            .ok_or(ValidationError::MissingClaim("tenant_id"))?;
        let tenant_id = TenantId(tenant_str);
        if !self.catalogue.contains(&tenant_id) {
            return Err(ValidationError::UnknownTenant);
        }
        let role_str = claims
            .kaleidoscope_role
            .ok_or(ValidationError::MissingClaim("kaleidoscope_role"))?;
        let role = match role_str.as_str() {
            "viewer" => Role::Viewer,
            "operator" => Role::Operator,
            _ => return Err(ValidationError::UnknownRole),
        };
        Ok(TenantContext { tenant_id, role })
    }
}

/// Map `jsonwebtoken`'s error type to ours. The library bundles
/// signature failures and structural failures behind one type, so we
/// inspect `ErrorKind` to discriminate.
fn map_err(err: jsonwebtoken::errors::Error) -> ValidationError {
    use jsonwebtoken::errors::ErrorKind;
    match err.kind() {
        ErrorKind::InvalidSignature => ValidationError::InvalidSignature,
        ErrorKind::InvalidToken
        | ErrorKind::InvalidKeyFormat
        | ErrorKind::Base64(_)
        | ErrorKind::Json(_)
        | ErrorKind::Utf8(_) => ValidationError::Malformed,
        _ => ValidationError::Malformed,
    }
}

#[derive(Debug, Deserialize)]
struct RawClaims {
    #[serde(default)]
    iss: Option<String>,
    #[serde(default)]
    aud: Option<String>,
    #[serde(default)]
    exp: Option<i64>,
    #[serde(default)]
    tenant_id: Option<String>,
    #[serde(default)]
    kaleidoscope_role: Option<String>,
}
