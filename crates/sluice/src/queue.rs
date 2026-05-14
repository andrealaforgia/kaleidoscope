// Kaleidoscope Sluice — queue port between Sieve and storage
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

//! Queue trait + in-memory adapter.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::Mutex;

use aegis::TenantId;

use crate::metrics::MetricsRecorder;

/// Stable identity for a message across enqueue / dequeue / ack /
/// nack. Monotonically increasing within an adapter instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessageId(pub u64);

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "msg-{}", self.0)
    }
}

/// One message in flight through Sluice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: MessageId,
    pub tenant: TenantId,
    pub payload: Vec<u8>,
}

/// Typed enqueue failures. v0 has one variant; adapters that fail
/// for other reasons (Kafka broker down, etc.) will extend the
/// enum at v1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueError {
    /// The tenant's queue is at capacity. Operator decides whether
    /// to drop, retry, or alert. Stable variant name for v0; v1
    /// adapters may add `BackendUnavailable`, `Timeout`, etc.
    Full { tenant: TenantId, cap: usize },
}

impl fmt::Display for EnqueueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnqueueError::Full { tenant, cap } => {
                write!(f, "queue full for tenant {tenant} (cap {cap})")
            }
        }
    }
}

impl std::error::Error for EnqueueError {}

/// The queue port. v0 ships [`InMemoryQueue`] as the only adapter;
/// Kafka / NATS / Redpanda adapters land at v1 behind this trait.
///
/// Semantics:
///
/// - **FIFO within a tenant.** Two enqueues to the same tenant
///   dequeue in enqueue order.
/// - **Isolated across tenants.** Tenant A's `dequeue` never
///   returns tenant B's messages.
/// - **At-least-once.** A dequeued message is held by the
///   consumer; `ack` removes it permanently, `nack` returns it to
///   the head of its tenant's queue for redelivery.
/// - **Bounded.** Each tenant's queue caps at a configurable size;
///   enqueue beyond cap returns [`EnqueueError::Full`].
pub trait Queue {
    /// Enqueue a payload for the given tenant. Returns the
    /// assigned message id or a typed error.
    fn enqueue(&self, tenant: &TenantId, payload: Vec<u8>) -> Result<MessageId, EnqueueError>;

    /// Dequeue the next pending message for this tenant, if any.
    /// The returned message is held by the consumer until `ack`
    /// or `nack` is called.
    fn dequeue(&self, tenant: &TenantId) -> Option<Message>;

    /// Permanently remove a message from the queue. Idempotent:
    /// acking an unknown id is a no-op.
    fn ack(&self, id: MessageId);

    /// Return a message to the head of its tenant's queue for
    /// redelivery. Idempotent: nacking an unknown id is a no-op.
    fn nack(&self, id: MessageId);

    /// Pending count for one tenant. O(1).
    fn depth(&self, tenant: &TenantId) -> usize;

    /// Pending count across every tenant. O(1).
    fn total_depth(&self) -> usize;
}

/// v0 in-process adapter. `HashMap<TenantId, VecDeque<Message>>`
/// plus a separate map of in-flight (dequeued but not yet acked)
/// messages keyed by id.
pub struct InMemoryQueue {
    cap: usize,
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    state: Mutex<InnerState>,
}

#[derive(Default)]
struct InnerState {
    next_id: u64,
    pending: HashMap<TenantId, VecDeque<Message>>,
    in_flight: HashMap<MessageId, Message>,
    total: usize,
}

impl InMemoryQueue {
    /// Construct an in-memory queue with the given per-tenant
    /// capacity and metrics recorder. The recorder is called on
    /// every enqueue / dequeue / ack / nack.
    pub fn new(cap: usize, recorder: Box<dyn MetricsRecorder + Send + Sync>) -> Self {
        Self {
            cap,
            recorder,
            state: Mutex::new(InnerState::default()),
        }
    }
}

impl fmt::Debug for InMemoryQueue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryQueue")
            .field("cap", &self.cap)
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl Queue for InMemoryQueue {
    fn enqueue(&self, tenant: &TenantId, payload: Vec<u8>) -> Result<MessageId, EnqueueError> {
        let mut state = self.state.lock().expect("poisoned");
        let queue = state.pending.entry(tenant.clone()).or_default();
        if queue.len() >= self.cap {
            self.recorder.record_enqueue(tenant, false);
            return Err(EnqueueError::Full {
                tenant: tenant.clone(),
                cap: self.cap,
            });
        }
        state.next_id += 1;
        let id = MessageId(state.next_id);
        let message = Message {
            id,
            tenant: tenant.clone(),
            payload,
        };
        state
            .pending
            .get_mut(tenant)
            .expect("just inserted")
            .push_back(message);
        state.total += 1;
        self.recorder.record_enqueue(tenant, true);
        Ok(id)
    }

    fn dequeue(&self, tenant: &TenantId) -> Option<Message> {
        let mut state = self.state.lock().expect("poisoned");
        let queue = state.pending.get_mut(tenant)?;
        let message = queue.pop_front()?;
        if queue.is_empty() {
            state.pending.remove(tenant);
        }
        state.total -= 1;
        state.in_flight.insert(message.id, message.clone());
        self.recorder.record_dequeue(tenant);
        Some(message)
    }

    fn ack(&self, id: MessageId) {
        let mut state = self.state.lock().expect("poisoned");
        if let Some(msg) = state.in_flight.remove(&id) {
            self.recorder.record_ack(&msg.tenant);
        }
    }

    fn nack(&self, id: MessageId) {
        let mut state = self.state.lock().expect("poisoned");
        if let Some(message) = state.in_flight.remove(&id) {
            let tenant = message.tenant.clone();
            state
                .pending
                .entry(tenant.clone())
                .or_default()
                .push_front(message);
            state.total += 1;
            self.recorder.record_nack(&tenant);
        }
    }

    fn depth(&self, tenant: &TenantId) -> usize {
        let state = self.state.lock().expect("poisoned");
        state.pending.get(tenant).map(|q| q.len()).unwrap_or(0)
    }

    fn total_depth(&self) -> usize {
        let state = self.state.lock().expect("poisoned");
        state.total
    }
}
