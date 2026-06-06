// Kaleidoscope Cinder — TieringStore trait + in-memory adapter
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

//! `TieringStore` trait + in-memory adapter.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use std::time::SystemTime;

use aegis::TenantId;

use crate::metrics::MetricsRecorder;
use crate::policy::TierPolicy;
use crate::tier::{ItemId, Tier, TierEntry};

/// Typed migration failures.
///
/// **v1 note**: a `PersistenceFailed` variant was added in
/// the v1 wave so that the `FileBackedTieringStore` adapter
/// can surface I/O errors through the same `TieringStore`
/// trait. The reason is stringified to keep
/// `Clone + PartialEq + Eq`. Callers that previously
/// pattern-matched exhaustively need to add a wildcard arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrateError {
    /// `migrate` was called on an item that was never
    /// `place`d. The caller is responsible for placing the
    /// item first; Cinder does not silently insert.
    UnknownItem { tenant: TenantId, item: ItemId },

    /// The underlying storage adapter failed to persist an
    /// operation. Only emitted by adapters with side
    /// effects (e.g. `FileBackedTieringStore`); the v0
    /// `InMemoryTieringStore` never returns this.
    PersistenceFailed { reason: String },
}

impl fmt::Display for MigrateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MigrateError::UnknownItem { tenant, item } => write!(
                f,
                "cannot migrate unknown item {:?} for tenant {tenant}",
                item.as_str()
            ),
            MigrateError::PersistenceFailed { reason } => {
                write!(f, "persistence failed: {reason}")
            }
        }
    }
}

impl std::error::Error for MigrateError {}

/// The tiering port.
///
/// Semantics:
///
/// - **Per-tenant + per-item isolation.**
/// - **Stores metadata, not payloads.** The storage engines
///   own the payload bytes.
/// - **Forward-only automatic migration** under
///   `evaluate_at`; manual `migrate` honours any direction.
pub trait TieringStore {
    /// Record `(tenant, item)` as living in `tier` at
    /// `placed_at`. Overwrites any prior placement for the
    /// same key.
    ///
    /// Adapters that persist (e.g. `FileBackedTieringStore`)
    /// follow write-ahead ordering: the WAL append happens
    /// FIRST and the in-memory map is mutated ONLY on success.
    /// A persistence failure returns
    /// `MigrateError::PersistenceFailed` and leaves the prior
    /// in-memory state untouched (a failed overwrite preserves
    /// the prior durable value). The `InMemoryTieringStore`
    /// never persists, so it never returns this error.
    fn place(
        &self,
        tenant: &TenantId,
        item: &ItemId,
        tier: Tier,
        placed_at: SystemTime,
    ) -> Result<(), MigrateError>;

    /// Current tier for `(tenant, item)`, or `None` if not
    /// placed.
    fn get_tier(&self, tenant: &TenantId, item: &ItemId) -> Option<Tier>;

    /// Full tier-metadata entry for `(tenant, item)`, or
    /// `None` if not placed.
    fn get_entry(&self, tenant: &TenantId, item: &ItemId) -> Option<TierEntry>;

    /// Move an item to a new tier, updating `migrated_at`.
    /// Errors if the item was never placed.
    fn migrate(
        &self,
        tenant: &TenantId,
        item: &ItemId,
        to_tier: Tier,
        migrated_at: SystemTime,
    ) -> Result<(), MigrateError>;

    /// Every item id this tenant currently has in `tier`.
    fn list_by_tier(&self, tenant: &TenantId, tier: Tier) -> Vec<ItemId>;

    /// Evaluate the policy at simulated time `now`. Returns
    /// the total count of items migrated across all
    /// tenants. Idempotent if `now` and policy are stable.
    ///
    /// Persisting adapters fail-whole on the first WAL append
    /// failure (D3): they return
    /// `MigrateError::PersistenceFailed` carrying no count. The
    /// migrations applied before the failure stay durable and
    /// in memory (memory == disk); the failing migration is
    /// neither on disk nor in memory; the remainder is
    /// untouched. On `Ok(n)`, `n` equals the durably-migrated
    /// count exactly.
    fn evaluate_at(&self, now: SystemTime, policy: &TierPolicy) -> Result<usize, MigrateError>;
}

/// v0 in-process adapter. `HashMap<(TenantId, ItemId),
/// TierEntry>`.
pub struct InMemoryTieringStore {
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    state: Mutex<InnerState>,
}

#[derive(Default)]
struct InnerState {
    entries: HashMap<(TenantId, ItemId), TierEntry>,
}

impl InMemoryTieringStore {
    pub fn new(recorder: Box<dyn MetricsRecorder + Send + Sync>) -> Self {
        Self {
            recorder,
            state: Mutex::new(InnerState::default()),
        }
    }
}

impl fmt::Debug for InMemoryTieringStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryTieringStore")
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl TieringStore for InMemoryTieringStore {
    fn place(
        &self,
        tenant: &TenantId,
        item: &ItemId,
        tier: Tier,
        placed_at: SystemTime,
    ) -> Result<(), MigrateError> {
        let mut state = self.state.lock().expect("poisoned");
        let key = (tenant.clone(), item.clone());
        state.entries.insert(
            key,
            TierEntry {
                tier,
                placed_at,
                migrated_at: placed_at,
            },
        );
        self.recorder.record_place(tenant, tier);
        Ok(())
    }

    fn get_tier(&self, tenant: &TenantId, item: &ItemId) -> Option<Tier> {
        let state = self.state.lock().expect("poisoned");
        state
            .entries
            .get(&(tenant.clone(), item.clone()))
            .map(|e| e.tier)
    }

    fn get_entry(&self, tenant: &TenantId, item: &ItemId) -> Option<TierEntry> {
        let state = self.state.lock().expect("poisoned");
        state.entries.get(&(tenant.clone(), item.clone())).cloned()
    }

    fn migrate(
        &self,
        tenant: &TenantId,
        item: &ItemId,
        to_tier: Tier,
        migrated_at: SystemTime,
    ) -> Result<(), MigrateError> {
        let mut state = self.state.lock().expect("poisoned");
        let key = (tenant.clone(), item.clone());
        let entry = state
            .entries
            .get_mut(&key)
            .ok_or_else(|| MigrateError::UnknownItem {
                tenant: tenant.clone(),
                item: item.clone(),
            })?;
        let from = entry.tier;
        entry.tier = to_tier;
        entry.migrated_at = migrated_at;
        self.recorder.record_migrate(tenant, from, to_tier);
        Ok(())
    }

    fn list_by_tier(&self, tenant: &TenantId, tier: Tier) -> Vec<ItemId> {
        let state = self.state.lock().expect("poisoned");
        state
            .entries
            .iter()
            .filter(|((t, _), e)| t == tenant && e.tier == tier)
            .map(|((_, id), _)| id.clone())
            .collect()
    }

    fn evaluate_at(&self, now: SystemTime, policy: &TierPolicy) -> Result<usize, MigrateError> {
        let mut state = self.state.lock().expect("poisoned");
        let mut to_migrate: Vec<((TenantId, ItemId), Tier, Tier)> = Vec::new();
        for ((tenant, item), entry) in state.entries.iter() {
            let Some(threshold) = policy.threshold_from(entry.tier) else {
                continue;
            };
            let Some(next) = entry.tier.next_forward() else {
                continue;
            };
            let Ok(age) = now.duration_since(entry.migrated_at) else {
                continue; // clock skew — skip
            };
            if age >= threshold {
                to_migrate.push(((tenant.clone(), item.clone()), entry.tier, next));
            }
        }
        let migrated_count = to_migrate.len();
        // Record per-tenant migration counts.
        let mut per_tenant: HashMap<TenantId, usize> = HashMap::new();
        for (key, from, to) in to_migrate {
            if let Some(entry) = state.entries.get_mut(&key) {
                entry.tier = to;
                entry.migrated_at = now;
                self.recorder.record_migrate(&key.0, from, to);
                *per_tenant.entry(key.0).or_insert(0) += 1;
            }
        }
        for (tenant, count) in per_tenant {
            self.recorder.record_evaluate(&tenant, count);
        }
        Ok(migrated_count)
    }
}
