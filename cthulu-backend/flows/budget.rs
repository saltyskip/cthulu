//! Budget gates for flow execution — prevents runaway resource usage.
//!
//! Adapted from the CI monitor pattern in bitcoin-portal/web-monorepo PR #972:
//! deterministic budget checks before expensive operations (agent calls, retries,
//! external API calls). Tracks action counts per flow run and enforces limits.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Budget configuration for a flow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunBudget {
    /// Max executor retries per node (default: 3).
    pub max_retries_per_node: u32,
    /// Max total executor calls across all nodes (default: 20).
    pub max_total_executor_calls: u32,
    /// Max total cost in USD (default: 5.0).
    pub max_total_cost_usd: f64,
    /// Max elapsed wall time in seconds (default: 600 = 10 min).
    pub max_elapsed_secs: u64,
}

impl Default for RunBudget {
    fn default() -> Self {
        Self {
            max_retries_per_node: 3,
            max_total_executor_calls: 20,
            max_total_cost_usd: 5.0,
            max_elapsed_secs: 600,
        }
    }
}

/// Tracks resource consumption during a flow run.
#[derive(Debug, Default)]
pub struct BudgetTracker {
    /// Per-node retry counts.
    retries: HashMap<String, u32>,
    /// Total executor calls made.
    total_executor_calls: u32,
    /// Accumulated cost.
    total_cost_usd: f64,
    /// When the run started.
    started_at: Option<std::time::Instant>,
}

/// Result of a budget gate check.
#[derive(Debug, PartialEq)]
pub enum BudgetCheck {
    /// Within budget, proceed.
    Allowed,
    /// Budget exceeded — contains the reason.
    Denied(String),
}

impl BudgetTracker {
    pub fn new() -> Self {
        Self {
            started_at: Some(std::time::Instant::now()),
            ..Default::default()
        }
    }

    /// Record an executor call for a node. Returns the new retry count.
    pub fn record_executor_call(&mut self, node_id: &str) -> u32 {
        let count = self.retries.entry(node_id.to_string()).or_insert(0);
        *count += 1;
        self.total_executor_calls += 1;
        *count
    }

    /// Record cost from an executor response.
    pub fn record_cost(&mut self, cost_usd: f64) {
        self.total_cost_usd += cost_usd;
    }

    /// Check if a node executor call is within budget.
    pub fn check(&self, node_id: &str, budget: &RunBudget) -> BudgetCheck {
        // Check per-node retries
        let node_retries = self.retries.get(node_id).copied().unwrap_or(0);
        if node_retries >= budget.max_retries_per_node {
            return BudgetCheck::Denied(format!(
                "node '{}' exceeded max retries ({}/{})",
                node_id, node_retries, budget.max_retries_per_node
            ));
        }

        // Check total executor calls
        if self.total_executor_calls >= budget.max_total_executor_calls {
            return BudgetCheck::Denied(format!(
                "total executor calls exceeded ({}/{})",
                self.total_executor_calls, budget.max_total_executor_calls
            ));
        }

        // Check cost
        if self.total_cost_usd >= budget.max_total_cost_usd {
            return BudgetCheck::Denied(format!(
                "total cost exceeded (${:.2}/${:.2})",
                self.total_cost_usd, budget.max_total_cost_usd
            ));
        }

        // Check elapsed time
        if let Some(started) = self.started_at {
            let elapsed = started.elapsed().as_secs();
            if elapsed >= budget.max_elapsed_secs {
                return BudgetCheck::Denied(format!(
                    "elapsed time exceeded ({}s/{}s)",
                    elapsed, budget.max_elapsed_secs
                ));
            }
        }

        BudgetCheck::Allowed
    }

    /// Get current stats for logging/reporting.
    pub fn stats(&self) -> BudgetStats {
        BudgetStats {
            total_executor_calls: self.total_executor_calls,
            total_cost_usd: self.total_cost_usd,
            elapsed_secs: self.started_at.map(|s| s.elapsed().as_secs()).unwrap_or(0),
            retries_by_node: self.retries.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BudgetStats {
    pub total_executor_calls: u32,
    pub total_cost_usd: f64,
    pub elapsed_secs: u64,
    pub retries_by_node: HashMap<String, u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_budget_is_reasonable() {
        let b = RunBudget::default();
        assert_eq!(b.max_retries_per_node, 3);
        assert_eq!(b.max_total_executor_calls, 20);
        assert!((b.max_total_cost_usd - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn tracker_allows_within_budget() {
        let mut t = BudgetTracker::new();
        let b = RunBudget::default();
        assert_eq!(t.check("node-1", &b), BudgetCheck::Allowed);

        t.record_executor_call("node-1");
        assert_eq!(t.check("node-1", &b), BudgetCheck::Allowed);
    }

    #[test]
    fn tracker_denies_retries_exceeded() {
        let mut t = BudgetTracker::new();
        let b = RunBudget { max_retries_per_node: 2, ..Default::default() };

        t.record_executor_call("node-1");
        t.record_executor_call("node-1");
        assert!(matches!(t.check("node-1", &b), BudgetCheck::Denied(_)));
        // Other nodes still allowed
        assert_eq!(t.check("node-2", &b), BudgetCheck::Allowed);
    }

    #[test]
    fn tracker_denies_total_calls_exceeded() {
        let mut t = BudgetTracker::new();
        let b = RunBudget { max_total_executor_calls: 3, ..Default::default() };

        t.record_executor_call("a");
        t.record_executor_call("b");
        t.record_executor_call("c");
        assert!(matches!(t.check("d", &b), BudgetCheck::Denied(_)));
    }

    #[test]
    fn tracker_denies_cost_exceeded() {
        let mut t = BudgetTracker::new();
        let b = RunBudget { max_total_cost_usd: 1.0, ..Default::default() };

        t.record_cost(0.8);
        assert_eq!(t.check("node-1", &b), BudgetCheck::Allowed);
        t.record_cost(0.3);
        assert!(matches!(t.check("node-1", &b), BudgetCheck::Denied(_)));
    }

    #[test]
    fn stats_are_accurate() {
        let mut t = BudgetTracker::new();
        t.record_executor_call("a");
        t.record_executor_call("a");
        t.record_executor_call("b");
        t.record_cost(1.5);

        let s = t.stats();
        assert_eq!(s.total_executor_calls, 3);
        assert!((s.total_cost_usd - 1.5).abs() < f64::EPSILON);
        assert_eq!(s.retries_by_node.get("a"), Some(&2));
        assert_eq!(s.retries_by_node.get("b"), Some(&1));
    }
}
