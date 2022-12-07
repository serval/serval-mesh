use axum::{
    http::StatusCode,
    routing::{get, post},
    Router,
};
use dotenvy::dotenv;
use std::net::SocketAddr;

/// Respond to ping. Useful for monitoring.
async fn ping() -> String {
    "pong".to_string()
}

/// Unimplemented main worker endpoint.
async fn incoming() -> (StatusCode, String) {
    (StatusCode::NOT_IMPLEMENTED, "unimplemented".to_string())
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    env_logger::init();

    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8000".to_string());

    let app = Router::new()
        .route("/monitor/ping", get(ping))
        .route("/jobs", post(incoming));

    let addr = format!("{}:{}", host, port);
    log::info!("wait-for-it daemon listening on {}", &addr);

    let addr: SocketAddr = addr.parse().unwrap();
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
