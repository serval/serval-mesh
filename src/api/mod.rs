pub mod v1;

use anyhow::anyhow;
use axum::{routing::get, Router};

use std::net::SocketAddr;

/// Initialize an HTTP service. Set up all routes.
pub async fn init_http(host: &str, port: u16) -> anyhow::Result<()> {
    let app = Router::new().route("/ping", get(v1::ping));

    let addr = format!("{}:{}", host, port);
    log::info!("Job queue service about to listen on http://{addr}");
    let addr: SocketAddr = addr.parse()?;
    match axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow!(e)),
    }
}
