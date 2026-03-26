//! Persistent Agent Pool — long-lived agent processes with LRU eviction.
//!
//! Instead of spawning a new SDK session per message, the pool keeps sessions
//! alive between messages. On first message the session starts; subsequent
//! messages reuse the existing connection. Idle sessions are evicted when the
//! pool exceeds `max_capacity`.

use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::agent_sdk::AgentSession;

/// Metadata for a pooled agent session.
struct PoolEntry {
    session: AgentSession,
    last_used: Instant,
    agent_id: String,
    session_id: String,
}

/// A pool of long-lived agent sessions with LRU eviction.
pub struct AgentPool {
    entries: Mutex<HashMap<String, PoolEntry>>,
    max_capacity: usize,
}

impl AgentPool {
    /// Create a new agent pool with the given maximum capacity.
    pub fn new(max_capacity: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            max_capacity,
        }
    }

    /// Get the number of active sessions in the pool.
    pub async fn len(&self) -> usize {
        self.entries.lock().await.len()
    }

    /// Check if a session exists and is connected.
    pub async fn is_connected(&self, key: &str) -> bool {
        let pool = self.entries.lock().await;
        pool.get(key).map_or(false, |e| e.session.is_connected())
    }

    /// Insert a session into the pool. Evicts the least-recently-used session
    /// if the pool is at capacity.
    pub async fn insert(
        &self,
        key: String,
        session: AgentSession,
        agent_id: String,
        session_id: String,
    ) {
        let mut pool = self.entries.lock().await;

        // Evict LRU if at capacity
        if pool.len() >= self.max_capacity && !pool.contains_key(&key) {
            if let Some(lru_key) = pool
                .iter()
                .filter(|(_, e)| !e.session.is_connected())
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone())
                .or_else(|| {
                    pool.iter()
                        .min_by_key(|(_, e)| e.last_used)
                        .map(|(k, _)| k.clone())
                })
            {
                tracing::info!(
                    evicted_key = %lru_key,
                    pool_size = pool.len(),
                    "evicting LRU agent session from pool"
                );
                if let Some(mut entry) = pool.remove(&lru_key) {
                    let _ = entry.session.disconnect().await;
                }
            }
        }

        pool.insert(
            key,
            PoolEntry {
                session,
                last_used: Instant::now(),
                agent_id,
                session_id,
            },
        );
    }

    /// Remove a session from the pool and disconnect it.
    pub async fn remove(&self, key: &str) -> Option<AgentSession> {
        let mut pool = self.entries.lock().await;
        pool.remove(key).map(|e| e.session)
    }

    /// Touch a session (update last_used timestamp). Returns false if not found.
    pub async fn touch(&self, key: &str) -> bool {
        let mut pool = self.entries.lock().await;
        if let Some(entry) = pool.get_mut(key) {
            entry.last_used = Instant::now();
            true
        } else {
            false
        }
    }

    /// Drain all sessions from the pool, disconnecting each one.
    pub async fn drain_all(&self) -> Vec<(String, AgentSession)> {
        let mut pool = self.entries.lock().await;
        pool.drain()
            .map(|(k, e)| (k, e.session))
            .collect()
    }

    /// Remove sessions that have been idle longer than the given duration.
    pub async fn evict_idle(&self, max_idle: std::time::Duration) -> usize {
        let mut pool = self.entries.lock().await;
        let now = Instant::now();
        let stale_keys: Vec<String> = pool
            .iter()
            .filter(|(_, e)| now.duration_since(e.last_used) > max_idle)
            .map(|(k, _)| k.clone())
            .collect();

        let count = stale_keys.len();
        for key in stale_keys {
            if let Some(mut entry) = pool.remove(&key) {
                tracing::info!(
                    key = %key,
                    agent_id = %entry.agent_id,
                    idle_secs = now.duration_since(entry.last_used).as_secs(),
                    "evicting idle agent session"
                );
                let _ = entry.session.disconnect().await;
            }
        }
        count
    }

    /// Get pool status for health checks.
    pub async fn status(&self) -> PoolStatus {
        let pool = self.entries.lock().await;
        let total = pool.len();
        let connected = pool.values().filter(|e| e.session.is_connected()).count();
        PoolStatus {
            total,
            connected,
            max_capacity: self.max_capacity,
        }
    }
}

/// Pool health status.
#[derive(Debug, serde::Serialize)]
pub struct PoolStatus {
    pub total: usize,
    pub connected: usize,
    pub max_capacity: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pool_basics() {
        let pool = AgentPool::new(10);
        assert_eq!(pool.len().await, 0);
        assert!(!pool.is_connected("nonexistent").await);

        let status = pool.status().await;
        assert_eq!(status.total, 0);
        assert_eq!(status.max_capacity, 10);
    }
}
