#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]

use anyhow::anyhow;
use dotenvy::dotenv;

use crate::api::init_http;
use crate::util::ports::find_nearest_port;

mod api;
mod util;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let did_find_dotenv = dotenv().ok().is_some();
    if cfg!(debug_assertions) && !did_find_dotenv {
        println!("Debug-only warning: no .env file found to configure logging; all logging will be disabled. Add RUST_LOG=info to .env to see logging.");
    }
    env_logger::init();

    let http_port = find_nearest_port(1717)?;
    init_http("0.0.0.0", http_port).await?;

    Err(anyhow!("HTTP server resolved unexpectedly"))
}
