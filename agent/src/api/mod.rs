use anyhow::Result;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http::header::HeaderValue;

use crate::structures::AppState;

pub mod v1;
// Follow this pattern for additional major versions. E.g.,
// pub mod v2;

/// Remember what is important.
pub async fn clacks<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let mut response = next.run(req).await;
    if !response.headers().contains_key("X-Clacks-Overhead") {
        response.headers_mut().append(
            "X-Clacks-Overhead",
            HeaderValue::from_static("GNU/Terry Pratchett"),
        );
    }
    Ok(response)
}

pub async fn http_logging<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let method = req.method().to_owned();
    let uri = req.uri().to_owned();
    let response = next.run(req).await;
    if let Some(proxied_from) = response.headers().get("Serval-Proxied-From") {
        log::info!(
            "{} {} {} (via {})",
            response.status().as_u16(),
            method,
            uri,
            proxied_from.to_str().unwrap(),
        );
    } else {
        log::info!("{} {} {}", response.status().as_u16(), method, uri);
    }
    Ok(response)
}

/// Respond to ping. Useful for monitoring.
pub async fn ping() -> String {
    metrics::increment_counter!("monitor:ping");
    "pong".to_string()
}

/// Report on node health.
pub async fn monitor_status(_state: State<AppState>) -> impl IntoResponse {
    metrics::increment_counter!("monitor:status");
    StatusCode::NOT_IMPLEMENTED
}
