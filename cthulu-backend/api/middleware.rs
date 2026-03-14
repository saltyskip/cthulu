use axum::{
    body::Body,
    http::{Request, Uri},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use tracing::Span;

/// Log method, path, status, and duration for each request.
/// Skips noisy paths like /health and SSE streams.
pub async fn request_logging(req: Request<Body>, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // Skip noisy endpoints
    if path == "/health" || path.ends_with("/stream") || path == "/api/changes" {
        return next.run(req).await;
    }

    let start = std::time::Instant::now();
    let response = next.run(req).await;
    let duration = start.elapsed();
    let status = response.status().as_u16();

    tracing::info!(
        method = %method,
        path = %path,
        status = status,
        duration_ms = duration.as_millis() as u64,
        "request"
    );

    response
}

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

pub async fn strip_trailing_slash(req: Request<Body>, next: Next) -> Response {
    let uri = req.uri();

    if let Some(path) = uri.path().strip_suffix('/') {
        let mut parts = uri.clone().into_parts();
        parts.path_and_query = Some(if let Some(query) = uri.query() {
            format!("{}?{}", path, query).parse().unwrap()
        } else {
            path.parse().unwrap()
        });

        let new_uri = Uri::from_parts(parts).unwrap();

        Redirect::permanent(&new_uri.to_string()).into_response()
    } else {
        next.run(req).await
    }
}
