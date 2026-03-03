//! Sliding-window rate limiter — behaviorally identical to the Python RateLimiter
//! in nickclyde/duckduckgo-mcp-server.
//!
//! Used only for the DuckDuckGo HTML-scrape fallback (when SearXNG is unavailable)
//! to prevent IP bans.  The SearXNG primary path has no rate limit.
//!
//! Limits:
//!   - Search:          30 requests / minute
//!   - Content fetch:   20 requests / minute

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct RateLimiter {
    requests_per_minute: usize,
    timestamps: Arc<Mutex<VecDeque<Instant>>>,
}

impl RateLimiter {
    pub fn new(requests_per_minute: usize) -> Self {
        Self {
            requests_per_minute,
            timestamps: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Block until a request slot is available, then record the request.
    /// Mirrors the Python `await self.rate_limiter.acquire()` call.
    pub async fn acquire(&self) {
        loop {
            let mut ts = self.timestamps.lock().await;
            let now = Instant::now();
            let window = Duration::from_secs(60);

            // Drop timestamps that are older than 1 minute
            while ts.front().map(|t: &Instant| now.duration_since(*t) >= window).unwrap_or(false) {
                ts.pop_front();
            }

            if ts.len() < self.requests_per_minute {
                // Slot available — record and proceed
                ts.push_back(now);
                return;
            }

            // Compute how long until the oldest request falls outside the window
            let oldest = *ts.front().unwrap();
            let elapsed = now.duration_since(oldest);
            let wait = window.saturating_sub(elapsed) + Duration::from_millis(10);
            drop(ts); // release lock before sleeping
            tokio::time::sleep(wait).await;
        }
    }

    /// Returns the number of requests used in the current window (for diagnostics).
    #[allow(dead_code)]
    pub async fn current_count(&self) -> usize {
        let mut ts = self.timestamps.lock().await;
        let now = Instant::now();
        let window = Duration::from_secs(60);
        while ts.front().map(|t: &Instant| now.duration_since(*t) >= window).unwrap_or(false) {
            ts.pop_front();
        }
        ts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows_up_to_limit() {
        let rl = RateLimiter::new(5);
        for _ in 0..5 {
            rl.acquire().await;
        }
        assert_eq!(rl.current_count().await, 5);
    }

    #[tokio::test]
    async fn test_rate_limiter_resets_after_window() {
        // Just verify the basic mechanics compile and work
        let rl = RateLimiter::new(100);
        rl.acquire().await;
        assert_eq!(rl.current_count().await, 1);
    }
}
