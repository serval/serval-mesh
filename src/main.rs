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
use tokio::try_join;

use crate::util::ports::find_nearest_port;

mod api;
mod mdns;
mod queue;
mod util;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let did_find_dotenv = dotenv().ok().is_some();
    if cfg!(debug_assertions) && !did_find_dotenv {
        println!("Debug-only warning: no .env file found to configure logging; all logging will be disabled. Add RUST_LOG=info to .env to see logging.");
    }
    env_logger::init();

    let http_port = find_nearest_port(1717)?;
    let mdns = mdns::init_mdns(http_port);
    let http_server = api::init_http("0.0.0.0", http_port);
    try_join!(mdns, http_server)?;

    Err(anyhow!("HTTP server resolved unexpectedly"))
}
