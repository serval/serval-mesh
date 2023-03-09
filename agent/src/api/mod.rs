use anyhow::Result;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
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
