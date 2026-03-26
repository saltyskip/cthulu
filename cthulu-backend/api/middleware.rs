use axum::{
    body::Body,
    http::{Request, Uri},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use hyper::StatusCode;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::Span;

pub async fn enrich_current_span_middleware(req: Request<Body>, next: Next) -> Response {
    let uri: &Uri = req.uri();

    let host = req
        .headers()
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("UNKNOWN");

    let current_span = Span::current();

    current_span.record("http.uri", &uri.path());
    current_span.record("http.host", &host);
    if let Some(query) = uri.query() {
        current_span.record("http.query", &query);
    }

    next.run(req).await
}

// ---------------------------------------------------------------------------
// Rate limiting (token bucket per IP)
// ---------------------------------------------------------------------------

/// In-memory rate limiter using a token bucket per client IP.
/// Designed for cloud multi-tenant deployments.
#[derive(Clone)]
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    /// Maximum requests per window.
    pub max_requests: u32,
    /// Window duration in seconds.
    pub window_secs: u64,
}

struct TokenBucket {
    tokens: u32,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window_secs,
        }
    }

    /// Check if a request from `client_ip` is allowed. Returns true if allowed.
    pub async fn check(&self, client_ip: &str) -> bool {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();

        let bucket = buckets.entry(client_ip.to_string()).or_insert(TokenBucket {
            tokens: self.max_requests,
            last_refill: now,
        });

        // Refill tokens if window has elapsed
        let elapsed = now.duration_since(bucket.last_refill).as_secs();
        if elapsed >= self.window_secs {
            bucket.tokens = self.max_requests;
            bucket.last_refill = now;
        }

        if bucket.tokens > 0 {
            bucket.tokens -= 1;
            true
        } else {
            false
        }
    }
}

/// Axum middleware that applies rate limiting based on client IP.
pub async fn rate_limit_middleware(
    axum::extract::State(limiter): axum::extract::State<RateLimiter>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .unwrap_or("unknown")
        .trim()
        .to_string();

    if !limiter.check(&client_ip).await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded. Try again later.",
        )
            .into_response();
    }

    next.run(req).await
}

pub async fn strip_trailing_slash(req: Request<Body>, next: Next) -> Response {
    let uri = req.uri();
    let path = uri.path();

    // Don't redirect root "/" — only strip trailing slash from longer paths
    if path.len() > 1 && path.ends_with('/') {
        let stripped = &path[..path.len() - 1];
        let mut parts = uri.clone().into_parts();
        parts.path_and_query = Some(if let Some(query) = uri.query() {
            format!("{stripped}?{query}").parse().unwrap()
        } else {
            stripped.parse().unwrap()
        });

        let new_uri = Uri::from_parts(parts).unwrap();
        Redirect::permanent(&new_uri.to_string()).into_response()
    } else {
        next.run(req).await
    }
}
