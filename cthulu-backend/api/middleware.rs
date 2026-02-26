use axum::{
    body::Body,
    http::{Request, Uri},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
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
