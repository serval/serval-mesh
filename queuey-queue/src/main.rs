#![forbid(unsafe_code)]
#![deny(future_incompatible)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    trivial_casts,
    unused_qualifications
)]

use std::{env, path::PathBuf};

use anyhow::anyhow;
use clap::Parser;
use dotenvy::dotenv;
use tokio::try_join;
use utils::ports::find_nearest_port;

mod api;
mod mdns;
mod queue;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    persist: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let did_find_dotenv = dotenv().ok().is_some();
    if cfg!(debug_assertions) && !did_find_dotenv {
        println!("Debug-only warning: no .env file found to configure logging; all logging will be disabled. Add RUST_LOG=info to .env to see logging.");
    }
    env_logger::init();

    // Figure out where we're storing data
    let args = Args::parse();
    let job_queue_persist_filename = args.persist.unwrap_or_else(|| {
        let default_filename = String::from("queuey-queue.json");
        log::warn!("No --persist filename specified; defaulting to $TMPDIR/{default_filename}");
        env::temp_dir().join(default_filename)
    });

    let http_port = find_nearest_port(1717)?;
    let mdns = mdns::init_mdns(http_port);
    let http_server = api::init_http("0.0.0.0", http_port, job_queue_persist_filename);
    try_join!(mdns, http_server)?;

    Err(anyhow!("HTTP server resolved unexpectedly"))
}
